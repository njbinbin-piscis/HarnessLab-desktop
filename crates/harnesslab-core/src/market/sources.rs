//! Source spec resolution — local, HTTPS tarball, git, and registry index.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;

use flate2::read::GzDecoder;
use sha2::{Digest, Sha256};
use tar::Archive;

use super::manifest::MANIFEST_FILENAME;
use super::registry::{self, parse_registry_spec, RegistryIndex, ResolvedRegistrySpec};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceKind {
    Local,
    Url,
    Git,
    Registry,
}

#[derive(Debug, Clone)]
pub struct FetchConfig {
    /// Registry index URLs (`https://…/index.json` or `file:…`).
    pub registry_urls: Vec<String>,
    /// Cache downloaded tarballs under this directory.
    pub cache_dir: PathBuf,
}

impl FetchConfig {
    pub fn empty_cache(cache_dir: PathBuf) -> Self {
        Self {
            registry_urls: Vec::new(),
            cache_dir,
        }
    }
}

/// Resolve `source` to a local package directory containing `harness-package.yaml`.
pub fn fetch(source: &str, dest_parent: &Path, config: &FetchConfig) -> Result<PathBuf, String> {
    fetch_with_kind(source, dest_parent, config).map(|(p, _)| p)
}

pub fn fetch_with_kind(
    source: &str,
    dest_parent: &Path,
    config: &FetchConfig,
) -> Result<(PathBuf, SourceKind), String> {
    std::fs::create_dir_all(dest_parent).map_err(|e| e.to_string())?;

    if source.starts_with("registry:") {
        let resolved = resolve_registry(source, config)?;
        let pkg = fetch(&resolved.source, dest_parent, config)?;
        return Ok((pkg, SourceKind::Registry));
    }
    if source.starts_with("git+") {
        let pkg = fetch_git(&source[4..], dest_parent)?;
        return Ok((pkg, SourceKind::Git));
    }
    if source.starts_with("https://") || source.starts_with("http://") {
        if source.ends_with(".tar.gz") || source.ends_with(".tgz") {
            let pkg = fetch_tarball(source, dest_parent, config)?;
            return Ok((pkg, SourceKind::Url));
        }
        return Err(format!(
            "unsupported URL source (expected .tar.gz or .tgz): {source}"
        ));
    }
    if source.starts_with("file:") {
        let pkg = fetch_local(&source[5..])?;
        return Ok((pkg, SourceKind::Local));
    }
    let pkg = fetch_local(source)?;
    Ok((pkg, SourceKind::Local))
}

pub fn resolve_registry(
    source: &str,
    config: &FetchConfig,
) -> Result<ResolvedRegistrySpec, String> {
    let (name, version) = parse_registry_spec(source)?;
    if config.registry_urls.is_empty() {
        return Err(
            "registry_not_configured: add registry URLs in Market settings or HARNESSLAB_REGISTRY"
                .into(),
        );
    }
    for url in &config.registry_urls {
        match try_resolve_in_registry_url(url, &name, version.as_deref()) {
            Ok(r) => return Ok(r),
            Err(e) if e.starts_with("package_not_found:") => continue,
            Err(e) => return Err(e),
        }
    }
    Err(format!(
        "package_not_found: {name} not in any configured registry"
    ))
}

fn try_resolve_in_registry_url(
    location: &str,
    name: &str,
    version: Option<&str>,
) -> Result<ResolvedRegistrySpec, String> {
    let (index, base) = load_index(location)?;
    registry::resolve_in_index(&index, name, version, base.as_deref())
}

pub fn load_index(location: &str) -> Result<(RegistryIndex, Option<PathBuf>), String> {
    let (text, base) = if location.starts_with("file:") {
        let path = PathBuf::from(&location[5..]);
        let path = if path.is_absolute() {
            path
        } else {
            std::env::current_dir()
                .map_err(|e| e.to_string())?
                .join(path)
        };
        (
            std::fs::read_to_string(&path).map_err(|e| format!("invalid_index: {e}"))?,
            Some(path.parent().unwrap_or(Path::new(".")).to_path_buf()),
        )
    } else if location.starts_with("https://") || location.starts_with("http://") {
        let text = http_get(location)?;
        (text, None)
    } else {
        let path = PathBuf::from(location);
        (
            std::fs::read_to_string(&path).map_err(|e| format!("invalid_index: {e}"))?,
            Some(path.parent().unwrap_or(Path::new(".")).to_path_buf()),
        )
    };
    let index: RegistryIndex =
        serde_json::from_str(&text).map_err(|e| format!("invalid_index: {e}"))?;
    let problems = registry::validate_index(&index);
    if !problems.is_empty() {
        return Err(format!("invalid_index: {problems:?}"));
    }
    Ok((index, base))
}

pub fn load_catalog(config: &FetchConfig) -> Result<Vec<registry::CatalogEntry>, String> {
    let mut out = Vec::new();
    for url in &config.registry_urls {
        let (index, _) = load_index(url)?;
        out.extend(registry::index_to_catalog(&index, url));
    }
    out.sort_by(|a, b| a.name.cmp(&b.name).then(a.registry.cmp(&b.registry)));
    Ok(out)
}

