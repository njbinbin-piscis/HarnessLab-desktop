//! Registry index — catalog schema and `registry:name@version` resolution.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

pub const INDEX_SCHEMA_VERSION: &str = "1";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistriesConfig {
    #[serde(default)]
    pub registries: Vec<String>,
}

impl Default for RegistriesConfig {
    fn default() -> Self {
        Self {
            registries: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryIndex {
    pub schema_version: String,
    pub registry: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub updated_at: Option<String>,
    #[serde(default)]
    pub homepage: Option<String>,
    #[serde(default)]
    pub packages: BTreeMap<String, PackageIndexEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageIndexEntry {
    #[serde(default)]
    pub latest: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(rename = "type", default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default)]
    pub keywords: Vec<String>,
    #[serde(default)]
    pub authors: Vec<String>,
    #[serde(default)]
    pub license: Option<String>,
    pub versions: BTreeMap<String, VersionRef>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum VersionRef {
    Source(String),
    Detail(VersionDetail),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionDetail {
    pub source: String,
    #[serde(default)]
    pub published_at: Option<String>,
    #[serde(default)]
    pub yanked: bool,
    #[serde(default)]
    pub changelog: Option<String>,
}

impl VersionRef {
    pub fn source(&self) -> &str {
        match self {
            VersionRef::Source(s) => s,
            VersionRef::Detail(d) => &d.source,
        }
    }

    pub fn yanked(&self) -> bool {
        match self {
            VersionRef::Source(_) => false,
            VersionRef::Detail(d) => d.yanked,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct CatalogEntry {
    pub registry: String,
    pub registry_url: String,
    pub name: String,
    pub latest: Option<String>,
    pub description: Option<String>,
    pub r#type: Option<String>,
    pub category: Option<String>,
    pub keywords: Vec<String>,
    pub versions: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct ResolvedRegistrySpec {
    pub registry: String,
    pub name: String,
    pub version: String,
    pub source: String,
}

pub fn parse_registry_spec(spec: &str) -> Result<(String, Option<String>), String> {
    let body = spec
        .strip_prefix("registry:")
        .ok_or_else(|| "not a registry spec".to_string())?;
    let (name, version) = match body.split_once('@') {
        Some((n, v)) if !v.is_empty() => (n.to_string(), Some(v.to_string())),
        _ => (body.to_string(), None),
    };
    if name.is_empty() {
        return Err("registry spec missing package name".into());
    }
    Ok((name, version))
}

pub fn resolve_in_index(
    index: &RegistryIndex,
    name: &str,
    version: Option<&str>,
    index_base: Option<&Path>,
) -> Result<ResolvedRegistrySpec, String> {
    let entry = index.packages.get(name).ok_or_else(|| {
        format!(
            "package_not_found: {name} not in registry {}",
            index.registry
        )
    })?;

    let chosen = version
        .map(|v| v.to_string())
        .or_else(|| entry.latest.clone())
        .or_else(|| entry.versions.keys().next().cloned())
        .ok_or_else(|| format!("package_not_found: no versions for {name}"))?;

    let ver_ref = entry
        .versions
        .get(&chosen)
        .ok_or_else(|| format!("package_not_found: {name}@{chosen}"))?;

    if ver_ref.yanked() {
        return Err(format!("version_yanked: {name}@{chosen}"));
    }

    let source = normalize_index_source(ver_ref.source(), index_base);

    Ok(ResolvedRegistrySpec {
        registry: index.registry.clone(),
        name: name.to_string(),
        version: chosen,
        source,
    })
}

/// Resolve `file:` paths in an index relative to the index file directory.
pub fn normalize_index_source(source: &str, index_base: Option<&Path>) -> String {
    let Some(base) = index_base else {
        return source.to_string();
    };
    let Some(local) = source.strip_prefix("file:") else {
        return source.to_string();
    };
    let path = Path::new(local);
    if path.is_absolute() {
        return source.to_string();
    }
    format!("file:{}", base.join(path).display())
}

pub fn index_to_catalog(index: &RegistryIndex, registry_url: &str) -> Vec<CatalogEntry> {
    index
        .packages
        .iter()
        .map(|(name, entry)| CatalogEntry {
            registry: index.registry.clone(),
            registry_url: registry_url.to_string(),
            name: name.clone(),
            latest: entry.latest.clone(),
            description: entry.description.clone(),
            r#type: entry.kind.clone(),
            category: entry.category.clone(),
            keywords: entry.keywords.clone(),
            versions: entry.versions.keys().cloned().collect(),
        })
        .collect()
}

pub fn validate_index(index: &RegistryIndex) -> Vec<String> {
    let mut problems = Vec::new();
    if index.schema_version != INDEX_SCHEMA_VERSION {
        problems.push(format!(
            "unsupported schema_version {} (expected {INDEX_SCHEMA_VERSION})",
            index.schema_version
        ));
    }
    if index.registry.is_empty() {
        problems.push("missing registry id".into());
    }
    for (name, entry) in &index.packages {
        if entry.versions.is_empty() {
            problems.push(format!("package {name} has no versions"));
        }
        if let Some(latest) = &entry.latest {
            if !entry.versions.contains_key(latest) {
                problems.push(format!("package {name}: latest {latest} not in versions"));
            }
        }
    }
    problems
}

pub fn bundled_example_index_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/registry/index.json")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn parses_version_string_and_detail() {
        let json = r#"{"source":"https://x/pkg.tar.gz","yanked":true}"#;
        let d: VersionRef = serde_json::from_str(json).unwrap();
        assert!(d.yanked());
        let s: VersionRef = serde_json::from_str(r#""file:./pkg""#).unwrap();
        assert_eq!(s.source(), "file:./pkg");
    }

    #[test]
    fn resolves_registry_spec_from_example_index() {
        let path = bundled_example_index_path();
        if !path.is_file() {
            return;
        }
        let text = fs::read_to_string(&path).unwrap();
        let index: RegistryIndex = serde_json::from_str(&text).unwrap();
        assert!(validate_index(&index).is_empty());
        let base = path.parent().unwrap();
        let resolved = resolve_in_index(&index, "cool-loop", None, Some(base)).unwrap();
        assert_eq!(resolved.version, "0.1.0");
        assert!(resolved.source.starts_with("file:"));
    }

    #[test]
    fn parse_registry_spec_splits_name_version() {
        let (n, v) = parse_registry_spec("registry:cool-loop@0.1.0").unwrap();
        assert_eq!(n, "cool-loop");
        assert_eq!(v.as_deref(), Some("0.1.0"));
        let (n, v) = parse_registry_spec("registry:cool-loop").unwrap();
        assert_eq!(n, "cool-loop");
        assert!(v.is_none());
    }
}
