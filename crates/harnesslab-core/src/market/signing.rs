//! Detached package signatures (HMAC-SHA256 + ed25519).

use std::collections::BTreeMap;

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use sha2::{Digest, Sha256};

use super::manifest::PackageManifest;

pub const ALGO_HMAC: &str = "hmac-sha256";
pub const ALGO_ED25519: &str = "ed25519";

pub fn content_digest(manifest: &PackageManifest) -> String {
    let payload = serde_json::json!({
        "name": manifest.name,
        "version": manifest.version,
        "integrity": manifest.integrity,
    });
    let bytes = serde_json::to_vec(&payload).unwrap_or_default();
    format!("{:x}", Sha256::digest(bytes))
}

pub fn sign_manifest_hmac(manifest: &mut PackageManifest, key_id: &str, secret: &str) {
    let digest = content_digest(manifest);
    let sig = hmac_sha256_hex(secret, &digest);
    manifest.signature = Some(super::manifest::Signature {
        algo: ALGO_HMAC.into(),
        public_key: key_id.into(),
        sig,
    });
}

pub fn sign_manifest_ed25519(manifest: &mut PackageManifest, seed: &[u8; 32]) {
    let signing_key = SigningKey::from_bytes(seed);
    let verifying_key = signing_key.verifying_key();
    let digest = content_digest(manifest);
    let sig = signing_key.sign(digest.as_bytes());
    manifest.signature = Some(super::manifest::Signature {
        algo: ALGO_ED25519.into(),
        public_key: hex::encode(verifying_key.as_bytes()),
        sig: hex::encode(sig.to_bytes()),
    });
}

pub fn verify(manifest: &PackageManifest, trusted: &BTreeMap<String, String>) -> (bool, String) {
    let Some(sig) = &manifest.signature else {
        return (false, "unsigned".into());
    };
    if sig.sig.is_empty() {
        return (false, "unsigned".into());
    }
    let digest = content_digest(manifest);
    let msg = digest.as_bytes();

    if sig.algo == ALGO_ED25519 {
        let mut allowed: Vec<String> = trusted.keys().cloned().collect();
        allowed.extend(trusted.values().cloned());
        if !allowed.iter().any(|k| k == &sig.public_key) {
            let prefix = &sig.public_key[..sig.public_key.len().min(16)];
            return (false, format!("untrusted public key {prefix}"));
        }
        let Ok(pk_bytes) = hex::decode(&sig.public_key) else {
            return (false, "malformed public key".into());
        };
        let Ok(sig_bytes) = hex::decode(&sig.sig) else {
            return (false, "malformed signature".into());
        };
        let Ok(arr): Result<[u8; 32], _> = pk_bytes.as_slice().try_into() else {
            return (false, "invalid public key length".into());
        };
        let Ok(pk) = VerifyingKey::from_bytes(&arr) else {
            return (false, "invalid public key".into());
        };
        let Ok(signature) = Signature::from_slice(&sig_bytes) else {
            return (false, "malformed signature".into());
        };
        return if pk.verify(msg, &signature).is_ok() {
            (true, "ok".into())
        } else {
            (false, "signature mismatch".into())
        };
    }

    if sig.algo == ALGO_HMAC {
        let Some(secret) = trusted.get(&sig.public_key) else {
            return (false, format!("untrusted key {}", sig.public_key));
        };
        let expected = hmac_sha256_hex(secret, &digest);
        return if constant_time_eq(&expected, &sig.sig) {
            (true, "ok".into())
        } else {
            (false, "signature mismatch".into())
        };
    }

    (false, format!("unsupported signature algo {}", sig.algo))
}

fn hmac_sha256_hex(secret: &str, digest_hex: &str) -> String {
    use hmac::{Hmac, Mac};
    type HmacSha256 = Hmac<Sha256>;
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).expect("hmac key");
    mac.update(digest_hex.as_bytes());
    hex::encode(mac.finalize().into_bytes())
}

fn constant_time_eq(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.bytes()
        .zip(b.bytes())
        .fold(0u8, |acc, (x, y)| acc | (x ^ y))
        == 0
}

pub fn load_trusted_keys(path: &std::path::Path) -> BTreeMap<String, String> {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

pub fn save_trusted_keys(
    path: &std::path::Path,
    keys: &BTreeMap<String, String>,
) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(
        path,
        serde_json::to_string_pretty(keys).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::market::manifest::PackageManifest;

    #[test]
    fn hmac_sign_and_verify() {
        let mut m = PackageManifest {
            name: "pkg".into(),
            version: "1".into(),
            r#type: "algorithm".into(),
            category: "loop".into(),
            entrypoint: Some("a.py".into()),
            description: String::new(),
            authors: vec![],
            license: "MIT".into(),
            keywords: vec![],
            homepage: String::new(),
            params_schema: serde_json::json!({}),
            codegen_templates: Default::default(),
            runtime: Default::default(),
            data: vec![],
            scorer: "exact_match".into(),
            num_tasks: 0,
            compatibility: Default::default(),
            integrity: Default::default(),
            signature: None,
            capabilities: vec![],
            dependencies: vec![],
            schema_version: "1".into(),
        };
        m.integrity.insert("a.py".into(), "abc".into());
        sign_manifest_hmac(&mut m, "dev", "secret");
        let mut trusted = BTreeMap::new();
        trusted.insert("dev".into(), "secret".into());
        let (ok, reason) = verify(&m, &trusted);
        assert!(ok, "{reason}");
    }
}
