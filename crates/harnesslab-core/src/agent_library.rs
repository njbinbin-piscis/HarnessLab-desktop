//! Reusable agent presets stored outside projects (the Agent Library).

use serde::{Deserialize, Serialize};

use crate::project::{new_id, now_secs, AgentSpec};

/// A persisted agent template users compose in the Agent Library.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LibraryAgent {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub spec: AgentSpec,
    #[serde(default)]
    pub created_at: f64,
    #[serde(default)]
    pub updated_at: f64,
}

/// `{id, name, description, model}` row for list views.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LibraryAgentSummary {
    pub id: String,
    pub name: String,
    pub description: String,
    pub model: String,
}

impl LibraryAgent {
    pub fn new(name: &str, description: &str, mut spec: AgentSpec) -> Self {
        spec.name = name.to_string();
        let now = now_secs();
        LibraryAgent {
            id: new_id(),
            name: name.to_string(),
            description: description.to_string(),
            spec,
            created_at: now,
            updated_at: now,
        }
    }

    pub fn summary(&self) -> LibraryAgentSummary {
        LibraryAgentSummary {
            id: self.id.clone(),
            name: self.name.clone(),
            description: self.description.clone(),
            model: self.spec.model.clone(),
        }
    }

    /// Copy the spec for embedding into a project (unique agent name within project).
    pub fn to_project_agent(&self) -> AgentSpec {
        let mut a = self.spec.clone();
        a.name = self.name.clone();
        a
    }
}

/// Seed agents for first-run demos.
pub fn sample_library_agents() -> Vec<LibraryAgent> {
    let mut solver = AgentSpec::new("solver");
    solver.role = "Proposes an answer.".into();
    solver.system_prompt =
        "You are a careful problem solver. Reply with FINAL ANSWER: when done.".into();

    let mut critic = AgentSpec::new("critic");
    critic.role = "Reviews and critiques.".into();
    critic.system_prompt =
        "You critique answers and suggest fixes. Reply with FINAL ANSWER: when done.".into();
    critic.r#loop = Some(crate::project::ComponentRef::new("reflexion"));

    vec![
        LibraryAgent::new("solver", "General problem solver (react loop)", solver),
        LibraryAgent::new("critic", "Reflexion-style critic", critic),
        architect_agent(),
    ]
}

/// The system-assembled **Architect** meta-agent.
///
/// Unlike user-authored presets, the Architect is assembled by the system with a
/// full meta-capability tool set: it can read/write/edit files, run shell
/// commands, search the web, and — crucially — `register_algorithm` to author and
/// hot-register brand-new loop strategies into the running engine. It is wired
/// with the reflexion loop so it self-checks its designs before finishing.
pub fn architect_agent() -> LibraryAgent {
    let mut architect = AgentSpec::new("Architect");
    architect.role = "System meta-agent: designs and hot-registers new agent algorithms.".into();
    architect.system_prompt = ARCHITECT_PROMPT.to_string();
    architect.r#loop = Some(crate::project::ComponentRef::new("reflexion"));
    // Full meta tool set — the Architect can inspect the repo, run commands, and
    // register new declarative algorithms live.
    architect.enabled_tools = Some(vec![
        "file_read".into(),
        "file_write".into(),
        "file_edit".into(),
        "file_list".into(),
        "file_search".into(),
        "shell".into(),
        "code_run".into(),
        "web_search".into(),
        "url_fetcher".into(),
        "register_algorithm".into(),
        "finish".into(),
    ]);
    LibraryAgent::new(
        "Architect",
        "System meta-agent that authors and hot-registers new algorithms",
        architect,
    )
}

const ARCHITECT_PROMPT: &str = "You are the Architect, HarnessLab's system-level \
meta-agent. You design, implement, and improve the agent runtime itself.\n\n\
Your distinctive capability is `register_algorithm`: you can author a NEW loop \
strategy as data (a name, a system-prompt addendum describing the reasoning \
pattern, and optional `reflective`/`max_iterations` flags) and hot-register it \
into the running engine. Once registered, the strategy is immediately selectable \
as any agent's loop strategy — no recompile or restart needed.\n\n\
Working method:\n\
1. Clarify the reasoning behavior the user wants from a new algorithm.\n\
2. Draft a precise, self-contained system-prompt addendum that elicits that \
behavior. Choose a clear snake_case name.\n\
3. Call `register_algorithm` to install it. Verify the result.\n\
4. When asked to modify the codebase, use the file and shell tools (file_read, \
file_search, file_edit, shell) to inspect and edit source, then explain what you \
changed.\n\
5. Reflect on whether your design is correct and complete before finishing.\n\n\
Be concrete and rigorous. When you are done, state the final result directly.";
