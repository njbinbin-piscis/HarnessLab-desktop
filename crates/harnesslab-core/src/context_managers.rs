//! Context Manager implementations for Agent Workbench.

use crate::plugins::{ContextEntry, ContextManager};
use anyhow::{Context, Result};
use glob::glob;
use std::path::Path;

/// Adapter that exposes a workbench [`ContextManager`] through the native
/// runtime's `harness_rt::context::ContextManager` trait so it can be wired
/// into the `harness_rt::agent::harness::HarnessConfig` context slot.
///
/// The kernel agent loop only calls `render`, so `discover` returns an empty
/// kernel instruction list; the real discovery + rendering happens via the
/// wrapped workbench manager, preserving its richer scan behaviour.
pub struct KernelContextAdapter {
    name: String,
    inner: Box<dyn ContextManager>,
}

impl KernelContextAdapter {
    pub fn new(name: impl Into<String>, inner: Box<dyn ContextManager>) -> Self {
        Self {
            name: name.into(),
            inner,
        }
    }
}

impl harness_rt::context::ContextManager for KernelContextAdapter {
    fn name(&self) -> &str {
        &self.name
    }

    fn discover(
        &self,
        _root: &Path,
    ) -> std::io::Result<Vec<harness_rt::context::ProjectInstructionFile>> {
        Ok(Vec::new())
    }

    fn render(&self, root: &Path, budget_chars: usize) -> std::io::Result<String> {
        let entries = self
            .inner
            .discover(root)
            .map_err(|e| std::io::Error::other(e.to_string()))?;
        let mut rendered = self.inner.render(&entries);
        if rendered.chars().count() > budget_chars {
            rendered = rendered.chars().take(budget_chars).collect();
        }
        Ok(rendered)
    }
}

/// Deep scan context manager that looks for PISCI.md, .pisci/rules.md, and kb/architecture/*.md
pub struct ProjectContextManager;

impl ContextManager for ProjectContextManager {
    fn discover(&self, root: &Path) -> Result<Vec<ContextEntry>> {
        let mut entries = Vec::new();

        // 1. Look for PISCI.md in root
        let pisci_md = root.join("PISCI.md");
        if pisci_md.exists() {
            let content = std::fs::read_to_string(&pisci_md)?;
            entries.push(ContextEntry {
                path: pisci_md,
                content,
                priority: 100,
            });
        }

        // 2. Look for .pisci/rules.md
        let rules_md = root.join(".pisci").join("rules.md");
        if rules_md.exists() {
            let content = std::fs::read_to_string(&rules_md)?;
            entries.push(ContextEntry {
                path: rules_md,
                content,
                priority: 90,
            });
        }

        // 3. Look for kb/architecture/*.md
        let arch_pattern = root.join("kb").join("architecture").join("*.md");
        if let Some(pattern_str) = arch_pattern.to_str() {
            for path in glob(pattern_str)
                .context("Failed to read glob pattern")?
                .flatten()
            {
                if path.is_file() {
                    if let Ok(content) = std::fs::read_to_string(&path) {
                        entries.push(ContextEntry {
                            path,
                            content,
                            priority: 80,
                        });
                    }
                }
            }
        }

        // Sort by priority descending
        entries.sort_by_key(|e| std::cmp::Reverse(e.priority));
        Ok(entries)
    }

    fn render(&self, entries: &[ContextEntry]) -> String {
        if entries.is_empty() {
            return String::new();
        }

        let mut rendered = String::from("# Project Context\n\n");
        for entry in entries {
            rendered.push_str(&format!(
                "## {}\n\n{}\n\n---\n\n",
                entry.path.display(),
                entry.content
            ));
        }
        rendered
    }
}

/// Shallow scan context manager that only looks at the immediate project root.
pub struct WorkspaceRootManager;

impl ContextManager for WorkspaceRootManager {
    fn discover(&self, root: &Path) -> Result<Vec<ContextEntry>> {
        let mut entries = Vec::new();

        // 1. Look for README.md
        let readme = root.join("README.md");
        if readme.exists() {
            let content = std::fs::read_to_string(&readme)?;
            entries.push(ContextEntry {
                path: readme,
                content,
                priority: 50,
            });
        }

        // 2. Look for PISCI.md
        let pisci = root.join("PISCI.md");
        if pisci.exists() {
            let content = std::fs::read_to_string(&pisci)?;
            entries.push(ContextEntry {
                path: pisci,
                content,
                priority: 100,
            });
        }

        // Sort by priority descending
        entries.sort_by_key(|e| std::cmp::Reverse(e.priority));
        Ok(entries)
    }

    fn render(&self, entries: &[ContextEntry]) -> String {
        if entries.is_empty() {
            return String::new();
        }

        let mut rendered = String::from("# Workspace Context\n\n");
        for entry in entries {
            rendered.push_str(&format!(
                "## {}\n\n{}\n\n---\n\n",
                entry.path.display(),
                entry.content
            ));
        }
        rendered
    }
}

/// Dynamic (RAG-style) context manager.
///
/// Performs a real retrieval over the workspace: it recursively collects
/// documentation/text files (`*.md`, `*.mdx`, `*.txt`, `*.rst`), ranks them by
/// a filename-importance heuristic plus size, and returns a budgeted set. This
/// surfaces the most relevant project knowledge without requiring an embedding
/// service, while remaining a genuine scan (not a fixed stub).
pub struct DynamicContextManager;

