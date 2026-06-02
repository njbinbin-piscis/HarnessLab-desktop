# Meta-Agent Interaction Wizard Design

## Overview
The "Agent Generation Wizard" is a multi-step frontend interface that guides users through creating custom agents via natural language description. It leverages the Meta-Agent backend to generate YAML configurations and system prompts.

## Wizard Steps

### Step 1: Goal Description (目标描述)
**Purpose**: Capture the user's high-level intent for the agent they want to create.

**UI Components**:
- **Main Input**: Large textarea (min-height: 200px) with placeholder text:
  > "Describe what you want this agent to do. For example: 'Create a Python code reviewer that checks for PEP8 compliance, suggests performance optimizations, and explains each recommendation.'"
  
- **Quick Templates**: Horizontal scrollable chips with common agent types:
  - `Code Reviewer`
  - `Data Analyst`
  - `Research Assistant`
  - `Documentation Writer`
  - `Test Generator`
  - `Security Auditor`
  
- **Optional Sliders**:
  - **Creativity (Temperature)**: Range 0.0–1.0, default 0.7
    - Tooltip: "Higher values produce more creative but less deterministic responses"
  - **Strictness (Policy Enforcement)**: Range 0.0–1.0, default 0.8
    - Tooltip: "Higher values enforce stricter adherence to rules and constraints"

**Validation**:
- Minimum 20 characters required
- Real-time character counter
- "Generate Draft" button disabled until valid input

**Backend Call**: `POST /api/v1/meta-agent/generate` with initial description


### Step 2: Constraint Selection (约束选择)
**Purpose**: Allow users to refine the agent's capabilities and limitations.

**UI Components**:

#### 2.1 Model Preference
- **Dropdown** with options:
  - `qwen-plus` (default, balanced)
  - `qwen-max` (high-quality, slower)
  - `qwen-turbo` (fast, cost-effective)
  - `gpt-4o` (if available)
  - `claude-3-sonnet` (if available)

#### 2.2 Tool Permissions
- **Checkbox Group** with categorized tools:
  
  **File Operations**:
  - ☑ `file_read` (enabled by default)
  - ☑ `file_write` (enabled by default)
  - ☐ `file_delete` (disabled by default, warning icon)
  
  **Code Execution**:
  - ☑ `code_run` (enabled by default)
  - ☐ `shell_command` (disabled by default, warning icon)
  
  **Web Access**:
  - ☐ `web_search`
  - ☐ `browser_automation`
  
  **Data Access**:
  - ☐ `sql_query`
  - ☐ `api_call`
  
  **Custom Tools**:
  - Button: "+ Add Custom Tool" → opens modal to define tool schema

#### 2.3 Execution Constraints
- **Max Turns**: Number input (1–50), default 10
  - Tooltip: "Maximum number of LLM call iterations before forced termination"
  
- **Timeout per Turn**: Dropdown with options:
  - `30s` (default)
  - `60s`
  - `120s`
  - `300s`
  
- **Memory Limit**: Toggle switch (default: ON)
  - When ON: Agent uses sliding window context management
  - When OFF: Agent accumulates full conversation history

#### 2.4 Strategy Selection
- **Radio Buttons**:
  - ○ **ReAct** (default): Reasoning + Acting loop, best for tool-using agents
  - ○ **Plan-and-Solve**: Two-phase approach, best for complex multi-step tasks
  - ○ **Direct Answer**: Single-turn response, best for simple Q&A agents

**Navigation**:
- "← Back" button (returns to Step 1, preserves description)
- "Refine Draft →" button (calls `POST /api/v1/meta-agent/refine` with updated constraints)


### Step 3: Tool Recommendation (工具推荐)
**Purpose**: Display Meta-Agent's suggested tools based on the description and allow manual overrides.

**UI Components**:

#### 3.1 Recommended Tools Panel
- **Card Layout** showing tools grouped by category:
  - Each tool shows:
    - Icon (e.g., 📄 for file_read, ⚙️ for code_run)
    - Tool name
    - Confidence score (e.g., "92% relevant")
    - Brief explanation: "Recommended because your agent needs to read source files"
  
- **Toggle Switches** next to each recommended tool to enable/disable
  
- **"Add More Tools"** button: Opens searchable dropdown of all available tools

#### 3.2 Tool Dependency Warnings
- If user disables a critical tool, show warning banner:
  > ⚠️ Disabling `file_read` may prevent the agent from analyzing code files as described.

#### 3.3 Custom Tool Definition (Optional)
- If user clicks "+ Add Custom Tool":
  - Modal with form fields:
    - Tool Name (e.g., `git_diff`)
    - Description
    - Input Schema (JSON editor with validation)
    - Output Schema (JSON editor)
  - "Test Tool" button to validate schema

**Navigation**:
- "← Back" button
- "Preview Configuration →" button


### Step 4: Configuration Preview (配置预览)
**Purpose**: Show the generated YAML config and system prompt in an editable split-view with real-time validation.

**UI Components**:

#### 4.1 Split-View Editor
- **Left Panel (60% width)**: YAML Configuration Editor
  - Monaco Editor with YAML syntax highlighting
  - Line numbers enabled
  - Auto-indentation
  - Collapsible sections
  
- **Right Panel (40% width)**: System Prompt Editor
  - Markdown editor with preview toggle
  - Shows the generated `system_prompt.md` content
  - Supports basic markdown formatting

#### 4.2 Validation Status Bar
- **Top-right corner** of the preview area:
  - 🟢 **Valid**: All checks passed
  - 🟡 **Warnings**: Config works but has non-critical issues
  - 🔴 **Errors**: Config will fail at runtime (lists errors below)

