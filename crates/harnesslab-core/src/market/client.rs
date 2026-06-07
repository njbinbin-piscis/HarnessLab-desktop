//! Market client — install / inspect / list / uninstall with lockfile.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::manifest::{load_manifest, verify_integrity, PackageManifest};
use super::registry::{self, bundled_example_index_path, RegistriesConfig};
use super::signing::{self, load_trusted_keys};
use super::sources::{self, FetchConfig};

#[derive(Debug, Clone, Serialize)]
pub struct InstalledPackage {
    pub name: String,
    pub version: String,
    pub r#type: String,
    pub category: String,
    pub source: String,
    pub installed_at: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct InstallSummary {
    pub name: String,
    pub version: String,
    pub r#type: String,
    pub category: String,
    pub wired: String,
    pub signature: String,
    pub capabilities: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InspectResult {
    pub manifest: Value,
    pub problems: Vec<String>,
}

#[derive(Debug)]
pub struct CapabilityConsentRequired {
    pub capabilities: Vec<String>,
}

impl std::fmt::Display for CapabilityConsentRequired {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "package declares capabilities requiring consent: {:?}",
            self.capabilities
        )
    }
}

#[derive(Debug)]
pub struct IntegrityError(pub String);

impl std::fmt::Display for IntegrityError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Default, Deserialize, Serialize)]
struct Lockfile {
    #[serde(default)]
    packages: BTreeMap<String, LockEntry>,
}

#[derive(Clone, Deserialize, Serialize)]
struct LockEntry {
    version: String,
    r#type: String,
    category: String,
    source: String,
    integrity: BTreeMap<String, String>,
    installed_at: f64,
}

pub struct MarketStore {
    root: PathBuf,
}

/// Browse entry returned from configured registry indexes.
pub type CatalogEntry = registry::CatalogEntry;

impl MarketStore {
    pub fn new(root: PathBuf) -> Self {
        std::fs::create_dir_all(&root).ok();
        let store = MarketStore { root };
        store.seed_registries_if_empty();
        store
    }

    fn seed_registries_if_empty(&self) {
        if self.registries_path().is_file() {
            return;
        }
        let example = bundled_example_index_path();
        if example.is_file() {
            let _ = self.set_registries(vec![format!("file:{}", example.display())]);
        }
    }

    fn fetch_config(&self) -> FetchConfig {
        FetchConfig {
            registry_urls: self.get_registries(),
            cache_dir: self.root.join("cache"),
        }
    }

    fn registries_path(&self) -> PathBuf {
        self.root.join("registries.json")
    }

    pub fn get_registries(&self) -> Vec<String> {
        if let Ok(text) = std::fs::read_to_string(self.registries_path()) {
            if let Ok(cfg) = serde_json::from_str::<RegistriesConfig>(&text) {
                if !cfg.registries.is_empty() {
                    return cfg.registries;
                }
            }
        }
        env_registry_urls()
    }

    pub fn set_registries(&self, urls: Vec<String>) -> Result<(), String> {
        let cfg = RegistriesConfig { registries: urls };
        std::fs::write(
            self.registries_path(),
            serde_json::to_string_pretty(&cfg).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())
    }

    pub fn catalog(&self) -> Result<Vec<CatalogEntry>, String> {
        sources::load_catalog(&self.fetch_config())
    }

    fn packages_dir(&self) -> PathBuf {
        self.root.join("packages")
    }

    fn datasets_dir(&self) -> PathBuf {
        self.root.join("datasets")
    }

    fn lockfile_path(&self) -> PathBuf {
        self.packages_dir().join("harness.lock")
    }

    fn trusted_keys_path(&self) -> PathBuf {
        self.packages_dir().join("trusted_keys.json")
    }

    fn read_lock(&self) -> Lockfile {
        std::fs::read_to_string(self.lockfile_path())
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default()
    }

