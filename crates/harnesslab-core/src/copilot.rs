//! Design Copilot — turn natural-language instructions into validated Edit Ops.
//!
//! The visual designer never hand-edits the project; it *proposes* declarative
//! ops that a human reviews and applies. Ported from HarnessLab's Python
//! `design/copilot.py` (deterministic rule planner + optional LLM fallback).

use serde::Serialize;
use serde_json::{json, Value};

use crate::project::Project;
use crate::registry::{self, tool_names};

/// A copilot proposal before dry-run validation.
#[derive(Debug, Clone, Serialize)]
pub struct CopilotProposal {
    pub ops: Vec<Value>,
    pub explanation: String,
    pub source: String,
}

const ROLE_LOOP: &[(&str, &str)] = &[
    ("critic", "reflexion"),
    ("reviewer", "reflexion"),
    ("judge", "reflexion"),
    ("planner", "plan_and_solve"),
    ("researcher", "react"),
    ("solver", "react"),
    ("coder", "react"),
    ("summarizer", "react"),
];

const ROLE_WORDS: &[&str] = &[
    "critic",
    "reviewer",
    "judge",
    "planner",
    "researcher",
    "solver",
    "coder",
    "summarizer",
    "agent",
    "worker",
    "assistant",
];

fn names(kind: &str) -> Vec<String> {
    registry::catalog()
        .get(kind)
        .map(|v| v.iter().map(|c| c.name.clone()).collect())
        .unwrap_or_default()
}

fn is_word_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'-' || b == b'_'
}

/// Return the longest candidate that appears as a whole word in `text`.
fn find_word(text: &str, candidates: &[String]) -> Option<String> {
    let mut hits: Vec<&String> = candidates.iter().filter(|c| word_match(text, c)).collect();
    hits.sort_by_key(|c| std::cmp::Reverse(c.len()));
    hits.first().map(|s| (*s).clone())
}

fn word_match(text: &str, word: &str) -> bool {
    if word.is_empty() {
        return false;
    }
    let wlen = word.len();
    let tbytes = text.as_bytes();
    for (i, _) in text.match_indices(word) {
        let before = i > 0 && is_word_char(tbytes[i - 1]);
        let after = i + wlen < tbytes.len() && is_word_char(tbytes[i + wlen]);
        if !before && !after {
            return true;
        }
    }
    false
}

fn split_clauses(text: &str) -> Vec<String> {
    let lower = text.to_lowercase();
    let mut parts: Vec<String> = Vec::new();
    let mut start = 0usize;
    let bytes = lower.as_bytes();
    let keywords: &[&[u8]] = &[b";", b"\n", b" then ", b" also ", b" and "];
    let mut i = 0;
    while i < bytes.len() {
        let mut matched = None;
        for kw in keywords {
            if i + kw.len() <= bytes.len() && &bytes[i..i + kw.len()] == *kw {
                matched = Some(kw.len());
                break;
            }
        }
        if let Some(kwlen) = matched {
            let chunk = lower[start..i].trim();
            if !chunk.is_empty() {
                parts.push(chunk.to_string());
            }
            start = i + kwlen;
            i = start;
        } else {
            i += 1;
        }
    }
    let tail = lower[start..].trim();
    if !tail.is_empty() {
        parts.push(tail.to_string());
    }
    if parts.is_empty() && !lower.trim().is_empty() {
        parts.push(lower.trim().to_string());
    }
    parts
}

fn role_loop(role: &str) -> &'static str {
    ROLE_LOOP
        .iter()
        .find(|(r, _)| *r == role)
        .map(|(_, l)| *l)
        .unwrap_or("react")
}

