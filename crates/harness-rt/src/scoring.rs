//! Native scorers for the evaluation pipeline.
//!
//! Eight deterministic scorers grade a prediction against a reference answer and
//! return a score in `[0.0, 1.0]`. They are pure (no network) so evals are
//! reproducible and unit-testable. The `llm_judge` scorer is implemented as a
//! deterministic semantic-overlap heuristic rather than an external model call,
//! keeping the whole pipeline offline-capable.

use serde_json::Value;

/// All scorer names this module understands.
pub const SCORERS: &[&str] = &[
    "exact_match",
    "f1",
    "numeric",
    "contains",
    "mcq_letter",
    "rubric_overlap",
    "llm_judge",
    "weighted_rubric",
];

/// Score `prediction` against `reference` using the named scorer.
///
/// Unknown scorer names fall back to `f1` (a sane, lenient default) so an eval
/// never panics, but callers should validate names against [`SCORERS`].
pub fn score(scorer: &str, prediction: &str, reference: &str, params: &Value) -> f64 {
    match scorer {
        "exact_match" => exact_match(prediction, reference),
        "f1" => f1(prediction, reference),
        "numeric" => numeric(prediction, reference, params),
        "contains" => contains(prediction, reference),
        "mcq_letter" => mcq_letter(prediction, reference),
        "rubric_overlap" => rubric_overlap(prediction, reference, params),
        "llm_judge" => llm_judge(prediction, reference),
        "weighted_rubric" => weighted_rubric(prediction, reference, params),
        // Fall through to contributed (Architect-authored) scorers before the
        // lenient f1 default.
        other => crate::agent::contrib::score_with_contrib(other, prediction, reference, params)
            .unwrap_or_else(|| f1(prediction, reference)),
    }
}

/// Whether `name` is a known scorer — a built-in or a contributed one.
pub fn is_known_scorer(name: &str) -> bool {
    SCORERS.contains(&name) || crate::agent::contrib::has_scorer(name)
}