fn fetch_local(path: &str) -> Result<PathBuf, String> {
    let src = PathBuf::from(path);
    let src = if src.is_absolute() {
        src
    } else {
        std::env::current_dir()
            .map_err(|e| e.to_string())?
            .join(src)
    };
    if !src.exists() {
        return Err(format!("local source not found: {}", src.display()));
    }
    find_pkg_root(&src)
}

fn fetch_tarball(url: &str, _dest_parent: &Path, config: &FetchConfig) -> Result<PathBuf, String> {
    std::fs::create_dir_all(&config.cache_dir).map_err(|e| e.to_string())?;
    let cache_key = hex::encode(Sha256::digest(url.as_bytes()));
    let extract_root = config.cache_dir.join(&cache_key);
    if extract_root.join(MANIFEST_FILENAME).is_file() {
        return find_pkg_root(&extract_root);
    }
    let _ = std::fs::remove_dir_all(&extract_root);
    std::fs::create_dir_all(&extract_root).map_err(|e| e.to_string())?;

    let bytes = http_get_bytes(url)?;
    let archive_path = extract_root.join("pkg.tar.gz");
    std::fs::write(&archive_path, &bytes).map_err(|e| e.to_string())?;

    let file = std::fs::File::open(&archive_path).map_err(|e| e.to_string())?;
    let decoder = GzDecoder::new(file);
    let mut archive = Archive::new(decoder);
    let inner = extract_root.join("extracted");
    std::fs::create_dir_all(&inner).map_err(|e| e.to_string())?;
    archive
        .unpack(&inner)
        .map_err(|e| format!("fetch_failed: tar extract: {e}"))?;
    find_pkg_root(&inner)
}

fn fetch_git(url: &str, dest_parent: &Path) -> Result<PathBuf, String> {
    let (repo_url, ref_name) = match url.split_once('#') {
        Some((u, r)) => (u, Some(r)),
        None => (url, None),
    };
    let work = dest_parent.join(format!(
        "git-{}",
        hex::encode(Sha256::digest(repo_url.as_bytes()))[..8].to_string()
    ));
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work).map_err(|e| e.to_string())?;
    let clone_dir = work.join("repo");

    let mut cmd = Command::new("git");
    cmd.args(["clone", "--depth", "1"]);
    if let Some(r) = ref_name {
        cmd.args(["--branch", r]);
    }
    cmd.arg(repo_url).arg(&clone_dir);
    let output = cmd
        .output()
        .map_err(|e| format!("fetch_failed: git not available: {e}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("fetch_failed: git clone: {stderr}"));
    }
    find_pkg_root(&clone_dir)
}

fn http_get(url: &str) -> Result<String, String> {
    let bytes = http_get_bytes(url)?;
    String::from_utf8(bytes).map_err(|e| format!("fetch_failed: invalid utf-8: {e}"))
}

fn http_get_bytes(url: &str) -> Result<Vec<u8>, String> {
    let mut reader = ureq::get(url)
        .call()
        .map_err(|e| format!("fetch_failed: {e}"))?
        .into_reader();
    let mut buf = Vec::new();
    reader
        .read_to_end(&mut buf)
        .map_err(|e| format!("fetch_failed: read body: {e}"))?;
    Ok(buf)
}

fn find_pkg_root(root: &Path) -> Result<PathBuf, String> {
    if root.join(MANIFEST_FILENAME).is_file() {
        return Ok(root.to_path_buf());
    }
    let mut matches = Vec::new();
    walk_for_manifest(root, &mut matches)?;
    matches
        .into_iter()
        .next()
        .map(|p| p.parent().unwrap_or(root).to_path_buf())
        .ok_or_else(|| format!("no {MANIFEST_FILENAME} found under {}", root.display()))
}

fn walk_for_manifest(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
    for entry in std::fs::read_dir(dir).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        if path.is_dir() {
            walk_for_manifest(&path, out)?;
        } else if path.file_name().and_then(|n| n.to_str()) == Some(MANIFEST_FILENAME) {
            out.push(path);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn finds_manifest_in_repo_example() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/packages/cool-loop");
        if root.join(MANIFEST_FILENAME).is_file() {
            let pkg = fetch_local(root.to_str().unwrap()).unwrap();
            assert!(pkg.join(MANIFEST_FILENAME).is_file());
        }
    }

    #[test]
    fn registry_resolves_bundled_example() {
        let index_path = registry::bundled_example_index_path();
        if !index_path.is_file() {
            return;
        }
        let dir = tempdir().unwrap();
        let config = FetchConfig {
            registry_urls: vec![format!("file:{}", index_path.display())],
            cache_dir: dir.path().join("cache"),
        };
        let spec = "registry:cool-loop@0.1.0";
        let resolved = resolve_registry(spec, &config).unwrap();
        let (pkg, kind) =
            fetch_with_kind(&resolved.source, &dir.path().join("staging"), &config).unwrap();
        assert_eq!(kind, SourceKind::Local);
        assert!(pkg.join(MANIFEST_FILENAME).is_file());
    }

    #[test]
    fn catalog_loads_bundled_example() {
        let index_path = registry::bundled_example_index_path();
        if !index_path.is_file() {
            return;
        }
        let dir = tempdir().unwrap();
        let config = FetchConfig {
            registry_urls: vec![format!("file:{}", index_path.display())],
            cache_dir: dir.path().join("cache"),
        };
        let catalog = load_catalog(&config).unwrap();
        assert!(catalog.iter().any(|e| e.name == "cool-loop"));
    }
}
