//! SQLite-backed project + layout store (P2).
//!
//! Replaces P1's in-memory map with a `rusqlite` (bundled) store so projects,
//! their canvas layout and revisions survive restarts. It also owns the
//! transactional Edit-Ops entry point used by the visual designer: validate →
//! diff → (optionally) persist, with `base_rev` optimistic-concurrency checks.

use std::collections::BTreeMap;
use std::path::Path;

use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::agent_library::{sample_library_agents, LibraryAgent, LibraryAgentSummary};
use crate::ops::{apply_ops as ops_apply_ops, diff_projects};
use crate::project::{new_id, now_secs, AgentSpec, ComponentRef, Project, Topology};
use harness_rt::agent::declarative::DeclarativeAlgorithm;

use crate::eval::{EvalRun, EvalRunSummary};
use crate::registry;
use crate::runner::NodeMetrics;
use crate::skills::InstalledSkill;

/// A node position on the canvas.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Pos {
    pub x: f64,
    pub y: f64,
}

/// Canvas view-state, stored separately from the runnable `Project` so it never
/// pollutes codegen/eval. Nested view bits stay as raw JSON for forward-compat.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Layout {
    #[serde(default)]
    pub nodes: BTreeMap<String, Pos>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub viewport: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub collapsed: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session: Option<Value>,
}

/// Minimal `{id, name}` summary returned by the project list.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectSummary {
    pub id: String,
    pub name: String,
}

/// The successful result of applying edit ops (matches the TS `OpsResult`).
#[derive(Debug, Clone, Serialize)]
pub struct OpsResult {
    pub project: Project,
    pub diff: BTreeMap<String, Value>,
    pub applied: usize,
    pub dry_run: bool,
    pub rev: u64,
}

/// A single-shot run launched from the canvas, persisted for the run console
/// and the session tree. Matches the TS `CanvasRun`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanvasRun {
    pub id: String,
    pub project_id: String,
    #[serde(default)]
    pub project_rev: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    pub task: String,
    pub result: String,
    pub created_at: f64,
    #[serde(default)]
    pub duration_ms: u64,
    #[serde(default)]
    pub total_tokens: u32,
    #[serde(default)]
    pub metrics: BTreeMap<String, NodeMetrics>,
}

/// A node in the branching session tree (a run, for now).
#[derive(Debug, Clone, Serialize)]
pub struct SessionNode {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub parent: Option<String>,
    pub task: String,
    pub result: String,
    pub created_at: f64,
    pub total_tokens: u32,
}

/// The session tree payload (matches the TS `sessionTree` response).
#[derive(Debug, Clone, Serialize)]
pub struct SessionTree {
    pub nodes: Vec<SessionNode>,
    pub roots: Vec<String>,
}

/// Why an `apply_ops` call could not be persisted.
#[derive(Debug)]
pub enum OpsError {
    NotFound,
    /// `base_rev` did not match the stored revision (a stale edit).
    Stale {
        current_rev: u64,
        project: Project,
    },
    /// One or more ops failed validation (nothing applied).
    Invalid {
        problems: Vec<String>,
    },
}

pub struct Store {
    conn: Connection,
}

impl Store {
    /// Open (creating if needed) a SQLite-backed store at `path`.
    pub fn open<P: AsRef<Path>>(path: P) -> rusqlite::Result<Self> {
        let conn = Connection::open(path)?;
        let store = Store { conn };
        store.init()?;
        Ok(store)
    }

    /// An ephemeral in-memory store (used by tests and the headless fallback).
    pub fn in_memory() -> Self {
        let conn = Connection::open_in_memory().expect("open in-memory sqlite");
        let store = Store { conn };
        store.init().expect("init schema");
        store
    }

