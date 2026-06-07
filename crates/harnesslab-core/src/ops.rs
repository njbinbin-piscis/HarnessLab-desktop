//! Structured edit operations over a [`Project`].
//!
//! The visual designer (and, later, the AI Copilot) never hand-edit code or
//! JSON; they emit a list of declarative **edit ops** which this module
//! validates against the component + tool registries and applies
//! **transactionally** (all-or-nothing) to a clone of the project. The result
//! is therefore always runnable, diffable and reversible.
//!
//! Mirrors HarnessLab's Python `project.ops`. (The `set_bus_stages` op is
//! intentionally omitted — the distributed bus is dropped for the desktop.)

use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;

use crate::project::{AgentSpec, ComponentRef, Project};

/// Algorithm + lifecycle-stage slots that `set_slot` / `clear_slot` accept.
pub const SLOT_KINDS: &[&str] = &[
    "loop",
    "compaction",
    "memory",
    "context",
    "credentials",
    "capabilities",
    "skills",
    "prompt",
    "approval",
    "budget",
    "hooks",
    "session",
    "tracing",
];

/// Agent scalar fields settable via `set_param`.
pub const AGENT_PARAMS: &[&str] = &[
    "model",
    "system_prompt",
    "role",
    "max_turns",
    "max_tokens",
    "max_total_tokens",
    "context_window",
];

/// Outcome of applying a batch of ops (all-or-nothing).
pub struct ApplyResult {
    /// The mutated project on success, or the original (unchanged) on failure.
    pub project: Project,
    pub problems: Vec<String>,
    pub applied: usize,
}

impl ApplyResult {
    pub fn ok(&self) -> bool {
        self.problems.is_empty()
    }
}

/// Validate every op, then apply all of them iff none are invalid.
///
/// `names` maps a component kind (e.g. `"loop"`, `"coordinator"`, `"scorer"`) to
/// the set of registered component names; `tools` is the set of known tool
/// names. Both are used to reject references to non-existent components/tools.
pub fn apply_ops(
    project: &Project,
    ops: &[Value],
    names: &BTreeMap<String, BTreeSet<String>>,
    tools: &BTreeSet<String>,
) -> ApplyResult {
    let mut staged = project.clone();
    let mut problems = Vec::new();

    for (i, op) in ops.iter().enumerate() {
        if let Err(e) = validate_and_apply(&mut staged, op, names, tools, i) {
            problems.push(e);
        }
    }

    if problems.is_empty() {
        ApplyResult {
            project: staged,
            problems,
            applied: ops.len(),
        }
    } else {
        ApplyResult {
            project: project.clone(),
            problems,
            applied: 0,
        }
    }
}

fn has_component(names: &BTreeMap<String, BTreeSet<String>>, kind: &str, name: &str) -> bool {
    names.get(kind).map(|s| s.contains(name)).unwrap_or(false)
}

fn agent_index(p: &Project, name: &str) -> Option<usize> {
    p.agents.iter().position(|a| a.name == name)
}

fn slot_mut<'a>(agent: &'a mut AgentSpec, kind: &str) -> Option<&'a mut Option<ComponentRef>> {
    Some(match kind {
        "loop" => &mut agent.r#loop,
        "compaction" => &mut agent.compaction,
        "memory" => &mut agent.memory,
        "context" => &mut agent.context,
        "credentials" => &mut agent.credentials,
        "capabilities" => &mut agent.capabilities,
        "skills" => &mut agent.skills,
        "prompt" => &mut agent.prompt,
        "approval" => &mut agent.approval,
        "budget" => &mut agent.budget,
        "hooks" => &mut agent.hooks,
        "session" => &mut agent.session,
        "tracing" => &mut agent.tracing,
        _ => return None,
    })
}

fn op_str<'a>(op: &'a Value, key: &str) -> Option<&'a str> {
    op.get(key).and_then(|v| v.as_str())
}

fn op_params(op: &Value) -> BTreeMap<String, Value> {
    op.get("params")
        .and_then(|v| v.as_object())
        .map(|m| m.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
        .unwrap_or_default()
}

/// Required argument keys per op type.
fn required(kind: &str) -> Option<&'static [&'static str]> {
    Some(match kind {
        "add_agent" => &["name"],
        "remove_agent" => &["name"],
        "rename_agent" => &["name", "to"],
        "set_slot" => &["agent", "kind", "ref"],
        "clear_slot" => &["agent", "kind"],
        "attach_tool" => &["agent", "tool"],
        "detach_tool" => &["agent", "tool"],
        "set_param" => &["agent", "key", "value"],
        "set_topology" => &["mode"],
        "set_scorer" => &["ref"],
        "clear_scorer" => &[],
        "add_dataset" => &["dataset"],
        "remove_dataset" => &["dataset"],
        _ => return None,
    })
}

