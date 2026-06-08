//! Project context managers.
//!
//! A [`ContextManager`] discovers project instruction files (and other context)
//! under a workspace root and renders them into a prompt-ready string that the
//! agent loop injects into the system prompt at run time.

use std::path::{Path, PathBuf};

/// A discovered project instruction file.
#[derive(Debug, Clone)]
pub struct ProjectInstructionFile {
    pub path: PathBuf,
    pub content: String,
}

/// Discovers and renders project context for the agent loop.
pub trait ContextManager: Send + Sync {
    /// A stable identifier for this manager.
    fn name(&self) -> &str;

    /// Discover instruction files under `root`.
    fn discover(&self, root: &Path) -> std::io::Result<Vec<ProjectInstructionFile>>;

    /// Render context for `root` within a character budget.
    fn render(&self, root: &Path, budget_chars: usize) -> std::io::Result<String>;
}

/// A declaratively-authored context manager: renders a fixed guidance `header`
/// plus the project documentation files (recursively, shallow) whose contents
/// match any of the configured `keywords`. With no keywords it includes the
/// top documentation files. Authored by the Architect and hot-registered via
/// [`super::agent::declarative`].
pub struct DeclarativeContextManager {
    name: String,
    header: String,
    keywords: Vec<String>,
}

impl DeclarativeContextManager {
    pub fn new(name: impl Into<String>, header: impl Into<String>, keywords: Vec<String>) -> Self {
        Self {
            name: name.into(),
            header: header.into(),
            keywords: keywords.into_iter().map(|k| k.to_lowercase()).collect(),
        }
    }

    fn collect(&self, dir: &Path, out: &mut Vec<ProjectInstructionFile>, depth: usize) {
        if depth > 4 || out.len() >= 32 {
            return;
        }
        let Ok(rd) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in rd.filter_map(|e| e.ok()) {
            let p = entry.path();
            let name = entry.file_name();
            let name = name.to_string_lossy().to_string();
            if name.starts_with('.') || name == "node_modules" || name == "target" {
                continue;
            }
            if p.is_dir() {
                self.collect(&p, out, depth + 1);
            } else {
                let lower = name.to_lowercase();
                let is_doc = lower.ends_with(".md")
                    || lower.ends_with(".mdx")
                    || lower.ends_with(".txt")
                    || lower.ends_with(".rst");
                if is_doc {
                    if let Ok(content) = std::fs::read_to_string(&p) {
                        out.push(ProjectInstructionFile { path: p, content });
                    }
                }
            }
            if out.len() >= 32 {
                return;
            }
        }
    }
}

impl ContextManager for DeclarativeContextManager {
    fn name(&self) -> &str {
        &self.name
    }

    fn discover(&self, root: &Path) -> std::io::Result<Vec<ProjectInstructionFile>> {
        let mut files = Vec::new();
        self.collect(root, &mut files, 0);
        if !self.keywords.is_empty() {
            files.retain(|f| {
                let hay = f.content.to_lowercase();
                self.keywords.iter().any(|k| hay.contains(k))
            });
        }
        Ok(files)
    }

    fn render(&self, root: &Path, budget_chars: usize) -> std::io::Result<String> {
        let files = self.discover(root)?;
        let mut out = String::new();
        if !self.header.trim().is_empty() {
            out.push_str(self.header.trim());
            out.push_str("\n\n");
        }
        for f in &files {
            out.push_str(&format!(
                "## {}\n\n{}\n\n---\n\n",
                f.path.display(),
                f.content
            ));
            if out.chars().count() >= budget_chars {
                break;
            }
        }
        if out.chars().count() > budget_chars {
            out = out.chars().take(budget_chars).collect();
        }
        Ok(out)
    }
}
