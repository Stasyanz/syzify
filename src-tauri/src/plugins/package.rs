//! `.syzify-ext` packages: a zip of `plugin.json` + `plugin.wasm` + `signature`.
//!
//! The signature is Ed25519 over `sha256(manifest) ++ sha256(wasm)` (64 bytes),
//! made by the author's key; the author's public key lives in the manifest
//! (`publicKey`). Verifying proves the package wasn't tampered with after signing
//! and binds it to that key. Trust in the author is established on first install
//! (TOFU): an upgrade must carry the same key (enforced by the install command).

use std::fs::File;
use std::io::Read;
use std::path::Path;

use ed25519_dalek::{Signature, VerifyingKey};
use sha2::{Digest, Sha256};
use zip::ZipArchive;

#[derive(Debug)]
pub struct Package {
    pub manifest_json: String,
    pub wasm: Vec<u8>,
    pub signature_hex: String,
}

/// The exact bytes a `.syzify-ext` signature covers.
fn signed_message(manifest_json: &str, wasm: &[u8]) -> [u8; 64] {
    let mut msg = [0u8; 64];
    msg[..32].copy_from_slice(&Sha256::digest(manifest_json.as_bytes()));
    msg[32..].copy_from_slice(&Sha256::digest(wasm));
    msg
}

// Per-entry decompression caps. Reading through `take(max + 1)` bounds memory
// even if a zip-bomb claims a huge uncompressed size, so a crafted package
// can't OOM the app at install time.
const MAX_MANIFEST: u64 = 256 * 1024; // 256 KiB
const MAX_WASM: u64 = 32 * 1024 * 1024; // 32 MiB
const MAX_SIGNATURE: u64 = 1024; // 1 KiB (hex of a 64-byte sig)

/// Read the manifest, wasm and signature out of a `.syzify-ext` archive.
pub fn open_package(path: &Path) -> Result<Package, String> {
    let file = File::open(path).map_err(|e| format!("Failed to open package: {e}"))?;
    let mut zip = ZipArchive::new(file).map_err(|e| format!("Not a valid package: {e}"))?;
    let manifest_bytes = read_capped(&mut zip, "plugin.json", MAX_MANIFEST)?;
    let manifest_json = String::from_utf8(manifest_bytes)
        .map_err(|_| "plugin.json is not valid UTF-8".to_string())?;
    let wasm = read_capped(&mut zip, "plugin.wasm", MAX_WASM)?;
    let signature_hex = String::from_utf8(read_capped(&mut zip, "signature", MAX_SIGNATURE)?)
        .map_err(|_| "signature is not valid UTF-8".to_string())?
        .trim()
        .to_string();
    Ok(Package { manifest_json, wasm, signature_hex })
}

/// Read a named zip entry, decompressing at most `max` bytes (zip-bomb guard via
/// the shared [`crate::util::read_capped`]).
fn read_capped<R: Read + std::io::Seek>(
    zip: &mut ZipArchive<R>,
    name: &str,
    max: u64,
) -> Result<Vec<u8>, String> {
    let mut entry = zip.by_name(name).map_err(|_| format!("package missing {name}"))?;
    crate::util::read_capped(&mut entry, max, name)
}

