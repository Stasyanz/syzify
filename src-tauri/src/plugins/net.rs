//! Brokered HTTP for plugins — `host_http`.
//!
//! Extism's built-in HTTP cannot carry a login: response headers are off in
//! our runtime, repeated `Set-Cookie` headers collapse into one, nothing
//! remembers cookies across requests or redirect hops, and a redirect is
//! not checked against the plugin's allow-list. This module is what the
//! runtime wires in instead. Every request — and every redirect hop — must
//! target a host the plugin declared via `net:host=`; a cookie jar lives
//! for one invocation, in memory, never persisted; the caps are the host's.
//! Tauri-free, like the rest of the host layer.

use std::io::Read;
use std::time::{Duration, Instant};

use reqwest::header::{HeaderName, HeaderValue, AUTHORIZATION, CONTENT_TYPE, COOKIE, LOCATION, SET_COOKIE};
use reqwest::{Method, StatusCode, Url};
use serde::{Deserialize, Serialize};

/// Longest response body a plugin may receive: a Garmin activity page is
/// ~100 KB, a FIT download a few MB.
pub const HTTP_MAX_RESPONSE_BYTES: usize = 5 * 1024 * 1024;
/// Longest request body a plugin may send.
pub const HTTP_MAX_REQUEST_BYTES: usize = 1024 * 1024;
/// Redirect hops followed for one request (a plugin may ask for fewer).
pub const HTTP_MAX_REDIRECTS: usize = 10;
/// Cookies one jar holds; the oldest goes when a new one does not fit
/// (RFC 6265 §6.1 asks for 50 per domain — one invocation talks to a
/// handful of hosts for seconds).
pub const JAR_MAX_COOKIES: usize = 100;
/// Longest cookie (name + value) the jar takes; longer ones are ignored.
pub const COOKIE_MAX_BYTES: usize = 4096;
/// Cap on one hop, connect to last body byte — shortened to what is left
/// of the invocation's budget, so a request never outlives its plugin.
pub const HTTP_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// Headers the transport owns; a plugin setting them would only confuse it.
const RESERVED_HEADERS: [&str; 7] =
    ["host", "content-length", "transfer-encoding", "connection", "expect", "upgrade", "te"];

/// What a plugin hands to `host_http` (JSON).
#[derive(Deserialize)]
pub struct HttpRequest {
    pub url: String,
    /// GET by default.
    #[serde(default)]
    pub method: Option<String>,
    /// Header pairs; a repeated name is sent repeatedly.
    #[serde(default)]
    pub headers: Vec<(String, String)>,
    /// Redirect hops to follow, at most [`HTTP_MAX_REDIRECTS`] (the
    /// default). 0 hands the first response back as it is — a login flow
    /// reads the ticket from a 302's `Location` that way.
    #[serde(default)]
    pub max_redirects: Option<usize>,
}

/// What `host_http_meta` returns: the final response of the last request.
#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
pub struct HttpMeta {
    pub status: u16,
    /// Where the final response came from — after redirects, so a login
    /// flow can read the ticket a redirect chain ends on.
    pub url: String,
    /// (lowercase name, value) pairs; a repeated header survives as pairs.
    pub headers: Vec<(String, String)>,
    /// The redirects followed on the way, in order.
    pub hops: Vec<Hop>,
}

/// One redirect the host followed.
#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
pub struct Hop {
    pub status: u16,
    /// The URL that answered with the redirect.
    pub url: String,
    /// Its `Location`, as sent.
    pub location: String,
}

/// The network side of one invocation.
#[derive(Default)]
pub struct NetState {
    pub jar: CookieJar,
    /// Meta of the last completed request, for `host_http_meta`. Cleared
    /// when a request starts, so a failed one cannot pass off its
    /// predecessor's status as its own.
    pub last: Option<HttpMeta>,
    /// When the invocation's wall-clock budget ends; no hop runs past it.
    pub deadline: Option<Instant>,
    /// Tests only: accept `http://` on any port of a loopback address, so
    /// the jar and redirect rules can be exercised against a plain server
    /// on 127.0.0.1. Compiled out of the app: there the rule is https on
    /// 443 and nothing else (`net:host=` refuses IP literals and
    /// `localhost` anyway).
    #[cfg(test)]
    pub plain_loopback: bool,
    client: Option<reqwest::blocking::Client>,
}

impl NetState {
    /// The runtime's state: nothing runs past `deadline`.
    pub fn with_deadline(deadline: Instant) -> Self {
        NetState { deadline: Some(deadline), ..NetState::default() }
    }

    #[cfg(test)]
    fn plain_loopback(&self) -> bool {
        self.plain_loopback
    }

    #[cfg(not(test))]
    fn plain_loopback(&self) -> bool {
        false
    }
}

/// One brokered request: the final response's body, its meta left in
/// `state.last`. `allowed` are the plugin's `net:host=` hosts. Redirects
/// are followed here (not by reqwest) so that every hop is checked
/// against `allowed`, the jar sees every hop's `Set-Cookie` and lends its
/// cookies to every hop, and `Authorization` / `Cookie` the plugin set do
/// not leak to another host.
pub fn fetch(
    state: &mut NetState,
    allowed: &[String],
    request_json: &str,
    body: &[u8],
) -> Result<Vec<u8>, String> {
    state.last = None;
    let request: HttpRequest =
        serde_json::from_str(request_json).map_err(|e| format!("bad request JSON: {e}"))?;
    let mut url = parse_url(&request.url)?;
    let plain_loopback = state.plain_loopback();
    check_hop(&url, allowed, plain_loopback)?;
    let mut method = parse_method(request.method.as_deref())?;
    if body.len() > HTTP_MAX_REQUEST_BYTES {
        return Err(format!(
            "request body is {} bytes — larger than the {} MiB limit",
            body.len(),
            HTTP_MAX_REQUEST_BYTES / (1024 * 1024)
        ));
    }
    if !body.is_empty() && matches!(method, Method::GET | Method::HEAD) {
        return Err(format!("a {method} request cannot carry a body"));
    }
    let mut body = body.to_vec();
    let mut headers = parse_headers(&request.headers)?;
    let max_redirects = request.max_redirects.unwrap_or(HTTP_MAX_REDIRECTS).min(HTTP_MAX_REDIRECTS);
    let mut hops = Vec::new();

    // A call without an invocation deadline (none in the app) still ends:
    // the per-hop cap is the whole call's.
    let deadline = state.deadline.unwrap_or_else(|| Instant::now() + HTTP_REQUEST_TIMEOUT);
    // One client per invocation — its own connection pool, nothing shared
    // between plugins. Building it loads the system root store (~300 ms
    // on macOS), paid once per invocation that talks to the network.
    let client = state
        .client
        .get_or_insert_with(|| {
            reqwest::blocking::Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .expect("reqwest client")
        })
        .clone();

    loop {
        let timeout = remaining(deadline)?;
        let mut req = client.request(method.clone(), url.clone()).timeout(timeout);
        for (name, value) in &headers {
            if name != COOKIE {
                req = req.header(name, value);
            }
        }
        // The jar's cookies first, then whatever the plugin carried in
        // itself (a session restored from `data:own`), as one header.
        if let Some(cookie) = cookie_header(&state.jar, &url, &headers)? {
            req = req.header(COOKIE, cookie);
        }
        if !body.is_empty() {
            req = req.body(body.clone());
        }
        let response = req.send().map_err(|e| describe(&e))?;
        let final_url = response.url().clone();
        for set_cookie in response.headers().get_all(SET_COOKIE) {
            if let Ok(s) = set_cookie.to_str() {
                state.jar.store(&final_url, s);
            }
        }
        let status = response.status();
        let location = response.headers().get(LOCATION).cloned();
        match (redirect_kind(status, &method), location) {
            // A redirect past the plugin's limit is the final response:
            // the plugin reads its Location itself.
            (Some(kind), Some(location)) if hops.len() < max_redirects => {
                let location = location
                    .to_str()
                    .map_err(|_| "redirect Location is not valid text".to_string())?;
                let next = final_url
                    .join(location)
                    .map_err(|e| format!("bad redirect Location from {}: {e}", shown(&final_url)))?;
                check_hop(&next, allowed, plain_loopback)?;
                hops.push(Hop { status: status.as_u16(), url: final_url.to_string(), location: location.to_string() });
                // Cookies are a host's (RFC 6265 ignores the port), an
                // Authorization is an origin's. A 307/308 body is replayed
                // to the new host, as a browser does — both are declared.
                let same_host = next.host_str() == url.host_str();
                let same_origin = same_host && next.port_or_known_default() == url.port_or_known_default();
                headers.retain(|(n, _)| (n != COOKIE || same_host) && (n != AUTHORIZATION || same_origin));
                if kind == Redirect::AsGet {
                    method = Method::GET;
                    body.clear();
                    headers.retain(|(n, _)| n != CONTENT_TYPE);
                }
                url = next;
            }
            _ => {
                let meta = HttpMeta {
                    status: status.as_u16(),
                    url: final_url.to_string(),
                    headers: response
                        .headers()
                        .iter()
                        .map(|(n, v)| (n.as_str().to_string(), String::from_utf8_lossy(v.as_bytes()).into_owned()))
                        .collect(),
                    hops: std::mem::take(&mut hops),
                };
                let body = read_body(response)?;
                state.last = Some(meta);
                return Ok(body);
            }
        }
    }
}

