//! OpenAI-compatible chat-completions client (also covers qwen/deepseek/etc.).

use async_trait::async_trait;
use serde_json::{json, Value};

use super::{
    ContentBlock, LlmClient, LlmError, LlmMessage, LlmRequest, LlmResponse, MessageContent,
    ToolCall,
};

/// A client for any OpenAI chat-completions compatible endpoint.
pub struct OpenAiClient {
    api_key: String,
    base_url: String,
    http: reqwest::Client,
}

impl OpenAiClient {
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

    fn endpoint(&self) -> String {
        format!("{}/chat/completions", self.base_url)
    }
}

/// Convert our message history into OpenAI wire messages.
fn to_wire_messages(system: &str, messages: &[LlmMessage]) -> Vec<Value> {
    let mut wire: Vec<Value> = Vec::new();
    if !system.trim().is_empty() {
        wire.push(json!({ "role": "system", "content": system }));
    }
    for m in messages {
        match (&m.role[..], &m.content) {
            ("tool", MessageContent::Blocks(blocks)) => {
                for b in blocks {
                    if let ContentBlock::ToolResult {
                        tool_use_id,
                        content,
                        ..
                    } = b
                    {
                        wire.push(json!({
                            "role": "tool",
                            "tool_call_id": tool_use_id,
                            "content": content,
                        }));
                    }
                }
            }
            ("assistant", MessageContent::Blocks(blocks)) => {
                let mut text = String::new();
                let mut tool_calls: Vec<Value> = Vec::new();
                for b in blocks {
                    match b {
                        ContentBlock::Text { text: t } => text.push_str(t),
                        ContentBlock::ToolUse { id, name, input } => {
                            tool_calls.push(json!({
                                "id": id,
                                "type": "function",
                                "function": {
                                    "name": name,
                                    "arguments": serde_json::to_string(input).unwrap_or_else(|_| "{}".into()),
                                },
                            }));
                        }
                        ContentBlock::ToolResult { .. } => {}
                    }
                }
                let mut msg = json!({ "role": "assistant" });
                msg["content"] = if text.is_empty() {
                    Value::Null
                } else {
                    Value::String(text)
                };
                if !tool_calls.is_empty() {
                    msg["tool_calls"] = Value::Array(tool_calls);
                }
                wire.push(msg);
            }
            (role, content) => {
                wire.push(json!({ "role": role, "content": content.as_text() }));
            }
        }
    }
    wire
}

fn to_wire_tools(req: &LlmRequest) -> Vec<Value> {
    req.tools
        .iter()
        .map(|t| {
            json!({
                "type": "function",
                "function": {
                    "name": t.name,
                    "description": t.description,
                    "parameters": t.parameters,
                },
            })
        })
        .collect()
}

#[async_trait]
impl LlmClient for OpenAiClient {
    async fn complete(&self, req: LlmRequest) -> Result<LlmResponse, LlmError> {
        if self.api_key.trim().is_empty() {
            return Err(LlmError::Config("missing API key".into()));
        }

        let mut body = json!({
            "model": req.model,
            "messages": to_wire_messages(&req.system, &req.messages),
            "temperature": req.temperature,
        });
        if req.max_tokens > 0 {
            body["max_tokens"] = json!(req.max_tokens);
        }
        let tools = to_wire_tools(&req);
        if !tools.is_empty() {
            body["tools"] = Value::Array(tools);
            body["tool_choice"] = json!("auto");
        }

        let resp = self
            .http
            .post(self.endpoint())
            .bearer_auth(&self.api_key)
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
    let choice = v
        .get("choices")
        .and_then(|c| c.as_array())
        .and_then(|a| a.first())
        .ok_or_else(|| LlmError::Decode("no choices in response".into()))?;
    let message = choice
        .get("message")
        .ok_or_else(|| LlmError::Decode("no message in choice".into()))?;

    let text = message
        .get("content")
        .and_then(|c| c.as_str())
        .unwrap_or("")
        .to_string();

    let mut tool_calls = Vec::new();
    if let Some(calls) = message.get("tool_calls").and_then(|c| c.as_array()) {
        for c in calls {
            let id = c
                .get("id")
                .and_then(|i| i.as_str())
                .unwrap_or("")
                .to_string();
            let func = c.get("function");
            let name = func
                .and_then(|f| f.get("name"))
                .and_then(|n| n.as_str())
                .unwrap_or("")
                .to_string();
            let args_str = func
                .and_then(|f| f.get("arguments"))
                .and_then(|a| a.as_str())
                .unwrap_or("{}");
            let arguments: Value = serde_json::from_str(args_str).unwrap_or(Value::Null);
            tool_calls.push(ToolCall {
                id,
                name,
                arguments,
            });
        }
    }

    let usage = v.get("usage");
    let input_tokens = usage
        .and_then(|u| u.get("prompt_tokens"))
        .and_then(|n| n.as_u64())
        .unwrap_or(0) as u32;
    let output_tokens = usage
        .and_then(|u| u.get("completion_tokens"))
        .and_then(|n| n.as_u64())
        .unwrap_or(0) as u32;

    Ok(LlmResponse {
        text,
        tool_calls,
        input_tokens,
        output_tokens,
    })
}
