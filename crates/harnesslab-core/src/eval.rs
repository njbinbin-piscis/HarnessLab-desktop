//! Evaluation pipeline — built-in datasets, the eval runner, and result types.
//!
//! An eval runs a project against every task in a dataset, scores each
//! prediction with the dataset's scorer (implemented natively in
//! [`harness_rt::scoring`]), and aggregates an accuracy + latency/token report.

use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::Instant;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::mpsc;

use crate::project::{new_id, now_secs, Project};
use crate::runner::{self, UiEvent};

/// A single graded example.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalTask {
    pub id: String,
    pub input: String,
    pub expected: String,
    /// Optional per-task scorer params (e.g. keywords, tolerance).
    #[serde(default)]
    pub params: Value,
}

/// A named collection of graded examples plus the scorer used to grade them.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Dataset {
    pub id: String,
    pub scorer: String,
    pub source: String,
    pub tasks: Vec<EvalTask>,
}

/// The list-view summary the dashboard renders in its dataset picker.
#[derive(Debug, Clone, Serialize)]
pub struct DatasetSummary {
    pub id: String,
    pub num_tasks: usize,
    pub scorer: String,
    pub source: String,
}

impl Dataset {
    pub fn summary(&self) -> DatasetSummary {
        DatasetSummary {
            id: self.id.clone(),
            num_tasks: self.tasks.len(),
            scorer: self.scorer.clone(),
            source: self.source.clone(),
        }
    }
}

/// One scored task result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskResult {
    pub task_id: String,
    pub score: f64,
    pub prediction: String,
    pub expected: String,
    #[serde(default)]
    pub latency_s: f64,
    #[serde(default)]
    pub est_tokens: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Aggregate run metrics.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EvalMetrics {
    pub avg_latency: f64,
    pub total_est_tokens: u32,
}

/// A persisted eval run (one project × one dataset).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalRun {
    pub id: String,
    pub project_id: String,
    #[serde(default)]
    pub project_name: String,
    pub dataset: String,
    pub scorer: String,
    pub accuracy: f64,
    pub task_results: Vec<TaskResult>,
    pub metrics: EvalMetrics,
    pub created_at: f64,
}

/// Leaderboard / list entry (a trimmed `EvalRun`).
#[derive(Debug, Clone, Serialize)]
pub struct EvalRunSummary {
    pub id: String,
    pub project_id: String,
    pub project_name: String,
    pub dataset: String,
    pub scorer: String,
    pub accuracy: f64,
    pub avg_latency: f64,
    pub total_est_tokens: u32,
    pub created_at: f64,
}

impl EvalRun {
    pub fn summary(&self) -> EvalRunSummary {
        EvalRunSummary {
            id: self.id.clone(),
            project_id: self.project_id.clone(),
            project_name: self.project_name.clone(),
            dataset: self.dataset.clone(),
            scorer: self.scorer.clone(),
            accuracy: self.accuracy,
            avg_latency: self.metrics.avg_latency,
            total_est_tokens: self.metrics.total_est_tokens,
            created_at: self.created_at,
        }
    }
}

fn task(id: &str, input: &str, expected: &str) -> EvalTask {
    EvalTask {
        id: id.to_string(),
        input: input.to_string(),
        expected: expected.to_string(),
        params: Value::Null,
    }
}

/// The built-in datasets shipped with the desktop app.
pub fn builtin_datasets() -> Vec<Dataset> {
    vec![
        Dataset {
            id: "smoke_qa".to_string(),
            scorer: "contains".to_string(),
            source: "builtin".to_string(),
            tasks: vec![
                task("qa1", "What is the capital of France? Answer with one word.", "Paris"),
                task("qa2", "What is the capital of Japan? Answer with one word.", "Tokyo"),
                task("qa3", "What planet is known as the Red Planet? One word.", "Mars"),
                task("qa4", "What gas do plants absorb from the air? One short phrase.", "carbon dioxide"),
                task("qa5", "Who wrote the play Romeo and Juliet? Last name only.", "Shakespeare"),
            ],
        },
        Dataset {
            id: "math_basic".to_string(),
            scorer: "numeric".to_string(),
            source: "builtin".to_string(),
            tasks: vec![
                task("m1", "Compute 12 + 30. Reply with the number only.", "42"),
                task("m2", "Compute 7 * 6. Reply with the number only.", "42"),
                task("m3", "Compute 100 - 58. Reply with the number only.", "42"),
                task("m4", "Compute 9 * 9. Reply with the number only.", "81"),
                task("m5", "Compute 144 / 12. Reply with the number only.", "12"),
            ],
        },
        Dataset {
            id: "mcq_sample".to_string(),
            scorer: "mcq_letter".to_string(),
            source: "builtin".to_string(),
            tasks: vec![
                task(
                    "c1",
                    "Which is a primary color? (A) Green (B) Red (C) Purple (D) Brown. Answer with the letter.",
                    "B",
                ),
                task(
                    "c2",
                    "Which planet is largest? (A) Earth (B) Mars (C) Jupiter (D) Venus. Answer with the letter.",
                    "C",
                ),
                task(
                    "c3",
                    "What is H2O? (A) Salt (B) Water (C) Oxygen (D) Hydrogen. Answer with the letter.",
                    "B",
                ),
                task(
                    "c4",
                    "Which is a prime number? (A) 4 (B) 6 (C) 9 (D) 7. Answer with the letter.",
                    "D",
                ),
            ],
        },
    ]
}