/// Verify a package's Ed25519 signature against the author public key.
pub fn verify(
    manifest_json: &str,
    wasm: &[u8],
    signature_hex: &str,
    public_key_hex: &str,
) -> Result<(), String> {
    let pk: [u8; 32] = hex::decode(public_key_hex)
        .map_err(|e| format!("bad public key hex: {e}"))?
        .try_into()
        .map_err(|_| "public key must be 32 bytes".to_string())?;
    let verifying_key =
        VerifyingKey::from_bytes(&pk).map_err(|e| format!("invalid public key: {e}"))?;

    let sig: [u8; 64] = hex::decode(signature_hex)
        .map_err(|e| format!("bad signature hex: {e}"))?
        .try_into()
        .map_err(|_| "signature must be 64 bytes".to_string())?;
    let signature = Signature::from_bytes(&sig);

    verifying_key
        .verify_strict(&signed_message(manifest_json, wasm), &signature)
        .map_err(|_| "signature verification failed".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};

    fn sign(manifest: &str, wasm: &[u8]) -> (String, String) {
        let sk = SigningKey::from_bytes(&[7u8; 32]);
        let sig = sk.sign(&signed_message(manifest, wasm));
        (hex::encode(sig.to_bytes()), hex::encode(sk.verifying_key().to_bytes()))
    }

    #[test]
    fn valid_signature_verifies() {
        let manifest = r#"{"id":"com.x","name":"X","version":"1.0.0"}"#;
        let wasm = b"\0asm fake module";
        let (sig, pk) = sign(manifest, wasm);
        assert!(verify(manifest, wasm, &sig, &pk).is_ok());
    }

    #[test]
    fn tampered_wasm_fails() {
        let manifest = r#"{"id":"com.x","name":"X","version":"1.0.0"}"#;
        let (sig, pk) = sign(manifest, b"original");
        assert!(verify(manifest, b"tampered", &sig, &pk).is_err());
    }

    #[test]
    fn tampered_manifest_fails() {
        let wasm = b"module";
        let (sig, pk) = sign(r#"{"id":"com.x"}"#, wasm);
        assert!(verify(r#"{"id":"com.evil"}"#, wasm, &sig, &pk).is_err());
    }

    #[test]
    fn wrong_key_fails() {
        let manifest = r#"{"id":"com.x"}"#;
        let wasm = b"module";
        let (sig, _) = sign(manifest, wasm);
        let other = hex::encode(SigningKey::from_bytes(&[9u8; 32]).verifying_key().to_bytes());
        assert!(verify(manifest, wasm, &sig, &other).is_err());
    }

    #[test]
    fn open_package_roundtrip() {
        use std::io::Write;
        use zip::write::SimpleFileOptions;

        let manifest = r#"{"id":"com.x","name":"X","version":"1.0.0","entry":"plugin.wasm"}"#;
        let wasm = b"\0asm reference";
        let (sig, pk) = sign(manifest, wasm);

        let dir = std::env::temp_dir().join(format!("syzify_pkg_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test.syzify-ext");
        {
            let f = File::create(&path).unwrap();
            let mut zip = zip::ZipWriter::new(f);
            let opts = SimpleFileOptions::default();
            zip.start_file("plugin.json", opts).unwrap();
            zip.write_all(manifest.as_bytes()).unwrap();
            zip.start_file("plugin.wasm", opts).unwrap();
            zip.write_all(wasm).unwrap();
            zip.start_file("signature", opts).unwrap();
            zip.write_all(sig.as_bytes()).unwrap();
            zip.finish().unwrap();
        }

        let pkg = open_package(&path).unwrap();
        assert_eq!(pkg.wasm, wasm);
        assert!(verify(&pkg.manifest_json, &pkg.wasm, &pkg.signature_hex, &pk).is_ok());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn oversized_entry_is_rejected() {
        use std::io::Write;
        use zip::write::SimpleFileOptions;

        // 300 KiB of 'x' — compresses to almost nothing in the zip, but would
        // decompress past MAX_MANIFEST; read_capped must reject it via take().
        let big = "x".repeat(MAX_MANIFEST as usize + 10);
        let dir = std::env::temp_dir().join(format!("syzify_pkg_big_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("big.syzify-ext");
        {
            let f = File::create(&path).unwrap();
            let mut zip = zip::ZipWriter::new(f);
            let opts = SimpleFileOptions::default();
            zip.start_file("plugin.json", opts).unwrap();
            zip.write_all(big.as_bytes()).unwrap();
            zip.start_file("plugin.wasm", opts).unwrap();
            zip.write_all(b"w").unwrap();
            zip.start_file("signature", opts).unwrap();
            zip.write_all(b"00").unwrap();
            zip.finish().unwrap();
        }

        let err = open_package(&path).unwrap_err();
        assert!(err.contains("exceeds"), "expected size-limit error, got: {err}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn committed_smart_route_package_verifies() {
        // Cross-checks the pack-plugin tool against this verifier: the committed
        // .syzify-ext must open and verify against the publicKey in its manifest.
        let path = Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../examples/plugins/smart-route/com.syzify.example.smart-route.syzify-ext"
        ));
        assert!(
            path.exists(),
            "missing committed {} — repackage with tools/pack-plugin",
            path.display()
        );
        let pkg = open_package(path).unwrap();
        let manifest: serde_json::Value = serde_json::from_str(&pkg.manifest_json).unwrap();
        let pk = manifest["publicKey"].as_str().expect("packaged manifest has publicKey");
        assert!(verify(&pkg.manifest_json, &pkg.wasm, &pkg.signature_hex, pk).is_ok());
    }
}
