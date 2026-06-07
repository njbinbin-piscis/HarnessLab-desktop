//! HarnessLab package manifest — the decentralized market protocol (v1).

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const PROTOCOL_VERSION: &str = "1";
pub const MANIFEST_FILENAME: &str = "harness-package.yaml";

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Compatibility {
    #[serde(default = "default_engine")]
    pub engine: String,
    #[serde(default = "default_protocol")]
    pub protocol_version: String,
}

fn default_engine() -> String {
    ">=0.1.0".into()
}
fn default_protocol() -> String {
    PROTOCOL_VERSION.into()
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Signature {
    #[serde(default)]
    pub algo: String,
    #[serde(default)]
    pub public_key: String,
    #[serde(default)]
    pub sig: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RuntimeSpec {
    #[serde(default = "default_python")]
    pub python: String,
    #[serde(default)]
    pub dependencies: Vec<String>,
}

fn default_python() -> String {
    ">=3.10".into()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageManifest {
    #[serde(default = "default_protocol")]
    pub schema_version: String,
    pub name: String,
    pub version: String,
    #[serde(default = "default_type")]
    pub r#type: String,
    #[serde(default = "default_category")]
    pub category: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub authors: Vec<String>,
    #[serde(default = "default_license")]
    pub license: String,
    #[serde(default)]
    pub keywords: Vec<String>,
    #[serde(default)]
    pub homepage: String,
    #[serde(default)]
    pub entrypoint: Option<String>,
    #[serde(default)]
    pub params_schema: serde_json::Value,
    #[serde(default)]
    pub codegen_templates: BTreeMap<String, String>,
    #[serde(default)]
    pub runtime: RuntimeSpec,
    #[serde(default)]
    pub data: Vec<String>,
    #[serde(default = "default_scorer")]
    pub scorer: String,
    #[serde(default)]
    pub num_tasks: u32,
    #[serde(default)]
    pub compatibility: Compatibility,
    #[serde(default)]
    pub integrity: BTreeMap<String, String>,
    #[serde(default)]
    pub signature: Option<Signature>,
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(default)]
    pub dependencies: Vec<String>,
}

fn default_type() -> String {
    "algorithm".into()
}
fn default_category() -> String {
    "loop".into()
}
fn default_license() -> String {
    "MIT".into()
}
fn default_scorer() -> String {
    "exact_match".into()
}

impl PackageManifest {
    pub fn validate_semantics(&self) -> Vec<String> {
        let mut problems = Vec::new();
        if self.r#type != "algorithm" && self.r#type != "dataset" {
            problems.push("type must be algorithm or dataset".into());
        }
        if self.r#type == "algorithm" && self.entrypoint.as_ref().is_none_or(|s| s.is_empty()) {
            problems.push("algorithm packages must set entrypoint".into());
        }
        if self.r#type == "dataset" && self.data.is_empty() {
            problems.push("dataset packages must list at least one data file".into());
        }
        if self.compatibility.protocol_version != PROTOCOL_VERSION {
            problems.push(format!(
                "protocol_version {} != client {PROTOCOL_VERSION}",
                self.compatibility.protocol_version
            ));
        }
        problems
    }
}

pub fn sha256_file(path: &Path) -> std::io::Result<String> {
    let bytes = std::fs::read(path)?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

pub fn compute_integrity(pkg_dir: &Path, files: &[String]) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for rel in files {
        let p = pkg_dir.join(rel);
        if p.is_file() {
            if let Ok(h) = sha256_file(&p) {
                out.insert(rel.clone(), h);
            }
        }
    }
    out
}

pub fn verify_integrity(pkg_dir: &Path, manifest: &PackageManifest) -> Vec<String> {
    let mut failed = Vec::new();
    for (rel, expected) in &manifest.integrity {
        let p = pkg_dir.join(rel);
        match sha256_file(&p) {
            Ok(actual) if actual == *expected => {}
            _ => failed.push(rel.clone()),
        }
    }
    failed
}

pub fn load_manifest(pkg_dir: &Path) -> Result<PackageManifest, String> {
    let f = pkg_dir.join(MANIFEST_FILENAME);
    let text = std::fs::read_to_string(&f).map_err(|e| format!("read {f:?}: {e}"))?;
    serde_yaml::from_str(&text).map_err(|e| format!("parse manifest: {e}"))
}

pub fn artifact_files(manifest: &PackageManifest) -> Vec<String> {
    let mut files = Vec::new();
    if let Some(ep) = &manifest.entrypoint {
        files.push(ep.clone());
    }
    files.extend(manifest.codegen_templates.values().cloned());
    files.extend(manifest.data.clone());
    files
}
