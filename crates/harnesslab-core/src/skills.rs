//! Installed skills (ClawHub `SKILL.md` packages) and their runtime injection.
//!
//! Skills are installed into the local store and rendered into the agent system
//! prompt at harness-build time. A process-global "active" list lets the desktop
//! command layer publish the currently-installed skill blocks once (at startup
//! and after install/uninstall) without threading them through every run call.

use std::sync::{OnceLock, RwLock};

use serde::{Deserialize, Serialize};

/// An installed skill package.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledSkill {
    /// ClawHub slug (unique id).
    pub slug: String,
    /// Human-readable skill name (parsed from SKILL.md front-matter / heading).
    pub name: String,
    /// Short description.
    #[serde(default)]
    pub description: String,
    /// Installed version (may be empty / "latest").
    #[serde(default)]
    pub version: String,
    /// The full SKILL.md instruction content.
    pub content: String,
    /// Whether this skill is injected into agent prompts.
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Unix install timestamp (seconds).
    #[serde(default)]
    pub installed_at: f64,
}

fn default_true() -> bool {
    true
}

impl InstalledSkill {
    /// Render this skill as a system-prompt instruction block.
    pub fn render_block(&self) -> String {
        format!(
            "## Skill: {}\n{}\n{}",
            self.name,
            if self.description.is_empty() {
                String::new()
            } else {
                format!("_{}_", self.description)
            },
            self.content.trim()
        )
    }
}

fn active() -> &'static RwLock<Vec<String>> {
    static ACTIVE: OnceLock<RwLock<Vec<String>>> = OnceLock::new();
    ACTIVE.get_or_init(|| RwLock::new(Vec::new()))
}

/// Publish the currently-active (enabled) skill instruction blocks.
pub fn set_active_blocks(blocks: Vec<String>) {
    *active().write().unwrap() = blocks;
}

/// The currently-active skill instruction blocks (injected into prompts).
pub fn active_blocks() -> Vec<String> {
    active().read().unwrap().clone()
}

/// Convenience: publish active blocks from a list of installed skills.
pub fn publish_from(skills: &[InstalledSkill]) {
    let blocks = skills
        .iter()
        .filter(|s| s.enabled)
        .map(|s| s.render_block())
        .collect();
    set_active_blocks(blocks);
}