/// Deterministic rule planner — dependency-free, always available.
pub fn propose_ops(project: &Project, instruction: &str) -> CopilotProposal {
    let names_by_kind = registry::names_by_kind();
    let loops = names("loop");
    let compaction = names("compaction");
    let memory = names("memory");
    let context = names("context");
    let coordinators = names("coordinator");
    let scorers = names("scorer");
    let tools: Vec<String> = tool_names().into_iter().collect();

    let mut ops: Vec<Value> = Vec::new();
    let mut notes: Vec<String> = Vec::new();
    let mut known_agents: Vec<String> = project.agents.iter().map(|a| a.name.clone()).collect();

    for clause in split_clauses(instruction) {
        let agent = find_word(&clause, &known_agents);
        let tool = find_word(&clause, &tools);
        let coord = find_word(&clause, &coordinators);
        let scorer = find_word(&clause, &scorers);

        let slot_hit = [
            ("loop", &loops),
            ("compaction", &compaction),
            ("memory", &memory),
            ("context", &context),
        ]
        .iter()
        .find_map(|(kind, names)| find_word(&clause, names).map(|ref_name| (*kind, ref_name)));

        // remove agent
        if clause.contains("remove") || clause.contains("delete") || clause.contains("drop") {
            if clause.contains("agent") {
                let target = clause
                    .split_whitespace()
                    .find(|w| known_agents.iter().any(|a| a == *w))
                    .map(|s| s.to_string())
                    .or(agent.clone());
                if let Some(name) = target {
                    if known_agents.contains(&name) {
                        ops.push(json!({ "op": "remove_agent", "name": name }));
                        known_agents.retain(|a| a != &name);
                        notes.push(format!("Remove agent '{name}'."));
                        continue;
                    }
                }
            }
        }

        // rename agent
        if let Some((from, to)) = parse_rename(&clause) {
            if known_agents.contains(&from) {
                ops.push(json!({ "op": "rename_agent", "name": from, "to": to }));
                if let Some(idx) = known_agents.iter().position(|a| *a == from) {
                    known_agents[idx] = to.clone();
                }
                notes.push(format!("Rename '{from}' to '{to}'."));
                continue;
            }
        }

        // detach tool
        if tool.is_some()
            && agent.is_some()
            && (clause.contains("remove")
                || clause.contains("detach")
                || clause.contains("disallow")
                || clause.contains("disable"))
        {
            let t = tool.unwrap();
            let a = agent.unwrap();
            ops.push(json!({ "op": "detach_tool", "agent": a, "tool": t }));
            notes.push(format!("Detach tool '{t}' from '{a}'."));
            continue;
        }

        // attach tool
        if let (Some(t), Some(a)) = (tool.clone(), agent.clone()) {
            let attach_verb = clause.contains("give")
                || clause.contains("attach")
                || clause.contains("allow")
                || clause.contains("enable")
                || clause.contains("grant");
            let not_detach = !clause.contains("remove")
                && !clause.contains("detach")
                && !clause.contains("disable");
            if attach_verb && not_detach {
                ops.push(json!({ "op": "attach_tool", "agent": a, "tool": t }));
                notes.push(format!("Attach tool '{t}' to '{a}'."));
                continue;
            }
        }

        // add agent
        if (clause.contains("add") || clause.contains("create") || clause.contains("new"))
            && (clause.contains("agent") || ROLE_WORDS.iter().any(|r| word_match(&clause, r)))
        {
            let role = ROLE_WORDS
                .iter()
                .find(|r| word_match(&clause, r) && ROLE_LOOP.iter().any(|(rl, _)| *rl == **r))
                .copied();
            let name = parse_called_name(&clause)
                .or(role.map(|r| r.to_string()))
                .unwrap_or_else(|| format!("agent{}", known_agents.len() + 1));
            let mut name = name;
            let base = name.clone();
            let mut n = 1u32;
            while known_agents.contains(&name) {
                n += 1;
                name = format!("{base}{n}");
            }
            let loop_name = find_word(&clause, &loops)
                .or_else(|| role.map(|r| role_loop(r).to_string()))
                .unwrap_or_else(|| "react".to_string());
            let mut op = json!({ "op": "add_agent", "name": name, "loop": loop_name });
            if let Some(r) = role {
                op["role"] = json!(r);
            }
            ops.push(op);
            known_agents.push(name.clone());
            notes.push(format!("Add agent '{name}' (loop={loop_name})."));
            continue;
        }

        // set slot
        if let (Some((kind, ref_name)), Some(a)) = (slot_hit, agent.clone()) {
            ops.push(json!({ "op": "set_slot", "agent": a, "kind": kind, "ref": ref_name }));
            notes.push(format!("Set {kind} of '{a}' to '{ref_name}'."));
            continue;
        }

        // single topology
        if (clause.contains("single") || clause.contains("one agent"))
            && (clause.contains("topology")
                || clause.contains("mode")
                || clause.contains("make")
                || clause.contains("switch")
                || clause.contains("set"))
        {
            ops.push(json!({ "op": "set_topology", "mode": "single" }));
            notes.push("Switch to single-agent topology.".to_string());
            continue;
        }

        // coordinated topology
        if coord.is_some()
            || clause.contains("coordinat")
            || clause.contains("multi-agent")
            || clause.contains("multi agent")
            || clause.contains("team")
        {
            let chosen = coord.or_else(|| coordinators.first().cloned());
            if let Some(c) = chosen {
                ops.push(json!({
                    "op": "set_topology",
                    "mode": "coordinated",
                    "coordinator": c,
                    "order": known_agents,
                }));
                notes.push(format!("Switch to coordinated topology via '{c}'."));
                continue;
            }
        }

        // scorer
        if let Some(s) = scorer {
            ops.push(json!({ "op": "set_scorer", "ref": s }));
            notes.push(format!("Set default scorer to '{s}'."));
            continue;
        }

        // dataset
        if let Some(ds) = parse_dataset(&clause) {
            ops.push(json!({ "op": "add_dataset", "dataset": ds }));
            notes.push(format!("Add dataset '{ds}'."));
            continue;
        }

        // max_turns
        if let Some(n) = parse_after(&clause, "max_turns") {
            if let Some(a) = agent.clone() {
                ops.push(json!({ "op": "set_param", "agent": a, "key": "max_turns", "value": n }));
                notes.push(format!("Set '{a}' max_turns to {n}."));
                continue;
            }
        }

        // token budget
        if let Some(n) = parse_token_budget(&clause) {
            if let Some(a) = agent.clone() {
                ops.push(json!({
                    "op": "set_param",
                    "agent": a,
                    "key": "max_total_tokens",
                    "value": n,
                }));
                notes.push(format!("Cap '{a}' at {n} total tokens."));
                continue;
            }
        }

        let _ = &names_by_kind; // referenced for future LLM planner context
    }

    let explanation = if notes.is_empty() {
        "I couldn't map that to a concrete change. Try e.g. 'add a critic agent' \
         or 'use reflexion for solver'."
            .to_string()
    } else {
        notes.join(" ")
    };

    CopilotProposal {
        ops,
        explanation,
        source: "rules".to_string(),
    }
}

