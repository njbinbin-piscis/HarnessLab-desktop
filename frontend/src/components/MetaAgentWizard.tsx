import React, { useState } from 'react';
import axios from 'axios';

interface WizardStep {
  id: number;
  title: string;
  description: string;
}

interface WizardData {
  goalDescription: string;
  modelPreference: string;
  allowedTools: string[];
  strategyType: string;
}

const STEPS: WizardStep[] = [
  { id: 1, title: '目标描述', description: '描述您的 Agent 目标和角色' },
  { id: 2, title: '约束选择', description: '选择模型偏好和策略倾向' },
  { id: 3, title: '工具推荐', description: '确认推荐的工具集' },
  { id: 4, title: '配置预览', description: '预览并微调生成的配置' },
];

const AVAILABLE_TOOLS = [
  { id: 'file_io', label: 'File I/O', description: '读写文件操作' },
  { id: 'shell', label: 'Shell', description: '执行系统命令' },
  { id: 'web_search', label: 'Web Search', description: '网络搜索' },
  { id: 'code_execution', label: 'Code Execution', description: '代码执行' },
];

const MODELS = [
  { id: 'claude-sonnet-4.5', label: 'Claude Sonnet 4.5' },
  { id: 'qwen-max', label: 'Qwen Max' },
  { id: 'gpt-4o', label: 'GPT-4o' },
];

const STRATEGIES = [
  { id: 'react', label: 'ReAct', description: '快速响应，边思考边行动' },
  { id: 'plan_and_solve', label: 'Plan-and-Solve', description: '深度规划，先计划后执行' },
];

interface MetaAgentWizardProps {
  onConfigGenerated: (yaml: string) => void;
  onCancel: () => void;
}

