//! The agent run loop — a ReAct tool-calling loop over an [`LlmClient`].
//!
//! [`AgentLoop::run`] streams [`AgentEvent`]s while it reasons, calls tools and
//! observes results, until the model produces a final answer (or calls the
//! `finish` tool). It returns the messages produced during the run plus the
//! cumulative input/output token counts.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use tokio::sync::mpsc;
use tokio::sync::Mutex;

use super::harness::HarnessConfig;
use super::hooks::{ContextHookEvent, HookDecision, ToolHookEvent};
use super::messages::AgentEvent;
use super::tool::ToolContext;
use crate::llm::{
    estimate_tokens, ContentBlock, LlmClient, LlmMessage, LlmRequest, MessageContent,
};

// Re-export so callers can `use harness_rt::agent::loop_::resolve_compaction_strategy`.
pub use super::compaction::resolve_compaction_strategy;

/// A runnable agent.
pub struct AgentLoop {
    config: HarnessConfig,
    client: Box<dyn LlmClient>,
    notification_rx: Mutex<Option<mpsc::Receiver<String>>>,
    _confirmation_responses: Mutex<Option<mpsc::Receiver<bool>>>,
}

impl AgentLoop {
    pub(crate) fn new(
        config: HarnessConfig,
        client: Box<dyn LlmClient>,
        notification_rx: Option<mpsc::Receiver<String>>,
        confirmation_responses: Option<mpsc::Receiver<bool>>,
    ) -> Self {
        Self {
            config,
            client,
            notification_rx: Mutex::new(notification_rx),
            _confirmation_responses: Mutex::new(confirmation_responses),
        }
    }

    /// Assemble the system prompt: base prompt + strategy addendum + project
    /// context + retrieved memories.
    async fn build_system_prompt(&self, ctx: &ToolContext, latest_user: &str) -> String {
        let mut prompt = self.config.layered_prompt.render();

        if let Some(strategy) = &self.config.loop_strategy {
            let addendum = strategy.system_addendum();
            if !addendum.is_empty() {
                prompt.push_str("\n\n");
                prompt.push_str(&addendum);
            }
        }

        if let Ok(project_ctx) = self
            .config
            .render_project_context(&ctx.workspace_root, 8_000)
        {
            if !project_ctx.trim().is_empty() {
                prompt.push_str("\n\n");
                prompt.push_str(&project_ctx);
            }
        }

        if let Some(memory) = &self.config.memory_plugin {
            if let Ok(hits) = memory.search(&ctx.memory_owner_id, latest_user, 5).await {
                if !hits.is_empty() {
                    let rendered = hits.join("\n- ");
                    let block = match &self.config.memory_retrieval_prompt {
                        Some(tpl) => tpl.replace("{memories}", &rendered),
                        None => format!("## Relevant memories\n- {rendered}"),
                    };
                    prompt.push_str("\n\n");
                    prompt.push_str(&block);
                }
            }
        }

        prompt
    }

    fn max_iterations(&self, ctx: &ToolContext) -> usize {
        if let Some(hint) = self
            .config
            .loop_strategy
            .as_ref()
            .and_then(|s| s.max_iterations_hint())
        {
            return hint;
        }
        ctx.max_iterations.unwrap_or(20)
    }

    /// Drain any pending steering messages into the conversation.
    async fn drain_injections(&self) -> Vec<LlmMessage> {
        let mut out = Vec::new();
        let mut guard = self.notification_rx.lock().await;
        if let Some(rx) = guard.as_mut() {
            while let Ok(msg) = rx.try_recv() {
                if !msg.trim().is_empty() {
                    out.push(LlmMessage {
                        role: "user".to_string(),
                        content: MessageContent::text(&msg),
                    });
                }
            }
        }
        out
    }

