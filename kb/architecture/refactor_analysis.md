# piscis-engine 架构重构分析报告

## 1. 核心 Loop 纯度分析

**现状：**
目前的 Agent Loop (`pisci-kernel/src/agent/loop_.rs`) 在逻辑上遵循“有工具调用则继续，无则退出”的原则，但存在以下影响纯度的因素：
- **硬编码的循环检测与熔断**：Loop 内部集成了复杂的 `LoopDetectorState`，用于检测重复调用、Ping-Pong 模式和视觉循环。虽然这是安全网，但它将策略逻辑耦合在了执行引擎中。
- **上下文压缩 (Compaction) 耦合**：Loop 直接管理消息历史的截断和摘要（`compact_trim_tool_results`, `compact_summarise`）。这使得 Loop 不仅负责执行，还承担了部分 Context Manager 的职责。
- **场景化配置 (Harness)**：通过 `HarnessConfig`，Loop 可以根据场景（Main, Koi, Fish）加载不同的持久化和钩子，这在一定程度上实现了插件化，但核心逻辑依然庞大（4800+ 行）。

**结论：**
Loop 的核心驱动是纯净的，但被大量的“辅助性”逻辑包裹。为了达到“极简 Loop”的目标，需要将这些辅助逻辑剥离为可插拔的组件。

## 2. 扩展点现状 (Plugins & Hooks)

### 插件化 (Plugins)
目前 `piscis-engine` 已经具备了初步的插件化接口，主要定义在 `pisci-core/src/host.rs` 和 `pisci-kernel` 的各模块中：

| 组件 | 接口位置 | 现状 |
| :--- | :--- | :--- |
| **LLM Client** | `pisci-kernel/src/llm/mod.rs` | `trait LlmClient`。已支持 OpenAI, Claude, DeepSeek, Qwen 等多提供商。 |
| **Memory** | `pisci-kernel/src/memory/vector.rs` | 仅有基础的向量相似度计算和混合检索工具函数，缺乏统一的 `trait MemoryStore` 接口。 |
| **Context Manager** | `pisci-kernel/src/project_context.rs` | 提供了 `discover_project_instruction_files` 等函数，但未抽象为接口。 |
| **Tools** | `pisci-kernel/src/agent/tool.rs` | `trait Tool`。支持动态注册，是目前最成熟的插件点。 |
| **Host Runtime** | `pisci-core/src/host.rs` | `trait HostRuntime`。定义了事件分发、通知、工具注册等宿主能力。 |

### 钩子机制 (Hooks)
目前已实现一个明确的 Hook 系统：
- **位置**：`pisci-kernel/src/agent/hooks.rs`
- **接口**：`trait AgentHooks`
- **事件点**：
  - `before_tool`: 工具执行前（可用于快照、权限二次确认）。
  - `after_tool`: 工具执行后（可用于审计、状态同步）。
  - `on_context_event`: 上下文生命周期事件（TurnStart, TurnEnd, BeforeCompact, AfterCompact）。

**缺失的 Hook 点：**
- **Prompt 组装前/后**：目前在 `harness/context_builder.rs` 中硬编码了 Prompt 的拼接逻辑，缺乏 `before_prompt_assembly` 或 `after_prompt_assembly` 钩子，难以动态注入策略或调试信息。
- **LLM 请求/响应拦截**：缺乏在发送给 LLM 之前或收到响应之后进行统一处理（如脱敏、格式校验）的钩子。

## 3. 重构路线图

为了实现“配置驱动、插件化、极简 Loop”的目标，建议分三个阶段进行重构：

### 第一阶段：接口标准化与解耦 (Interface Standardization)
1. **定义 `ContextManager` Trait**：
   - 将 `project_context.rs` 中的逻辑抽象为接口。
   - 允许通过配置切换不同的上下文注入策略（如：仅注入根目录指令 vs 递归注入所有祖先指令）。
2. **定义 `MemoryPlugin` Trait**：
   - 统一内存存储接口（Vector DB, SQLite, In-Memory）。
   - 实现至少三种插件：`SqliteMemory`, `ChromaMemory`, `NoOpMemory`。
3. **完善 Hook 体系**：
   - 增加 `PromptHook` 和 `LlmInterceptor`。
   - 确保 Hook 可以在 `HarnessConfig` 中通过配置动态挂载。

### 第二阶段：Loop 瘦身与策略外置 (Loop Slimming)
1. **剥离循环检测逻辑**：
   - 将 `LoopDetector` 移出 `loop_.rs`，作为一个独立的 `PolicyPlugin` 或 `Guard` 组件。
   - 通过配置决定是否启用以及启用的阈值。
2. **剥离上下文压缩逻辑**：
   - 将 Compaction 逻辑封装为 `CompactionStrategy` 插件。
   - 支持通过 YAML 配置切换压缩策略（如：基于 Token 数、基于轮次、基于语义密度）。
3. **简化 `AgentLoop` 结构**：
   - 目标是让 `AgentLoop` 只保留：`fetch_tools -> call_llm -> parse_response -> execute_tools` 的核心闭环。

### 第三阶段：配置驱动与策略切换 (Configuration Driven)
1. **YAML 配置 schema 设计**：
   - 定义 `agent.yaml`，包含：`model`, `plugins` (list), `hooks` (list), `strategy` (ReAct/Plan-and-Solve)。
2. **实现策略插件**：
   - **ReAct Strategy**: 标准的观察-思考-行动循环。
   - **Plan-and-Solve Strategy**: 先调用 `plan_todo` 生成计划，再逐步执行。
   - 通过配置 `strategy: "plan_and_solve"` 自动加载对应的 System Prompt 模板和工具白名单。
3. **0 代码 Agent 设计器**：
   - 开发一个简单的配置加载器，读取 YAML 并实例化对应的 Plugin 和 Hook 链。

## 4. 关键文件清单

| 文件路径 | 作用 | 重构优先级 |
| :--- | :--- | :--- |
| `pisci-kernel/src/agent/loop_.rs` | 核心循环 | High (需瘦身) |
| `pisci-kernel/src/agent/hooks.rs` | 钩子定义 | Medium (需扩展) |
| `pisci-kernel/src/agent/harness/config.rs` | 场景配置 | High (作为插件入口) |
| `pisci-kernel/src/llm/mod.rs` | LLM 接口 | Low (已成熟) |
| `pisci-kernel/src/memory/vector.rs` | 内存工具 | High (需封装为 Trait) |
| `pisci-core/src/host.rs` | 宿主接口 | Medium (需补充 Pool 事件) |

## 5. 总结

`piscis-engine` 已经具备了成为通用 Agent 内核的雏形，特别是在 Tool 系统和 Host 抽象方面做得很好。当前的主要瓶颈在于 `loop_.rs` 过于臃肿，且 Context/Memory 的管理逻辑分散且硬编码。

**下一步建议：**
由 Coder 开始实施“第一阶段”，首先提取 `ContextManager` 和 `MemoryPlugin` 接口，并在 `HarnessConfig` 中增加对它们的引用。同时，Architect 将提供详细的 YAML 配置 Schema 草案。
