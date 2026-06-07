//! In-memory project + layout store.
//!
//! P1 keeps everything in memory (seeded with a sample project so the canvas
//! has something to render). P2 swaps this for a `rusqlite`-backed store with
//! the same surface, and adds the transactional Edit-Ops engine.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::project::{now_secs, new_id, AgentSpec, ComponentRef, Project, Topology};

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
    pub viewport: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub collapsed: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session: Option<serde_json::Value>,
}

/// Minimal `{id, name}` summary returned by the project list.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectSummary {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Default)]
pub struct Store {
    projects: BTreeMap<String, Project>,
    layouts: BTreeMap<String, Layout>,
}

impl Store {
    pub fn new() -> Self {
        Store::default()
    }

    /// A store pre-populated with one illustrative multi-agent project.
    pub fn seeded() -> Self {
        let mut store = Store::new();
        store.insert_project(sample_project());
        store
    }

    fn insert_project(&mut self, mut project: Project) {
        if project.id.is_empty() {
            project.id = new_id();
        }
        if project.created_at == 0.0 {
            project.created_at = now_secs();
        }
        project.updated_at = now_secs();
        self.projects.insert(project.id.clone(), project);
    }

    pub fn list_projects(&self) -> Vec<ProjectSummary> {
        self.projects
            .values()
            .map(|p| ProjectSummary {
                id: p.id.clone(),
                name: p.name.clone(),
            })
            .collect()
    }

    pub fn get_project(&self, id: &str) -> Option<Project> {
        self.projects.get(id).cloned()
    }

    pub fn create_project(&mut self, mut project: Project) -> Project {
        project.id = new_id();
        project.rev = 0;
        project.created_at = now_secs();
        project.updated_at = now_secs();
        self.projects.insert(project.id.clone(), project.clone());
        project
    }

    pub fn update_project(&mut self, id: &str, mut project: Project) -> Option<Project> {
        let existing = self.projects.get(id)?;
        project.id = id.to_string();
        project.created_at = existing.created_at;
        project.rev = existing.rev + 1;
        project.updated_at = now_secs();
        self.projects.insert(id.to_string(), project.clone());
        Some(project)
    }

    pub fn delete_project(&mut self, id: &str) -> bool {
        self.layouts.remove(id);
        self.projects.remove(id).is_some()
    }

    pub fn get_layout(&self, id: &str) -> Layout {
        self.layouts.get(id).cloned().unwrap_or_default()
    }

    pub fn save_layout(&mut self, id: &str, layout: Layout) -> Layout {
        self.layouts.insert(id.to_string(), layout.clone());
        layout
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

    #[test]
    fn seeded_store_has_sample_project() {
        let store = Store::seeded();
        let list = store.list_projects();
        assert_eq!(list.len(), 1);
        let p = store.get_project("sample").expect("sample project");
        assert_eq!(p.agents.len(), 2);
        assert_eq!(p.topology.mode, "coordinated");
    }

    #[test]
    fn crud_round_trip() {
        let mut store = Store::new();
        let mut p = Project {
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
        };
        let created = store.create_project(p.clone());
        assert!(!created.id.is_empty());
        assert_eq!(created.rev, 0);

        p = created.clone();
        p.name = "Renamed".to_string();
        let updated = store.update_project(&created.id, p).expect("update");
        assert_eq!(updated.name, "Renamed");
        assert_eq!(updated.rev, 1);

        assert!(store.delete_project(&created.id));
        assert!(store.get_project(&created.id).is_none());
    }

    #[test]
    fn agent_loop_field_serializes_as_loop() {
        // The `r#loop` raw identifier must serialize to the JSON key "loop" so
        // the ported frontend (which reads `agent.loop`) round-trips unchanged.
        let agent = AgentSpec::new("a");
        let json = serde_json::to_value(&agent).unwrap();
        assert_eq!(json["loop"]["name"], "react");
        assert!(json.get("r#loop").is_none());

        // And it deserializes back from "loop".
        let parsed: AgentSpec = serde_json::from_value(json).unwrap();
        assert_eq!(parsed.r#loop.unwrap().name, "react");
    }
}