fn parse_rename(clause: &str) -> Option<(String, String)> {
    let words: Vec<&str> = clause.split_whitespace().collect();
    if let Some(i) = words.iter().position(|w| *w == "rename") {
        if i + 3 < words.len() && words[i + 2] == "to" {
            return Some((words[i + 1].to_string(), words[i + 3].to_string()));
        }
    }
    None
}

fn parse_called_name(clause: &str) -> Option<String> {
    let words: Vec<&str> = clause.split_whitespace().collect();
    for (i, w) in words.iter().enumerate() {
        if (*w == "called" || *w == "named") && i + 1 < words.len() {
            return Some(words[i + 1].to_string());
        }
    }
    None
}

fn parse_dataset(clause: &str) -> Option<String> {
    let words: Vec<&str> = clause.split_whitespace().collect();
    if let Some(i) = words.iter().position(|w| *w == "dataset") {
        if i + 1 < words.len() {
            return Some(words[i + 1].to_string());
        }
    }
    None
}

fn parse_after(clause: &str, key: &str) -> Option<u64> {
    let pattern = key.replace('_', " ");
    if let Some(idx) = clause.find(&pattern) {
        let rest = &clause[idx + pattern.len()..];
        return first_number(rest);
    }
    if clause.contains(key) {
        return first_number(clause);
    }
    None
}

fn parse_token_budget(clause: &str) -> Option<u64> {
    if clause.contains("max total tokens")
        || clause.contains("max_total_tokens")
        || clause.contains("token budget")
        || clause.contains("token ceiling")
        || clause.contains("token cap")
    {
        return first_number(clause);
    }
    if clause.contains("tokens") || clause.contains("token") {
        if clause.contains("cap") || clause.contains("total") {
            return first_number(clause);
        }
    }
    None
}

fn first_number(s: &str) -> Option<u64> {
    let mut digits = String::new();
    let mut started = false;
    for c in s.chars() {
        if c.is_ascii_digit() {
            digits.push(c);
            started = true;
        } else if started {
            break;
        }
    }
    if digits.is_empty() {
        None
    } else {
        digits.parse().ok()
    }
}

