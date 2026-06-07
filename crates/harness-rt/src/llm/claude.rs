//! Anthropic Claude (Messages API) client.

use async_trait::async_trait;
use serde_json::{json, Value};

use super::{
    ContentBlock, LlmClient, LlmError, LlmMessage, LlmRequest, LlmResponse, MessageContent,
    ToolCall,
};

/// A client for the Anthropic Messages API.
pub struct ClaudeClient {
    api_key: String,
    base_url: String,
    http: reqwest::Client,
}

impl ClaudeClient {
    pub fn new(api_key: &str, base_url: &str, timeout_secs: u64) -> Self {
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(timeout_secs))
            .build()
            .unwrap_or_default();
        Self {
            api_key: api_key.to_string(),
            base_url: base_url.trim_end_matches('/').to_string(),
            http,
        }
    }
}

/// Convert our history into Anthropic message blocks.
fn to_claude_messages(messages: &[LlmMessage]) -> Vec<Value> {
    let mut out: Vec<Value> = Vec::new();
    for m in messages {
        match (&m.role[..], &m.content) {
            ("assistant", MessageContent::Blocks(blocks)) => {
                let mut content: Vec<Value> = Vec::new();
                for b in blocks {
                    match b {
                        ContentBlock::Text { text } if !text.is_empty() => {
                            content.push(json!({ "type": "text", "text": text }));
                        }
                        ContentBlock::ToolUse { id, name, input } => {
                            content.push(json!({
                                "type": "tool_use",
                                "id": id,
                                "name": name,
                                "input": input,
                            }));
                        }
                        _ => {}
                    }
                }
                out.push(json!({ "role": "assistant", "content": content }));
            }
            ("tool", MessageContent::Blocks(blocks)) => {
                let mut content: Vec<Value> = Vec::new();
                for b in blocks {
                    if let ContentBlock::ToolResult {
                        tool_use_id,
                        content: c,
                        is_error,
                    } = b
                    {
                        content.push(json!({
                            "type": "tool_result",
                            "tool_use_id": tool_use_id,
                            "content": c,
                            "is_error": is_error,
                        }));
                    }
                }
                out.push(json!({ "role": "user", "content": content }));
            }
            (role, content) => {
                let mapped = if role == "assistant" {
                    "assistant"
                } else {
                    "user"
                };
                out.push(json!({ "role": mapped, "content": content.as_text() }));
            }
        }
    }
    out
}

#[async_trait]
impl LlmClient for ClaudeClient {
    async fn complete(&self, req: LlmRequest) -> Result<LlmResponse, LlmError> {
        if self.api_key.trim().is_empty() {
            return Err(LlmError::Config("missing API key".into()));
        }

        let tools: Vec<Value> = req
            .tools
            .iter()
            .map(|t| {
                json!({
                    "name": t.name,
                    "description": t.description,
                    "input_schema": t.parameters,
                })
            })
            .collect();

        let mut body = json!({
            "model": req.model,
            "max_tokens": if req.max_tokens > 0 { req.max_tokens } else { 4096 },
            "messages": to_claude_messages(&req.messages),
            "temperature": req.temperature,
        });
        if !req.system.trim().is_empty() {
            body["system"] = Value::String(req.system.clone());
        }
        if !tools.is_empty() {
            body["tools"] = Value::Array(tools);
        }

        let resp = self
            .http
            .post(format!("{}/v1/messages", self.base_url))
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .json(&body)
            .send()
            .await
            .map_err(|e| LlmError::Http(e.to_string()))?;

        let status = resp.status();
        let text = resp
            .text()
            .await
            .map_err(|e| LlmError::Http(e.to_string()))?;
        if !status.is_success() {
            return Err(LlmError::Api {
                status: status.as_u16(),
                body: text.chars().take(500).collect(),
            });
        }

        let v: Value = serde_json::from_str(&text).map_err(|e| LlmError::Decode(e.to_string()))?;
        parse_response(&v)
    }
}

fn parse_response(v: &Value) -> Result<LlmResponse, LlmError> {
    let mut text = String::new();
    let mut tool_calls = Vec::new();
    if let Some(content) = v.get("content").and_then(|c| c.as_array()) {
        for block in content {
            match block.get("type").and_then(|t| t.as_str()) {
                Some("text") => {
                    if let Some(t) = block.get("text").and_then(|t| t.as_str()) {
                        text.push_str(t);
                    }
                }
                Some("tool_use") => {
                    tool_calls.push(ToolCall {
                        id: block
                            .get("id")
                            .and_then(|i| i.as_str())
                            .unwrap_or("")
                            .to_string(),
                        name: block
                            .get("name")
                            .and_then(|n| n.as_str())
                            .unwrap_or("")
                            .to_string(),
                        arguments: block.get("input").cloned().unwrap_or(Value::Null),
                    });
                }
                _ => {}
            }
        }
    }

    let usage = v.get("usage");
    let input_tokens = usage
        .and_then(|u| u.get("input_tokens"))
        .and_then(|n| n.as_u64())
        .unwrap_or(0) as u32;
    let output_tokens = usage
        .and_then(|u| u.get("output_tokens"))
        .and_then(|n| n.as_u64())
        .unwrap_or(0) as u32;

    Ok(LlmResponse {
        text,
        tool_calls,
        input_tokens,
        output_tokens,
    })
}
