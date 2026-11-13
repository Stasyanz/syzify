// The app's own key rules (parsing, the hash), so the build checks the key
// exactly the way the app will read it.
#[path = "src/trainer/ant/key_rules.rs"]
mod key_rules;

/// The repository's `.env`, from the package root build scripts run in:
/// where a local build keeps the key (ignored by git).
const DOTENV: &str = "../.env";

fn stop(problem: &str) -> ! {
    eprintln!("error: {problem}");
    std::process::exit(1);
}

fn main() {
    // The ANT+ network key is given at build time, never kept in the
    // sources (only its hash, trainer/ant/key_rules.rs): a change of it is
    // a rebuild. A key that is not the key, or none in a release, stops the
    // build here instead of shipping one that asks every user for it.
    println!("cargo:rerun-if-env-changed={}", key_rules::KEY_VAR);
    println!("cargo:rerun-if-env-changed={}", key_rules::REQUIRED_VAR);
    // Watched while it is there (a missing file watched would rerun this,
    // and rebuild the app, on every build): a .env made later is read once
    // build.rs is touched.
    let read = std::fs::read(DOTENV);
    if !matches!(&read, Err(e) if e.kind() == std::io::ErrorKind::NotFound) {
        println!("cargo:rerun-if-changed={DOTENV}");
    }
    let dotenv = key_rules::dotenv_text(read).unwrap_or_else(|problem| stop(problem));
    let key = key_rules::build_key(|name| std::env::var_os(name), dotenv.as_deref(), key_rules::KEY_SHA256)
        .unwrap_or_else(|problem| stop(problem));
    tauri_build::build();
    // Last: a build script that fails prints everything it printed, and
    // this line holds the key. The environment has none: the app's
    // option_env! gets the file's.
    if let Some(key) = key {
        println!("cargo:rustc-env={}={key}", key_rules::KEY_VAR);
    }
}
