//! Multi-agent coordination algorithms for live project runs (P3).
//!
//! Each coordinator orchestrates a list of kernel-backed agent runs into a
//! single answer. Logic mirrors HarnessLab's Python
//! `library/coordinator/coordinators.py` and the Factory codegen snippets.

use std::collections::{BTreeMap, HashMap};

use serde_json::Value;

use super::{run_agent_task, AgentRunContext, AgentRunOutcome};
use crate::project::AgentSpec;

/// Run the named coordinator over `agents` and `task`.
pub async fn coordinate(
    name: &str,
    params: &BTreeMap<String, Value>,
    agents: &[AgentSpec],
    task: &str,
    ctx: &AgentRunContext,
) -> Result<AgentRunOutcome, String> {
    if agents.is_empty() {
        return Ok(AgentRunOutcome::default());
    }
    match name {
        "sequential_pipeline" => sequential_pipeline(agents, task, ctx).await,
        "supervisor" => supervisor(agents, task, ctx).await,
        "group_chat" => group_chat(agents, task, params, ctx).await,
        "debate" => debate(agents, task, ctx).await,
        "majority_vote" => majority_vote(agents, task, ctx).await,
        "auction" => auction(agents, task, ctx).await,
        "blackboard" => blackboard(agents, task, params, ctx).await,
        "map_reduce" => map_reduce(agents, task, ctx).await,
        "hierarchical" => hierarchical(agents, task, ctx).await,
        "tournament" => tournament(agents, task, params, ctx).await,
        "coscientist" => coscientist(agents, task, params, ctx).await,
        other => {
            // Unknown coordinator — sequential fallback (matches Factory emitters).
            let _ = other;
            sequential_pipeline(agents, task, ctx).await
        }
    }
}

fn merge(mut acc: AgentRunOutcome, next: AgentRunOutcome) -> AgentRunOutcome {
    acc.result = next.result;
    acc.total_input_tokens += next.total_input_tokens;
    acc.total_output_tokens += next.total_output_tokens;
    acc
}

async fn run_one(
    ctx: &AgentRunContext,
    spec: &AgentSpec,
    task: &str,
    context: &str,
) -> Result<AgentRunOutcome, String> {
    run_agent_task(ctx, spec, task, context).await
}

async fn run_all_parallel(
    ctx: &AgentRunContext,
    agents: &[AgentSpec],
    task: &str,
    context: &str,
) -> Result<Vec<AgentRunOutcome>, String> {
    let mut handles = Vec::with_capacity(agents.len());
    for spec in agents {
        let ctx = ctx.clone();
        let spec = spec.clone();
        let task = task.to_string();
        let context = context.to_string();
        handles.push(tokio::spawn(async move {
            run_one(&ctx, &spec, &task, &context).await
        }));
    }
    let mut out = Vec::with_capacity(handles.len());
    for h in handles {
        out.push(h.await.map_err(|e| format!("agent task panicked: {e}"))??);
    }
    Ok(out)
}

async fn sequential_pipeline(
    agents: &[AgentSpec],
    task: &str,
    ctx: &AgentRunContext,
) -> Result<AgentRunOutcome, String> {
    let mut context = String::new();
    let mut acc = AgentRunOutcome::default();
    for spec in agents {
        let step = run_one(ctx, spec, task, &context).await?;
        context = format!("{context}\n[{}] {}", spec.name, step.result)
            .trim()
            .to_string();
        acc = merge(acc, step);
    }
    Ok(acc)
}