/// Find a built-in dataset by id.
pub fn dataset_by_id(id: &str) -> Option<Dataset> {
    builtin_datasets().into_iter().find(|d| d.id == id)
}

/// Run `project` against every task in `dataset`, scoring each prediction.
///
/// `base_workspace` is a directory under which a per-task scratch workspace is
/// created. Each task runs sequentially so token/latency accounting is simple
/// and the host LLM provider isn't hammered concurrently.
pub async fn run_eval(
    project: &Project,
    dataset: &Dataset,
    base_workspace: &Path,
) -> Result<EvalRun, String> {
    let scorer = if harness_rt::scoring::SCORERS.contains(&dataset.scorer.as_str()) {
        dataset.scorer.clone()
    } else {
        return Err(format!("unknown scorer '{}'", dataset.scorer));
    };

    let mut results: Vec<TaskResult> = Vec::with_capacity(dataset.tasks.len());
    let mut total_tokens: u32 = 0;
    let mut total_latency: f64 = 0.0;

    for t in &dataset.tasks {
        let ws = base_workspace.join(format!("eval-{}-{}", dataset.id, t.id));
        let _ = std::fs::create_dir_all(&ws);

        let (tx, mut rx) = mpsc::channel::<UiEvent>(256);
        // Drain events (we only need the final outcome for scoring).
        let drain = tokio::spawn(async move { while rx.recv().await.is_some() {} });
        let cancel = Arc::new(AtomicBool::new(false));

        let started = Instant::now();
        let outcome = runner::run_project_task(project, &t.input, &ws, tx, cancel).await;
        let latency = started.elapsed().as_secs_f64();
        let _ = drain.await;
        total_latency += latency;

        let tr = match outcome {
            Ok(out) => {
                let toks = out.total_input_tokens + out.total_output_tokens;
                total_tokens = total_tokens.saturating_add(toks);
                let s = harness_rt::scoring::score(&scorer, &out.result, &t.expected, &t.params);
                TaskResult {
                    task_id: t.id.clone(),
                    score: s,
                    prediction: out.result,
                    expected: t.expected.clone(),
                    latency_s: latency,
                    est_tokens: toks,
                    error: None,
                }
            }
            Err(e) => TaskResult {
                task_id: t.id.clone(),
                score: 0.0,
                prediction: String::new(),
                expected: t.expected.clone(),
                latency_s: latency,
                est_tokens: 0,
                error: Some(e),
            },
        };
        results.push(tr);
    }

    let n = results.len().max(1) as f64;
    let accuracy = results.iter().map(|r| r.score).sum::<f64>() / n;
    let avg_latency = (total_latency / n * 100.0).round() / 100.0;

    Ok(EvalRun {
        id: new_id(),
        project_id: project.id.clone(),
        project_name: project.name.clone(),
        dataset: dataset.id.clone(),
        scorer,
        accuracy,
        task_results: results,
        metrics: EvalMetrics {
            avg_latency,
            total_est_tokens: total_tokens,
        },
        created_at: now_secs(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn datasets_are_well_formed() {
        for d in builtin_datasets() {
            assert!(!d.tasks.is_empty(), "{} has tasks", d.id);
            assert!(
                harness_rt::scoring::SCORERS.contains(&d.scorer.as_str()),
                "{} uses a real scorer",
                d.id
            );
        }
        assert!(dataset_by_id("smoke_qa").is_some());
        assert!(dataset_by_id("nope").is_none());
    }
}
