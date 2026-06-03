//! Context Manager implementations for Agent Workbench.

use crate::plugins::{ContextManager, ContextEntry};
use std::path::Path;
use anyhow::{Result, Context};
use glob::glob;

/// Adapter that exposes a workbench [`ContextManager`] through the kernel's
/// [`piscis_kernel::context::ContextManager`] trait so it can be wired into the
/// [`HarnessConfig`](piscis_kernel::agent::harness::HarnessConfig) context slot.
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

impl piscis_kernel::context::ContextManager for KernelContextAdapter {
    fn name(&self) -> &str {
        &self.name
    }

    fn discover(
        &self,
        _root: &Path,
    ) -> std::io::Result<Vec<piscis_kernel::context::ProjectInstructionFile>> {
        Ok(Vec::new())
    }

    fn render(&self, root: &Path, budget_chars: usize) -> std::io::Result<String> {
        let entries = self
            .inner
            .discover(root)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;
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
            for entry in glob(pattern_str).context("Failed to read glob pattern")? {
                if let Ok(path) = entry {
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
        }

        // Sort by priority descending
        entries.sort_by(|a, b| b.priority.cmp(&a.priority));
        Ok(entries)
    }

    fn render(&self, entries: &[ContextEntry]) -> String {
        if entries.is_empty() {
            return String::new();
        }

        let mut rendered = String::from("# Project Context\n\n");
        for entry in entries {
            rendered.push_str(&format!("## {}\n\n{}\n\n---\n\n", 
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
        entries.sort_by(|a, b| b.priority.cmp(&a.priority));
        Ok(entries)
    }

    fn render(&self, entries: &[ContextEntry]) -> String {
        if entries.is_empty() {
            return String::new();
        }

        let mut rendered = String::from("# Workspace Context\n\n");
        for entry in entries {
            rendered.push_str(&format!("## {}\n\n{}\n\n---\n\n", 
                entry.path.display(), 
                entry.content
            ));
        }
        rendered
    }
}

/// Dynamic context manager using semantic search (stub implementation).
pub struct DynamicContextManager;

impl ContextManager for DynamicContextManager {
    fn discover(&self, _root: &Path) -> Result<Vec<ContextEntry>> {
        // Stub: In a real implementation, this would use an embedding model
        // to search a vector store. For now, we return empty or a fixed set.
        Ok(Vec::new())
    }

    fn render(&self, entries: &[ContextEntry]) -> String {
        if entries.is_empty() {
            return "# Dynamic Context\n\nNo relevant context found via semantic search.\n".to_string();
        }
        
        let mut rendered = String::from("# Dynamic Context\n\n");
        for entry in entries {
            rendered.push_str(&format!("## {}\n\n{}\n\n---\n\n", 
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
    use std::path::PathBuf;
    use std::fs;
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
        fs::write(root.join("kb").join("architecture").join("design.md"), "Design Doc").unwrap();

        let manager = ProjectContextManager;
        let entries = manager.discover(root).unwrap();

        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].priority, 100); // PISCI.md
        assert_eq!(entries[1].priority, 90);  // rules.md
        assert_eq!(entries[2].priority, 80);  // design.md
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
        assert_eq!(entries[1].priority, 50);  // README.md
    }

    #[test]
    fn test_rendering() {
        let entries = vec![
            ContextEntry {
                path: PathBuf::from("test.md"),
                content: "Test Content".to_string(),
                priority: 10,
            }
        ];

        let manager = ProjectContextManager;
        let rendered = manager.render(&entries);
        assert!(rendered.contains("# Project Context"));
        assert!(rendered.contains("Test Content"));
    }
}