/// A URL as error messages show it: no query, no fragment — a login flow
/// carries its ticket in the query, and the message reaches the UI and
/// the log.
fn shown(url: &Url) -> String {
    let port = url.port().map(|p| format!(":{p}")).unwrap_or_default();
    format!("{}://{}{port}{}", url.scheme(), url.host_str().unwrap_or(""), url.path())
}

#[derive(Debug, PartialEq, Eq)]
enum Redirect {
    /// 307/308: same method and body.
    Same,
    /// 303, and 301/302 after a POST: browsers switch to GET.
    AsGet,
}

fn redirect_kind(status: StatusCode, method: &Method) -> Option<Redirect> {
    match status {
        StatusCode::TEMPORARY_REDIRECT | StatusCode::PERMANENT_REDIRECT => Some(Redirect::Same),
        StatusCode::SEE_OTHER => Some(Redirect::AsGet),
        StatusCode::MOVED_PERMANENTLY | StatusCode::FOUND => {
            Some(if *method == Method::POST { Redirect::AsGet } else { Redirect::Same })
        }
        _ => None,
    }
}

fn parse_url(raw: &str) -> Result<Url, String> {
    let url = Url::parse(raw).map_err(|e| format!("bad URL: {e}"))?;
    if !url.username().is_empty() || url.password().is_some() {
        return Err(format!("{} carries credentials in the URL; send them as a header", shown(&url)));
    }
    Ok(url)
}

/// The allow-list rule, applied to the first request and to every
/// redirect hop: `https://` on its default port and a declared host name
/// — what "network access to one host" promises on the Plugins screen.
fn check_hop(url: &Url, allowed: &[String], plain_loopback: bool) -> Result<(), String> {
    let host = url
        .host_str()
        .ok_or_else(|| format!("{} has no host", shown(url)))?
        .to_ascii_lowercase();
    let plain_ok = plain_loopback && is_loopback(url);
    if url.scheme() != "https" && !plain_ok {
        return Err(format!("{}: only https:// is allowed", shown(url)));
    }
    // `Url` drops a scheme's default port, so `:443` reads as none.
    if url.port().is_some() && !plain_ok {
        return Err(format!("{}: only the default port is allowed", shown(url)));
    }
    if !allowed.iter().any(|h| h.eq_ignore_ascii_case(&host)) {
        return Err(format!("{}: host {host:?} is not declared (net:host={host})", shown(url)));
    }
    Ok(())
}

fn is_loopback(url: &Url) -> bool {
    host_ip(url).map_or_else(|| url.domain().is_some_and(|d| d.eq_ignore_ascii_case("localhost")), |ip| ip.is_loopback())
}

/// The host as an IP literal, if it is one (`[::1]` included).
fn host_ip(url: &Url) -> Option<std::net::IpAddr> {
    url.host_str()?.trim_start_matches('[').trim_end_matches(']').parse().ok()
}

fn parse_method(raw: Option<&str>) -> Result<Method, String> {
    let raw = raw.unwrap_or("GET").to_ascii_uppercase();
    match raw.as_str() {
        "GET" | "HEAD" | "POST" | "PUT" | "PATCH" | "DELETE" | "OPTIONS" => Method::from_bytes(raw.as_bytes()).map_err(|e| e.to_string()),
        other => Err(format!("unsupported method {other:?}")),
    }
}

fn parse_headers(raw: &[(String, String)]) -> Result<Vec<(HeaderName, HeaderValue)>, String> {
    raw.iter()
        .map(|(name, value)| {
            let n = HeaderName::from_bytes(name.as_bytes()).map_err(|_| format!("bad header name {name:?}"))?;
            if RESERVED_HEADERS.contains(&n.as_str()) {
                return Err(format!("header {name:?} is set by the host"));
            }
            let v = HeaderValue::from_str(value).map_err(|_| format!("bad value for header {name:?}"))?;
            Ok((n, v))
        })
        .collect()
}

/// The one `Cookie` header of a hop: the jar's cookies, then every
/// `Cookie` header the plugin set, joined.
fn cookie_header(
    jar: &CookieJar,
    url: &Url,
    headers: &[(HeaderName, HeaderValue)],
) -> Result<Option<HeaderValue>, String> {
    let parts: Vec<String> = jar
        .header_for(url)
        .into_iter()
        .chain(
            headers
                .iter()
                .filter(|(n, _)| n == COOKIE)
                .filter_map(|(_, v)| v.to_str().ok())
                .filter(|s| !s.is_empty())
                .map(str::to_string),
        )
        .collect();
    if parts.is_empty() {
        return Ok(None);
    }
    // Every part came out of a valid header value; the check is for form.
    HeaderValue::from_str(&parts.join("; ")).map(Some).map_err(|_| "the Cookie header is not a valid header value".to_string())
}

fn remaining(deadline: Instant) -> Result<Duration, String> {
    let left = deadline.saturating_duration_since(Instant::now());
    if left.is_zero() {
        return Err("the invocation's time budget is spent".to_string());
    }
    Ok(left.min(HTTP_REQUEST_TIMEOUT))
}