    fn init(&self) -> rusqlite::Result<()> {
        self.conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS projects (
                 id         TEXT PRIMARY KEY,
                 name       TEXT NOT NULL,
                 data       TEXT NOT NULL,
                 updated_at REAL NOT NULL
             );
             CREATE TABLE IF NOT EXISTS layouts (
                 project_id TEXT PRIMARY KEY,
                 data       TEXT NOT NULL
             );
             CREATE TABLE IF NOT EXISTS canvas_runs (
                 id         TEXT PRIMARY KEY,
                 project_id TEXT NOT NULL,
                 created_at REAL NOT NULL,
                 data       TEXT NOT NULL
             );
             CREATE INDEX IF NOT EXISTS idx_runs_project
                 ON canvas_runs (project_id, created_at DESC);
             CREATE TABLE IF NOT EXISTS library_agents (
                 id         TEXT PRIMARY KEY,
                 name       TEXT NOT NULL,
                 data       TEXT NOT NULL,
                 updated_at REAL NOT NULL
             );
             CREATE TABLE IF NOT EXISTS skills (
                 slug         TEXT PRIMARY KEY,
                 name         TEXT NOT NULL,
                 data         TEXT NOT NULL,
                 installed_at REAL NOT NULL
             );
             CREATE TABLE IF NOT EXISTS eval_runs (
                 id         TEXT PRIMARY KEY,
                 project_id TEXT NOT NULL,
                 created_at REAL NOT NULL,
                 data       TEXT NOT NULL
             );
             CREATE INDEX IF NOT EXISTS idx_evals_project
                 ON eval_runs (project_id, created_at DESC);
             CREATE TABLE IF NOT EXISTS algorithms (
                 name       TEXT PRIMARY KEY,
                 kind       TEXT NOT NULL,
                 data       TEXT NOT NULL,
                 created_at REAL NOT NULL
             );",
        )
    }

    /// Seed sample project + library agents the first time the store is empty.
    pub fn seed_if_empty(&self) -> rusqlite::Result<()> {
        let count: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM projects", [], |r| r.get(0))?;
        if count == 0 {
            self.put_project(&sample_project())?;
        }
        let lib_count: i64 =
            self.conn
                .query_row("SELECT COUNT(*) FROM library_agents", [], |r| r.get(0))?;
        if lib_count == 0 {
            for agent in sample_library_agents() {
                let _ = self.put_library_agent(&agent);
            }
        }
        Ok(())
    }

    fn put_project(&self, project: &Project) -> rusqlite::Result<()> {
        let data = serde_json::to_string(project).expect("serialize project");
        self.conn.execute(
            "INSERT INTO projects (id, name, data, updated_at) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(id) DO UPDATE SET name = ?2, data = ?3, updated_at = ?4",
            rusqlite::params![project.id, project.name, data, project.updated_at],
        )?;
        Ok(())
    }

    pub fn list_projects(&self) -> Vec<ProjectSummary> {
        let mut stmt = match self
            .conn
            .prepare("SELECT id, name FROM projects ORDER BY updated_at DESC")
        {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        let rows = stmt
            .query_map([], |r| {
                Ok(ProjectSummary {
                    id: r.get(0)?,
                    name: r.get(1)?,
                })
            })
            .map(|it| it.filter_map(Result::ok).collect())
            .unwrap_or_default();
        rows
    }

    pub fn get_project(&self, id: &str) -> Option<Project> {
        let data: Option<String> = self
            .conn
            .query_row("SELECT data FROM projects WHERE id = ?1", [id], |r| {
                r.get(0)
            })
            .optional()
            .ok()
            .flatten();
        data.and_then(|d| serde_json::from_str(&d).ok())
    }

    pub fn create_project(&self, mut project: Project) -> Project {
        project.id = new_id();
        project.rev = 0;
        project.created_at = now_secs();
        project.updated_at = now_secs();
        let _ = self.put_project(&project);
        project
    }

    pub fn update_project(&self, id: &str, mut project: Project) -> Option<Project> {
        let existing = self.get_project(id)?;
        project.id = id.to_string();
        project.created_at = existing.created_at;
        project.rev = existing.rev + 1;
        project.updated_at = now_secs();
        self.put_project(&project).ok()?;
        Some(project)
    }

    pub fn delete_project(&self, id: &str) -> bool {
        let _ = self
            .conn
            .execute("DELETE FROM layouts WHERE project_id = ?1", [id]);
        self.conn
            .execute("DELETE FROM projects WHERE id = ?1", [id])
            .map(|n| n > 0)
            .unwrap_or(false)
    }

    // ── Agent library ─────────────────────────────────────────────────────────

    fn put_library_agent(&self, agent: &LibraryAgent) -> rusqlite::Result<()> {
        let data = serde_json::to_string(agent).expect("serialize library agent");
        self.conn.execute(
            "INSERT INTO library_agents (id, name, data, updated_at) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(id) DO UPDATE SET name = ?2, data = ?3, updated_at = ?4",
            rusqlite::params![agent.id, agent.name, data, agent.updated_at],
        )?;
        Ok(())
    }

    pub fn list_library_agents(&self) -> Vec<LibraryAgentSummary> {
        let mut stmt = match self
            .conn
            .prepare("SELECT data FROM library_agents ORDER BY updated_at DESC")
        {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        stmt.query_map([], |r| r.get::<_, String>(0))
            .map(|rows| {
                rows.filter_map(|row| row.ok())
                    .filter_map(|d| serde_json::from_str::<LibraryAgent>(&d).ok())
                    .map(|a| a.summary())
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn get_library_agent(&self, id: &str) -> Option<LibraryAgent> {
        let data: Option<String> = self
            .conn
            .query_row("SELECT data FROM library_agents WHERE id = ?1", [id], |r| {
                r.get(0)
            })
            .optional()
            .ok()
            .flatten();
        data.and_then(|d| serde_json::from_str(&d).ok())
    }

    pub fn create_library_agent(&self, mut agent: LibraryAgent) -> LibraryAgent {
        if agent.id.is_empty() {
            agent.id = new_id();
        }
        agent.created_at = now_secs();
        agent.updated_at = agent.created_at;
        agent.spec.name = agent.name.clone();
        let _ = self.put_library_agent(&agent);
        agent
    }

    pub fn update_library_agent(&self, id: &str, mut agent: LibraryAgent) -> Option<LibraryAgent> {
        let existing = self.get_library_agent(id)?;
        agent.id = id.to_string();
        agent.created_at = existing.created_at;
        agent.updated_at = now_secs();
        agent.spec.name = agent.name.clone();
        self.put_library_agent(&agent).ok()?;
        Some(agent)
    }

    pub fn delete_library_agent(&self, id: &str) -> bool {
        self.conn
            .execute("DELETE FROM library_agents WHERE id = ?1", [id])
            .map(|n| n > 0)
            .unwrap_or(false)
    }

    // ── Skills (ClawHub-installed SKILL.md packages) ────────────────────────

    /// Insert or update an installed skill, keyed by `slug`.
    pub fn put_skill(&self, skill: &InstalledSkill) -> rusqlite::Result<()> {
        let data = serde_json::to_string(skill).expect("serialize skill");
        self.conn.execute(
            "INSERT INTO skills (slug, name, data, installed_at) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(slug) DO UPDATE SET name = ?2, data = ?3, installed_at = ?4",
            rusqlite::params![skill.slug, skill.name, data, skill.installed_at],
        )?;
        Ok(())
    }

    /// All installed skills, most-recently-installed first.
    pub fn list_skills(&self) -> Vec<InstalledSkill> {
        let mut stmt = match self
            .conn
            .prepare("SELECT data FROM skills ORDER BY installed_at DESC")
        {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        stmt.query_map([], |r| r.get::<_, String>(0))
            .map(|rows| {
                rows.filter_map(|row| row.ok())
                    .filter_map(|d| serde_json::from_str::<InstalledSkill>(&d).ok())
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn get_skill(&self, slug: &str) -> Option<InstalledSkill> {
        let data: Option<String> = self
            .conn
            .query_row("SELECT data FROM skills WHERE slug = ?1", [slug], |r| {
                r.get(0)
            })
            .optional()
            .ok()
            .flatten();
        data.and_then(|d| serde_json::from_str(&d).ok())
    }

    pub fn delete_skill(&self, slug: &str) -> bool {
        self.conn
            .execute("DELETE FROM skills WHERE slug = ?1", [slug])
            .map(|n| n > 0)
            .unwrap_or(false)
    }

    /// Toggle a skill's enabled flag; returns the updated skill.
    pub fn set_skill_enabled(&self, slug: &str, enabled: bool) -> Option<InstalledSkill> {
        let mut skill = self.get_skill(slug)?;
        skill.enabled = enabled;
        self.put_skill(&skill).ok()?;
        Some(skill)
    }

    // ── Eval runs ───────────────────────────────────────────────────────────

    /// Persist a completed eval run.
    pub fn put_eval_run(&self, run: &EvalRun) -> rusqlite::Result<()> {
        let data = serde_json::to_string(run).expect("serialize eval run");
        self.conn.execute(
            "INSERT INTO eval_runs (id, project_id, created_at, data) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(id) DO UPDATE SET project_id = ?2, created_at = ?3, data = ?4",
            rusqlite::params![run.id, run.project_id, run.created_at, data],
        )?;
        Ok(())
    }

    pub fn get_eval_run(&self, id: &str) -> Option<EvalRun> {
        let data: Option<String> = self
            .conn
            .query_row("SELECT data FROM eval_runs WHERE id = ?1", [id], |r| {
                r.get(0)
            })
            .optional()
            .ok()
            .flatten();
        data.and_then(|d| serde_json::from_str(&d).ok())
    }

    /// Eval runs, optionally filtered to one project, newest first.
    pub fn list_eval_runs(&self, project_id: Option<&str>) -> Vec<EvalRun> {
        let (sql, has_filter) = match project_id {
            Some(_) => (
                "SELECT data FROM eval_runs WHERE project_id = ?1 ORDER BY created_at DESC",
                true,
            ),
            None => ("SELECT data FROM eval_runs ORDER BY created_at DESC", false),
        };
        let mut stmt = match self.conn.prepare(sql) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        let map = |row: &rusqlite::Row| row.get::<_, String>(0);
        let rows = if has_filter {
            stmt.query_map([project_id.unwrap()], map)
        } else {
            stmt.query_map([], map)
        };
        rows.map(|it| {
            it.filter_map(Result::ok)
                .filter_map(|d| serde_json::from_str::<EvalRun>(&d).ok())
                .collect()
        })
        .unwrap_or_default()
    }

    // ── Declarative algorithms (meta-agent hot-registered) ──────────────────

    /// Persist a declarative algorithm (last writer wins by name).
    pub fn put_algorithm(&self, algo: &DeclarativeAlgorithm) -> rusqlite::Result<()> {
        let data = serde_json::to_string(algo).expect("serialize algorithm");
        self.conn.execute(
            "INSERT INTO algorithms (name, kind, data, created_at) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(name) DO UPDATE SET kind = ?2, data = ?3, created_at = ?4",
            rusqlite::params![algo.name, algo.kind, data, crate::project::now_secs()],
        )?;
        Ok(())
    }

    /// All persisted declarative algorithms.
    pub fn list_algorithms(&self) -> Vec<DeclarativeAlgorithm> {
        let mut stmt = match self
            .conn
            .prepare("SELECT data FROM algorithms ORDER BY created_at DESC")
        {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        stmt.query_map([], |r| r.get::<_, String>(0))
            .map(|rows| {
                rows.filter_map(|row| row.ok())
                    .filter_map(|d| serde_json::from_str::<DeclarativeAlgorithm>(&d).ok())
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn delete_algorithm(&self, name: &str) -> bool {
        self.conn
            .execute("DELETE FROM algorithms WHERE name = ?1", [name])
            .map(|n| n > 0)
            .unwrap_or(false)
    }

    /// Top eval runs by accuracy (leaderboard), newest-as-tiebreak.
    pub fn leaderboard(&self, limit: usize) -> Vec<EvalRunSummary> {
        let mut runs = self.list_eval_runs(None);
        runs.sort_by(|a, b| {
            b.accuracy
                .partial_cmp(&a.accuracy)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(
                    b.created_at
                        .partial_cmp(&a.created_at)
                        .unwrap_or(std::cmp::Ordering::Equal),
                )
        });
        runs.into_iter().take(limit).map(|r| r.summary()).collect()
    }

    pub fn get_layout(&self, id: &str) -> Layout {
        let data: Option<String> = self
            .conn
            .query_row(
                "SELECT data FROM layouts WHERE project_id = ?1",
                [id],
                |r| r.get(0),
            )
            .optional()
            .ok()
            .flatten();
        data.and_then(|d| serde_json::from_str(&d).ok())
            .unwrap_or_default()
    }

    pub fn save_layout(&self, id: &str, layout: Layout) -> Layout {
        let data = serde_json::to_string(&layout).expect("serialize layout");
        let _ = self.conn.execute(
            "INSERT INTO layouts (project_id, data) VALUES (?1, ?2)
             ON CONFLICT(project_id) DO UPDATE SET data = ?2",
            rusqlite::params![id, data],
        );
        layout
    }

    // ── Canvas runs / session tree (P3) ─────────────────────────────────────

    pub fn save_canvas_run(&self, run: &CanvasRun) {
        let data = serde_json::to_string(run).expect("serialize run");
        let _ = self.conn.execute(
            "INSERT INTO canvas_runs (id, project_id, created_at, data) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(id) DO UPDATE SET data = ?4",
            rusqlite::params![run.id, run.project_id, run.created_at, data],
        );
    }

    pub fn list_canvas_runs(&self, project_id: &str, limit: u32) -> Vec<CanvasRun> {
        let mut stmt = match self.conn.prepare(
            "SELECT data FROM canvas_runs WHERE project_id = ?1 ORDER BY created_at DESC LIMIT ?2",
        ) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        stmt.query_map(rusqlite::params![project_id, limit], |r| {
            r.get::<_, String>(0)
        })
        .map(|it| {
            it.filter_map(Result::ok)
                .filter_map(|d| serde_json::from_str(&d).ok())
                .collect()
        })
        .unwrap_or_default()
    }

    pub fn get_canvas_run(&self, run_id: &str) -> Option<CanvasRun> {
        let data: Option<String> = self
            .conn
            .query_row(
                "SELECT data FROM canvas_runs WHERE id = ?1",
                [run_id],
                |r| r.get(0),
            )
            .optional()
            .ok()
            .flatten();
        data.and_then(|d| serde_json::from_str(&d).ok())
    }

    /// Build the branching session tree from a project's runs (newest first).
    pub fn session_tree(&self, project_id: &str, limit: u32) -> SessionTree {
        let runs = self.list_canvas_runs(project_id, limit);
        let run_ids: std::collections::BTreeSet<String> =
            runs.iter().map(|r| r.id.clone()).collect();
        let nodes: Vec<SessionNode> = runs
            .iter()
            .map(|r| {
                let parent = r.parent.as_ref().filter(|p| run_ids.contains(*p)).cloned();
                SessionNode {
                    id: r.id.clone(),
                    kind: "run".to_string(),
                    parent,
                    task: r.task.clone(),
                    result: r.result.chars().take(200).collect(),
                    created_at: r.created_at,
                    total_tokens: r.total_tokens,
                }
            })
            .collect();
        let roots = nodes
            .iter()
            .filter(|n| n.parent.is_none())
            .map(|n| n.id.clone())
            .collect();
        SessionTree { nodes, roots }
    }

    /// Validate + apply structured edit ops transactionally (all-or-nothing).
    ///
    /// With `dry_run` the change is previewed (diff returned) but not saved.
    /// `base_rev`, when given, must match the stored revision or the edit is
    /// rejected as stale.
    pub fn apply_ops(
        &self,
        id: &str,
        ops: &[Value],
        dry_run: bool,
        base_rev: Option<u64>,
        extra_names: &[(String, String)],
    ) -> Result<OpsResult, OpsError> {
        let project = self.get_project(id).ok_or(OpsError::NotFound)?;

        if !dry_run {
            if let Some(base) = base_rev {
                if base != project.rev {
                    return Err(OpsError::Stale {
                        current_rev: project.rev,
                        project,
                    });
                }
            }
        }

        let names = registry::names_by_kind_merged(extra_names);
        let tools = registry::tool_names();
        let result = ops_apply_ops(&project, ops, &names, &tools);
        if !result.ok() {
            return Err(OpsError::Invalid {
                problems: result.problems,
            });
        }

        let diff = diff_projects(&project, &result.project);
        let mut next = result.project;

        let rev = if dry_run {
            next.rev
        } else {
            // Persist with a bumped revision, preserving id + created_at.
            next.id = id.to_string();
            next.created_at = project.created_at;
            next.rev = project.rev + 1;
            next.updated_at = now_secs();
            let _ = self.put_project(&next);
            next.rev
        };

        Ok(OpsResult {
            project: next,
            diff,
            applied: result.applied,
            dry_run,
            rev,
        })
    }
}

/// A small coordinated two-agent project (solver + critic, debate topology).
fn sample_project() -> Project {
    let mut solver = AgentSpec::new("solver");
    solver.role = "Proposes an answer.".to_string();
    solver.system_prompt = "You are a careful problem solver.".to_string();

    let mut critic = AgentSpec::new("critic");
    critic.role = "Reviews and critiques the solver.".to_string();
    critic.system_prompt = "You critique answers and suggest fixes.".to_string();
    critic.r#loop = Some(ComponentRef::new("reflexion"));
    critic.context = Some(ComponentRef::new("rag"));

    Project {
        id: "sample".to_string(),
        name: "Sample · Solver+Critic".to_string(),
        description: "A seeded example: a solver and a critic coordinated by debate.".to_string(),
        agents: vec![solver, critic],
        topology: Topology {
            mode: "coordinated".to_string(),
            coordinator: Some(ComponentRef::new("debate")),
            agent_order: vec!["solver".to_string(), "critic".to_string()],
        },
        datasets: Vec::new(),
        scorer: Some(ComponentRef::new("f1")),
        rev: 0,
        created_at: now_secs(),
        updated_at: now_secs(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn seeded() -> Store {
        let s = Store::in_memory();
        s.seed_if_empty().unwrap();
        s
    }

    #[test]
    fn seeded_store_has_sample_project() {
        let store = seeded();
        assert_eq!(store.list_projects().len(), 1);
        let p = store.get_project("sample").expect("sample project");
        assert_eq!(p.agents.len(), 2);
        assert_eq!(p.topology.mode, "coordinated");
    }

    #[test]
    fn crud_round_trip_and_persistence() {
        let store = Store::in_memory();
        let created = store.create_project(Project {
            id: String::new(),
            name: "Temp".to_string(),
            description: String::new(),
            agents: vec![AgentSpec::new("a1")],
            topology: Topology::default(),
            datasets: vec![],
            scorer: None,
            rev: 0,
            created_at: 0.0,
            updated_at: 0.0,
        });
        assert!(!created.id.is_empty());
        let reloaded = store.get_project(&created.id).expect("reload");
        assert_eq!(reloaded.name, "Temp");

        let mut p = reloaded;
        p.name = "Renamed".to_string();
        let updated = store.update_project(&created.id, p).expect("update");
        assert_eq!(updated.rev, 1);
        assert!(store.delete_project(&created.id));
        assert!(store.get_project(&created.id).is_none());
    }

    #[test]
    fn apply_ops_transactional_and_bumps_rev() {
        let store = seeded();
        // A valid 2-op batch: add an agent, then set its loop slot.
        let ops = vec![
            json!({ "op": "add_agent", "name": "verifier" }),
            json!({ "op": "set_slot", "agent": "verifier", "kind": "loop", "ref": "reflexion" }),
        ];
        let res = store
            .apply_ops("sample", &ops, false, Some(0), &[])
            .expect("apply");
        assert_eq!(res.applied, 2);
        assert_eq!(res.rev, 1);
        let p = store.get_project("sample").unwrap();
        assert_eq!(p.agents.len(), 3);
        assert_eq!(
            p.agent("verifier").unwrap().r#loop.as_ref().unwrap().name,
            "reflexion"
        );
    }

    #[test]
    fn apply_ops_all_or_nothing_on_invalid() {
        let store = seeded();
        let before = store.get_project("sample").unwrap();
        // Second op references a non-existent component → whole batch rejected.
        let ops = vec![
            json!({ "op": "add_agent", "name": "x" }),
            json!({ "op": "set_slot", "agent": "x", "kind": "loop", "ref": "does_not_exist" }),
        ];
        let err = store
            .apply_ops("sample", &ops, false, None, &[])
            .unwrap_err();
        match err {
            OpsError::Invalid { problems } => assert_eq!(problems.len(), 1),
            _ => panic!("expected Invalid"),
        }
        // Nothing persisted.
        let after = store.get_project("sample").unwrap();
        assert_eq!(after.agents.len(), before.agents.len());
        assert_eq!(after.rev, before.rev);
    }

    #[test]
    fn apply_ops_dry_run_does_not_persist() {
        let store = seeded();
        let ops = vec![json!({ "op": "add_agent", "name": "ghost" })];
        let res = store
            .apply_ops("sample", &ops, true, None, &[])
            .expect("dry run");
        assert!(res.dry_run);
        assert_eq!(res.project.agents.len(), 3); // preview includes the add
                                                 // …but the stored project is unchanged.
        assert_eq!(store.get_project("sample").unwrap().agents.len(), 2);
    }

    #[test]
    fn apply_ops_stale_base_rev_rejected() {
        let store = seeded();
        let err = store
            .apply_ops(
                "sample",
                &[json!({ "op": "clear_scorer" })],
                false,
                Some(99),
                &[],
            )
            .unwrap_err();
        assert!(matches!(err, OpsError::Stale { .. }));
    }
}