async fn supervisor(
    agents: &[AgentSpec],
    task: &str,
    ctx: &AgentRunContext,
) -> Result<AgentRunOutcome, String> {
    let supervisor = &agents[0];
    let workers: Vec<_> = if agents.len() > 1 {
        agents[1..].to_vec()
    } else {
        agents.to_vec()
    };
    let plan = run_one(
        ctx,
        supervisor,
        &format!("Break down and delegate this task: {task}"),
        "",
    )
    .await?;
    let mut acc = plan;
    let mut joined = String::new();
    for w in &workers {
        let out = run_one(ctx, w, task, &acc.result).await?;
        joined.push_str(&format!("[{}] {}\n", w.name, out.result));
        acc.total_input_tokens += out.total_input_tokens;
        acc.total_output_tokens += out.total_output_tokens;
    }
    let final_ = run_one(
        ctx,
        supervisor,
        &format!("Synthesize a final answer for: {task}"),
        &joined,
    )
    .await?;
    Ok(merge(acc, final_))
}

async fn group_chat(
    agents: &[AgentSpec],
    task: &str,
    params: &BTreeMap<String, Value>,
    ctx: &AgentRunContext,
) -> Result<AgentRunOutcome, String> {
    let rounds = param_usize(params, "rounds", 2).max(1);
    let mut transcript = String::new();
    let mut acc = AgentRunOutcome::default();
    for _ in 0..rounds {
        for agent in agents {
            let reply = run_one(ctx, agent, task, &transcript).await?;
            transcript.push_str(&format!("\n[{}] {}", agent.name, reply.result));
            acc.total_input_tokens += reply.total_input_tokens;
            acc.total_output_tokens += reply.total_output_tokens;
        }
    }
    let summary = run_one(
        ctx,
        agents.last().unwrap(),
        &format!("Summarize the group's conclusion for: {task}"),
        &transcript,
    )
    .await?;
    Ok(merge(acc, summary))
}

async fn debate(
    agents: &[AgentSpec],
    task: &str,
    ctx: &AgentRunContext,
) -> Result<AgentRunOutcome, String> {
    let answers = run_all_parallel(ctx, agents, task, "").await?;
    let mut acc = AgentRunOutcome::default();
    let board: String = agents
        .iter()
        .zip(&answers)
        .map(|(a, o)| {
            acc.total_input_tokens += o.total_input_tokens;
            acc.total_output_tokens += o.total_output_tokens;
            format!("[{}] {}", a.name, o.result)
        })
        .collect::<Vec<_>>()
        .join("\n");
    let mut refined_handles = Vec::with_capacity(agents.len());
    for agent in agents {
        let ctx = ctx.clone();
        let agent = agent.clone();
        let task = format!("Critique the other answers and refine yours for: {task}");
        let board = board.clone();
        refined_handles.push(tokio::spawn(async move {
            run_one(&ctx, &agent, &task, &board).await
        }));
    }
    let mut refined = Vec::new();
    for h in refined_handles {
        refined.push(h.await.map_err(|e| format!("agent task panicked: {e}"))??);
    }
    let board2: String = agents
        .iter()
        .zip(&refined)
        .map(|(a, o)| {
            acc.total_input_tokens += o.total_input_tokens;
            acc.total_output_tokens += o.total_output_tokens;
            format!("[{}] {}", a.name, o.result)
        })
        .collect::<Vec<_>>()
        .join("\n");
    let final_ = run_one(
        ctx,
        &agents[0],
        &format!("Reconcile the debate into one answer for: {task}"),
        &board2,
    )
    .await?;
    Ok(merge(acc, final_))
}

async fn majority_vote(
    agents: &[AgentSpec],
    task: &str,
    ctx: &AgentRunContext,
) -> Result<AgentRunOutcome, String> {
    let answers = run_all_parallel(ctx, agents, task, "").await?;
    let mut acc = AgentRunOutcome::default();
    let mut counts: HashMap<String, usize> = HashMap::new();
    for o in &answers {
        acc.total_input_tokens += o.total_input_tokens;
        acc.total_output_tokens += o.total_output_tokens;
        *counts.entry(o.result.trim().to_lowercase()).or_insert(0) += 1;
    }
    let winner_key = counts
        .iter()
        .max_by_key(|(_, c)| *c)
        .map(|(k, _)| k.clone())
        .unwrap_or_default();
    for o in &answers {
        if o.result.trim().to_lowercase() == winner_key {
            acc.result = o.result.clone();
            return Ok(acc);
        }
    }
    acc.result = answers
        .first()
        .map(|o| o.result.clone())
        .unwrap_or_default();
    Ok(acc)
}

