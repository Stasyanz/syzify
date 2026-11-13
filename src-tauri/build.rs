fn main() {
    // The ANT+ network key is given at build time, never kept in the
    // sources (trainer/ant/key.rs): a change of it is a rebuild.
    println!("cargo:rerun-if-env-changed=SYZIFY_ANT_KEY");
    tauri_build::build()
}