    fn write_lock(&self, lock: &Lockfile) -> Result<(), String> {
        std::fs::create_dir_all(self.packages_dir()).map_err(|e| e.to_string())?;
        std::fs::write(
            self.lockfile_path(),
            serde_json::to_string_pretty(lock).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())
    }

    pub fn list_installed(&self) -> Vec<InstalledPackage> {
        self.read_lock()
            .packages
            .into_iter()
            .map(|(name, e)| InstalledPackage {
                name,
                version: e.version,
                r#type: e.r#type,
                category: e.category,
                source: e.source,
                installed_at: e.installed_at,
            })
            .collect()
    }

    pub fn inspect(&self, source: &str) -> Result<InspectResult, String> {
        let staging = self.packages_dir().join(".inspect");
        let _ = std::fs::remove_dir_all(&staging);
        std::fs::create_dir_all(&staging).map_err(|e| e.to_string())?;
        let pkg_dir = sources::fetch(source, &staging, &self.fetch_config())?;
        let manifest = load_manifest(&pkg_dir)?;
        Ok(InspectResult {
            manifest: serde_json::to_value(&manifest).unwrap_or(Value::Null),
            problems: manifest.validate_semantics(),
        })
    }

    pub fn install(
        &self,
        source: &str,
        consent_capabilities: bool,
        verify_signature: bool,
    ) -> Result<InstallSummary, String> {
        let staging = self.packages_dir().join(".staging");
        let _ = std::fs::remove_dir_all(&staging);
        std::fs::create_dir_all(&staging).map_err(|e| e.to_string())?;
        let pkg_dir = sources::fetch(source, &staging, &self.fetch_config())?;
        let manifest = load_manifest(&pkg_dir)?;
        let problems = manifest.validate_semantics();
        if !problems.is_empty() {
            return Err(format!("invalid manifest: {problems:?}"));
        }
        if !manifest.integrity.is_empty() {
            let failed = verify_integrity(&pkg_dir, &manifest);
            if !failed.is_empty() {
                return Err(
                    IntegrityError(format!("integrity check failed for: {failed:?}")).to_string(),
                );
            }
        }
        let trusted = load_trusted_keys(&self.trusted_keys_path());
        let mut signature_status = "unsigned".to_string();
        if manifest
            .signature
            .as_ref()
            .is_some_and(|s| !s.sig.is_empty())
        {
            let (ok, reason) = signing::verify(&manifest, &trusted);
            signature_status = if ok {
                "valid".into()
            } else {
                format!("invalid ({reason})")
            };
            if verify_signature && !ok {
                return Err(
                    IntegrityError(format!("signature verification failed: {reason}")).to_string(),
                );
            }
        } else if verify_signature {
            return Err(IntegrityError(
                "signature verification requested but package is unsigned".into(),
            )
            .to_string());
        }
        if !manifest.capabilities.is_empty() && !consent_capabilities {
            return Err(CapabilityConsentRequired {
                capabilities: manifest.capabilities.clone(),
            }
            .to_string());
        }

        let target = self
            .packages_dir()
            .join(&manifest.name)
            .join(&manifest.version);
        if target.exists() {
            std::fs::remove_dir_all(&target).map_err(|e| e.to_string())?;
        }
        copy_dir_recursive(&pkg_dir, &target)?;

        let wired = self.wire_package(&manifest, &target)?;

        let mut lock = self.read_lock();
        lock.packages.insert(
            manifest.name.clone(),
            LockEntry {
                version: manifest.version.clone(),
                r#type: manifest.r#type.clone(),
                category: manifest.category.clone(),
                source: source.to_string(),
                integrity: manifest.integrity.clone(),
                installed_at: crate::project::now_secs(),
            },
        );
        self.write_lock(&lock)?;
        let _ = std::fs::remove_dir_all(&staging);

        Ok(InstallSummary {
            name: manifest.name,
            version: manifest.version,
            r#type: manifest.r#type,
            category: manifest.category,
            wired,
            signature: signature_status,
            capabilities: manifest.capabilities,
        })
    }

