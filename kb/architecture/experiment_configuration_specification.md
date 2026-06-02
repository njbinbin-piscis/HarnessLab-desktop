# Agent Workbench 实验配置完全组合规范 (Experiment Configuration Specification)

## 1. 概述
本规范定义了 Agent Workbench 中 YAML 配置文件的标准结构。该结构旨在覆盖从底层模型连接、上下文管理、记忆系统到高层编排策略和钩子机制的全方位配置，实现“0 代码” Agent 设计与实验。

## 2. 顶层结构
配置文件由以下五个核心模块组成：

```yaml
agent:      # 基础身份与模型配置
plugins:    # 核心插件选择（记忆、上下文等）
policy:     # 运行时安全与资源策略
strategy:   # 编排逻辑与提示词模板
hooks:      # 细粒度生命周期钩子
```

## 3. 模块详细定义

### 3.1 `agent`: 基础身份与模型配置
定义 Agent 的基本属性和 LLM 连接参数。

| 字段 | 类型 | 必填 | 说明 |
| :--- | :--- | :--- | :--- |
| `name` | String | 是 | Agent 的唯一标识名称（如 "ReAct-Explorer"）。 |
| `model` | String | 是 | 主用模型 ID（如 "claude-sonnet-4.5", "qwen-max"）。 |
| `context_window` | Integer | 否 | 模型的上下文窗口大小（默认 200000）。 |
| `max_tokens` | Integer | 否 | 单次响应的最大 Token 数。 |
| `fallback_models` | List[String] | 否 | 故障转移模型列表。 |
| `vision_override` | String/null | 否 | 指定视觉分析模型，null 则使用主模型。 |
| `enable_streaming` | Boolean | 否 | 是否开启流式输出。 |

### 3.2 `plugins`: 核心插件选择
通过接口替换核心组件，支持不同场景下的能力组合。

| 字段 | 类型 | 说明 | 可选值示例 |
| :--- | :--- | :--- | :--- |
| `memory` | String | 记忆系统插件名。 | `InMemoryVectorStore`, `SqliteMemory`, `NoOpMemory` |
| `context` | String | 上下文管理器插件名。 | `ProjectContextManager`, `WorkspaceRootManager` |

### 3.3 `policy`: 运行时安全与资源策略
控制 Agent 的行为边界和资源消耗。

| 字段 | 类型 | 说明 |
| :--- | :--- | :--- |
| `confirm_shell` | Boolean | 执行 Shell 命令前是否需要人工确认。 |
| `confirm_file_write` | Boolean | 写入文件前是否需要人工确认。 |
| `auto_compact_threshold` | Integer | 触发自动上下文压缩的 Token 阈值。 |
| `compaction_tiers` | Object | 压缩层级配置（micro/auto/full 百分比）。 |
| `max_tool_result_tokens` | Integer | 工具返回结果的最大 Token 限制。 |
| `summary_model` | String | 用于长文本摘要的轻量级模型 ID。 |

### 3.4 `strategy`: 编排逻辑与提示词模板
定义 Agent 的思考模式和行动框架。

| 字段 | 类型 | 说明 |
| :--- | :--- | :--- |
| `type` | String | 策略类型（如 "react", "plan_and_solve"）。 |
| `system_prompt_template` | String | 注入给 LLM 的系统提示词模板。 |

### 3.5 `hooks`: 细粒度生命周期钩子
在关键节点注入自定义逻辑（审计、日志、副作用）。

| 字段 | 类型 | 说明 |
| :--- | :--- | :--- |
| `type` | String | 钩子类型（如 "FileJournal", "PlanTracker"）。 |
| `path` | String | 钩子相关的存储路径（如日志文件路径）。 |
| `enabled` | Boolean | 是否启用该钩子。 |

## 4. 配置组合示例

### 4.1 ReAct 探索者 (快速响应型)
适用于简单任务，强调直接的工具调用和即时推理。
```yaml
agent:
  name: "ReAct-Explorer"
  model: "claude-sonnet-4.5"
plugins:
  memory: "InMemoryVectorStore"
  context: "ProjectContextManager"
policy:
  confirm_shell: false
  auto_compact_threshold: 100000
strategy:
  type: "react"
  system_prompt_template: "You are a ReAct agent. Think then Act."
hooks:
  - type: "FileJournal"
    path: ".pisci/journal.db"
    enabled: true
```

### 4.2 Plan-and-Solve 战略家 (复杂规划型)
适用于多步复杂任务，强调先规划后执行。
```yaml
agent:
  name: "Plan-and-Solve-Strategist"
  model: "claude-sonnet-4.5"
plugins:
  memory: "InMemoryVectorStore"
  context: "ProjectContextManager"
policy:
  confirm_file_write: true
  auto_compact_threshold: 120000
  summary_model: "claude-haiku-4.5"
strategy:
  type: "plan_and_solve"
  system_prompt_template: "Create a detailed plan first, then execute step-by-step."
hooks:
  - type: "PlanTracker"
    enabled: true
```

## 5. 扩展性说明
- **自定义插件**: 开发者可以通过实现 `IPlugin` 接口并在 `plugins` 模块中注册新名称来扩展功能。
- **动态钩子**: `hooks` 列表支持多个钩子并存，内核会按顺序触发所有已启用的钩子。
- **元代理集成**: 本规范是 Meta-Agent 生成配置的基础结构，Meta-Agent 将根据用户描述自动填充这些字段。

[ProjectStatus] follow_up_needed
@!Coder 请根据此规范更新后端的配置解析器 (`backend/api/config.py` 或 `src/config.rs`)，确保所有新增字段（如 `vision_override`, `summary_model`）能被正确解析并映射到内核配置中。同时，请为前端可视化编辑器提供对应的 JSON Schema。
