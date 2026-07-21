import { openUrl } from "@tauri-apps/plugin-opener";

/** True when `href` resolves to an http(s) URL on a different origin than the
 * app itself — i.e. a link that must open in the system browser rather than
 * navigate (and hijack) the Tauri webview. Relative/in-app links, and
 * non-web schemes (mailto:, tile:, photo:, tauri:, blob:…) return false. */
export function isExternalHttpHref(href: string, appOrigin: string): boolean {
  let url: URL;
  try {
    url = new URL(href, appOrigin);
  } catch {
    return false;
  }
  if (url.protocol !== "http:" && url.protocol !== "https:") return false;
  return url.origin !== appOrigin;
}

/** Capture clicks on external links anywhere in the app (e.g. the Leaflet map
 * attribution) and open them in the system browser instead of letting the
 * webview navigate away from the app. */
export function initExternalLinks(): void {
  document.addEventListener(
    "click",
    (e) => {
      if (e.defaultPrevented || e.button !== 0) return;
      const anchor = (e.target as HTMLElement | null)?.closest?.("a");
      const href = anchor?.getAttribute("href");
      if (!anchor || !href) return;
      // Use the resolved absolute href so relative links are judged correctly.
      if (!isExternalHttpHref(anchor.href, window.location.origin)) return;
      e.preventDefault();
      openUrl(anchor.href).catch(() => {});
    },
    true,
  );
}