    fn wire_package(&self, manifest: &PackageManifest, target: &Path) -> Result<String, String> {
        if manifest.r#type == "algorithm" {
            let plugins = self.root.join("plugins");
            std::fs::create_dir_all(&plugins).map_err(|e| e.to_string())?;
            let ep = manifest.entrypoint.as_ref().ok_or("missing entrypoint")?;
            let src = target.join(ep);
            let dest = plugins.join(format!("pkg_{}.txt", manifest.name.replace('-', "_")));
            std::fs::copy(&src, &dest).map_err(|e| e.to_string())?;
            return Ok(format!(
                "algorithm metadata registered ({}) — palette updated",
                manifest.category
            ));
        }
        if manifest.r#type == "dataset" {
            std::fs::create_dir_all(self.datasets_dir()).map_err(|e| e.to_string())?;
            let first = manifest.data.first().ok_or("missing data files")?;
            let src = target.join(first);
            let dest = self.datasets_dir().join(format!("{}.jsonl", manifest.name));
            std::fs::copy(&src, &dest).map_err(|e| e.to_string())?;
            let meta = serde_json::json!({
                "scorer": manifest.scorer,
                "description": manifest.description,
            });
            std::fs::write(
                self.datasets_dir()
                    .join(format!("{}.meta.json", manifest.name)),
                serde_json::to_string_pretty(&meta).unwrap_or_default(),
            )
            .map_err(|e| e.to_string())?;
            return Ok("dataset registered".into());
        }
        Ok("no-op".into())
    }

    pub fn uninstall(&self, name: &str) -> Result<bool, String> {
        let mut lock = self.read_lock();
        let Some(entry) = lock.packages.remove(name) else {
            return Ok(false);
        };
        let pkg_root = self.packages_dir().join(name);
        let _ = std::fs::remove_dir_all(&pkg_root);
        if entry.r#type == "algorithm" {
            let p = self
                .root
                .join("plugins")
                .join(format!("pkg_{}.txt", name.replace('-', "_")));
            let _ = std::fs::remove_file(p);
        } else if entry.r#type == "dataset" {
            for suffix in [".jsonl", ".meta.json"] {
                let _ = std::fs::remove_file(self.datasets_dir().join(format!("{name}{suffix}")));
            }
        }
        self.write_lock(&lock)?;
        Ok(true)
    }

    /// Extra palette entries from installed algorithm packages.
    pub fn market_components(&self) -> Vec<(String, String, String)> {
        let lock = self.read_lock();
        lock.packages
            .iter()
            .filter(|(_, e)| e.r#type == "algorithm")
            .map(|(name, e)| {
                (
                    e.category.clone(),
                    name.clone(),
                    format!("Installed package {name}"),
                )
            })
            .collect()
    }
}

fn env_registry_urls() -> Vec<String> {
    std::env::var("HARNESSLAB_REGISTRY")
        .ok()
        .map(|s| {
            s.split(',')
                .map(|x| x.trim().to_string())
                .filter(|x| !x.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dst).map_err(|e| e.to_string())?;
    for entry in std::fs::read_dir(src).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let ty = entry.file_type().map_err(|e| e.to_string())?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir_recursive(&from, &to)?;
        } else {
            std::fs::copy(&from, &to).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn install_local_cool_loop_example() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/packages/cool-loop");
        if !root.join("harness-package.yaml").is_file() {
            return;
        }
        let dir = tempdir().unwrap();
        let store = MarketStore::new(dir.path().to_path_buf());
        let source = format!("file:{}", root.display());
        let summary = store.install(&source, true, false).expect("install");
        assert_eq!(summary.name, "cool-loop");
        assert_eq!(store.list_installed().len(), 1);
        assert!(store.uninstall("cool-loop").unwrap());
    }
}