fn validate_and_apply(
    p: &mut Project,
    op: &Value,
    names: &BTreeMap<String, BTreeSet<String>>,
    tools: &BTreeSet<String>,
    index: usize,
) -> Result<(), String> {
    let kind = op
        .get("op")
        .and_then(|v| v.as_str())
        .ok_or_else(|| format!("op[{index}]: missing 'op' type"))?;
    let req = required(kind).ok_or_else(|| format!("op[{index}]: unknown op type {kind:?}"))?;
    for key in req {
        if op.get(*key).map(|v| v.is_null()).unwrap_or(true) {
            return Err(format!("op[{index}] {kind}: missing required field {key:?}"));
        }
    }

    match kind {
        "add_agent" => {
            let name = op_str(op, "name").unwrap().to_string();
            if agent_index(p, &name).is_some() {
                return Err(format!("op[{index}] add_agent: agent {name:?} already exists"));
            }
            let mut spec = AgentSpec::new(&name);
            for slot in SLOT_KINDS {
                if let Some(comp) = op_str(op, slot) {
                    if !comp.is_empty() {
                        if !has_component(names, slot, comp) {
                            return Err(format!(
                                "op[{index}] add_agent: no {slot} component {comp:?}"
                            ));
                        }
                        if let Some(field) = slot_mut(&mut spec, slot) {
                            *field = Some(ComponentRef::new(comp));
                        }
                    }
                }
            }
            if let Some(v) = op_str(op, "model") {
                spec.model = v.to_string();
            }
            if let Some(v) = op_str(op, "system_prompt") {
                spec.system_prompt = v.to_string();
            }
            if let Some(v) = op_str(op, "role") {
                spec.role = v.to_string();
            }
            p.agents.push(spec);
            if !p.topology.agent_order.contains(&name) {
                p.topology.agent_order.push(name);
            }
            Ok(())
        }

        "remove_agent" => {
            let name = op_str(op, "name").unwrap();
            if agent_index(p, name).is_none() {
                return Err(format!("op[{index}] remove_agent: no agent {name:?}"));
            }
            p.agents.retain(|a| a.name != name);
            p.topology.agent_order.retain(|n| n != name);
            Ok(())
        }

        "rename_agent" => {
            let name = op_str(op, "name").unwrap().to_string();
            let to = op_str(op, "to").unwrap().to_string();
            if agent_index(p, &name).is_none() {
                return Err(format!("op[{index}] rename_agent: no agent {name:?}"));
            }
            if agent_index(p, &to).is_some() {
                return Err(format!(
                    "op[{index}] rename_agent: target name {to:?} already exists"
                ));
            }
            let idx = agent_index(p, &name).unwrap();
            p.agents[idx].name = to.clone();
            for n in p.topology.agent_order.iter_mut() {
                if *n == name {
                    *n = to.clone();
                }
            }
            Ok(())
        }

        "set_slot" => {
            let agent = op_str(op, "agent").unwrap().to_string();
            let slot = op_str(op, "kind").unwrap().to_string();
            let comp = op_str(op, "ref").unwrap().to_string();
            if !SLOT_KINDS.contains(&slot.as_str()) {
                return Err(format!("op[{index}] set_slot: kind must be one of {SLOT_KINDS:?}"));
            }
            if !has_component(names, &slot, &comp) {
                return Err(format!("op[{index}] set_slot: no {slot} component {comp:?}"));
            }
            let params = op_params(op);
            let idx = agent_index(p, &agent)
                .ok_or_else(|| format!("op[{index}] set_slot: no agent {agent:?}"))?;
            if let Some(field) = slot_mut(&mut p.agents[idx], &slot) {
                *field = Some(ComponentRef { name: comp, params });
            }
            Ok(())
        }

        "clear_slot" => {
            let agent = op_str(op, "agent").unwrap().to_string();
            let slot = op_str(op, "kind").unwrap().to_string();
            if !SLOT_KINDS.contains(&slot.as_str()) {
                return Err(format!("op[{index}] clear_slot: kind must be one of {SLOT_KINDS:?}"));
            }
            let idx = agent_index(p, &agent)
                .ok_or_else(|| format!("op[{index}] clear_slot: no agent {agent:?}"))?;
            if let Some(field) = slot_mut(&mut p.agents[idx], &slot) {
                *field = None;
            }
            Ok(())
        }

        "attach_tool" => {
            let agent = op_str(op, "agent").unwrap().to_string();
            let tool = op_str(op, "tool").unwrap().to_string();
            if !tools.contains(&tool) {
                return Err(format!("op[{index}] attach_tool: unknown tool {tool:?}"));
            }
            let idx = agent_index(p, &agent)
                .ok_or_else(|| format!("op[{index}] attach_tool: no agent {agent:?}"))?;
            let a = &mut p.agents[idx];
            let mut current: Vec<String> = a
                .enabled_tools
                .clone()
                .unwrap_or_else(|| tools.iter().cloned().collect());
            if !current.contains(&tool) {
                current.push(tool);
            }
            a.enabled_tools = Some(current);
            Ok(())
        }

        "detach_tool" => {
            let agent = op_str(op, "agent").unwrap().to_string();
            let tool = op_str(op, "tool").unwrap().to_string();
            let idx = agent_index(p, &agent)
                .ok_or_else(|| format!("op[{index}] detach_tool: no agent {agent:?}"))?;
            let a = &mut p.agents[idx];
            let current: Vec<String> = a
                .enabled_tools
                .clone()
                .unwrap_or_else(|| tools.iter().cloned().collect());
            a.enabled_tools = Some(current.into_iter().filter(|t| t != &tool).collect());
            Ok(())
        }

        "set_param" => {
            let agent = op_str(op, "agent").unwrap().to_string();
            let key = op_str(op, "key").unwrap().to_string();
            if !AGENT_PARAMS.contains(&key.as_str()) {
                return Err(format!(
                    "op[{index}] set_param: param {key:?} not settable (allowed: {AGENT_PARAMS:?})"
                ));
            }
            let value = op.get("value").unwrap();
            let idx = agent_index(p, &agent)
                .ok_or_else(|| format!("op[{index}] set_param: no agent {agent:?}"))?;
            let a = &mut p.agents[idx];
            match key.as_str() {
                "model" | "system_prompt" | "role" => {
                    let s = value.as_str().ok_or_else(|| {
                        format!("op[{index}] set_param: {key:?} expects a string")
                    })?;
                    match key.as_str() {
                        "model" => a.model = s.to_string(),
                        "system_prompt" => a.system_prompt = s.to_string(),
                        "role" => a.role = s.to_string(),
                        _ => unreachable!(),
                    }
                }
                _ => {
                    let n = value.as_u64().ok_or_else(|| {
                        format!("op[{index}] set_param: {key:?} expects a non-negative integer")
                    })? as u32;
                    match key.as_str() {
                        "max_turns" => a.max_turns = n,
                        "max_tokens" => a.max_tokens = n,
                        "max_total_tokens" => a.max_total_tokens = n,
                        "context_window" => a.context_window = n,
                        _ => unreachable!(),
                    }
                }
            }
            Ok(())
        }

        "set_topology" => {
            let mode = op_str(op, "mode").unwrap().to_string();
            if mode != "single" && mode != "coordinated" {
                return Err(format!(
                    "op[{index}] set_topology: mode must be 'single' or 'coordinated'"
                ));
            }
            if mode == "coordinated" {
                let coord = op_str(op, "coordinator").filter(|s| !s.is_empty()).ok_or_else(|| {
                    format!("op[{index}] set_topology: coordinated mode requires 'coordinator'")
                })?;
                if !has_component(names, "coordinator", coord) {
                    return Err(format!(
                        "op[{index}] set_topology: no coordinator component {coord:?}"
                    ));
                }
                p.topology.coordinator = Some(ComponentRef {
                    name: coord.to_string(),
                    params: op_params(op),
                });
            } else {
                p.topology.coordinator = None;
            }
            if let Some(order) = op.get("order").and_then(|v| v.as_array()) {
                let order: Vec<String> = order
                    .iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect();
                let unknown: Vec<&String> =
                    order.iter().filter(|n| agent_index(p, n).is_none()).collect();
                if !unknown.is_empty() {
                    return Err(format!(
                        "op[{index}] set_topology: unknown agents in order: {unknown:?}"
                    ));
                }
                p.topology.agent_order = order;
            }
            p.topology.mode = mode;
            Ok(())
        }

        "set_scorer" => {
            let comp = op_str(op, "ref").unwrap().to_string();
            if !has_component(names, "scorer", &comp) {
                return Err(format!("op[{index}] set_scorer: no scorer component {comp:?}"));
            }
            p.scorer = Some(ComponentRef {
                name: comp,
                params: op_params(op),
            });
            Ok(())
        }

        "clear_scorer" => {
            p.scorer = None;
            Ok(())
        }

        "add_dataset" => {
            let ds = op_str(op, "dataset").unwrap().to_string();
            if !p.datasets.contains(&ds) {
                p.datasets.push(ds);
            }
            Ok(())
        }

        "remove_dataset" => {
            let ds = op_str(op, "dataset").unwrap();
            p.datasets.retain(|d| d != ds);
            Ok(())
        }

        _ => Err(format!("op[{index}]: unhandled op {kind:?}")),
    }
}

/// A shallow, human-readable diff of the fields the ops can touch.
pub fn diff_projects(before: &Project, after: &Project) -> BTreeMap<String, Value> {
    let b = serde_json::to_value(before).unwrap_or(Value::Null);
    let a = serde_json::to_value(after).unwrap_or(Value::Null);
    let mut changed = BTreeMap::new();
    for field in ["agents", "topology", "datasets", "scorer", "name", "description"] {
        let bv = b.get(field).cloned().unwrap_or(Value::Null);
        let av = a.get(field).cloned().unwrap_or(Value::Null);
        if bv != av {
            changed.insert(
                field.to_string(),
                serde_json::json!({ "before": bv, "after": av }),
            );
        }
    }
    changed
}
