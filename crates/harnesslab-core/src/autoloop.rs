//! Experiment autoloop — iterative self-improvement.
//!
//! Given a project and an eval dataset, the autoloop repeatedly: (1) evaluates
//! the current best variant, (2) proposes a concrete change (an LLM-authored
//! edit op when credentials are available, otherwise a deterministic loop-/
//! strategy-swap hill-climb), (3) applies + re-evaluates, and (4) keeps the
//! change only if accuracy improved. It returns the full trajectory and the
//! best variant found so the host can persist it.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde::Serialize;
use serde_json::{json, Value};

use crate::eval::{self, Dataset};
use crate::project::Project;
use crate::{copilot, ops};

/// One iteration of the autoloop.
#[derive(Clone, Serialize)]
pub struct AutoloopRound {
    pub round: usize,
    pub accuracy: f64,
    /// `true` for the round-0 baseline (no change applied).
    pub baseline: bool,
    /// The improvement instruction / strategy considered this round.
    pub instruction: String,
    /// The edit ops applied this round (empty if none/invalid).
    pub ops: Vec<Value>,
    /// Whether the change was kept (i.e. it improved on the best so far).
    pub kept: bool,
    pub note: String,
}

/// Final autoloop outcome.
#[derive(Clone, Serialize)]
pub struct AutoloopResult {
    pub rounds: Vec<AutoloopRound>,
    pub best_round: usize,
    pub best_accuracy: f64,
    pub best_project: Project,
}

fn primary_loop(project: &Project) -> Option<String> {
    project
        .agents
        .first()
        .and_then(|a| a.r#loop.as_ref())
        .map(|c| c.name.clone())
}

/// Build a deterministic candidate: swap every agent's loop strategy to the next
/// untried option from the catalog. Returns `(instruction, ops)` or `None` when
/// the loop search space is exhausted.
fn deterministic_candidate(
    project: &Project,
    loop_names: &[String],
    tried: &BTreeSet<String>,
) -> Option<(String, Vec<Value>)> {
    let next = loop_names.iter().find(|n| !tried.contains(*n))?;
    let ops: Vec<Value> = project
        .agents
        .iter()
        .map(|a| json!({ "op": "set_slot", "agent": a.name, "kind": "loop", "ref": next }))
        .collect();
    if ops.is_empty() {
        return None;
    }
    Some((format!("Try loop strategy '{next}' on all agents"), ops))
}

/// Run the autoloop for `rounds` improvement iterations. `names`/`tools` are the
/// component/tool registries used to validate proposed ops. `on_round` is called
/// after each round (baseline + each iteration) for streaming.
pub async fn run_autoloop<F>(
    project: &Project,
    dataset: &Dataset,
    rounds: usize,
    base_workspace: &Path,
    names: &BTreeMap<String, BTreeSet<String>>,
    tools: &BTreeSet<String>,
    mut on_round: F,
) -> Result<AutoloopResult, String>
where
    F: FnMut(&AutoloopRound),
{
    let loop_names: Vec<String> = names
        .get("loop")
        .map(|s| s.iter().cloned().collect())
        .unwrap_or_default();

    // Round 0 — baseline.
    let base_eval = eval::run_eval(project, dataset, base_workspace).await?;
    let mut best_project = project.clone();
    let mut best_accuracy = base_eval.accuracy;
    let mut best_round = 0usize;

    let mut tried: BTreeSet<String> = BTreeSet::new();
    if let Some(l) = primary_loop(project) {
        tried.insert(l);
    }

    let mut trajectory = Vec::new();
    let baseline = AutoloopRound {
        round: 0,
        accuracy: best_accuracy,
        baseline: true,
        instruction: "baseline".to_string(),
        ops: Vec::new(),
        kept: true,
        note: format!("baseline accuracy {:.1}%", best_accuracy * 100.0),
    };
    on_round(&baseline);
    trajectory.push(baseline);

    for r in 1..=rounds {
        // Prefer an LLM-authored improvement op; fall back to the deterministic
        // loop-swap hill-climb when the LLM yields nothing valid.
        let llm_instruction = format!(
            "Improve accuracy on dataset '{}'. Current best accuracy is {:.0}%. \
             Suggest ONE concrete change (swap a loop or compaction strategy, or enable \
             reflection) as edit ops.",
            dataset.id,
            best_accuracy * 100.0
        );
        let proposal = copilot::propose_ops_llm(&best_project, &llm_instruction).await;
        let llm_result = ops::apply_ops(&best_project, &proposal.ops, names, tools);

        let (instruction, candidate_ops, candidate_project) =
            if !proposal.ops.is_empty() && llm_result.ok() {
                (llm_instruction, proposal.ops.clone(), llm_result.project)
            } else if let Some((instr, det_ops)) =
                deterministic_candidate(&best_project, &loop_names, &tried)
            {
                let det = ops::apply_ops(&best_project, &det_ops, names, tools);
                if !det.ok() {
                    let round = AutoloopRound {
                        round: r,
                        accuracy: best_accuracy,
                        baseline: false,
                        instruction: instr,
                        ops: det_ops,
                        kept: false,
                        note: format!("invalid ops: {}", det.problems.join("; ")),
                    };
                    on_round(&round);
                    trajectory.push(round);
                    continue;
                }
                (instr, det_ops, det.project)
            } else {
                let round = AutoloopRound {
                    round: r,
                    accuracy: best_accuracy,
                    baseline: false,
                    instruction: "no candidate".to_string(),
                    ops: Vec::new(),
                    kept: false,
                    note: "no further improvements to try".to_string(),
                };
                on_round(&round);
                trajectory.push(round);
                break;
            };

        // Record the loop we're trying so the deterministic search advances.
        if let Some(l) = primary_loop(&candidate_project) {
            tried.insert(l);
        }

        let cand_eval = eval::run_eval(&candidate_project, dataset, base_workspace).await?;
        let acc = cand_eval.accuracy;
        let improved = acc > best_accuracy + 1e-9;
        let note = if improved {
            format!(
                "improved {:.1}% → {:.1}% (kept)",
                best_accuracy * 100.0,
                acc * 100.0
            )
        } else {
            format!(
                "no gain ({:.1}% ≤ {:.1}%, discarded)",
                acc * 100.0,
                best_accuracy * 100.0
            )
        };
        if improved {
            best_project = candidate_project;
            best_accuracy = acc;
            best_round = r;
        }

        let round = AutoloopRound {
            round: r,
            accuracy: acc,
            baseline: false,
            instruction,
            ops: candidate_ops,
            kept: improved,
            note,
        };
        on_round(&round);
        trajectory.push(round);
    }

    Ok(AutoloopResult {
        rounds: trajectory,
        best_round,
        best_accuracy,
        best_project,
    })
}