    /// Maybe compact the conversation to fit the context window.
    async fn maybe_compact(&self, conversation: &mut Vec<LlmMessage>, ctx: &ToolContext) {
        let window = self.config.context_window;
        if window == 0 {
            return;
        }
        let Some(strategy) = &self.config.compaction_strategy else {
            return;
        };
        let trigger =
            window.saturating_mul(self.config.compaction_settings.full_percent as u32) / 100;
        let estimated: u32 = conversation
            .iter()
            .map(|m| estimate_tokens(&m.content.as_text()) + 4)
            .sum();
        if trigger > 0 && estimated > trigger {
            if let Some(hooks) = &self.config.hooks {
                hooks
                    .on_context_event(&ContextHookEvent::BeforeCompact {
                        session_id: &ctx.session_id,
                        message_count: conversation.len(),
                    })
                    .await;
            }
            let budget = window
                .saturating_sub(self.config.max_tokens)
                .max(window / 2);
            let compacted = strategy.compact(std::mem::take(conversation), budget);
            *conversation = compacted;
            if let Some(hooks) = &self.config.hooks {
                hooks
                    .on_context_event(&ContextHookEvent::AfterCompact {
                        session_id: &ctx.session_id,
                        message_count: conversation.len(),
                    })
                    .await;
            }
        }
    }

    /// Run the agent loop to completion.
    pub async fn run(
        &self,
        messages: Vec<LlmMessage>,
        event_tx: mpsc::Sender<AgentEvent>,
        cancel: Arc<AtomicBool>,
        tool_ctx: ToolContext,
    ) -> Result<(Vec<LlmMessage>, u32, u32), String> {
        let latest_user = messages
            .iter()
            .rev()
            .find(|m| m.role == "user")
            .map(|m| m.content.as_text())
            .unwrap_or_default();
        let system = self.build_system_prompt(&tool_ctx, &latest_user).await;

        let mut conversation = messages;
        let mut produced: Vec<LlmMessage> = Vec::new();
        let mut total_in = 0u32;
        let mut total_out = 0u32;
        let max_iterations = self.max_iterations(&tool_ctx);
        let reflective = self
            .config
            .loop_strategy
            .as_ref()
            .map(|s| s.reflective())
            .unwrap_or(false);
        let mut reflected = false;

        let tools = self.config.registry.schemas();

        if let Some(hooks) = &self.config.hooks {
            hooks
                .on_context_event(&ContextHookEvent::TurnStart {
                    session_id: &tool_ctx.session_id,
                })
                .await;
        }

        let mut iteration = 0u32;
        while (iteration as usize) < max_iterations {
            if cancel.load(Ordering::Relaxed) {
                let _ = event_tx.send(AgentEvent::Cancelled).await;
                return Ok((produced, total_in, total_out));
            }

            for inj in self.drain_injections().await {
                conversation.push(inj.clone());
                produced.push(inj);
            }

            self.maybe_compact(&mut conversation, &tool_ctx).await;

            let _ = event_tx
                .send(AgentEvent::TextSegmentStart { iteration })
                .await;

            let req = LlmRequest {
                model: self.config.model.clone(),
                system: system.clone(),
                messages: conversation.clone(),
                tools: tools.clone(),
                max_tokens: self.config.max_tokens,
                temperature: 0.7,
            };

            let resp = match self.client.complete(req).await {
                Ok(r) => r,
                Err(e) => {
                    let msg = format!("LLM request failed: {e}");
                    let _ = event_tx
                        .send(AgentEvent::Error {
                            message: msg.clone(),
                        })
                        .await;
                    return Err(msg);
                }
            };

            total_in = total_in.saturating_add(resp.input_tokens);
            total_out = total_out.saturating_add(resp.output_tokens);

            let estimated_input: u32 = conversation
                .iter()
                .map(|m| estimate_tokens(&m.content.as_text()) + 4)
                .sum();
            let _ = event_tx
                .send(AgentEvent::ContextUsage {
                    estimated_input_tokens: estimated_input,
                    cumulative_input_tokens: total_in,
                    cumulative_output_tokens: total_out,
                    context_window: self.config.context_window,
                })
                .await;

            if !resp.text.is_empty() {
                let _ = event_tx
                    .send(AgentEvent::TextDelta {
                        delta: resp.text.clone(),
                    })
                    .await;
            }

            if resp.tool_calls.is_empty() {
                // Final answer (unless a reflective strategy wants one more pass).
                let assistant = LlmMessage {
                    role: "assistant".to_string(),
                    content: MessageContent::text(&resp.text),
                };
                conversation.push(assistant.clone());
                produced.push(assistant);

                if reflective && !reflected {
                    reflected = true;
                    let nudge = LlmMessage {
                        role: "user".to_string(),
                        content: MessageContent::text(
                            "Reflect on your answer above. If it is correct and complete, \
                             restate it as the final answer. Otherwise, correct it.",
                        ),
                    };
                    conversation.push(nudge.clone());
                    produced.push(nudge);
                    iteration += 1;
                    continue;
                }

                self.remember(&tool_ctx, &latest_user, &resp.text).await;
                break;
            }

            // Build the assistant message carrying its tool calls.
            let mut blocks: Vec<ContentBlock> = Vec::new();
            if !resp.text.is_empty() {
                blocks.push(ContentBlock::Text {
                    text: resp.text.clone(),
                });
            }
            for call in &resp.tool_calls {
                blocks.push(ContentBlock::ToolUse {
                    id: call.id.clone(),
                    name: call.name.clone(),
                    input: call.arguments.clone(),
                });
            }
            let assistant = LlmMessage {
                role: "assistant".to_string(),
                content: MessageContent::Blocks(blocks),
            };
            conversation.push(assistant.clone());
            produced.push(assistant);

            let mut finished = false;
            for call in &resp.tool_calls {
                let _ = event_tx
                    .send(AgentEvent::ToolStart {
                        id: call.id.clone(),
                        name: call.name.clone(),
                        input: call.arguments.clone(),
                    })
                    .await;

                let result = self.execute_tool(call, &tool_ctx).await;

                let _ = event_tx
                    .send(AgentEvent::ToolEnd {
                        id: call.id.clone(),
                        name: call.name.clone(),
                        result: result.content.clone(),
                        is_error: result.is_error,
                    })
                    .await;

                let tool_msg = LlmMessage {
                    role: "tool".to_string(),
                    content: MessageContent::Blocks(vec![ContentBlock::ToolResult {
                        tool_use_id: call.id.clone(),
                        content: result.content.clone(),
                        is_error: result.is_error,
                    }]),
                };
                conversation.push(tool_msg.clone());
                produced.push(tool_msg);

                if call.name == "finish" {
                    finished = true;
                    if !result.content.is_empty() {
                        let _ = event_tx
                            .send(AgentEvent::TextDelta {
                                delta: result.content.clone(),
                            })
                            .await;
                    }
                    self.remember(&tool_ctx, &latest_user, &result.content)
                        .await;
                }
            }

            if finished {
                break;
            }

            iteration += 1;
        }

        if let Some(hooks) = &self.config.hooks {
            hooks
                .on_context_event(&ContextHookEvent::TurnEnd {
                    session_id: &tool_ctx.session_id,
                })
                .await;
        }

        let _ = event_tx
            .send(AgentEvent::Done {
                total_input_tokens: total_in,
                total_output_tokens: total_out,
            })
            .await;

        Ok((produced, total_in, total_out))
    }