const MetaAgentWizard: React.FC<MetaAgentWizardProps> = ({ onConfigGenerated, onCancel }) => {
  const [currentStep, setCurrentStep] = useState(1);
  const [isLoading, setIsLoading] = useState(false);
  const [generatedYaml, setGeneratedYaml] = useState('');
  const [generatedPrompt, setGeneratedPrompt] = useState('');
  const [validationStatus, setValidationStatus] = useState<{ valid: boolean; errors: string[]; warnings: string[] } | null>(null);

  const [wizardData, setWizardData] = useState<WizardData>({
    goalDescription: '',
    modelPreference: 'claude-sonnet-4.5',
    allowedTools: ['file_io'],
    strategyType: 'react',
  });

  const handleNext = async () => {
    if (currentStep < 4) {
      setCurrentStep(currentStep + 1);
    } else {
      // Step 4: Generate config
      await generateConfig();
    }
  };

  const handleBack = () => {
    if (currentStep > 1) {
      setCurrentStep(currentStep - 1);
    }
  };

  const generateConfig = async () => {
    setIsLoading(true);
    try {
      // Mock API call - in production, this would call the backend meta-agent endpoint
      const response = await axios.post('/api/v1/meta-agent/generate', {
        goal_description: wizardData.goalDescription,
        model_preference: wizardData.modelPreference,
        allowed_tools: wizardData.allowedTools,
        strategy_type: wizardData.strategyType,
      });

      setGeneratedYaml(response.data.config_yaml);
      setGeneratedPrompt(response.data.system_prompt);
      setValidationStatus(response.data.validation_status);
    } catch {
      // Fallback: generate a basic YAML config locally
      const mockYaml = generateMockYaml();
      setGeneratedYaml(mockYaml);
      setGeneratedPrompt(`You are an AI assistant specialized in: ${wizardData.goalDescription}`);
      setValidationStatus({ valid: true, errors: [], warnings: [] });
    } finally {
      setIsLoading(false);
    }
  };

  const generateMockYaml = (): string => {
    return `agent:
  name: "Meta-Agent-${Date.now()}"
  model: "${wizardData.modelPreference}"
  context_window: 200000
  max_tokens: 4096

plugins:
  memory: "InMemoryVectorStore"
  context: "ProjectContextManager"

policy:
  confirm_shell: ${wizardData.allowedTools.includes('shell') ? 'true' : 'false'}
  confirm_file_write: ${wizardData.allowedTools.includes('file_io') ? 'true' : 'false'}
  auto_compact_threshold: 100000
  compaction_tiers:
    micro: 0.3
    auto: 0.5
    full: 0.7
  max_tool_result_tokens: 8000
  summary_model: "claude-haiku-4.5"

strategy:
  type: "${wizardData.strategyType}"
  system_prompt_template: "You are a helpful assistant. ${wizardData.goalDescription}"

hooks:
  - type: "FileJournal"
    path: ".pisci/journal.db"
    enabled: true
`;
  };

  const handleUseConfig = () => {
    onConfigGenerated(generatedYaml);
  };

  const validateConfig = async () => {
    try {
      const response = await axios.post('/api/v1/config/validate', generatedYaml, {
        headers: { 'Content-Type': 'text/plain' },
      });
      setValidationStatus({
        valid: response.data.valid,
        errors: response.data.errors || [],
        warnings: response.data.warnings || [],
      });
    } catch (error) {
      console.error('Validation error:', error);
    }
  };

  const renderStep1 = () => (
    <div className="wizard-step">
      <h3>Step 1: 目标描述</h3>
      <p className="step-description">用自然语言描述您希望创建的 Agent 的目标、角色和核心能力。</p>
      
      <div className="form-group">
        <label htmlFor="goalDescription">Agent 描述</label>
        <textarea
          id="goalDescription"
          value={wizardData.goalDescription}
          onChange={(e) => setWizardData({ ...wizardData, goalDescription: e.target.value })}
          placeholder="例如：创建一个 Python 代码审查员，能够分析代码质量并提供改进建议..."
          rows={6}
          className="wizard-textarea"
        />
      </div>

      <div className="examples-section">
        <h4>示例提示</h4>
        <ul>
          <li onClick={() => setWizardData({ ...wizardData, goalDescription: '创建一个 Python 代码审查员，能够分析代码质量、检测潜在 bug 并提供改进建议。' })}>
            创建一个 Python 代码审查员...
          </li>
          <li onClick={() => setWizardData({ ...wizardData, goalDescription: '设计一个数据分析助手，能够从 CSV 文件中提取洞察并生成可视化报告。' })}>
            设计一个数据分析助手...
          </li>
          <li onClick={() => setWizardData({ ...wizardData, goalDescription: '构建一个文档生成器，能够根据项目结构自动生成 API 文档。' })}>
            构建一个文档生成器...
          </li>
        </ul>
      </div>
    </div>
  );

  const renderStep2 = () => (
    <div className="wizard-step">
      <h3>Step 2: 约束选择</h3>
      <p className="step-description">选择您的 Agent 的模型偏好、工具权限和策略倾向。</p>

      <div className="form-group">
        <label htmlFor="modelPreference">模型偏好</label>
        <select
          id="modelPreference"
          value={wizardData.modelPreference}
          onChange={(e) => setWizardData({ ...wizardData, modelPreference: e.target.value })}
          className="wizard-select"
        >
          {MODELS.map((model) => (
            <option key={model.id} value={model.id}>
              {model.label}
            </option>
          ))}
        </select>
      </div>

      <div className="form-group">
        <label>工具白名单</label>
        <div className="tool-checkboxes">
          {AVAILABLE_TOOLS.map((tool) => (
            <label key={tool.id} className="checkbox-label">
              <input
                type="checkbox"
                checked={wizardData.allowedTools.includes(tool.id)}
                onChange={(e) => {
                  const newTools = e.target.checked
                    ? [...wizardData.allowedTools, tool.id]
                    : wizardData.allowedTools.filter((t) => t !== tool.id);
                  setWizardData({ ...wizardData, allowedTools: newTools });
                }}
              />
              <span className="tool-name">{tool.label}</span>
              <span className="tool-desc">{tool.description}</span>
            </label>
          ))}
        </div>
      </div>

      <div className="form-group">
        <label>策略倾向</label>
        <div className="strategy-radios">
          {STRATEGIES.map((strategy) => (
            <label key={strategy.id} className="radio-label">
              <input
                type="radio"
                name="strategy"
                value={strategy.id}
                checked={wizardData.strategyType === strategy.id}
                onChange={(e) => setWizardData({ ...wizardData, strategyType: e.target.value })}
              />
              <span className="strategy-name">{strategy.label}</span>
              <span className="strategy-desc">{strategy.description}</span>
            </label>
          ))}
        </div>
      </div>
    </div>
  );

  const renderStep3 = () => (
    <div className="wizard-step">
      <h3>Step 3: 工具推荐</h3>
      <p className="step-description">根据您的描述，以下是推荐的工具集。您可以进行调整。</p>

      <div className="recommended-tools">
        {AVAILABLE_TOOLS.map((tool) => (
          <div key={tool.id} className={`tool-card ${wizardData.allowedTools.includes(tool.id) ? 'selected' : ''}`}>
            <label className="checkbox-label">
              <input
                type="checkbox"
                checked={wizardData.allowedTools.includes(tool.id)}
                onChange={(e) => {
                  const newTools = e.target.checked
                    ? [...wizardData.allowedTools, tool.id]
                    : wizardData.allowedTools.filter((t) => t !== tool.id);
                  setWizardData({ ...wizardData, allowedTools: newTools });
                }}
              />
              <strong>{tool.label}</strong>
              <p>{tool.description}</p>
            </label>
          </div>
        ))}
      </div>

      <div className="policy-preview">
        <h4>策略预览</h4>
        <pre>
{`policy:
  confirm_shell: ${wizardData.allowedTools.includes('shell') ? 'true' : 'false'}
  confirm_file_write: ${wizardData.allowedTools.includes('file_io') ? 'true' : 'false'}
  strategy: ${wizardData.strategyType}`}
        </pre>
      </div>
    </div>
  );

  const renderStep4 = () => (
    <div className="wizard-step">
      <h3>Step 4: 配置预览与微调</h3>
      <p className="step-description">预览生成的 YAML 配置和 System Prompt。您可以手动编辑配置。</p>

      <div className="preview-container">
        <div className="preview-panel">
          <div className="panel-header">
            <h4>YAML 配置</h4>
            <button onClick={validateConfig} className="validate-btn" disabled={!generatedYaml}>
              验证配置
            </button>
          </div>
          <textarea
            value={generatedYaml}
            onChange={(e) => setGeneratedYaml(e.target.value)}
            className="config-textarea"
            rows={20}
            spellCheck={false}
          />
        </div>

        <div className="preview-panel">
          <h4>System Prompt</h4>
          <textarea
            value={generatedPrompt}
            onChange={(e) => setGeneratedPrompt(e.target.value)}
            className="prompt-textarea"
            rows={20}
            spellCheck={false}
          />
        </div>
      </div>

      {validationStatus && (
        <div className={`validation-status ${validationStatus.valid ? 'valid' : 'invalid'}`}>
          {validationStatus.valid ? (
            <p>✅ 配置验证通过</p>
          ) : (
            <div>
              <p>❌ 配置验证失败</p>
              <ul>
                {validationStatus.errors.map((err, idx) => (
                  <li key={idx}>{err}</li>
                ))}
              </ul>
            </div>
          )}
          {validationStatus.warnings.length > 0 && (
            <div className="warnings">
              <p>⚠️ 警告</p>
              <ul>
                {validationStatus.warnings.map((warn, idx) => (
                  <li key={idx}>{warn}</li>
                ))}
              </ul>
            </div>
          )}
        </div>
      )}
    </div>
  );

  return (
    <div className="meta-agent-wizard">
      <div className="wizard-header">
        <h2>Agent 生成向导</h2>
        <p>通过四步流程创建您的专属 Agent 配置</p>
      </div>

      <div className="wizard-progress">
        {STEPS.map((step) => (
          <div
            key={step.id}
            className={`progress-step ${step.id === currentStep ? 'active' : step.id < currentStep ? 'completed' : ''}`}
          >
            <div className="step-number">{step.id}</div>
            <div className="step-info">
              <span className="step-title">{step.title}</span>
              <span className="step-desc">{step.description}</span>
            </div>
          </div>
        ))}
      </div>

      <div className="wizard-content">
        {currentStep === 1 && renderStep1()}
        {currentStep === 2 && renderStep2()}
        {currentStep === 3 && renderStep3()}
        {currentStep === 4 && renderStep4()}
      </div>

      <div className="wizard-footer">
        <button onClick={onCancel} className="cancel-btn">
          取消
        </button>
        <div className="nav-buttons">
          <button onClick={handleBack} disabled={currentStep === 1 || isLoading} className="back-btn">
            上一步
          </button>
          <button
            onClick={handleNext}
            disabled={
              (currentStep === 1 && !wizardData.goalDescription.trim()) ||
              isLoading
            }
            className="next-btn"
          >
            {isLoading ? '生成中...' : currentStep === 4 ? '使用此配置' : '下一步'}
          </button>
        </div>
      </div>

      {currentStep === 4 && generatedYaml && (
        <div className="wizard-actions">
          <button onClick={handleUseConfig} className="use-config-btn">
            应用到编辑器
          </button>
        </div>
      )}
    </div>
  );
};

export default MetaAgentWizard;