async fn auction(
    agents: &[AgentSpec],
    task: &str,
    ctx: &AgentRunContext,
) -> Result<AgentRunOutcome, String> {
    let answers = run_all_parallel(ctx, agents, task, "").await?;
    let mut acc = AgentRunOutcome::default();
    let mut best = String::new();
    let mut best_bid = -1i64;
    for o in answers {
        acc.total_input_tokens += o.total_input_tokens;
        acc.total_output_tokens += o.total_output_tokens;
        let bid =
            o.result.len() as i64 + 5 * o.result.to_lowercase().matches("because").count() as i64;
        if bid > best_bid {
            best_bid = bid;
            best = o.result.clone();
        }
    }
    acc.result = best;
    Ok(acc)
}

async fn blackboard(
    agents: &[AgentSpec],
    task: &str,
    params: &BTreeMap<String, Value>,
    ctx: &AgentRunContext,
) -> Result<AgentRunOutcome, String> {
    let max_it = param_usize(params, "max_iterations", 3).max(1);
    let mut board = String::new();
    let mut acc = AgentRunOutcome::default();
    for _ in 0..max_it {
        let prev = board.clone();
        for agent in agents {
            let step = run_one(
                ctx,
                agent,
                &format!("Improve the shared solution for: {task}"),
                &board,
            )
            .await?;
            board = step.result.clone();
            acc.total_input_tokens += step.total_input_tokens;
            acc.total_output_tokens += step.total_output_tokens;
        }
        if board.trim() == prev.trim() {
            break;
        }
    }
    acc.result = board;
    Ok(acc)
}

async fn map_reduce(
    agents: &[AgentSpec],
    task: &str,
    ctx: &AgentRunContext,
) -> Result<AgentRunOutcome, String> {
    if agents.len() == 1 {
        return run_one(ctx, &agents[0], task, "").await;
    }
    let mappers = &agents[..agents.len() - 1];
    let reducer = agents.last().unwrap();
    let partials = run_all_parallel(
        ctx,
        mappers,
        &format!("Produce your partial analysis for: {task}"),
        "",
    )
    .await?;
    let mut acc = AgentRunOutcome::default();
    let mapped: String = mappers
        .iter()
        .zip(&partials)
        .map(|(m, o)| {
            acc.total_input_tokens += o.total_input_tokens;
            acc.total_output_tokens += o.total_output_tokens;
            format!("[{}] {}", m.name, o.result)
        })
        .collect::<Vec<_>>()
        .join("\n");
    let reduced = run_one(
        ctx,
        reducer,
        &format!("Reduce these partial results into a final answer for: {task}"),
        &mapped,
    )
    .await?;
    Ok(merge(acc, reduced))
}

async fn hierarchical(
    agents: &[AgentSpec],
    task: &str,
    ctx: &AgentRunContext,
) -> Result<AgentRunOutcome, String> {
    if agents.len() == 1 {
        return run_one(ctx, &agents[0], task, "").await;
    }
    let manager = &agents[0];
    let workers = &agents[1..];
    let plan = run_one(
        ctx,
        manager,
        &format!(
            "Decompose into {} subtasks, one per line, for: {task}",
            workers.len()
        ),
        "",
    )
    .await?;
    let mut acc = plan;
    let mut joined = String::new();
    for w in workers {
        let out = run_one(
            ctx,
            w,
            task,
            &format!("Your assignment (from the manager's plan):\n{}", acc.result),
        )
        .await?;
        joined.push_str(&format!("[{}] {}\n", w.name, out.result));
        acc.total_input_tokens += out.total_input_tokens;
        acc.total_output_tokens += out.total_output_tokens;
    }
    let final_ = run_one(
        ctx,
        manager,
        &format!("Integrate the workers' results into one final answer for: {task}"),
        &format!("{}\n\n## Worker results\n{}", acc.result, joined),
    )
    .await?;
    Ok(merge(acc, final_))
}

