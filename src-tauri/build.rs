// The app's own key rules (parsing, the hash), so the build checks the key
// exactly the way the app will read it.
#[path = "src/trainer/ant/key_rules.rs"]
mod key_rules;

fn main() {
    // The ANT+ network key is given at build time, never kept in the
    // sources (only its hash, trainer/ant/key_rules.rs): a change of it is
    // a rebuild. A key that is not the key, or none in a release, stops the
    // build here instead of shipping one that asks every user for it.
    println!("cargo:rerun-if-env-changed={}", key_rules::KEY_VAR);
    println!("cargo:rerun-if-env-changed={}", key_rules::REQUIRED_VAR);
    if let Some(problem) = key_rules::build_problem(|name| std::env::var_os(name), key_rules::KEY_SHA256) {
        eprintln!("error: {problem}");
        std::process::exit(1);
    }
    tauri_build::build()
}
