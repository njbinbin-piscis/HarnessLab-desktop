//! Text embeddings for semantic ranking.
//!
//! Two always-available local backends — `hashing` (feature-hashed bag of words)
//! and `bow` (sublinear term-frequency over the same hashed space) — plus an
//! optional OpenAI-compatible `/embeddings` API backend resolved from the same
//! environment credentials the runner uses. Everything degrades to local so the
//! market search / context ranking work offline.

use std::time::Duration;

use serde::Serialize;
use serde_json::json;

use crate::runner::LlmCreds;

/// Dimensionality of the local hashed embedding space.
pub const DIM: usize = 256;

fn tokenize(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.len() >= 2)
        .map(|w| w.to_lowercase())
        .collect()
}

fn fnv1a(s: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in s.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

fn l2_normalize(v: &mut [f32]) {
    let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 0.0 {
        for x in v.iter_mut() {
            *x /= norm;
        }
    }
}

/// Feature-hashing bag of words (default local backend).
pub fn embed_hashing(text: &str) -> Vec<f32> {
    let mut v = vec![0f32; DIM];
    for tok in tokenize(text) {
        let i = (fnv1a(&tok) as usize) % DIM;
        v[i] += 1.0;
    }
    l2_normalize(&mut v);
    v
}

/// Bag of words with sublinear term-frequency (1 + ln tf) weighting.
pub fn embed_bow(text: &str) -> Vec<f32> {
    use std::collections::HashMap;
    let mut counts: HashMap<String, f32> = HashMap::new();
    for tok in tokenize(text) {
        *counts.entry(tok).or_default() += 1.0;
    }
    let mut v = vec![0f32; DIM];
    for (tok, c) in counts {
        let i = (fnv1a(&tok) as usize) % DIM;
        v[i] += 1.0 + c.ln();
    }
    l2_normalize(&mut v);
    v
}

/// Default local embedding used for ranking when no API backend is selected.
pub fn embed(text: &str) -> Vec<f32> {
    embed_hashing(text)
}

/// Cosine similarity. Inputs are expected L2-normalized, so this is a dot
/// product; it still works (just unnormalized) on raw vectors.
pub fn cosine(a: &[f32], b: &[f32]) -> f32 {
    let n = a.len().min(b.len());
    let mut dot = 0f32;
    for i in 0..n {
        dot += a[i] * b[i];
    }
    dot
}

/// Rank `items` by descending similarity of their text to `query` using local
/// embeddings. Returns indices into `items` ordered best-first. Empty `query`
/// preserves the original order.
pub fn rank_by_query<'a, I>(query: &str, items: I) -> Vec<usize>
where
    I: IntoIterator<Item = &'a str>,
{
    let texts: Vec<&str> = items.into_iter().collect();
    let mut idx: Vec<usize> = (0..texts.len()).collect();
    if query.trim().is_empty() {
        return idx;
    }
    let q = embed(query);
    let scores: Vec<f32> = texts.iter().map(|t| cosine(&q, &embed(t))).collect();
    idx.sort_by(|&a, &b| {
        scores[b]
            .partial_cmp(&scores[a])
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    idx
}

#[derive(Clone, Serialize)]
pub struct Probe {
    pub ok: bool,
    pub dim: Option<usize>,
    pub note: Option<String>,
    pub reason: Option<String>,
}

#[derive(Clone, Serialize)]
pub struct EmbeddingStatus {
    pub backends: Vec<String>,
    pub api_configured: bool,
    pub base_url: String,
    pub model: String,
    pub probe: Option<Probe>,
}

/// Embedding model id used for the API backend.
pub fn embed_model() -> String {
    std::env::var("PISCIS_EMBED_MODEL").unwrap_or_else(|_| "text-embedding-3-small".to_string())
}

fn api_base(creds: &LlmCreds) -> String {
    creds
        .base_url
        .clone()
        .unwrap_or_else(|| match creds.provider.to_lowercase().as_str() {
            "qwen" => "https://dashscope.aliyuncs.com/compatible-mode/v1".to_string(),
            "deepseek" => "https://api.deepseek.com/v1".to_string(),
            _ => "https://api.openai.com/v1".to_string(),
        })
}

/// Call an OpenAI-compatible `/embeddings` endpoint (blocking). Errors if no
/// credentials are configured or the request fails.
pub fn embed_api(text: &str) -> Result<Vec<f32>, String> {
    let creds = LlmCreds::from_env()?;
    let base = api_base(&creds);
    let url = format!("{}/embeddings", base.trim_end_matches('/'));
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(10))
        .timeout_read(Duration::from_secs(20))
        .build();
    let payload = json!({ "model": embed_model(), "input": text }).to_string();
    let resp = agent
        .post(&url)
        .set("Authorization", &format!("Bearer {}", creds.api_key))
        .set("Content-Type", "application/json")
        .send_string(&payload)
        .map_err(|e| e.to_string())?;
    let resp_text = resp.into_string().map_err(|e| e.to_string())?;
    let body: serde_json::Value = serde_json::from_str(&resp_text).map_err(|e| e.to_string())?;
    let arr = body
        .get("data")
        .and_then(|d| d.get(0))
        .and_then(|d| d.get("embedding"))
        .and_then(|e| e.as_array())
        .ok_or("no embedding in response")?;
    Ok(arr
        .iter()
        .filter_map(|x| x.as_f64().map(|f| f as f32))
        .collect())
}

/// Report the available embedding backends and, when `probe` is set, exercise
/// the best backend (API if configured, else local) and report the result.
pub fn status(probe: bool) -> EmbeddingStatus {
    let creds = LlmCreds::from_env().ok();
    let api_configured = creds.is_some();
    let base_url = creds.as_ref().map(api_base).unwrap_or_default();
    let model = embed_model();
    let mut backends = vec!["hashing".to_string(), "bow".to_string()];
    if api_configured {
        backends.push("openai".to_string());
    }
    let probe = if probe {
        if api_configured {
            match embed_api("harnesslab embedding probe") {
                Ok(v) => Some(Probe {
                    ok: true,
                    dim: Some(v.len()),
                    note: Some("api".into()),
                    reason: None,
                }),
                Err(e) => {
                    let v = embed("harnesslab embedding probe");
                    Some(Probe {
                        ok: true,
                        dim: Some(v.len()),
                        note: Some("local-fallback".into()),
                        reason: Some(e),
                    })
                }
            }
        } else {
            let v = embed("harnesslab embedding probe");
            Some(Probe {
                ok: true,
                dim: Some(v.len()),
                note: Some("local".into()),
                reason: None,
            })
        }
    } else {
        None
    };
    EmbeddingStatus {
        backends,
        api_configured,
        base_url,
        model,
        probe,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn similar_text_ranks_first() {
        let items = [
            "a tree-search planning loop",
            "a json formatter",
            "react loop",
        ];
        let order = rank_by_query("planning search loop", items);
        assert_eq!(order[0], 0);
    }

    #[test]
    fn empty_query_keeps_order() {
        let items = ["one", "two", "three"];
        assert_eq!(rank_by_query("", items), vec![0, 1, 2]);
    }
}