    /// Execute one tool call through the registry + hooks.
    async fn execute_tool(
        &self,
        call: &crate::llm::ToolCall,
        ctx: &ToolContext,
    ) -> super::tool::ToolResult {
        let tool = match self.config.registry.get(&call.name) {
            Some(t) => t,
            None => return super::tool::ToolResult::error(format!("unknown tool '{}'", call.name)),
        };

        if let Some(hooks) = &self.config.hooks {
            let ev = ToolHookEvent {
                session_id: &ctx.session_id,
                tool_use_id: call.id.as_str(),
                tool_name: call.name.as_str(),
                input: &call.arguments,
                workspace_root: &ctx.workspace_root,
            };
            if let HookDecision::Block { reason } = hooks.before_tool(&ev).await {
                return super::tool::ToolResult::error(format!("blocked by policy: {reason}"));
            }
        }

        let result = tool.execute(call.arguments.clone(), ctx).await;

        if let Some(hooks) = &self.config.hooks {
            let ev = ToolHookEvent {
                session_id: &ctx.session_id,
                tool_use_id: call.id.as_str(),
                tool_name: call.name.as_str(),
                input: &call.arguments,
                workspace_root: &ctx.workspace_root,
            };
            hooks.after_tool(&ev, &result).await;
        }

        result
    }

    /// Persist a task→answer memory when a memory backend is wired.
    async fn remember(&self, ctx: &ToolContext, task: &str, answer: &str) {
        if let Some(memory) = &self.config.memory_plugin {
            if !answer.trim().is_empty() {
                let _ = memory
                    .store(
                        &ctx.memory_owner_id,
                        &format!("Task: {task}\nAnswer: {answer}"),
                    )
                    .await;
            }
        }
    }
}
