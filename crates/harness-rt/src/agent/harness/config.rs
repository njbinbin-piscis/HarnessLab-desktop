//! Harness configuration value types.

/// Confirmation requirements for side-effecting tools.
#[derive(Debug, Clone, Copy, Default)]
pub struct ConfirmFlags {
    pub confirm_shell: bool,
    pub confirm_file_write: bool,
}

/// Compaction tier thresholds and caps (separate from the pluggable strategy).
#[derive(Debug, Clone)]
pub struct CompactionSettings {
    pub micro_percent: u8,
    pub auto_percent: u8,
    pub full_percent: u8,
    pub max_tool_result_tokens: u32,
    pub summary_model: Option<String>,
}

impl Default for CompactionSettings {
    fn default() -> Self {
        Self {
            micro_percent: 50,
            auto_percent: 70,
            full_percent: 90,
            max_tool_result_tokens: 4000,
            summary_model: None,
        }
    }
}