fn normalize(s: &str) -> String {
    s.trim()
        .to_lowercase()
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c.is_whitespace() {
                c
            } else {
                ' '
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn tokens(s: &str) -> Vec<String> {
    normalize(s)
        .split_whitespace()
        .map(|t| t.to_string())
        .collect()
}

pub fn exact_match(prediction: &str, reference: &str) -> f64 {
    if normalize(prediction) == normalize(reference) {
        1.0
    } else {
        0.0
    }
}

pub fn f1(prediction: &str, reference: &str) -> f64 {
    let pred = tokens(prediction);
    let refr = tokens(reference);
    if pred.is_empty() && refr.is_empty() {
        return 1.0;
    }
    if pred.is_empty() || refr.is_empty() {
        return 0.0;
    }
    // Multiset overlap.
    let mut ref_counts: std::collections::HashMap<&str, i32> = std::collections::HashMap::new();
    for t in &refr {
        *ref_counts.entry(t.as_str()).or_insert(0) += 1;
    }
    let mut overlap = 0i32;
    for t in &pred {
        if let Some(c) = ref_counts.get_mut(t.as_str()) {
            if *c > 0 {
                *c -= 1;
                overlap += 1;
            }
        }
    }
    if overlap == 0 {
        return 0.0;
    }
    let precision = overlap as f64 / pred.len() as f64;
    let recall = overlap as f64 / refr.len() as f64;
    2.0 * precision * recall / (precision + recall)
}

fn first_number(s: &str) -> Option<f64> {
    let mut buf = String::new();
    let mut found = false;
    for c in s.chars() {
        if c.is_ascii_digit() || c == '.' || c == '-' || c == '+' {
            buf.push(c);
            found = true;
        } else if found {
            break;
        }
    }
    buf.parse::<f64>().ok()
}

pub fn numeric(prediction: &str, reference: &str, params: &Value) -> f64 {
    let (Some(p), Some(r)) = (first_number(prediction), first_number(reference)) else {
        return 0.0;
    };
    let tol = params
        .get("tolerance")
        .and_then(|v| v.as_f64())
        .unwrap_or(1e-6);
    if (p - r).abs() <= tol {
        return 1.0;
    }
    // Graded closeness relative to the reference magnitude.
    let denom = r.abs().max(1.0);
    let rel_err = (p - r).abs() / denom;
    (1.0 - rel_err).max(0.0)
}

pub fn contains(prediction: &str, reference: &str) -> f64 {
    let p = normalize(prediction);
    let r = normalize(reference);
    if r.is_empty() {
        return 0.0;
    }
    if p.contains(&r) || r.contains(&p) {
        1.0
    } else {
        0.0
    }
}

fn letters(s: &str) -> std::collections::BTreeSet<char> {
    // Pull standalone A-E choice letters (case-insensitive).
    let norm = s.to_lowercase();
    let mut out = std::collections::BTreeSet::new();
    let bytes: Vec<char> = norm.chars().collect();
    for (i, &c) in bytes.iter().enumerate() {
        if ('a'..='e').contains(&c) {
            let prev_ok = i == 0 || !bytes[i - 1].is_alphanumeric();
            let next_ok = i + 1 >= bytes.len() || !bytes[i + 1].is_alphanumeric();
            if prev_ok && next_ok {
                out.insert(c);
            }
        }
    }
    out
}

pub fn mcq_letter(prediction: &str, reference: &str) -> f64 {
    let p = letters(prediction);
    let r = letters(reference);
    if r.is_empty() {
        return 0.0;
    }
    if p == r {
        1.0
    } else {
        0.0
    }
}

fn keywords_from(reference: &str, params: &Value) -> Vec<String> {
    if let Some(arr) = params.get("keywords").and_then(|v| v.as_array()) {
        let kws: Vec<String> = arr
            .iter()
            .filter_map(|v| v.as_str())
            .map(|s| s.to_lowercase())
            .collect();
        if !kws.is_empty() {
            return kws;
        }
    }
    // Default: significant reference tokens (length > 3).
    tokens(reference)
        .into_iter()
        .filter(|t| t.len() > 3)
        .collect()
}

pub fn rubric_overlap(prediction: &str, reference: &str, params: &Value) -> f64 {
    let kws = keywords_from(reference, params);
    if kws.is_empty() {
        return exact_match(prediction, reference);
    }
    let p = normalize(prediction);
    let hit = kws.iter().filter(|k| p.contains(k.as_str())).count();
    hit as f64 / kws.len() as f64
}

/// Deterministic stand-in for an LLM grader: a blend of token-F1 and keyword
/// recall, rewarding answers that both overlap lexically and cover the key
/// concepts of the reference.
pub fn llm_judge(prediction: &str, reference: &str) -> f64 {
    let lexical = f1(prediction, reference);
    let recall = rubric_overlap(prediction, reference, &Value::Null);
    let exact = exact_match(prediction, reference);
    (0.5 * lexical + 0.5 * recall).max(exact)
}

/// Multi-criteria weighted rubric. `params.criteria` is an array of
/// `{ keywords: [..], weight: f64 }`; the score is the weighted fraction of
/// criteria whose keywords appear in the prediction.
pub fn weighted_rubric(prediction: &str, reference: &str, params: &Value) -> f64 {
    let Some(criteria) = params.get("criteria").and_then(|v| v.as_array()) else {
        return rubric_overlap(prediction, reference, params);
    };
    if criteria.is_empty() {
        return rubric_overlap(prediction, reference, params);
    }
    let p = normalize(prediction);
    let mut total_w = 0.0;
    let mut got = 0.0;
    for crit in criteria {
        let weight = crit.get("weight").and_then(|v| v.as_f64()).unwrap_or(1.0);
        total_w += weight;
        let kws: Vec<String> = crit
            .get("keywords")
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str())
                    .map(|s| s.to_lowercase())
                    .collect()
            })
            .unwrap_or_default();
        if kws.is_empty() {
            continue;
        }
        let hit = kws.iter().filter(|k| p.contains(k.as_str())).count();
        let frac = hit as f64 / kws.len() as f64;
        got += weight * frac;
    }
    if total_w == 0.0 {
        return 0.0;
    }
    got / total_w
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn exact_and_contains() {
        assert_eq!(exact_match("Paris", " paris "), 1.0);
        assert_eq!(exact_match("Paris", "London"), 0.0);
        assert_eq!(contains("The capital is Paris.", "paris"), 1.0);
        assert_eq!(contains("London", "paris"), 0.0);
    }

    #[test]
    fn f1_partial() {
        let s = f1("the quick brown fox", "quick brown");
        assert!(s > 0.5 && s < 1.0, "got {s}");
        assert_eq!(f1("a b c", "a b c"), 1.0);
        assert_eq!(f1("x y z", "a b c"), 0.0);
    }

    #[test]
    fn numeric_tolerance() {
        assert_eq!(numeric("42", "42", &Value::Null), 1.0);
        assert_eq!(numeric("the answer is 42", "42", &Value::Null), 1.0);
        assert!(numeric("40", "42", &Value::Null) < 1.0);
    }

    #[test]
    fn mcq() {
        assert_eq!(mcq_letter("The answer is (B)", "B"), 1.0);
        assert_eq!(mcq_letter("A", "B"), 0.0);
    }

    #[test]
    fn rubric_and_weighted() {
        let p = json!({ "keywords": ["mitochondria", "energy"] });
        assert_eq!(rubric_overlap("mitochondria produce energy", "", &p), 1.0);
        let w = json!({ "criteria": [
            { "keywords": ["mitochondria"], "weight": 2.0 },
            { "keywords": ["nucleus"], "weight": 1.0 }
        ]});
        let s = weighted_rubric("the mitochondria is here", "", &w);
        assert!((s - (2.0 / 3.0)).abs() < 1e-9, "got {s}");
    }

    #[test]
    fn judge_blend() {
        assert_eq!(llm_judge("Paris", "Paris"), 1.0);
        assert!(llm_judge("the capital city is paris", "paris is the capital") > 0.4);
    }

    #[test]
    fn dispatch() {
        assert_eq!(score("exact_match", "a", "a", &Value::Null), 1.0);
        assert_eq!(score("unknown", "a b", "a b", &Value::Null), 1.0);
    }
}
