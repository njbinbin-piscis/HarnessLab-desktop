# 元代理（Meta-Agent）交互流程与 Prompt 模板设计

## 1. 交互向导设计 (Agent Generation Wizard)

前端“Agent 生成向导”分为四个步骤，引导用户从自然语言描述过渡到可执行的 YAML 配置。

### Step 1: 目标描述 (Goal Description)
- **输入**：多行文本框，用户用自然语言描述 Agent 的目标、角色和核心能力。
- **辅助**：提供“示例提示”，如“创建一个 Python 代码审查员...”或“设计一个数据分析助手...”。

### Step 2: 约束选择 (Constraint Selection)
- **模型偏好**：下拉菜单选择首选模型（如 `claude-sonnet-4.5`, `qwen-max`）。
- **工具白名单**：复选框勾选允许使用的工具类别（File I/O, Shell, Web Search, Code Execution）。
- **策略倾向**：单选按钮选择编排风格（ReAct - 快速响应 / Plan-and-Solve - 深度规划）。

### Step 3: 工具推荐 (Tool Recommendation)
- **动态展示**：根据 Step 1 的描述，Meta-Agent 在后端分析并推荐最相关的工具集。
- **用户确认**：用户可以增删推荐的工具，系统实时更新配置的 `policy` 字段。

### Step 4: 配置预览与微调 (Config Preview & Refinement)
- **双栏视图**：左侧显示生成的 YAML 配置，右侧显示对应的 System Prompt。
- **实时验证**：调用后端 `/api/v1/config/validate` 接口，实时反馈语法错误或逻辑冲突。
- **沙盒测试**：提供“试运行”按钮，在隔离环境中执行一个简单的任务以验证 Agent 行为。

---

## 2. Prompt 模板库 (Prompt Template Library)

位于 `kb/prompts/meta_agent/` 目录下，包含用于生成配置和 Prompt 的核心模板。

### 2.1 配置生成模板 (`config_generator.md`)
**用途**：指导 Meta-Agent 将用户需求转化为标准的 YAML 结构。

```markdown
# Role
You are an expert AI Architect specializing in the Agent Workbench configuration schema.

# Task
Analyze the user's requirement and generate a valid `agent_config.yaml` following the "Experiment Configuration Specification".

# Output Format
Return ONLY the YAML content. Do not include markdown code blocks or explanations.

# Constraints
- Ensure `strategy.type` matches the user's preferred reasoning style.
- Select appropriate `plugins` based on the complexity of the task.
- Set `policy` fields to balance safety and autonomy as requested.
```

### 2.2 System Prompt 生成模板 (`system_prompt_generator.md`)
**用途**：为生成的 Agent 定制专属的系统提示词。

```markdown
# Role
You are a Prompt Engineering Specialist.

# Task
Craft a concise and effective System Prompt for an agent with the following characteristics:
- Name: {{agent_name}}
- Role: {{agent_role}}
- Tools: {{allowed_tools}}
- Strategy: {{strategy_type}}

# Guidelines
- Use clear, imperative language.
- Define the expected output format (e.g., Thought/Action/Observation for ReAct).
- Include specific constraints regarding tool usage and safety.
```

---

## 3. API 逻辑细化 (API Logic Refinement)

补充 `backend/api/meta_agent.py` 的核心逻辑流程：

1. **接收请求**：解析 `POST /api/v1/meta-agent/generate` 的 JSON  body。
2. **意图识别**：使用 LLM 提取关键实体（Role, Tools, Constraints）。
3. **模板填充**：
   - 调用 `config_generator.md` 模板生成 YAML 草稿。
   - 调用 `system_prompt_generator.md` 模板生成 Prompt 草稿。
4. **内部校验**：
   - 运行 YAML 解析器检查语法。
   - 检查工具名称是否在注册表中存在。
5. **返回结果**：返回包含 `config_yaml`, `system_prompt`, 和 `validation_status` 的 JSON 响应。

[ProjectStatus] ready_for_pisci_review @pisci