fn read_body(mut response: reqwest::blocking::Response) -> Result<Vec<u8>, String> {
    let cap = HTTP_MAX_RESPONSE_BYTES;
    let too_large = |len: usize| format!("response is {len} bytes — larger than the {} MiB limit", cap / (1024 * 1024));
    if let Some(len) = response.content_length() {
        if len > cap as u64 {
            return Err(too_large(len as usize));
        }
    }
    let mut body = Vec::new();
    (&mut response)
        .take(cap as u64 + 1)
        .read_to_end(&mut body)
        .map_err(|e| {
            // reqwest's request timeout covers the body too; it comes out
            // of `read` as an io error wrapping the reqwest one.
            let timed_out = e
                .get_ref()
                .and_then(|inner| inner.downcast_ref::<reqwest::Error>())
                .is_some_and(reqwest::Error::is_timeout);
            let mut cause: &dyn std::error::Error = &e;
            while let Some(next) = cause.source() {
                cause = next;
            }
            format!("reading the response {}: {cause}", if timed_out { "timed out" } else { "failed" })
        })?;
    if body.len() > cap {
        return Err(too_large(body.len()));
    }
    Ok(body)
}

/// reqwest's messages nest ("error sending request for url (…): …"); the
/// plugin gets the URL and the innermost cause.
fn describe(e: &reqwest::Error) -> String {
    let url = e.url().map(shown).unwrap_or_default();
    let mut cause: &dyn std::error::Error = e;
    while let Some(next) = cause.source() {
        cause = next;
    }
    let what = if e.is_timeout() { "timed out" } else { "failed" };
    format!("request to {url} {what}: {cause}")
}

/// A cookie jar the size of one invocation. RFC 6265 as far as a login
/// flow needs it: name/value, Domain (must cover the responding host),
/// Path (default path from the request), Secure, a `Max-Age` of zero or
/// an `Expires` in the past deletes; HttpOnly and SameSite are ignored —
/// everything here is one "browser" living for seconds.
#[derive(Default, Debug)]
pub struct CookieJar {
    cookies: Vec<Cookie>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Cookie {
    name: String,
    value: String,
    /// Lowercase, no leading dot.
    domain: String,
    /// No Domain attribute: only the exact host that set it gets it back.
    host_only: bool,
    path: String,
    secure: bool,
}

impl CookieJar {
    /// Take in one `Set-Cookie` header of a response from `url`. A cookie
    /// whose Domain does not cover the responding host is ignored, as a
    /// browser would.
    pub fn store(&mut self, url: &Url, set_cookie: &str) {
        let Some((cookie, delete)) = parse_set_cookie(url, set_cookie) else { return };
        let same = |c: &Cookie| c.name == cookie.name && c.domain == cookie.domain && c.path == cookie.path;
        match (self.cookies.iter().position(same), delete) {
            (Some(i), true) => {
                self.cookies.remove(i);
            }
            // Replaced in place: the creation order is the tiebreak among
            // equal path lengths (RFC 6265 §5.3 step 11, §5.4).
            (Some(i), false) => self.cookies[i] = cookie,
            (None, true) => {}
            (None, false) => {
                if self.cookies.len() >= JAR_MAX_COOKIES {
                    self.cookies.remove(0);
                }
                self.cookies.push(cookie);
            }
        }
    }

    /// The `Cookie` header value for a request to `url`, longest path first.
    pub fn header_for(&self, url: &Url) -> Option<String> {
        let host = url.host_str()?.to_ascii_lowercase();
        let path = request_path(url);
        let secure = url.scheme() == "https";
        let mut matching: Vec<&Cookie> = self
            .cookies
            .iter()
            .filter(|c| (secure || !c.secure) && c.covers(&host) && path_matches(&c.path, path))
            .collect();
        if matching.is_empty() {
            return None;
        }
        matching.sort_by_key(|c| std::cmp::Reverse(c.path.len()));
        Some(matching.iter().map(|c| format!("{}={}", c.name, c.value)).collect::<Vec<_>>().join("; "))
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.cookies.len()
    }

    #[cfg(test)]
    pub fn is_empty(&self) -> bool {
        self.cookies.is_empty()
    }
}

impl Cookie {
    fn covers(&self, host: &str) -> bool {
        if self.host_only {
            host == self.domain
        } else {
            domain_matches(host, &self.domain)
        }
    }
}

fn domain_matches(host: &str, domain: &str) -> bool {
    host == domain || host.strip_suffix(domain).is_some_and(|rest| rest.ends_with('.'))
}

fn path_matches(cookie_path: &str, request_path: &str) -> bool {
    request_path == cookie_path
        || request_path
            .strip_prefix(cookie_path)
            .is_some_and(|rest| cookie_path.ends_with('/') || rest.starts_with('/'))
}

fn request_path(url: &Url) -> &str {
    let p = url.path();
    if p.is_empty() { "/" } else { p }
}

/// RFC 6265 §5.1.4: the request path up to (not including) its last `/`.
fn default_path(url: &Url) -> String {
    let p = request_path(url);
    match p.rfind('/') {
        Some(0) | None => "/".to_string(),
        Some(i) => p[..i].to_string(),
    }
}

/// `(cookie, delete)` — or `None` when the header is not a cookie for this
/// host.
fn parse_set_cookie(url: &Url, header: &str) -> Option<(Cookie, bool)> {
    let host = url.host_str()?.to_ascii_lowercase();
    let host_is_ip = host_ip(url).is_some();
    let mut parts = header.split(';');
    let (name, value) = parts.next()?.split_once('=')?;
    let name = name.trim();
    if name.is_empty()
        || name.contains(|c: char| c.is_control() || c.is_whitespace())
        || name.len() + value.len() > COOKIE_MAX_BYTES
    {
        return None;
    }
    let mut cookie = Cookie {
        name: name.to_string(),
        value: value.trim().to_string(),
        domain: host.clone(),
        host_only: true,
        path: default_path(url),
        secure: false,
    };
    // Max-Age wins over Expires when both are present (RFC 6265 §5.3).
    let (mut max_age, mut expired) = (None, false);
    for attr in parts {
        let (key, val) = match attr.split_once('=') {
            Some((k, v)) => (k.trim(), v.trim()),
            None => (attr.trim(), ""),
        };
        match key.to_ascii_lowercase().as_str() {
            "domain" => {
                let domain = val.trim_start_matches('.').to_ascii_lowercase();
                if domain.is_empty() {
                    continue;
                }
                // An IP host takes no Domain; a domain must cover the host
                // and be more than a bare TLD (no public-suffix list here).
                if host_is_ip || !domain.contains('.') || !domain_matches(&host, &domain) {
                    return None;
                }
                cookie.domain = domain;
                cookie.host_only = false;
            }
            "path" if val.starts_with('/') => cookie.path = val.to_string(),
            "secure" => cookie.secure = true,
            "max-age" => {
                if let Ok(n) = val.parse::<i64>() {
                    max_age = Some(n);
                }
            }
            "expires" => {
                if cookie_date(val).is_some_and(|t| t <= chrono::Utc::now()) {
                    expired = true;
                }
            }
            _ => {}
        }
    }
    let delete = max_age.map_or(expired, |n| n <= 0);
    Some((cookie, delete))
}

/// RFC 6265 §5.1.1: a cookie date, tolerant of the formats in the wild
/// ("Thu, 01 Jan 1970 00:00:00 GMT", "Thu, 01-Jan-70 00:00:00 GMT"). The
/// tokens are found in any order; `None` when one is missing or off.
fn cookie_date(raw: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    const MONTHS: [&str; 12] = ["jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec"];
    let (mut time, mut day, mut month, mut year) = (None, None, None, None);
    for token in raw.split(|c: char| !(c.is_ascii_alphanumeric() || c == ':')).filter(|t| !t.is_empty()) {
        if time.is_none() && token.contains(':') {
            let mut parts = token.split(':').map(|p| p.parse::<u32>().ok());
            if let (Some(Some(h)), Some(Some(m)), Some(Some(s)), None) = (parts.next(), parts.next(), parts.next(), parts.next()) {
                time = Some((h, m, s));
                continue;
            }
        }
        let digits = token.chars().take_while(|c| c.is_ascii_digit()).count();
        if day.is_none() && (1..=2).contains(&digits) && digits == token.len() {
            day = token.parse::<u32>().ok();
        } else if month.is_none() && token.len() >= 3 {
            month = MONTHS.iter().position(|m| token[..3].eq_ignore_ascii_case(m)).map(|i| i as u32 + 1);
        } else if year.is_none() && (2..=4).contains(&digits) && digits == token.len() {
            year = token.parse::<i32>().ok().map(|y| match y {
                0..=69 => y + 2000,
                70..=99 => y + 1900,
                y => y,
            });
        }
    }
    let ((h, m, s), day, month, year) = (time?, day?, month?, year?);
    chrono::NaiveDate::from_ymd_opt(year, month, day)?
        .and_hms_opt(h, m, s)
        .map(|t| t.and_utc())
}

/// A plain HTTP/1.1 server on a loopback port for the tests here and the
/// wasm e2e tests in `runtime.rs`: records every request, answers from a
/// handler, closes the connection after each exchange.
#[cfg(test)]
pub(crate) mod test_server {
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::sync::{Arc, Mutex};

