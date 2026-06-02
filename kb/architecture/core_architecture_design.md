# Agent Workbench 核心架构设计 (Core Architecture Design)

## 1. 极简 Loop 提取 (Minimalist Loop Extraction)

从 `piscis-engine` 中提取出的极简 Agent Loop 逻辑由以下三个核心组件构成：

### 1.1 State (状态)
- **Session State**: 包含当前会话 ID、模型配置、Token 预算 (`LayeredBudget`)。
- **Context State**: 包含消息历史 (`Vec<LlmMessage>`)、滚动摘要 (`SummarySnapshot`)、状态帧 (`StateFrame`)。
- **Project State**: 包含项目上下文指令 (`InstructionFile`)、内存条目 (`MemoryEntry`)。

### 1.2 Policy (策略)
- **PolicyGate**: 决定工具调用的权限（允许/拒绝/需确认）。
- **CompactionStrategy**: 决定何时触发上下文压缩（Micro/Auto/Full）。
- **ContextManager**: 决定如何发现和注入项目指令（如 `ProjectContextManager`）。
- **MemoryPlugin**: 决定如何存储和检索长期记忆（如 `InMemoryVectorStore`）。

### 1.3 Action (动作)
- **LLM Call**: 通过 `LlmClient` 接口调用模型，获取响应。
- **Tool Execution**: 通过 `ToolRegistry` 执行工具，产生 `ToolResult`。
- **Hook Invocation**: 在关键节点触发 `AgentHooks`（如 `before_tool`, `after_tool`, `on_prompt_event`）。

---

## 2. 插件接口定义 (Plugin Interface Definitions)

为了支持 0 代码配置驱动，所有核心组件均通过 Trait 抽象，并在 `HarnessConfig` 中注入。

### 2.1 IPlugin: LLM Client
- **Trait**: `crate::llm::LlmClient`
- **职责**: 统一不同厂商（Anthropic, OpenAI, Qwen 等）的 API 调用。
- **配置**: 通过 YAML 中的 `model` 字段指定，内核根据 `ProviderKind` 自动路由。

### 2.2 IPlugin: Memory Plugin
- **Trait**: `crate::memory::plugin::MemoryPlugin`
- **职责**: 提供记忆的 CRUD 和向量搜索能力。
- **实现**:
    - `NoOpMemory`: 用于 Fish 等临时任务。
    - `InMemoryVectorStore`: 用于短期会话。
    - `SqliteMemory` (待实现): 用于持久化长期记忆。
- **配置**: 在 `HarnessConfig` 中通过 `memory_plugin` 字段注入。

### 2.3 IPlugin: Context Manager
- **Trait**: `crate::context::ContextManager`
- **职责**: 发现并渲染项目特定的指令文件（如 `PISCI.md`）。
- **实现**:
    - `ProjectContextManager`: 默认实现，扫描祖先目录。
    - `WorkspaceRootManager` (待实现): 仅扫描根目录。
- **配置**: 在 `HarnessConfig` 中通过 `context_manager` 字段注入。

---

## 3. 钩子机制设计 (Hook Mechanism Design)

钩子机制允许在不修改内核代码的情况下，在关键生命周期注入细粒度逻辑。

### 3.1 IHook: Agent Hooks
- **Trait**: `crate::agent::hooks::AgentHooks`
- **事件点**:
    - `before_tool`: 工具执行前，可拦截或记录前置状态。
    - `after_tool`: 工具执行后，可记录结果或触发副作用。
    - `on_context_event`: 上下文管理事件（TurnStart, TurnEnd, BeforeCompact, AfterCompact）。
    - `on_prompt_event`: Prompt 组装事件（BeforePromptAssembly, AfterPromptAssembly）。
    - `on_llm_call_event`: LLM 调用事件（BeforeLlmCall, AfterLlmResponse）。

### 3.2 钩子应用场景
- **CodeZ File Journal**: 在 `file_write` 前快照文件内容，支持 Undo。
- **审计日志**: 记录所有工具调用的输入输出。
- **动态 Prompt 注入**: 在 `BeforePromptAssembly` 时根据当前状态注入额外信息。

---

## 4. 配置驱动架构 (Configuration-Driven Architecture)

所有实验参数、策略、角色定义均通过 YAML 配置，实现 0 代码 Agent 设计。

### 4.1 配置文件结构示例
```yaml
agent:
  name: "Researcher"
  model: "claude-sonnet-4.5"
  context_window: 200000
  max_tokens: 8192
  
plugins:
  memory: "InMemoryVectorStore"
  context: "ProjectContextManager"
  
policy:
  confirm_shell: false
  confirm_file_write: true
  auto_compact_threshold: 100000

hooks:
  - type: "FileJournal"
    path: ".pisci/journal.db"
```

### 4.2 策略切换
- **ReAct vs Plan-and-Solve**: 通过在 YAML 中定义不同的 `system_prompt` 模板和 `tool_registry` 组合来实现。
- **场景工厂**: `HarnessConfig` 提供了 `for_main_chat`, `for_koi`, `for_fish` 等工厂方法，封装了不同场景的默认配置。

---

## 5. 总结与下一步

本架构设计完成了对 `piscis-engine` 的内核提取，明确了插件接口和钩子机制。下一步建议 Coder 执行：
1. **配置解析器开发**: 实现 YAML 到 `HarnessConfig` 的映射。
2. **更多插件实现**: 补充 `SqliteMemory` 和 `WorkspaceRootManager`。
3. **基准测试**: 验证不同配置下的 Agent 行为一致性。

[ProjectStatus] follow_up_needed
@!Coder 请根据此架构设计，开始实施“第二阶段：配置驱动与插件扩展”。重点实现 YAML 配置解析器，并确保 `HarnessConfig` 能完全从配置文件中构建。