/// Heuristic importance for a documentation filename (higher = more relevant).
fn doc_priority(name: &str) -> u8 {
    let lower = name.to_lowercase();
    if lower.starts_with("pisci") {
        100
    } else if lower.contains("readme") {
        90
    } else if lower.contains("architecture") || lower.contains("design") {
        80
    } else if lower.contains("guide") || lower.contains("overview") || lower.contains("spec") {
        70
    } else if lower.ends_with(".md") || lower.ends_with(".mdx") {
        50
    } else {
        30
    }
}

fn is_doc_file(name: &str) -> bool {
    let lower = name.to_lowercase();
    lower.ends_with(".md")
        || lower.ends_with(".mdx")
        || lower.ends_with(".txt")
        || lower.ends_with(".rst")
}

fn collect_docs(dir: &Path, out: &mut Vec<ContextEntry>, depth: usize) {
    if depth > 6 || out.len() >= 64 {
        return;
    }
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in rd.filter_map(|e| e.ok()) {
        let p = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with('.') || name == "node_modules" || name == "target" {
            continue;
        }
        if p.is_dir() {
            collect_docs(&p, out, depth + 1);
        } else if is_doc_file(&name) {
            if let Ok(content) = std::fs::read_to_string(&p) {
                out.push(ContextEntry {
                    priority: doc_priority(&name),
                    path: p,
                    content,
                });
            }
        }
        if out.len() >= 64 {
            return;
        }
    }
}

impl ContextManager for DynamicContextManager {
    fn discover(&self, root: &Path) -> Result<Vec<ContextEntry>> {
        let mut entries = Vec::new();
        collect_docs(root, &mut entries, 0);

        // Embedding-based re-rank: prefer the most representative ("central")
        // documents within each importance tier. Embeddings are local and
        // dependency-free, so this is always available; on a single document it
        // degrades to the filename/size heuristic below.
        let embeds: Vec<Vec<f32>> = entries
            .iter()
            .map(|e| {
                let name = e
                    .path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();
                let snippet: String = e.content.chars().take(2000).collect();
                crate::embeddings::embed(&format!("{name} {snippet}"))
            })
            .collect();
        let centrality: Vec<f32> = (0..entries.len())
            .map(|i| {
                let mut s = 0f32;
                for j in 0..embeds.len() {
                    if i != j {
                        s += crate::embeddings::cosine(&embeds[i], &embeds[j]);
                    }
                }
                s
            })
            .collect();

        let mut order: Vec<usize> = (0..entries.len()).collect();
        order.sort_by(|&a, &b| {
            entries[b]
                .priority
                .cmp(&entries[a].priority)
                .then(
                    centrality[b]
                        .partial_cmp(&centrality[a])
                        .unwrap_or(std::cmp::Ordering::Equal),
                )
                .then(entries[b].content.len().cmp(&entries[a].content.len()))
        });
        let mut ranked: Vec<ContextEntry> = order.into_iter().map(|i| entries[i].clone()).collect();
        ranked.truncate(8);
        Ok(ranked)
    }

    fn render(&self, entries: &[ContextEntry]) -> String {
        if entries.is_empty() {
            return "# Dynamic Context\n\nNo relevant context found via semantic search.\n"
                .to_string();
        }

        let mut rendered = String::from("# Dynamic Context\n\n");
        for entry in entries {
            rendered.push_str(&format!(
                "## {}\n\n{}\n\n---\n\n",
                entry.path.display(),
                entry.content
            ));
        }
        rendered
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;
    use tempfile::TempDir;

    #[test]
    fn test_project_context_manager_discovery() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();

        // Create PISCI.md
        fs::write(root.join("PISCI.md"), "Project Rules").unwrap();

        // Create .pisci/rules.md
        fs::create_dir_all(root.join(".pisci")).unwrap();
        fs::write(root.join(".pisci").join("rules.md"), "Detailed Rules").unwrap();

        // Create kb/architecture/design.md
        fs::create_dir_all(root.join("kb").join("architecture")).unwrap();
        fs::write(
            root.join("kb").join("architecture").join("design.md"),
            "Design Doc",
        )
        .unwrap();

        let manager = ProjectContextManager;
        let entries = manager.discover(root).unwrap();

        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].priority, 100); // PISCI.md
        assert_eq!(entries[1].priority, 90); // rules.md
        assert_eq!(entries[2].priority, 80); // design.md
    }

    #[test]
    fn test_workspace_root_manager_discovery() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();

        fs::write(root.join("README.md"), "Readme Content").unwrap();
        fs::write(root.join("PISCI.md"), "Pisci Content").unwrap();

        let manager = WorkspaceRootManager;
        let entries = manager.discover(root).unwrap();

        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].priority, 100); // PISCI.md
        assert_eq!(entries[1].priority, 50); // README.md
    }

    #[test]
    fn test_rendering() {
        let entries = vec![ContextEntry {
            path: PathBuf::from("test.md"),
            content: "Test Content".to_string(),
            priority: 10,
        }];

        let manager = ProjectContextManager;
        let rendered = manager.render(&entries);
        assert!(rendered.contains("# Project Context"));
        assert!(rendered.contains("Test Content"));
    }
}