    #[derive(Clone, Debug)]
    pub struct Recorded {
        pub method: String,
        pub path: String,
        pub headers: Vec<(String, String)>,
        pub body: Vec<u8>,
    }

    impl Recorded {
        pub fn header(&self, name: &str) -> Option<&str> {
            self.headers.iter().find(|(n, _)| n.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
        }
    }

    pub struct Reply {
        pub status: u16,
        pub headers: Vec<(String, String)>,
        pub body: Vec<u8>,
        /// No Content-Length: the body runs until the connection closes.
        pub unsized_body: bool,
        /// Pause between body bytes — a server that drips.
        pub drip: std::time::Duration,
    }

    impl Reply {
        pub fn ok(body: &str) -> Reply {
            Reply {
                status: 200,
                headers: vec![],
                body: body.as_bytes().to_vec(),
                unsized_body: false,
                drip: std::time::Duration::ZERO,
            }
        }
        pub fn redirect(status: u16, to: &str) -> Reply {
            Reply::ok("").status(status).header("Location", to)
        }
        pub fn status(mut self, status: u16) -> Reply {
            self.status = status;
            self
        }
        pub fn header(mut self, name: &str, value: &str) -> Reply {
            self.headers.push((name.to_string(), value.to_string()));
            self
        }
    }

    pub struct Server {
        pub port: u16,
        requests: Arc<Mutex<Vec<Recorded>>>,
    }

    impl Server {
        /// Listens on 127.0.0.1 and, where the port is free there too, on
        /// [::1] — so `localhost` reaches it whichever address a resolver
        /// hands out first (ubuntu runners resolve it to ::1 first).
        pub fn start(handler: impl Fn(&Recorded) -> Reply + Send + Sync + 'static) -> Server {
            let v4 = TcpListener::bind("127.0.0.1:0").unwrap();
            let port = v4.local_addr().unwrap().port();
            let requests: Arc<Mutex<Vec<Recorded>>> = Arc::default();
            let handler = Arc::new(handler);
            let listeners = std::iter::once(v4).chain(TcpListener::bind(("::1", port)).ok());
            for listener in listeners {
                let (log, handler) = (requests.clone(), handler.clone());
                std::thread::spawn(move || {
                    for stream in listener.incoming().flatten() {
                        if let Some(request) = read_request(&stream) {
                            let reply = handler(&request);
                            log.lock().unwrap().push(request);
                            write_reply(stream, reply);
                        }
                    }
                });
            }
            Server { port, requests }
        }

        pub fn url(&self, path: &str) -> String {
            format!("http://127.0.0.1:{}{path}", self.port)
        }

        pub fn requests(&self) -> Vec<Recorded> {
            self.requests.lock().unwrap().clone()
        }
    }

    fn read_request(stream: &TcpStream) -> Option<Recorded> {
        let mut reader = BufReader::new(stream.try_clone().ok()?);
        let mut line = String::new();
        reader.read_line(&mut line).ok()?;
        let mut parts = line.split_whitespace();
        let method = parts.next()?.to_string();
        let path = parts.next()?.to_string();
        let mut headers = Vec::new();
        loop {
            let mut line = String::new();
            reader.read_line(&mut line).ok()?;
            let line = line.trim_end();
            if line.is_empty() {
                break;
            }
            let (name, value) = line.split_once(':')?;
            headers.push((name.trim().to_string(), value.trim().to_string()));
        }
        let length: usize = headers
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case("content-length"))
            .and_then(|(_, v)| v.parse().ok())
            .unwrap_or(0);
        let mut body = vec![0; length];
        reader.read_exact(&mut body).ok()?;
        Some(Recorded { method, path, headers, body })
    }

    fn write_reply(mut stream: TcpStream, reply: Reply) {
        let mut head = format!("HTTP/1.1 {} X\r\nConnection: close\r\n", reply.status);
        if !reply.unsized_body {
            head.push_str(&format!("Content-Length: {}\r\n", reply.body.len()));
        }
        for (name, value) in &reply.headers {
            head.push_str(&format!("{name}: {value}\r\n"));
        }
        head.push_str("\r\n");
        let _ = stream.write_all(head.as_bytes());
        if reply.drip.is_zero() {
            let _ = stream.write_all(&reply.body);
        } else {
            for byte in &reply.body {
                if stream.write_all(std::slice::from_ref(byte)).and_then(|_| stream.flush()).is_err() {
                    break;
                }
                std::thread::sleep(reply.drip);
            }
        }
        let _ = stream.flush();
        // Only the write half: closing the read half with bytes still
        // unread would RST the connection and truncate the reply on Linux.
        let _ = stream.shutdown(std::net::Shutdown::Write);
    }
}

#[cfg(test)]
mod tests {
    use super::test_server::{Reply, Server};
    use super::*;
    use serde_json::json;

    fn url(s: &str) -> Url {
        Url::parse(s).unwrap()
    }