/// LLM planner with rule fallback when no API key or parse failure.
pub async fn propose_ops_llm(project: &Project, instruction: &str) -> CopilotProposal {
    if !llm_available() {
        return propose_ops(project, instruction);
    }
    match try_llm_ops(project, instruction).await {
        Ok(proposal) if !proposal.ops.is_empty() => proposal,
        _ => propose_ops(project, instruction),
    }
}

fn llm_available() -> bool {
    [
        "OPENAI_API_KEY",
        "ANTHROPIC_API_KEY",
        "DASHSCOPE_API_KEY",
        "DEEPSEEK_API_KEY",
        "PISCIS_API_KEY",
    ]
    .iter()
    .any(|k| std::env::var(k).map(|v| !v.is_empty()).unwrap_or(false))
}

const COPILOT_SYSTEM: &str = r#"You edit a multi-agent project by emitting JSON edit-ops.
Reply with a single JSON array of ops. Allowed ops:
{"op":"add_agent","name":str,"loop":str?}, {"op":"remove_agent","name":str},
{"op":"rename_agent","name":str,"to":str},
{"op":"set_slot","agent":str,"kind":"loop"|"compaction"|"memory"|"context","ref":str},
{"op":"clear_slot","agent":str,"kind":str},
{"op":"attach_tool","agent":str,"tool":str}, {"op":"detach_tool","agent":str,"tool":str},
{"op":"set_param","agent":str,"key":str,"value":any},
{"op":"set_topology","mode":"single"|"coordinated","coordinator":str?,"order":[str]?},
{"op":"set_scorer","ref":str}, {"op":"clear_scorer"},
{"op":"add_dataset","dataset":str}, {"op":"remove_dataset","dataset":str}.
Emit only the JSON array."#;

async fn try_llm_ops(project: &Project, instruction: &str) -> Result<CopilotProposal, String> {
    use harness_rt::agent::loop_::AgentLoop;
    use harness_rt::agent::messages::AgentEvent;
    use harness_rt::agent::tool::{ToolContext, ToolRegistry, ToolSettings};
    use harness_rt::llm::{build_client_with_timeout, LlmMessage, MessageContent};
    use harness_rt::policy::PolicyGate;
    use std::sync::atomic::AtomicBool;
    use std::sync::Arc;
    use tokio::sync::mpsc;

    crate::contrib::register_all();

    let creds = match crate::runner::LlmCreds::from_env() {
        Ok(c) => c,
        Err(_) => return Err("no creds".into()),
    };

    let model = std::env::var("HARNESSLAB_DEFAULT_MODEL").unwrap_or_else(|_| "gpt-4o-mini".into());
    let brief = format!(
        "Current project: {}\nInstruction: {}",
        serde_json::to_string(project)
            .unwrap_or_default()
            .chars()
            .take(2000)
            .collect::<String>(),
        instruction
    );

    let cfg = crate::runner::agent_config_for(&{
        let mut spec = crate::project::AgentSpec::new("Copilot");
        spec.model = model;
        spec.system_prompt = COPILOT_SYSTEM.to_string();
        spec.max_turns = 2;
        spec
    });

    let registry = Arc::new(ToolRegistry::new());
    let policy = Arc::new(PolicyGate::new(std::path::PathBuf::from(".")));
    let harness = crate::loader::build_harness_config(&cfg, registry, policy, None)
        .map_err(|e| e.to_string())?;
    let client = build_client_with_timeout(
        &creds.provider,
        &creds.api_key,
        creds.base_url.as_deref(),
        120,
    );
    let agent: Arc<AgentLoop> = Arc::new(harness.into_agent_loop(client, None, None));

    let (tx, mut rx) = mpsc::channel::<AgentEvent>(64);
    let cancel = Arc::new(AtomicBool::new(false));
    let ctx = ToolContext {
        session_id: "copilot".into(),
        workspace_root: std::path::PathBuf::from("."),
        bypass_permissions: true,
        settings: Arc::new(ToolSettings::default()),
        max_iterations: Some(4),
        memory_owner_id: "copilot".into(),
        pool_session_id: None,
        tool_use_id: None,
        cancel: cancel.clone(),
    };
    let messages = vec![LlmMessage {
        role: "user".to_string(),
        content: MessageContent::text(&brief),
    }];
    let handle = tokio::spawn(async move { agent.run(messages, tx, cancel, ctx).await });

    let mut text = String::new();
    while let Some(ev) = rx.recv().await {
        if let AgentEvent::TextDelta { delta } = ev {
            text.push_str(&delta);
        }
    }
    let _ = handle.await;

    let ops = extract_json_array(&text)?;
    Ok(CopilotProposal {
        ops,
        explanation: "Proposed by the LLM copilot.".to_string(),
        source: "agent".to_string(),
    })
}

