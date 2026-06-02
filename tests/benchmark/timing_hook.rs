use std::time::Instant;
use pisci_kernel::agent::hooks::{AgentHooks, HookContext};

/// A hook that records timing information for tool executions and LLM calls.
pub struct TimingHook {
    start_times: std::sync::Mutex<std::collections::HashMap<String, Instant>>,
}

impl TimingHook {
    pub fn new() -> Self {
        Self {
            start_times: std::sync::Mutex::new(std::collections::HashMap::new()),
        }
    }
}

#[async_trait::async_trait]
impl AgentHooks for TimingHook {
    async fn before_tool(&self, ctx: &HookContext) {
        let mut map = self.start_times.lock().unwrap();
        map.insert(ctx.tool_call.id.clone(), Instant::now());
    }

    async fn after_tool(&self, ctx: &HookContext) {
        let mut map = self.start_times.lock().unwrap();
        if let Some(start) = map.remove(&ctx.tool_call.id) {
            let duration = start.elapsed();
            eprintln!(
                "[TimingHook] Tool '{}' executed in {:?}",
                ctx.tool_call.name, duration
            );
        }
    }

    async fn on_llm_call_event(&self, ctx: &HookContext) {
        // For simplicity, we'll just log the event. 
        // In a real implementation, we'd track Before/After pairs similarly to tools.
        eprintln!("[TimingHook] LLM call event: {:?}", ctx.event_type);
    }
}