- **Error/Warning List** (collapsible):
  - Each item shows severity icon, message, and line number (if applicable)
  - Clicking an error scrolls the editor to the problematic line

#### 4.3 Live Validation
- Debounced validation (500ms after last edit)
- Calls `POST /api/v1/config/validate` internally
- Updates status bar in real-time

#### 4.4 Quick Actions
- **"Auto-Fix Errors"** button: Sends config back to Meta-Agent for correction
- **"Reset to Generated"** button: Discards manual edits, reloads original draft
- **"Copy YAML"** button: Copies config to clipboard
- **"Download Config"** button: Downloads as `.yaml` file

**Navigation**:
- "← Back" button (returns to Step 3)
- "Test Drive →" button (proceeds to sandbox testing)


### Step 5: Test Drive (测试运行)
**Purpose**: Run a short, isolated experiment to verify the agent behaves as expected.

**UI Components**:

#### 5.1 Sandbox Environment Indicator
- Banner at top: "🧪 Running in Isolated Sandbox"
- Info icon with tooltip explaining sandbox limits:
  - Max 3 turns
  - No file writes to permanent storage
  - Network access disabled (unless explicitly allowed)

#### 5.2 Test Scenario Selector
- **Dropdown** with predefined test scenarios:
  - "Simple Question": Ask a basic question related to the agent's domain
  - "Tool Usage Test": Trigger a tool call to verify permissions
  - "Edge Case": Test error handling with invalid input
  - "Custom Test": User provides their own test prompt

- **Custom Test Input**: Textarea (shown when "Custom Test" selected)

#### 5.3 Live Execution Log
- **StreamLog Component** (reused from existing experiment viewer):
  - Shows thought/action/observation events in real-time
  - Color-coded by event type:
    - 💭 Thought (blue)
    - ⚙️ Action (orange)
    - 📥 Observation (green)
    - ❌ Error (red)
  
- **Expandable Details**: Click any event to see full metadata

#### 5.4 Performance Metrics
- Small dashboard showing:
  - Total turns used: `2/3`
  - Total tokens: `1,247`
  - Execution time: `4.2s`
  - Tool calls made: `1`

#### 5.5 Test Result Summary
- After completion, shows:
  - ✅ **Success**: Agent completed task within constraints
  - ⚠️ **Partial Success**: Agent worked but hit limits (e.g., max turns)
  - ❌ **Failure**: Agent errored or produced invalid output
  - Brief explanation and suggestions for improvement

**Navigation**:
- "← Back to Edit" button (returns to Step 4 with feedback)
- "Save & Deploy →" button (proceeds to final step)


### Step 6: Save & Deploy (保存与部署)
**Purpose**: Finalize the agent configuration and choose deployment options.

**UI Components**:

#### 6.1 Agent Metadata
- **Name Input**: Pre-filled from config, editable
- **Description Input**: Multi-line text for library catalog
- **Tags Input**: Comma-separated tags (e.g., "code-review, python, static-analysis")
- **Version**: Auto-incremented (starts at `1.0.0`)

#### 6.2 Deployment Options
- **Radio Buttons**:
  - ○ **Save to Library Only**: Stores in `configs/custom/`, no immediate execution
  - ○ **Save and Start Experiment**: Saves config and launches full experiment
  - ○ **Export Shareable Link**: Generates a URL to share the config (requires backend support)

#### 6.3 Library Location
- **Dropdown** for save location:
  - `configs/custom/` (user-specific)
  - `configs/team/` (shared, requires permission)
  - `configs/templates/` (public template, requires admin approval)

#### 6.4 Confirmation Checklist
- ☑ Configuration validated
- ☑ Test drive completed
- ☑ Metadata provided
- "Deploy Agent" button (disabled until all checked)

**Post-Deployment**:
- Success notification with links:
  - "View in Library"
  - "Start New Experiment"
  - "Create Another Agent"


## State Management

### Wizard State Object
```typescript
interface WizardState {
  currentStep: number; // 1-6
  description: string;
  constraints: {
    model: string;
    tools: string[];
    maxTurns: number;
    timeout: number;
    memoryLimit: boolean;
    strategy: 'react' | 'plan-and-solve' | 'direct';
  };
  draftId: string | null;
  configYaml: string;
  systemPrompt: string;
  validationErrors: string[];
  validationWarnings: string[];
  testResult: TestResult | null;
  metadata: {
    name: string;
    description: string;
    tags: string[];
    version: string;
  };
}
```

### Persistence
- Auto-save to `localStorage` every 30 seconds
- Restore prompt on page reload: "You have an unsaved draft. Restore?"
- Clear draft after successful deployment


## Error Handling

### Backend Errors
- **Generation Timeout**: Show "Meta-Agent is taking longer than expected. Retry?" with retry button
- **Invalid Description**: Highlight Step 1 input with specific feedback
- **Validation Failure**: Display errors inline in Step 4 editor

### Network Errors
- Retry with exponential backoff (1s, 2s, 4s)
- After 3 failures: "Connection lost. Please check your network and retry."

### Sandbox Errors
- If test drive fails due to sandbox limits: Suggest increasing constraints in Step 2
- If tool permission error: Highlight the missing tool in Step 3


## Accessibility
- All form inputs have associated `<label>` elements
- Keyboard navigation: Tab through steps, Enter to proceed
- Screen reader announcements for validation status changes
- High-contrast mode support for error/warning indicators