fn extract_json_array(text: &str) -> Result<Vec<Value>, String> {
    let start = text.find('[').ok_or_else(|| "no JSON array".to_string())?;
    let slice = &text[start..];
    let end = slice
        .rfind(']')
        .ok_or_else(|| "unclosed array".to_string())?;
    let parsed: Value = serde_json::from_str(&slice[..=end]).map_err(|e| e.to_string())?;
    parsed
        .as_array()
        .cloned()
        .filter(|a| !a.is_empty())
        .ok_or_else(|| "empty ops".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ops::apply_ops;
    use crate::project::{AgentSpec, ComponentRef, Project, Topology};
    use crate::registry::{names_by_kind, tool_names};

    fn sample() -> Project {
        Project {
            id: "p".into(),
            name: "demo".into(),
            description: String::new(),
            agents: vec![AgentSpec {
                name: "solver".into(),
                r#loop: Some(ComponentRef::new("react")),
                ..AgentSpec::new("solver")
            }],
            topology: Topology {
                mode: "single".into(),
                coordinator: None,
                agent_order: vec!["solver".into()],
            },
            datasets: vec![],
            scorer: None,
            rev: 0,
            created_at: 0.0,
            updated_at: 0.0,
        }
    }

    fn apply(p: &Project, instruction: &str) -> (Vec<Value>, Project) {
        let proposal = propose_ops(p, instruction);
        let names = names_by_kind();
        let tools = tool_names();
        let res = apply_ops(p, &proposal.ops, &names, &tools);
        assert!(res.ok(), "{instruction:?} {:?}", res.problems);
        (proposal.ops, res.project)
    }

    #[test]
    fn add_critic_agent() {
        let (ops, p) = apply(&sample(), "add a critic agent");
        assert!(ops
            .iter()
            .any(|o| o.get("op").and_then(|v| v.as_str()) == Some("add_agent")));
        assert!(p.agents.iter().any(|a| a.name == "critic"));
        let critic = p.agents.iter().find(|a| a.name == "critic").unwrap();
        assert_eq!(critic.r#loop.as_ref().unwrap().name, "reflexion");
    }

    #[test]
    fn set_loop_for_agent() {
        let (_, p) = apply(&sample(), "use plan_and_solve for solver");
        assert_eq!(p.agents[0].r#loop.as_ref().unwrap().name, "plan_and_solve");
    }

    #[test]
    fn attach_tool() {
        let (_, p) = apply(&sample(), "give solver file_read");
        assert!(p.agents[0]
            .enabled_tools
            .as_ref()
            .map(|t| t.contains(&"file_read".to_string()))
            .unwrap_or(false));
    }

    #[test]
    fn switch_coordinated_with_debate() {
        let (_, p) = apply(&sample(), "add a critic; switch to coordinated with debate");
        assert_eq!(p.topology.mode, "coordinated");
        assert_eq!(p.topology.coordinator.as_ref().unwrap().name, "debate");
    }

    #[test]
    fn set_scorer() {
        let (_, p) = apply(&sample(), "set scorer to f1");
        assert_eq!(p.scorer.as_ref().unwrap().name, "f1");
    }

    #[test]
    fn token_budget_param() {
        let (_, p) = apply(&sample(), "cap solver at 5000 total tokens");
        assert_eq!(p.agents[0].max_total_tokens, 5000);
    }

    #[test]
    fn unmappable_instruction_returns_no_ops() {
        let out = propose_ops(&sample(), "make it really good please");
        assert!(out.ops.is_empty());
        assert!(out.explanation.to_lowercase().contains("couldn't"));
    }
}
