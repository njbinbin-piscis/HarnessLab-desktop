//! Loop-control strategies.
//!
//! A [`LoopStrategy`] shapes the agent loop's behavior: the system-prompt
//! addendum it injects, whether it runs a reflection pass, and any iteration
//! budget hint. Strategies resolve by name, falling through to the runtime
//! [`contrib`](super::contrib) registry for design-assistant-authored entries.

use std::sync::Arc;

/// Shapes how the agent loop runs.
pub trait LoopStrategy: Send + Sync {
    /// Stable identifier (selectable via `strategy.type`).
    fn name(&self) -> &str;

    /// Guidance appended to the system prompt for this pattern.
    fn system_addendum(&self) -> String {
        String::new()
    }

    /// When `true`, the loop runs a self-reflection step before finishing.
    fn reflective(&self) -> bool {
        false
    }

    /// Optional cap on iterations (e.g. budgeted strategies).
    fn max_iterations_hint(&self) -> Option<usize> {
        None
    }
}

/// The canonical ReAct (reason + act) strategy.
pub struct ReActStrategy;

impl LoopStrategy for ReActStrategy {
    fn name(&self) -> &str {
        "react"
    }
    fn system_addendum(&self) -> String {
        "Work step by step: think about what to do, call a tool when it helps, \
         observe the result, then continue. When you have the final answer, \
         state it directly without calling further tools."
            .to_string()
    }
}

/// A strategy distinguished purely by its prompt pattern.
pub struct PromptPatternStrategy {
    name: String,
    addendum: String,
    reflective: bool,
    max_iterations: Option<usize>,
}

impl PromptPatternStrategy {
    /// Build a prompt-pattern strategy with the canonical guidance for `name`.
    pub fn new(name: &str) -> Self {
        let (addendum, reflective, max_iterations) = pattern_for(name);
        Self {
            name: name.to_string(),
            addendum,
            reflective,
            max_iterations,
        }
    }

    /// Build a fully custom prompt-pattern strategy (used by the meta-agent).
    pub fn custom(
        name: &str,
        addendum: &str,
        reflective: bool,
        max_iterations: Option<usize>,
    ) -> Self {
        Self {
            name: name.to_string(),
            addendum: addendum.to_string(),
            reflective,
            max_iterations,
        }
    }
}

impl LoopStrategy for PromptPatternStrategy {
    fn name(&self) -> &str {
        &self.name
    }
    fn system_addendum(&self) -> String {
        self.addendum.clone()
    }
    fn reflective(&self) -> bool {
        self.reflective
    }
    fn max_iterations_hint(&self) -> Option<usize> {
        self.max_iterations
    }
}

/// Canonical guidance + behavior flags for each built-in pattern name.
fn pattern_for(name: &str) -> (String, bool, Option<usize>) {
    match name {
        "plan_and_solve" => (
            "First write a concise numbered PLAN of the steps needed. Then execute \
             the plan step by step, using tools as needed, and finish with the answer."
                .to_string(),
            false,
            None,
        ),
        "reflexion" => (
            "After producing a candidate answer, critically REFLECT on whether it is \
             correct and complete. If you find a flaw, revise using tools, then give \
             the improved final answer."
                .to_string(),
            true,
            None,
        ),
        "tree_of_thought" => (
            "Explore multiple distinct approaches (branches) before committing. Briefly \
             sketch 2-3 candidate lines of reasoning, evaluate them, then pursue the most \
             promising one to the final answer."
                .to_string(),
            false,
            None,
        ),
        "rewoo" => (
            "Plan the full sequence of tool calls UP FRONT (reasoning without \
             observation), then execute them, then synthesize the observations into a \
             final answer. Avoid interleaving planning with each observation."
                .to_string(),
            false,
            None,
        ),
        "tool_calling" => (
            "Prefer direct tool calls. Minimize free-form reasoning; call the most \
             appropriate tool, read its result, and answer concisely."
                .to_string(),
            false,
            None,
        ),
        "budgeted" => (
            "You operate under a tight step budget. Be decisive: take the most \
             informative action each step and finish as soon as you can answer."
                .to_string(),
            false,
            Some(8),
        ),
        "reflect_retry" => (
            "If a tool call fails or yields an unexpected result, REFLECT on the cause \
             and RETRY with a corrected approach (at most twice) before giving up."
                .to_string(),
            true,
            None,
        ),
        _ => (String::new(), false, None),
    }
}

/// Names of all built-in (compiled-in) loop strategies.
pub const BUILTIN_LOOP_STRATEGIES: &[&str] = &[
    "react",
    "plan_and_solve",
    "reflexion",
    "tree_of_thought",
    "rewoo",
    "tool_calling",
    "budgeted",
    "reflect_retry",
];

/// Resolve a loop strategy by name, falling through to the contrib registry.
pub fn resolve_loop_strategy(name: &str) -> Option<Arc<dyn LoopStrategy>> {
    match name {
        "" | "react" => Some(Arc::new(ReActStrategy)),
        n if BUILTIN_LOOP_STRATEGIES.contains(&n) => Some(Arc::new(PromptPatternStrategy::new(n))),
        other => super::contrib::resolve_loop_strategy(other),
    }
}
