//! Harness configuration + builder.
//!
//! [`HarnessConfig`] is the fully-resolved description of an agent: model,
//! prompt, tool registry, policy, and the pluggable strategy / memory /
//! context slots. [`HarnessConfig::into_agent_loop`] turns it into a runnable
//! [`AgentLoop`](crate::agent::loop_::AgentLoop) by injecting an LLM client.

pub mod config;

pub use config::{CompactionSettings, ConfirmFlags};

use std::path::Path;
use std::sync::Arc;

use tokio::sync::mpsc;

use crate::agent::compaction::CompactionStrategy;
use crate::agent::hooks::AgentHooks;
use crate::agent::loop_::AgentLoop;
use crate::agent::loop_strategy::LoopStrategy;
use crate::agent::tool::ToolRegistry;
use crate::context::ContextManager;
use crate::llm::LlmClient;
use crate::memory::plugin::MemoryPlugin;
use crate::policy::PolicyGate;

/// A layered system prompt. The workbench uses a single monolithic layer, but
/// the type keeps the layered shape for forward compatibility.
#[derive(Debug, Clone, Default)]
pub struct LayeredPrompt {
    layers: Vec<String>,
}

impl LayeredPrompt {
    /// Build a prompt from a single monolithic string.
    pub fn from_monolithic(text: String) -> Self {
        Self { layers: vec![text] }
    }

    /// Append a layer.
    pub fn push_layer(&mut self, text: String) {
        self.layers.push(text);
    }

    /// Render all layers into a single prompt string.
    pub fn render(&self) -> String {
        self.layers
            .iter()
            .filter(|l| !l.trim().is_empty())
            .cloned()
            .collect::<Vec<_>>()
            .join("\n\n")
    }
}

/// A fully-resolved agent harness configuration.
pub struct HarnessConfig {
    pub scene: String,
    pub model: String,
    pub fallback_models: Vec<String>,
    pub max_tokens: u32,
    pub context_window: u32,
    pub enable_streaming: bool,
    pub vision_override: Option<bool>,
    pub vision_model: Option<String>,
    pub confirm_flags: ConfirmFlags,
    pub auto_compact_threshold: u32,
    pub compaction_settings: CompactionSettings,
    pub layered_prompt: LayeredPrompt,
    pub registry: Arc<ToolRegistry>,
    pub policy: Arc<PolicyGate>,
    pub loop_strategy: Option<Arc<dyn LoopStrategy>>,
    pub compaction_strategy: Option<Arc<dyn CompactionStrategy>>,
    pub context_manager: Option<Arc<dyn ContextManager>>,
    pub memory_plugin: Option<Arc<dyn MemoryPlugin>>,
    pub memory_retrieval_prompt: Option<String>,
    pub hooks: Option<Arc<dyn AgentHooks>>,
}

impl HarnessConfig {
    /// Start building a harness for `scene` / `model` with the given registry
    /// and policy.
    pub fn builder(
        scene: &str,
        model: &str,
        registry: Arc<ToolRegistry>,
        policy: Arc<PolicyGate>,
    ) -> HarnessConfigBuilder {
        HarnessConfigBuilder {
            inner: HarnessConfig {
                scene: scene.to_string(),
                model: model.to_string(),
                fallback_models: Vec::new(),
                max_tokens: 4096,
                context_window: 0,
                enable_streaming: false,
                vision_override: None,
                vision_model: None,
                confirm_flags: ConfirmFlags::default(),
                auto_compact_threshold: 0,
                compaction_settings: CompactionSettings::default(),
                layered_prompt: LayeredPrompt::default(),
                registry,
                policy,
                loop_strategy: None,
                compaction_strategy: None,
                context_manager: None,
                memory_plugin: None,
                memory_retrieval_prompt: None,
                hooks: None,
            },
        }
    }

    /// Render the project context via the wired context-manager slot.
    pub fn render_project_context(
        &self,
        root: &Path,
        budget_chars: usize,
    ) -> std::io::Result<String> {
        match &self.context_manager {
            Some(cm) => cm.render(root, budget_chars),
            None => Ok(String::new()),
        }
    }

    /// Turn this config into a runnable agent loop by injecting an LLM client.
    ///
    /// `notification_rx` carries mid-turn steering messages (interactive mode);
    /// `confirmation_responses` is reserved for a permission-confirmation UI.
    pub fn into_agent_loop(
        self,
        client: Box<dyn LlmClient>,
        notification_rx: Option<mpsc::Receiver<String>>,
        confirmation_responses: Option<mpsc::Receiver<bool>>,
    ) -> AgentLoop {
        AgentLoop::new(self, client, notification_rx, confirmation_responses)
    }
}

/// Builder for [`HarnessConfig`].
pub struct HarnessConfigBuilder {
    inner: HarnessConfig,
}

impl HarnessConfigBuilder {
    pub fn with_fallback_models(mut self, models: Vec<String>) -> Self {
        self.inner.fallback_models = models;
        self
    }
    pub fn with_layered_prompt(mut self, prompt: LayeredPrompt) -> Self {
        self.inner.layered_prompt = prompt;
        self
    }
    pub fn with_max_tokens(mut self, max_tokens: u32) -> Self {
        self.inner.max_tokens = max_tokens;
        self
    }
    pub fn with_context_window(mut self, context_window: u32) -> Self {
        self.inner.context_window = context_window;
        self
    }
    pub fn with_confirm_flags(mut self, flags: ConfirmFlags) -> Self {
        self.inner.confirm_flags = flags;
        self
    }
    pub fn with_vision_override(mut self, vision_override: Option<bool>) -> Self {
        self.inner.vision_override = vision_override;
        self
    }
    pub fn with_auto_compact_threshold(mut self, threshold: u32) -> Self {
        self.inner.auto_compact_threshold = threshold;
        self
    }
    pub fn with_compaction_settings(mut self, settings: &CompactionSettings) -> Self {
        self.inner.compaction_settings = settings.clone();
        self
    }
    pub fn with_compaction_strategy(mut self, strategy: Arc<dyn CompactionStrategy>) -> Self {
        self.inner.compaction_strategy = Some(strategy);
        self
    }
    pub fn with_context_manager(mut self, cm: Arc<dyn ContextManager>) -> Self {
        self.inner.context_manager = Some(cm);
        self
    }
    pub fn with_loop_strategy(mut self, strategy: Arc<dyn LoopStrategy>) -> Self {
        self.inner.loop_strategy = Some(strategy);
        self
    }
    pub fn with_streaming(mut self, enabled: bool) -> Self {
        self.inner.enable_streaming = enabled;
        self
    }
    pub fn with_memory_plugin(mut self, plugin: Arc<dyn MemoryPlugin>) -> Self {
        self.inner.memory_plugin = Some(plugin);
        self
    }
    pub fn with_memory_retrieval_prompt(mut self, prompt: Option<String>) -> Self {
        self.inner.memory_retrieval_prompt = prompt;
        self
    }
    pub fn with_vision_model(mut self, model: String) -> Self {
        self.inner.vision_model = Some(model);
        self
    }
    pub fn with_hooks(mut self, hooks: Arc<dyn AgentHooks>) -> Self {
        self.inner.hooks = Some(hooks);
        self
    }
    pub fn build(self) -> HarnessConfig {
        self.inner
    }
}