async fn tournament(
    agents: &[AgentSpec],
    task: &str,
    params: &BTreeMap<String, Value>,
    ctx: &AgentRunContext,
) -> Result<AgentRunOutcome, String> {
    let candidates_out = run_all_parallel(ctx, agents, task, "").await?;
    if candidates_out.len() == 1 {
        return Ok(candidates_out.into_iter().next().unwrap());
    }
    let judge_idx = param_usize(params, "judge", 0).min(agents.len() - 1);
    let judge = &agents[judge_idx];
    let k = param_f64(params, "k_factor", 16.0);
    let rounds = param_usize(params, "rounds", 1).max(1);
    let candidates: Vec<String> = candidates_out.iter().map(|o| o.result.clone()).collect();
    let mut acc = AgentRunOutcome::default();
    for o in &candidates_out {
        acc.total_input_tokens += o.total_input_tokens;
        acc.total_output_tokens += o.total_output_tokens;
    }
    let mut elo = vec![1000.0f64; candidates.len()];
    for _ in 0..rounds {
        for i in 0..candidates.len() {
            for j in (i + 1)..candidates.len() {
                let winner = compare_pair(ctx, judge, task, &candidates[i], &candidates[j]).await?;
                acc.total_input_tokens += winner.input_tokens;
                acc.total_output_tokens += winner.output_tokens;
                let win_i = winner.winner_is_a;
                let exp_i = 1.0 / (1.0 + 10f64.powf((elo[j] - elo[i]) / 400.0));
                let score_i = if win_i { 1.0 } else { 0.0 };
                elo[i] += k * (score_i - exp_i);
                elo[j] += k * ((1.0 - score_i) - (1.0 - exp_i));
            }
        }
    }
    let best = elo
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal))
        .map(|(i, _)| i)
        .unwrap_or(0);
    acc.result = candidates[best].clone();
    Ok(acc)
}

struct CompareOutcome {
    winner_is_a: bool,
    input_tokens: u32,
    output_tokens: u32,
}

async fn compare_pair(
    ctx: &AgentRunContext,
    judge: &AgentSpec,
    task: &str,
    a: &str,
    b: &str,
) -> Result<CompareOutcome, String> {
    let verdict = run_one(
        ctx,
        judge,
        &format!("Compare two candidate answers for the task: {task}\nReply with 'A' or 'B' for the better one."),
        &format!("[A]\n{a}\n\n[B]\n{b}"),
    )
    .await?;
    let low = verdict.result.trim().to_lowercase();
    let win_a =
        low.contains("final answer: a") || low.ends_with(" a") || low == "a" || a.len() >= b.len();
    Ok(CompareOutcome {
        winner_is_a: win_a,
        input_tokens: verdict.total_input_tokens,
        output_tokens: verdict.total_output_tokens,
    })
}

fn agent_by_role<'a>(agents: &'a [AgentSpec], role: &str, fallback: usize) -> &'a AgentSpec {
    for a in agents {
        let n = a.name.to_lowercase();
        if n.starts_with(role) || n.contains(role) {
            return a;
        }
    }
    &agents[fallback.min(agents.len().saturating_sub(1))]
}

fn dedup_hypotheses(hyps: &[String], threshold: f64) -> Vec<String> {
    let mut kept: Vec<String> = Vec::new();
    for h in hyps {
        let h_lower = h.to_lowercase();
        let hw: std::collections::HashSet<_> = h_lower.split_whitespace().collect();
        if hw.is_empty() {
            continue;
        }
        let duplicate = kept.iter().any(|k| {
            let k_lower = k.to_lowercase();
            let kw: std::collections::HashSet<_> = k_lower.split_whitespace().collect();
            let union = hw.union(&kw).count();
            union > 0 && (hw.intersection(&kw).count() as f64 / union as f64) >= threshold
        });
        if !duplicate {
            kept.push(h.clone());
        }
    }
    kept
}

