//! Project templates — one-click starting points for the visual designer.

use crate::project::{new_id, now_secs, AgentSpec, ComponentRef, Project, Topology};

#[derive(Debug, Clone, serde::Serialize)]
pub struct ProjectTemplate {
    pub id: String,
    pub name: String,
    pub description: String,
}

const TEMPLATES: &[(&str, &str, &str)] = &[
    (
        "single-react",
        "Single agent (ReAct)",
        "One general-purpose ReAct agent.",
    ),
    (
        "debate-duo",
        "Solver + critic (debate)",
        "Two agents coordinated by the debate algorithm.",
    ),
    (
        "supervisor-team",
        "Supervisor + workers",
        "A supervisor delegates to two worker agents.",
    ),
];

pub fn list_templates() -> Vec<ProjectTemplate> {
    TEMPLATES
        .iter()
        .map(|(id, name, desc)| ProjectTemplate {
            id: (*id).into(),
            name: (*name).into(),
            description: (*desc).into(),
        })
        .collect()
}

pub fn instantiate_template(id: &str, display_name: Option<&str>) -> Result<Project, String> {
    let name = display_name.unwrap_or(match id {
        "single-react" => "My Agent",
        "debate-duo" => "Debate Team",
        "supervisor-team" => "Supervisor Team",
        _ => "New Project",
    });
    let now = now_secs();
    let project = match id {
        "single-react" => Project {
            id: new_id(),
            name: name.into(),
            description: "Single ReAct agent starter.".into(),
            agents: vec![AgentSpec::new("solver")],
            topology: Topology {
                mode: "single".into(),
                coordinator: None,
                agent_order: vec!["solver".into()],
            },
            datasets: vec![],
            scorer: None,
            rev: 0,
            created_at: now,
            updated_at: now,
        },
        "debate-duo" => {
            let mut critic = AgentSpec::new("critic");
            critic.r#loop = Some(ComponentRef::new("reflexion"));
            Project {
                id: new_id(),
                name: name.into(),
                description: "Solver and critic in a debate topology.".into(),
                agents: vec![AgentSpec::new("solver"), critic],
                topology: Topology {
                    mode: "coordinated".into(),
                    coordinator: Some(ComponentRef::new("debate")),
                    agent_order: vec!["solver".into(), "critic".into()],
                },
                datasets: vec![],
                scorer: None,
                rev: 0,
                created_at: now,
                updated_at: now,
            }
        }
        "supervisor-team" => Project {
            id: new_id(),
            name: name.into(),
            description: "Supervisor with two worker agents.".into(),
            agents: vec![
                AgentSpec::new("supervisor"),
                AgentSpec::new("worker_a"),
                AgentSpec::new("worker_b"),
            ],
            topology: Topology {
                mode: "coordinated".into(),
                coordinator: Some(ComponentRef::new("supervisor")),
                agent_order: vec!["supervisor".into(), "worker_a".into(), "worker_b".into()],
            },
            datasets: vec![],
            scorer: None,
            rev: 0,
            created_at: now,
            updated_at: now,
        },
        other => return Err(format!("unknown template {other:?}")),
    };
    Ok(project)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn templates_instantiate() {
        for t in list_templates() {
            assert!(instantiate_template(&t.id, None).is_ok());
        }
    }
}
