//! Workspace policy gate.
//!
//! Constrains tool side effects to the configured workspace root and decides
//! whether shell / file-write operations require confirmation.

use std::path::{Path, PathBuf};

/// Confirmation requirements for side-effecting tools.
#[derive(Debug, Clone, Copy, Default)]
pub struct PolicyFlags {
    pub confirm_shell: bool,
    pub confirm_file_write: bool,
}

/// Gates tool side effects against a workspace root.
#[derive(Debug, Clone)]
pub struct PolicyGate {
    root: PathBuf,
    flags: PolicyFlags,
}

impl PolicyGate {
    /// Create a gate rooted at `root` with default (permissive) flags.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            flags: PolicyFlags::default(),
        }
    }

    /// Set the confirmation flags.
    pub fn with_flags(mut self, flags: PolicyFlags) -> Self {
        self.flags = flags;
        self
    }

    /// The workspace root all paths are resolved against.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Resolve a (possibly relative) path against the workspace root and verify
    /// it does not escape the root via `..` traversal.
    pub fn resolve_in_workspace(&self, path: &str) -> Result<PathBuf, String> {
        let candidate = Path::new(path);
        let joined = if candidate.is_absolute() {
            candidate.to_path_buf()
        } else {
            self.root.join(candidate)
        };
        let normalized = normalize(&joined);
        let root_norm = normalize(&self.root);
        if normalized.starts_with(&root_norm) {
            Ok(normalized)
        } else {
            Err(format!("path '{path}' resolves outside the workspace root"))
        }
    }

    pub fn flags(&self) -> PolicyFlags {
        self.flags
    }
}

/// Lexically normalize a path (resolve `.` / `..` without touching the FS).
fn normalize(p: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for comp in p.components() {
        match comp {
            std::path::Component::ParentDir => {
                out.pop();
            }
            std::path::Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    out
}