async fn coscientist(
    agents: &[AgentSpec],
    task: &str,
    params: &BTreeMap<String, Value>,
    ctx: &AgentRunContext,
) -> Result<AgentRunOutcome, String> {
    let reflection = agent_by_role(agents, "reflection", 1);
    let ranking = agent_by_role(agents, "ranking", 2);
    let evolution = agent_by_role(agents, "evolution", 3);
    let meta = agent_by_role(agents, "meta", agents.len().saturating_sub(1));
    let rounds = param_usize(params, "rounds", 1).max(1);
    let dedup = param_bool(params, "dedup", true);
    let threshold = param_f64(params, "dedup_threshold", 0.85);

    let mut board: Vec<String> = Vec::new();
    let mut review = String::new();
    let mut ranked = String::new();
    let mut acc = AgentRunOutcome::default();
    let mut seed = task.to_string();

    for _ in 0..rounds {
        let hyps = run_all_parallel(ctx, agents, &seed, "").await?;
        for o in &hyps {
            acc.total_input_tokens += o.total_input_tokens;
            acc.total_output_tokens += o.total_output_tokens;
            board.push(o.result.clone());
        }
        if dedup {
            board = dedup_hypotheses(&board, threshold);
        }
        let board_text: String = board
            .iter()
            .enumerate()
            .map(|(i, h)| format!("[H{i}] {h}"))
            .collect::<Vec<_>>()
            .join("\n");

        let rev = run_one(
            ctx,
            reflection,
            &format!("Critique these hypotheses for: {task}"),
            &board_text,
        )
        .await?;
        review = rev.result.clone();
        acc.total_input_tokens += rev.total_input_tokens;
        acc.total_output_tokens += rev.total_output_tokens;

        let rank = run_one(
            ctx,
            ranking,
            &format!("Rank the hypotheses and state the best one for: {task}"),
            &format!("{board_text}\n\n## Reflection\n{review}"),
        )
        .await?;
        ranked = rank.result.clone();
        acc.total_input_tokens += rank.total_input_tokens;
        acc.total_output_tokens += rank.total_output_tokens;

        let evo = run_one(
            ctx,
            evolution,
            &format!("Improve the leading hypothesis for: {task}"),
            &ranked,
        )
        .await?;
        board.push(evo.result.clone());
        acc.total_input_tokens += evo.total_input_tokens;
        acc.total_output_tokens += evo.total_output_tokens;
        if dedup {
            board = dedup_hypotheses(&board, threshold);
        }
        seed = format!("{task}\n\n## Leading hypothesis so far\n{}", evo.result);
    }

    let final_ = run_one(
        ctx,
        meta,
        &format!("Write the final research answer for: {task}"),
        &format!(
            "## Ranked\n{ranked}\n\n## Reflection\n{review}\n\n## Board\n{}",
            board.join("\n")
        ),
    )
    .await?;
    Ok(merge(acc, final_))
}

fn param_usize(params: &BTreeMap<String, Value>, key: &str, default: usize) -> usize {
    params
        .get(key)
        .and_then(|v| v.as_u64())
        .map(|n| n as usize)
        .unwrap_or(default)
}

fn param_f64(params: &BTreeMap<String, Value>, key: &str, default: f64) -> f64 {
    params.get(key).and_then(|v| v.as_f64()).unwrap_or(default)
}

fn param_bool(params: &BTreeMap<String, Value>, key: &str, default: bool) -> bool {
    params.get(key).and_then(|v| v.as_bool()).unwrap_or(default)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dedup_removes_near_duplicates() {
        let hyps = vec![
            "the cat sat on mat".into(),
            "the cat sat on the mat".into(),
            "completely different hypothesis".into(),
        ];
        let kept = dedup_hypotheses(&hyps, 0.85);
        assert!(kept.len() <= 2);
        assert!(kept.iter().any(|h| h.contains("different")));
    }
}
