//! Credential resolution for the `credentials` lifecycle slot.

use crate::project::AgentSpec;
use crate::runner::LlmCreds;

/// Resolved LLM connection for one agent run.
#[derive(Debug, Clone)]
pub struct ResolvedLlm {
    pub creds: LlmCreds,
    pub model: String,
}

fn param_str(spec: &AgentSpec, key: &str) -> Option<String> {
    spec.credentials
        .as_ref()
        .and_then(|c| c.params.get(key))
        .and_then(|v| match v {
            serde_json::Value::String(s) if !s.is_empty() => Some(s.clone()),
            _ => None,
        })
}

fn effective_model(spec: &AgentSpec) -> String {
    if !spec.model.is_empty() && spec.model != "mock" {
        return spec.model.clone();
    }
    std::env::var("PISCIS_DEFAULT_MODEL").unwrap_or_else(|_| "qwen-plus".to_string())
}

/// Resolve provider/key/base_url/model for an agent from its `credentials` slot.
///
/// * `credentials/env` or unset → `PISCIS_*` environment variables
/// * `credentials/static` → `api_key` + `base_url` params (optional `provider`, `model`)
pub fn resolve_for_agent(spec: &AgentSpec) -> Result<ResolvedLlm, String> {
    let slot = spec
        .credentials
        .as_ref()
        .map(|c| c.name.as_str())
        .unwrap_or("env");

    match slot {
        "static" => {
            let api_key = param_str(spec, "api_key")
                .ok_or("credentials/static 需要 params.api_key — 在 Agent 编辑器里填写 API Key")?;
            let base_url = param_str(spec, "base_url")
                .ok_or("credentials/static 需要 params.base_url — 填写 OpenAI 兼容 endpoint")?;
            let provider = param_str(spec, "provider")
                .or_else(|| param_str(spec, "name"))
                .unwrap_or_else(|| "custom".to_string());
            let model = param_str(spec, "model").unwrap_or_else(|| effective_model(spec));
            Ok(ResolvedLlm {
                creds: LlmCreds {
                    provider,
                    api_key,
                    base_url: Some(base_url),
                },
                model,
            })
        }
        "env" | _ => {
            let creds = LlmCreds::from_env()?;
            Ok(ResolvedLlm {
                model: effective_model(spec),
                creds,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::ComponentRef;
    use std::collections::BTreeMap;

    #[test]
    fn static_credentials_from_params() {
        let mut params = BTreeMap::new();
        params.insert(
            "api_key".into(),
            serde_json::Value::String("sk-test".into()),
        );
        params.insert(
            "base_url".into(),
            serde_json::Value::String("https://example.com/v1".into()),
        );
        params.insert(
            "provider".into(),
            serde_json::Value::String("openai".into()),
        );
        params.insert(
            "model".into(),
            serde_json::Value::String("gpt-4o-mini".into()),
        );
        let spec = AgentSpec {
            credentials: Some(ComponentRef {
                name: "static".into(),
                params,
            }),
            ..AgentSpec::new("a")
        };
        let r = resolve_for_agent(&spec).expect("static resolve");
        assert_eq!(r.model, "gpt-4o-mini");
        assert_eq!(r.creds.api_key, "sk-test");
        assert_eq!(r.creds.base_url.as_deref(), Some("https://example.com/v1"));
    }
}
