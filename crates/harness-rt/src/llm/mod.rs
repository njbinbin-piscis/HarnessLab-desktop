//! LLM client abstraction and provider routing.
//!
//! [`LlmClient`] is the single injection point the [`AgentLoop`](crate::agent::loop_::AgentLoop)
//! talks to. [`build_client_with_timeout`] routes a provider id to a concrete
//! client: every OpenAI-compatible provider (openai/custom/qwen/deepseek/…)
//! uses [`OpenAiClient`], and Anthropic uses [`ClaudeClient`].

mod claude;
mod openai;

pub use claude::ClaudeClient;
pub use openai::OpenAiClient;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A single message in a conversation.
///
/// `content` carries either plain text (the common case for user turns) or a
/// list of [`ContentBlock`]s when the message includes tool calls / results.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmMessage {
    pub role: String,
    pub content: MessageContent,
}

/// Message payload — plain text or a structured block list.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MessageContent {
    Text(String),
    Blocks(Vec<ContentBlock>),
}

/// A structured content block (tool call / tool result / text segment).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ContentBlock {
    Text {
        text: String,
    },
    ToolUse {
        id: String,
        name: String,
        input: Value,
    },
    ToolResult {
        tool_use_id: String,
        content: String,
        #[serde(default)]
        is_error: bool,
    },
}

impl MessageContent {
    /// Build a plain-text content payload.
    pub fn text(s: &str) -> Self {
        MessageContent::Text(s.to_string())
    }

    /// Flatten the content to a plain string (text blocks concatenated).
    pub fn as_text(&self) -> String {
        match self {
            MessageContent::Text(t) => t.clone(),
            MessageContent::Blocks(blocks) => {
                let mut out = String::new();
                for b in blocks {
                    if let ContentBlock::Text { text } = b {
                        out.push_str(text);
                    }
                }
                out
            }
        }
    }

    /// Borrow the structured blocks, if any.
    pub fn blocks(&self) -> Option<&[ContentBlock]> {
        match self {
            MessageContent::Blocks(b) => Some(b),
            MessageContent::Text(_) => None,
        }
    }
}

/// A tool invocation requested by the model.
#[derive(Debug, Clone)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: Value,
}

/// Tool description advertised to the model (OpenAI function schema).
#[derive(Debug, Clone)]
pub struct ToolSchema {
    pub name: String,
    pub description: String,
    pub parameters: Value,
}

/// A completion request.
pub struct LlmRequest {
    pub model: String,
    pub system: String,
    pub messages: Vec<LlmMessage>,
    pub tools: Vec<ToolSchema>,
    pub max_tokens: u32,
    pub temperature: f32,
}

/// A completion response.
#[derive(Debug, Clone, Default)]
pub struct LlmResponse {
    pub text: String,
    pub tool_calls: Vec<ToolCall>,
    pub input_tokens: u32,
    pub output_tokens: u32,
}

/// Errors raised by an LLM client.
#[derive(Debug, thiserror::Error)]
pub enum LlmError {
    #[error("http error: {0}")]
    Http(String),
    #[error("api error ({status}): {body}")]
    Api { status: u16, body: String },
    #[error("decode error: {0}")]
    Decode(String),
    #[error("configuration error: {0}")]
    Config(String),
}

/// The single abstraction the agent loop talks to.
#[async_trait]
pub trait LlmClient: Send + Sync {
    async fn complete(&self, req: LlmRequest) -> Result<LlmResponse, LlmError>;
}

/// Estimate a token count for a string (≈4 chars/token) when an API omits usage.
pub fn estimate_tokens(s: &str) -> u32 {
    ((s.chars().count() as f64) / 4.0).ceil() as u32
}

/// Route a provider id to a concrete [`LlmClient`].
///
/// Every OpenAI-compatible provider shares [`OpenAiClient`] with a provider
/// default base URL; the catch-all default maps to Anthropic via
/// [`ClaudeClient`] (mirroring the previous kernel routing).
pub fn build_client_with_timeout(
    provider: &str,
    api_key: &str,
    base_url: Option<&str>,
    timeout_secs: u64,
) -> Box<dyn LlmClient> {
    let provider_lc = provider.trim().to_lowercase();
    let default_base = match provider_lc.as_str() {
        "openai" | "custom" | "ollama" => "https://api.openai.com/v1",
        "deepseek" => "https://api.deepseek.com/v1",
        "qwen" | "tongyi" | "dashscope" => "https://dashscope.aliyuncs.com/compatible-mode/v1",
        "minimax" => "https://api.minimax.chat/v1",
        "zhipu" | "glm" => "https://open.bigmodel.cn/api/paas/v4",
        "kimi" | "moonshot" => "https://api.moonshot.cn/v1",
        _ => "",
    };

    match provider_lc.as_str() {
        // Anthropic Claude uses a distinct Messages API.
        "anthropic" | "claude" => Box::new(ClaudeClient::new(
            api_key,
            base_url.unwrap_or("https://api.anthropic.com"),
            timeout_secs,
        )),
        // Everything else is OpenAI chat-completions compatible.
        _ => {
            let base = base_url
                .filter(|s| !s.is_empty())
                .unwrap_or(if default_base.is_empty() {
                    "https://api.openai.com/v1"
                } else {
                    default_base
                });
            Box::new(OpenAiClient::new(api_key, base, timeout_secs))
        }
    }
}
