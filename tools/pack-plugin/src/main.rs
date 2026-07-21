//! pack-plugin — sign and package a Syzify plugin into a `.syzify-ext`.
//!
//!   cargo run --manifest-path tools/pack-plugin/Cargo.toml -- <plugin-dir> [key.hex]
//!
//! Reads `<dir>/plugin.json` + the wasm named by its `entry`, embeds the
//! author's Ed25519 public key into the manifest, signs
//! `sha256(manifest) ++ sha256(wasm)`, and writes `<dir>/<id>.syzify-ext`
//! (zip of plugin.json + plugin.wasm + signature). The source `plugin.json` on
//! disk is left untouched. Without a key file, a new keypair is generated and
//! the secret saved to `<dir>/signing-key.hex` (keep it private; never commit).

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use ed25519_dalek::{Signer, SigningKey};
use rand::rngs::OsRng;
use sha2::{Digest, Sha256};
use zip::write::SimpleFileOptions;

fn main() {
    if let Err(e) = run() {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().collect();
    let dir = args
        .get(1)
        .ok_or("usage: pack-plugin <plugin-dir> [signing-key.hex]")?;
    let dir = Path::new(dir);

    let key_path: PathBuf = args
        .get(2)
        .map(PathBuf::from)
        .unwrap_or_else(|| dir.join("signing-key.hex"));
    let signing = load_or_create_key(&key_path)?;
    let public_key = hex::encode(signing.verifying_key().to_bytes());

    // Embed the public key into the manifest that goes into the package
    // (the on-disk plugin.json is not modified).
    let manifest_path = dir.join("plugin.json");
    let raw = fs::read_to_string(&manifest_path).map_err(|e| format!("read plugin.json: {e}"))?;
    let mut manifest: serde_json::Value =
        serde_json::from_str(&raw).map_err(|e| format!("parse plugin.json: {e}"))?;
    manifest["publicKey"] = serde_json::Value::String(public_key.clone());
    let manifest_json =
        serde_json::to_string_pretty(&manifest).map_err(|e| format!("serialize manifest: {e}"))?;

    let id = manifest["id"].as_str().ok_or("manifest missing id")?;
    let entry = manifest["entry"].as_str().unwrap_or("plugin.wasm");
    let wasm = fs::read(dir.join(entry)).map_err(|e| format!("read {entry}: {e}"))?;

    // Sign sha256(manifest) ++ sha256(wasm).
    let mut msg = [0u8; 64];
    msg[..32].copy_from_slice(&Sha256::digest(manifest_json.as_bytes()));
    msg[32..].copy_from_slice(&Sha256::digest(&wasm));
    let sig_hex = hex::encode(signing.sign(&msg).to_bytes());

    let out = dir.join(format!("{id}.syzify-ext"));
    write_zip(&out, &manifest_json, &wasm, &sig_hex)?;

    println!("Wrote {}", out.display());
    println!("publicKey: {public_key}");
    Ok(())
}

fn load_or_create_key(path: &Path) -> Result<SigningKey, String> {
    if path.exists() {
        let hex = fs::read_to_string(path).map_err(|e| format!("read key: {e}"))?;
        let bytes: [u8; 32] = hex::decode(hex.trim())
            .map_err(|e| format!("bad key hex: {e}"))?
            .try_into()
            .map_err(|_| "signing key must be 32 bytes".to_string())?;
        Ok(SigningKey::from_bytes(&bytes))
    } else {
        let sk = SigningKey::generate(&mut OsRng);
        fs::write(path, hex::encode(sk.to_bytes())).map_err(|e| format!("write key: {e}"))?;
        eprintln!("Generated signing key at {} — keep it secret, never commit it.", path.display());
        Ok(sk)
    }
}

fn write_zip(out: &Path, manifest_json: &str, wasm: &[u8], sig_hex: &str) -> Result<(), String> {
    let f = fs::File::create(out).map_err(|e| format!("create package: {e}"))?;
    let mut zip = zip::ZipWriter::new(f);
    let opts = SimpleFileOptions::default();
    let mut add = |name: &str, bytes: &[u8]| -> Result<(), String> {
        zip.start_file(name, opts).map_err(|e| format!("zip {name}: {e}"))?;
        zip.write_all(bytes).map_err(|e| format!("write {name}: {e}"))
    };
    add("plugin.json", manifest_json.as_bytes())?;
    add("plugin.wasm", wasm)?;
    add("signature", sig_hex.as_bytes())?;
    zip.finish().map_err(|e| format!("finish zip: {e}"))?;
    Ok(())
}
