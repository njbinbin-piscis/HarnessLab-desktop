//! Context-compaction strategies.
//!
//! When a conversation approaches the model's context window, the agent loop
//! asks the active [`CompactionStrategy`] to shrink the message history while
//! preserving the most useful content.

use std::sync::Arc;

use crate::llm::{estimate_tokens, LlmMessage};

/// Shrinks a message history to fit a token budget.
pub trait CompactionStrategy: Send + Sync {
    fn name(&self) -> &str;

    /// Return a (possibly reduced) history that fits within `max_tokens`.
    fn compact(&self, messages: Vec<LlmMessage>, max_tokens: u32) -> Vec<LlmMessage>;
}

fn message_tokens(m: &LlmMessage) -> u32 {
    estimate_tokens(&m.content.as_text()) + 4
}

fn total_tokens(messages: &[LlmMessage]) -> u32 {
    messages.iter().map(message_tokens).sum()
}

/// Keep the most recent messages that fit, always retaining the first message.
fn keep_recent(messages: Vec<LlmMessage>, max_tokens: u32) -> Vec<LlmMessage> {
    if total_tokens(&messages) <= max_tokens || messages.len() <= 2 {
        return messages;
    }
    let first = messages.first().cloned();
    let mut budget = max_tokens.saturating_sub(first.as_ref().map(message_tokens).unwrap_or(0));
    let mut tail: Vec<LlmMessage> = Vec::new();
    for m in messages.iter().skip(1).rev() {
        let cost = message_tokens(m);
        if cost > budget {
            break;
        }
        budget -= cost;
        tail.push(m.clone());
    }
    tail.reverse();
    let mut out = Vec::new();
    if let Some(f) = first {
        out.push(f);
    }
    out.extend(tail);
    out
}

/// No compaction — history is returned unchanged.
pub struct NoneCompaction;
impl CompactionStrategy for NoneCompaction {
    fn name(&self) -> &str {
        "none"
    }
    fn compact(&self, messages: Vec<LlmMessage>, _max_tokens: u32) -> Vec<LlmMessage> {
        messages
    }
}

/// Token-budget truncation (no LLM).
pub struct SlidingWindowCompaction;
impl CompactionStrategy for SlidingWindowCompaction {
    fn name(&self) -> &str {
        "sliding_window"
    }
    fn compact(&self, messages: Vec<LlmMessage>, max_tokens: u32) -> Vec<LlmMessage> {
        keep_recent(messages, max_tokens)
    }
}

/// Drop older turns but prepend a lexical synopsis of what was dropped.
pub struct SummaryCompaction {
    prompt: Option<String>,
}
impl CompactionStrategy for SummaryCompaction {
    fn name(&self) -> &str {
        "summary_based"
    }
    fn compact(&self, messages: Vec<LlmMessage>, max_tokens: u32) -> Vec<LlmMessage> {
        if total_tokens(&messages) <= max_tokens || messages.len() <= 2 {
            return messages;
        }
        let kept = keep_recent(messages.clone(), max_tokens.saturating_sub(256).max(256));
        let kept_len = kept.len();
        let dropped = messages.len().saturating_sub(kept_len);
        if dropped == 0 {
            return kept;
        }
        let synopsis: String = messages
            .iter()
            .take(dropped)
            .map(|m| {
                let line: String = m.content.as_text().lines().take(1).collect();
                format!(
                    "- {}: {}",
                    m.role,
                    line.chars().take(120).collect::<String>()
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        let header = self
            .prompt
            .clone()
            .unwrap_or_else(|| "## Earlier conversation (summary)".to_string());
        let mut out = vec![LlmMessage {
            role: "user".to_string(),
            content: crate::llm::MessageContent::text(&format!("{header}\n{synopsis}")),
        }];
        out.extend(kept);
        out
    }
}

/// Keep recent turns plus the older turns most lexically similar to the latest.
pub struct VectorRetrievalCompaction;
impl CompactionStrategy for VectorRetrievalCompaction {
    fn name(&self) -> &str {
        "vector_retrieval"
    }
    fn compact(&self, messages: Vec<LlmMessage>, max_tokens: u32) -> Vec<LlmMessage> {
        if total_tokens(&messages) <= max_tokens || messages.len() <= 3 {
            return messages;
        }
        let query = messages
            .last()
            .map(|m| m.content.as_text())
            .unwrap_or_default();
        let recent_budget = max_tokens / 2;
        let recent = keep_recent(messages.clone(), recent_budget);
        let recent_start = messages.len().saturating_sub(recent.len());
        // Score older messages by token overlap with the query.
        let q_tokens = tokenize(&query);
        let mut scored: Vec<(f64, usize)> = messages
            .iter()
            .take(recent_start)
            .enumerate()
            .map(|(i, m)| (overlap(&tokenize(&m.content.as_text()), &q_tokens), i))
            .collect();
        scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        let mut budget = max_tokens.saturating_sub(total_tokens(&recent));
        let mut keep_idx: Vec<usize> = Vec::new();
        for (score, idx) in scored {
            if score <= 0.0 {
                break;
            }
            let cost = message_tokens(&messages[idx]);
            if cost > budget {
                continue;
            }
            budget -= cost;
            keep_idx.push(idx);
        }
        keep_idx.sort_unstable();
        let mut out: Vec<LlmMessage> = keep_idx.into_iter().map(|i| messages[i].clone()).collect();
        out.extend(recent);
        out
    }
}

fn tokenize(s: &str) -> Vec<String> {
    s.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| !t.is_empty())
        .map(|t| t.to_string())
        .collect()
}

fn overlap(a: &[String], b: &[String]) -> f64 {
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    let hits = a.iter().filter(|t| b.contains(t)).count();
    hits as f64 / (a.len() as f64).sqrt()
}

/// Names of built-in compaction strategies.
pub const BUILTIN_COMPACTIONS: &[&str] = &[
    "none",
    "summary_based",
    "sliding_window",
    "vector_retrieval",
];

/// Resolve a compaction strategy by name, falling through to the contrib registry.
pub fn resolve_compaction_strategy(
    name: &str,
    prompt: Option<String>,
) -> Option<Arc<dyn CompactionStrategy>> {
    match name {
        "none" => Some(Arc::new(NoneCompaction)),
        "summary_based" => Some(Arc::new(SummaryCompaction { prompt })),
        "sliding_window" => Some(Arc::new(SlidingWindowCompaction)),
        "vector_retrieval" => Some(Arc::new(VectorRetrievalCompaction)),
        other => super::contrib::resolve_compaction_strategy(other),
    }
}