    fn hosts(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    /// A state the tests' plain-HTTP loopback server is acceptable to.
    fn local() -> NetState {
        NetState { plain_loopback: true, ..NetState::default() }
    }

    fn get(state: &mut NetState, allowed: &[String], url: &str) -> Result<Vec<u8>, String> {
        fetch(state, allowed, &json!({ "url": url }).to_string(), b"")
    }

    // ---- the jar

    #[test]
    fn jar_keeps_every_cookie_and_sends_them_back_longest_path_first() {
        let mut jar = CookieJar::default();
        let from = url("https://sso.garmin.com/sso/signin?x=1");
        jar.store(&from, "SESSIONID=abc; Path=/; Secure; HttpOnly");
        jar.store(&from, "__cflb=xyz; SameSite=None; Secure; Path=/");
        jar.store(&from, "CASTGC=TGT-1; Path=/sso; Secure; HttpOnly");
        jar.store(&from, "GARMIN-SSO=1; Domain=garmin.com; Path=/");
        assert_eq!(jar.len(), 4);
        assert_eq!(
            jar.header_for(&url("https://sso.garmin.com/sso/verify")).unwrap(),
            "CASTGC=TGT-1; SESSIONID=abc; __cflb=xyz; GARMIN-SSO=1"
        );
        // Only the domain cookie travels to a sibling host.
        assert_eq!(jar.header_for(&url("https://connect.garmin.com/modern/")).unwrap(), "GARMIN-SSO=1");
        // Nothing for a stranger, nothing outside the path.
        assert!(jar.header_for(&url("https://example.com/")).is_none());
        assert!(jar.header_for(&url("https://sso.garmin.com/other")).unwrap().starts_with("SESSIONID"));
        assert!(!jar.header_for(&url("https://sso.garmin.com/other")).unwrap().contains("CASTGC"));
        // A secure cookie stays off a plain request.
        assert_eq!(jar.header_for(&url("http://sso.garmin.com/sso/")).unwrap(), "GARMIN-SSO=1");
    }

    #[test]
    fn jar_replaces_by_name_domain_path_and_deletes_on_max_age_zero() {
        let mut jar = CookieJar::default();
        let from = url("https://a.example.com/x/y");
        jar.store(&from, "s=1");
        jar.store(&from, "s=2");
        assert_eq!(jar.header_for(&url("https://a.example.com/x/z")).unwrap(), "s=2");
        // Default path is the request's directory: not sent to the root.
        assert!(jar.header_for(&url("https://a.example.com/")).is_none());
        jar.store(&from, "s=; Max-Age=0");
        assert!(jar.is_empty());
        // Max-Age wins over Expires: a positive one keeps the cookie.
        jar.store(&from, "t=1; Max-Age=3600; Expires=Thu, 01 Jan 1970 00:00:00 GMT");
        assert_eq!(jar.len(), 1);
        jar.store(&from, "t=1; Max-Age=0; Expires=Fri, 31 Dec 2999 00:00:00 GMT");
        assert!(jar.is_empty());
    }

    #[test]
    fn jar_ignores_cookies_for_domains_the_responder_does_not_own() {
        let mut jar = CookieJar::default();
        let from = url("https://sso.garmin.com/sso/signin");
        jar.store(&from, "evil=1; Domain=example.com");
        jar.store(&from, "tld=1; Domain=com");
        jar.store(&from, "sub=1; Domain=deep.sso.garmin.com");
        jar.store(&from, "=novalue");
        jar.store(&from, "junk");
        jar.store(&from, "bad name=1");
        assert!(jar.is_empty(), "{jar:?}");
        // A Domain equal to the host is fine; a leading dot is tolerated;
        // an empty one is as if absent.
        jar.store(&from, "ok=1; Domain=.sso.garmin.com; Path=/");
        jar.store(&from, "hostonly=1; Domain=; Path=/");
        assert_eq!(jar.header_for(&url("https://sso.garmin.com/")).unwrap(), "ok=1; hostonly=1");
        jar.store(&from, "hostonly=; Max-Age=0; Path=/");
        assert_eq!(jar.header_for(&url("https://deep.sso.garmin.com/")).unwrap(), "ok=1");
        // An IP host takes no Domain attribute at all.
        jar.store(&url("http://127.0.0.1:8/"), "ip=1; Domain=127.0.0.1");
        assert_eq!(jar.len(), 1);
        jar.store(&url("http://127.0.0.1:8/"), "ip=1");
        assert_eq!(jar.header_for(&url("http://127.0.0.1:9/")).unwrap(), "ip=1");
    }

    #[test]
    fn path_and_domain_matching_follow_rfc_6265() {
        assert!(path_matches("/", "/"));
        assert!(path_matches("/", "/a"));
        assert!(path_matches("/a", "/a"));
        assert!(path_matches("/a", "/a/b"));
        assert!(!path_matches("/a", "/ab"));
        assert!(path_matches("/a/", "/a/b"));
        assert!(!path_matches("/a/b", "/a"));
        assert!(domain_matches("sso.garmin.com", "garmin.com"));
        assert!(domain_matches("garmin.com", "garmin.com"));
        assert!(!domain_matches("notgarmin.com", "garmin.com"));
        assert_eq!(default_path(&url("https://h/")), "/");
        assert_eq!(default_path(&url("https://h")), "/");
        assert_eq!(default_path(&url("https://h/a")), "/");
        assert_eq!(default_path(&url("https://h/a/b?c")), "/a");
    }

    // ---- the hop rule and request validation

    #[test]
    fn hop_rule_wants_https_and_a_declared_host() {
        let allowed = hosts(&["api.example.com", "127.0.0.1"]);
        assert!(check_hop(&url("https://api.example.com/x"), &allowed, false).is_ok());
        assert!(check_hop(&url("https://API.example.com:443/x"), &allowed, false).is_ok());
        let err = check_hop(&url("https://api.example.com:8443/x"), &allowed, false).unwrap_err();
        assert!(err.contains("only the default port"), "{err}");
        let err = check_hop(&url("https://other.example.com/"), &allowed, false).unwrap_err();
        assert!(err.contains("not declared (net:host=other.example.com)"), "{err}");
        let err = check_hop(&url("http://api.example.com/"), &allowed, false).unwrap_err();
        assert!(err.contains("only https://"), "{err}");
        // Loopback over plain HTTP only when the test flag says so.
        assert!(check_hop(&url("http://127.0.0.1:1/"), &allowed, false).is_err());
        assert!(check_hop(&url("http://127.0.0.1:1/"), &allowed, true).is_ok());
        assert!(check_hop(&url("http://[::1]:1/"), &hosts(&["[::1]"]), true).is_ok());
        assert!(check_hop(&url("http://localhost:1/"), &hosts(&["localhost"]), true).is_ok());
        assert!(check_hop(&url("http://10.0.0.1/"), &hosts(&["10.0.0.1"]), true).is_err());
        assert!(check_hop(&url("ftp://api.example.com/"), &allowed, true).is_err());
        assert!(check_hop(&url("https://api.example.com/"), &[], false).is_err());
        assert!(check_hop(&url("file:///etc/hosts"), &allowed, true).unwrap_err().contains("has no host"));
    }

    #[test]
    fn requests_are_validated_before_anything_is_sent() {
        let server = Server::start(|_| Reply::ok("hi"));
        let allowed = hosts(&["127.0.0.1"]);
        let mut state = local();
        let base = server.url("/");
        let try_ = |state: &mut NetState, req: serde_json::Value, body: &[u8]| fetch(state, &allowed, &req.to_string(), body).unwrap_err();

        assert!(try_(&mut state, json!({ "url": "not a url" }), b"").contains("bad URL"));
        let err = try_(&mut state, json!({ "url": format!("http://u:p@127.0.0.1:{}/?t=1", server.port) }), b"");
        assert!(err.contains("credentials") && !err.contains("u:p") && !err.contains("t=1"), "{err}");
        assert!(try_(&mut state, json!({ "url": base, "method": "TRACE" }), b"").contains("unsupported method"));
        assert!(try_(&mut state, json!({ "url": base, "headers": [["Host", "x"]] }), b"").contains("set by the host"));
        assert!(try_(&mut state, json!({ "url": base, "headers": [["bad header", "x"]] }), b"").contains("bad header name"));
        assert!(try_(&mut state, json!({ "url": base, "headers": [["X-A", "line\nbreak"]] }), b"").contains("bad value"));
        assert!(try_(&mut state, json!({ "url": base }), b"body").contains("cannot carry a body"));
        let big = vec![b'x'; HTTP_MAX_REQUEST_BYTES + 1];
        assert!(try_(&mut state, json!({ "url": base, "method": "POST" }), &big).contains("larger than the 1 MiB limit"));
        assert!(fetch(&mut state, &allowed, "{", b"").unwrap_err().contains("bad request JSON"));
        // A spent budget refuses before connecting.
        let mut spent = local();
        spent.deadline = Some(Instant::now() - Duration::from_millis(1));
        assert!(get(&mut spent, &allowed, &base).unwrap_err().contains("budget is spent"));
        assert!(server.requests().is_empty(), "nothing may reach the server");
        assert!(state.last.is_none());
    }

    // ---- through the wire

    #[test]
    fn repeated_set_cookie_headers_survive_and_come_back_on_the_next_request() {
        let server = Server::start(|req| match req.path.as_str() {
            "/login" => Reply::ok("in")
                .header("Set-Cookie", "SESSIONID=abc; Path=/; HttpOnly")
                .header("Set-Cookie", "CASTGC=TGT-1; Path=/sso")
                .header("Set-Cookie", "__cflb=xyz; Path=/")
                .header("X-Twice", "1")
                .header("X-Twice", "2"),
            _ => Reply::ok("home"),
        });
        let allowed = hosts(&["127.0.0.1"]);
        let mut state = local();

        let body = get(&mut state, &allowed, &server.url("/login")).unwrap();
        assert_eq!(body, b"in");
        let meta = state.last.clone().unwrap();
        assert_eq!(meta.status, 200);
        assert_eq!(meta.url, server.url("/login"));
        let set: Vec<&str> = meta.headers.iter().filter(|(n, _)| n == "set-cookie").map(|(_, v)| v.as_str()).collect();
        assert_eq!(set.len(), 3, "{:?}", meta.headers);
        assert_eq!(meta.headers.iter().filter(|(n, _)| n == "x-twice").count(), 2);
        assert_eq!(state.jar.len(), 3);

        get(&mut state, &allowed, &server.url("/sso/verify")).unwrap();
        get(&mut state, &allowed, &server.url("/home")).unwrap();
        let sent = server.requests();
        assert_eq!(sent[0].header("cookie"), None, "the jar starts empty");
        assert_eq!(sent[1].header("cookie"), Some("CASTGC=TGT-1; SESSIONID=abc; __cflb=xyz"));
        assert_eq!(sent[2].header("cookie"), Some("SESSIONID=abc; __cflb=xyz"));
        // A fresh state is a fresh jar.
        assert!(local().jar.is_empty());
    }

    #[test]
    fn a_cookie_set_on_a_redirect_hop_reaches_the_final_request() {
        let server = Server::start(|req| match req.path.as_str() {
            "/login" => Reply::redirect(302, "/ticket").header("Set-Cookie", "hop=1; Path=/"),
            "/ticket" => Reply::redirect(302, "/home?ticket=ST-9").header("Set-Cookie", "hop=2; Path=/"),
            _ => Reply::ok("home"),
        });
        let allowed = hosts(&["127.0.0.1"]);
        let mut state = local();
        let body = get(&mut state, &allowed, &server.url("/login")).unwrap();
        assert_eq!(body, b"home");
        let sent = server.requests();
        assert_eq!(sent.len(), 3);
        assert_eq!(sent[1].header("cookie"), Some("hop=1"));
        assert_eq!(sent[2].header("cookie"), Some("hop=2"));
        assert_eq!(sent[2].path, "/home?ticket=ST-9");
        let meta = state.last.unwrap();
        assert_eq!(meta.url, server.url("/home?ticket=ST-9"), "the final URL, where the ticket is");
        assert_eq!(meta.status, 200);
        assert_eq!(
            meta.hops,
            [
                Hop { status: 302, url: server.url("/login"), location: "/ticket".to_string() },
                Hop { status: 302, url: server.url("/ticket"), location: "/home?ticket=ST-9".to_string() },
            ]
        );
    }

    #[test]
    fn a_plugin_may_stop_at_a_redirect_and_read_its_location() {
        let server = Server::start(|req| match req.path.as_str() {
            "/login" => Reply::redirect(302, "https://undeclared.example.com/cb?ticket=ST-9").header("Set-Cookie", "s=1; Path=/"),
            "/two" => Reply::redirect(302, "/three"),
            "/three" => Reply::redirect(302, "/four"),
            _ => Reply::ok("four"),
        });
        let allowed = hosts(&["127.0.0.1"]);
        let mut state = local();
        // Not following: the 302 is the answer, Location and all — the
        // hop that would have been refused is never checked.
        let req = json!({ "url": server.url("/login"), "max_redirects": 0 }).to_string();
        fetch(&mut state, &allowed, &req, b"").unwrap();
        let meta = state.last.clone().unwrap();
        assert_eq!(meta.status, 302);
        assert!(meta.hops.is_empty());
        assert!(meta.headers.contains(&("location".to_string(), "https://undeclared.example.com/cb?ticket=ST-9".to_string())));
        assert_eq!(state.jar.len(), 1, "its cookies still land");
        // A limit in the middle of a chain: the last allowed hop's answer.
        let req = json!({ "url": server.url("/two"), "max_redirects": 1 }).to_string();
        fetch(&mut state, &allowed, &req, b"").unwrap();
        let meta = state.last.clone().unwrap();
        assert_eq!((meta.status, meta.url.as_str(), meta.hops.len()), (302, server.url("/three").as_str(), 1));
        // More than the host's cap is the host's cap.
        let req = json!({ "url": server.url("/two"), "max_redirects": 99 }).to_string();
        fetch(&mut state, &allowed, &req, b"").unwrap();
        assert_eq!(state.last.clone().unwrap().status, 200);
    }

    #[test]
    fn a_redirect_to_an_undeclared_host_is_refused() {
        let server = Server::start(|_| Reply::redirect(302, "https://undeclared.example.com/steal"));
        let allowed = hosts(&["127.0.0.1"]);
        let mut state = local();
        let err = get(&mut state, &allowed, &server.url("/go?ticket=ST-9")).unwrap_err();
        assert!(err.contains("undeclared.example.com") && err.contains("not declared"), "{err}");
        assert!(!err.contains("ST-9"), "no query of either side: {err}");
        assert_eq!(server.requests().len(), 1);
        assert!(state.last.is_none());
        // Downgrading to plain HTTP on a hop is refused too.
        let server = Server::start(|_| Reply::redirect(302, "http://api.example.com/"));
        let err = get(&mut local(), &hosts(&["127.0.0.1", "api.example.com"]), &server.url("/go")).unwrap_err();
        assert!(err.contains("only https://"), "{err}");
        // A Location that is not a URL, or not even text.
        let server = Server::start(|req| match req.path.as_str() {
            "/junk" => Reply::redirect(302, "http://["),
            _ => Reply::redirect(302, "/caf\u{e9}"),
        });
        let err = get(&mut local(), &allowed, &server.url("/junk")).unwrap_err();
        assert!(err.contains("bad redirect Location"), "{err}");
        let err = get(&mut local(), &allowed, &server.url("/bytes")).unwrap_err();
        assert!(err.contains("not valid text"), "{err}");
    }

    #[test]
    fn plain_http_is_refused_unless_the_test_flag_allows_loopback() {
        let server = Server::start(|_| Reply::ok("x"));
        let err = get(&mut NetState::default(), &hosts(&["127.0.0.1"]), &server.url("/")).unwrap_err();
        assert!(err.contains("only https://"), "{err}");
        assert!(server.requests().is_empty());
    }

    #[test]
    fn crossing_hosts_drops_authorization_and_the_plugins_own_cookie() {
        // 127.0.0.1 and localhost are two host names for one server.
        let server = Server::start(|req| match req.path.as_str() {
            "/a" => Reply::redirect(307, &format!("http://localhost:{}/b", req.header("host").unwrap().rsplit(':').next().unwrap())),
            "/same" => Reply::redirect(307, "/b"),
            _ => Reply::ok("b"),
        });
        let allowed = hosts(&["127.0.0.1", "localhost"]);
        let req = |path: &str| {
            json!({
                "url": server.url(path),
                "method": "POST",
                "headers": [["Authorization", "Bearer t"], ["Cookie", "mine=1"], ["X-Keep", "yes"]],
            })
            .to_string()
        };
        let mut state = local();
        fetch(&mut state, &allowed, &req("/a"), b"payload").unwrap();
        fetch(&mut state, &allowed, &req("/same"), b"payload").unwrap();
        let sent = server.requests();
        assert_eq!(sent.len(), 4);
        assert_eq!(sent[0].header("authorization"), Some("Bearer t"));
        assert_eq!(sent[0].header("cookie"), Some("mine=1"));
        // Across hosts: both gone, the rest — and the 307's body — kept.
        assert_eq!(sent[1].header("authorization"), None);
        assert_eq!(sent[1].header("cookie"), None);
        assert_eq!(sent[1].header("x-keep"), Some("yes"));
        assert_eq!((sent[1].method.as_str(), sent[1].body.as_slice()), ("POST", &b"payload"[..]));
        // Same host: kept.
        assert_eq!(sent[3].header("authorization"), Some("Bearer t"));
        assert_eq!(sent[3].header("cookie"), Some("mine=1"));
    }

    #[test]
    fn a_cross_origin_hop_on_the_same_host_drops_authorization_but_keeps_cookies() {
        // Two servers on 127.0.0.1: the first redirects to the second's port.
        let target = Server::start(|_| Reply::ok("b"));
        let port = target.port;
        let source = Server::start(move |_| Reply::redirect(307, &format!("http://127.0.0.1:{port}/b")));
        let allowed = hosts(&["127.0.0.1"]);
        let req = json!({ "url": source.url("/a"), "headers": [["Authorization", "Bearer t"], ["Cookie", "mine=1"]] }).to_string();
        fetch(&mut local(), &allowed, &req, b"").unwrap();
        let landed = target.requests();
        assert_eq!(landed.len(), 1);
        assert_eq!(landed[0].header("authorization"), None);
        assert_eq!(landed[0].header("cookie"), Some("mine=1"));
    }

    #[test]
    fn replacing_a_cookie_keeps_its_place_in_the_order() {
        let mut jar = CookieJar::default();
        let from = url("https://a.example.com/");
        jar.store(&from, "first=1; Path=/");
        jar.store(&from, "second=1; Path=/");
        jar.store(&from, "first=2; Path=/");
        assert_eq!(jar.header_for(&from).unwrap(), "first=2; second=1");
    }

    #[test]
    fn see_other_and_found_after_post_turn_into_a_get_without_the_body() {
        let server = Server::start(|req| match req.path.as_str() {
            "/post303" => Reply::redirect(303, "/done"),
            "/post302" => Reply::redirect(302, "/done"),
            "/get302" => Reply::redirect(302, "/done"),
            _ => Reply::ok("done"),
        });
        let allowed = hosts(&["127.0.0.1"]);
        let mut state = local();
        let post = |path: &str| json!({ "url": server.url(path), "method": "POST", "headers": [["Content-Type", "text/plain"]] }).to_string();
        fetch(&mut state, &allowed, &post("/post303"), b"form").unwrap();
        fetch(&mut state, &allowed, &post("/post302"), b"form").unwrap();
        get(&mut state, &allowed, &server.url("/get302")).unwrap();
        let sent = server.requests();
        assert_eq!(sent.len(), 6);
        for i in [1, 3, 5] {
            assert_eq!(sent[i].method, "GET", "hop {i}");
            assert!(sent[i].body.is_empty());
            assert_eq!(sent[i].header("content-type"), None);
        }
        assert_eq!(sent[0].header("content-type"), Some("text/plain"));
        // The jar and every Cookie header the plugin set ride as one header.
        let mut state = local();
        state.jar.store(&Url::parse(&server.url("/")).unwrap(), "j=1; Path=/");
        let req = json!({ "url": server.url("/done"), "headers": [["Cookie", "mine=2"], ["Cookie", ""], ["cookie", "more=3"]] });
        fetch(&mut state, &allowed, &req.to_string(), b"").unwrap();
        assert_eq!(server.requests()[6].header("cookie"), Some("j=1; mine=2; more=3"));
    }

    #[test]
    fn jar_honours_expires_and_stays_bounded() {
        let mut jar = CookieJar::default();
        let from = url("https://a.example.com/");
        jar.store(&from, "s=1; Path=/");
        jar.store(&from, "s=; Expires=Thu, 01 Jan 1970 00:00:00 GMT; Path=/");
        assert!(jar.is_empty(), "an Expires in the past deletes");
        jar.store(&from, "s=1; Expires=Thu, 01-Jan-70 00:00:00 GMT");
        assert!(jar.is_empty(), "the two-digit form too");
        jar.store(&from, "s=2; Expires=Fri, 31 Dec 2999 23:59:59 GMT");
        jar.store(&from, "t=3; Expires=garbage");
        assert_eq!(jar.len(), 2, "a future or unreadable Expires keeps the cookie");
        assert_eq!(cookie_date("Sun, 06 Nov 1994 08:49:37 GMT").unwrap().to_rfc3339(), "1994-11-06T08:49:37+00:00");
        assert_eq!(cookie_date("06-Nov-94 08:49:37").unwrap().to_rfc3339(), "1994-11-06T08:49:37+00:00");
        assert_eq!(cookie_date("Wed, 05 Nov 2025 8:49:37 GMT").map(|d| d.to_rfc3339()), Some("2025-11-05T08:49:37+00:00".to_string()));
        assert!(cookie_date("Sun, 06 Nov 1994").is_none());
        assert!(cookie_date("Sun, 06 Nov 1994 08:49").is_none(), "a clock without seconds is no time");
        assert_eq!(cookie_date("06 Nov 24 08:49:37").unwrap().to_rfc3339(), "2024-11-06T08:49:37+00:00");
        assert!(cookie_date("31 Feb 2020 00:00:00").is_none());
        assert!(cookie_date("06 Nov 1994 25:00:00").is_none());
        // Oversized cookies are ignored; a full jar drops its oldest.
        let mut jar = CookieJar::default();
        jar.store(&from, &format!("big={}", "x".repeat(COOKIE_MAX_BYTES)));
        assert!(jar.is_empty());
        for i in 0..JAR_MAX_COOKIES + 5 {
            jar.store(&from, &format!("c{i}=1; Path=/"));
        }
        assert_eq!(jar.len(), JAR_MAX_COOKIES);
        let sent = jar.header_for(&from).unwrap();
        assert!(!sent.contains("c0=") && !sent.contains("c4=") && sent.contains("c5=") && sent.ends_with(&format!("c{}=1", JAR_MAX_COOKIES + 4)));
    }

    #[test]
    fn oversized_responses_and_endless_redirects_are_refused() {
        let server = Server::start(|req| match req.path.as_str() {
            "/big" => Reply { body: vec![b'x'; HTTP_MAX_RESPONSE_BYTES + 1], ..Reply::ok("") },
            "/big-unsized" => Reply { body: vec![b'x'; HTTP_MAX_RESPONSE_BYTES + 1], unsized_body: true, ..Reply::ok("") },
            "/fits-unsized" => Reply { body: b"fits".to_vec(), unsized_body: true, ..Reply::ok("") },
            _ => Reply::redirect(302, "/loop?again"),
        });
        let allowed = hosts(&["127.0.0.1"]);
        let mut state = local();
        let err = get(&mut state, &allowed, &server.url("/big")).unwrap_err();
        assert!(err.contains("larger than the 5 MiB limit"), "{err}");
        let err = get(&mut state, &allowed, &server.url("/big-unsized")).unwrap_err();
        assert!(err.contains("larger than the 5 MiB limit"), "{err}");
        assert_eq!(get(&mut state, &allowed, &server.url("/fits-unsized")).unwrap(), b"fits");
        // An endless chain ends where the cap is, as a redirect the plugin
        // sees for what it is.
        get(&mut state, &allowed, &server.url("/loop")).unwrap();
        let meta = state.last.clone().unwrap();
        assert_eq!((meta.status, meta.hops.len()), (302, HTTP_MAX_REDIRECTS));
        assert_eq!(server.requests().iter().filter(|r| r.path.starts_with("/loop")).count(), HTTP_MAX_REDIRECTS + 1);
    }

    #[test]
    fn a_hop_never_outlives_the_invocation_budget() {
        let server = Server::start(|req| {
            if req.path == "/slow" {
                std::thread::sleep(Duration::from_millis(2000));
            }
            Reply::ok("late")
        });
        let mut state = local();
        // The first request of an invocation builds the client (the system
        // root store, ~300 ms on macOS); warm it before the budget starts.
        get(&mut state, &hosts(&["127.0.0.1"]), &server.url("/warm")).unwrap();
        state.deadline = Some(Instant::now() + Duration::from_millis(1000));
        let started = Instant::now();
        let err = get(&mut state, &hosts(&["127.0.0.1"]), &server.url("/slow")).unwrap_err();
        assert!(err.contains("timed out"), "{err}");
        assert!(started.elapsed() < Duration::from_millis(1400), "{:?}", started.elapsed());
        assert!(state.last.is_none());
    }

    /// The budget bounds the whole call — every hop, and the body bytes
    /// of the last one. The body part rests on reqwest wrapping the body
    /// read in its request timeout; a change there would show up here.
    #[test]
    fn neither_a_redirect_chain_nor_a_dripping_body_outlives_the_budget() {
        let server = Server::start(|req| match req.path.as_str() {
            "/warm" => Reply::ok(""),
            "/drip" => Reply { body: vec![b'x'; 50], drip: Duration::from_millis(100), ..Reply::ok("") },
            _ => {
                std::thread::sleep(Duration::from_millis(300));
                Reply::redirect(302, "/again")
            }
        });
        let allowed = hosts(&["127.0.0.1"]);
        for path in ["/chain", "/drip"] {
            let mut state = local();
            get(&mut state, &allowed, &server.url("/warm")).unwrap();
            state.deadline = Some(Instant::now() + Duration::from_millis(1000));
            let started = Instant::now();
            let err = get(&mut state, &allowed, &server.url(path)).unwrap_err();
            assert!(err.contains("timed out") || err.contains("budget is spent"), "{path}: {err}");
            assert!(started.elapsed() < Duration::from_millis(2000), "{path}: {:?}", started.elapsed());
        }
        assert!(server.requests().iter().filter(|r| r.path == "/again").count() < HTTP_MAX_REDIRECTS);
    }

    #[test]
    fn a_connection_failure_names_the_url_and_the_cause() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        let err = get(&mut local(), &hosts(&["127.0.0.1"]), &format!("http://127.0.0.1:{port}/x?ticket=ST-9")).unwrap_err();
        assert!(err.starts_with(&format!("request to http://127.0.0.1:{port}/x failed:")), "{err}");
        assert!(!err.contains("ST-9"));
        assert!(!err.contains("error sending request"), "innermost cause only: {err}");
    }

    #[test]
    fn redirect_kinds() {
        assert_eq!(redirect_kind(StatusCode::TEMPORARY_REDIRECT, &Method::POST), Some(Redirect::Same));
        assert_eq!(redirect_kind(StatusCode::PERMANENT_REDIRECT, &Method::GET), Some(Redirect::Same));
        assert_eq!(redirect_kind(StatusCode::SEE_OTHER, &Method::GET), Some(Redirect::AsGet));
        assert_eq!(redirect_kind(StatusCode::FOUND, &Method::POST), Some(Redirect::AsGet));
        assert_eq!(redirect_kind(StatusCode::FOUND, &Method::PUT), Some(Redirect::Same));
        assert_eq!(redirect_kind(StatusCode::MOVED_PERMANENTLY, &Method::GET), Some(Redirect::Same));
        assert_eq!(redirect_kind(StatusCode::NOT_MODIFIED, &Method::GET), None);
        assert_eq!(redirect_kind(StatusCode::OK, &Method::GET), None);
    }
}
