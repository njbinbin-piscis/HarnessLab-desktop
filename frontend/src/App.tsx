import { useState } from 'react'
import ConfigEditor from './components/ConfigEditor'
import ExecutionMonitor from './components/ExecutionMonitor'
import MetaAgentWizard from './components/MetaAgentWizard'
import EvaluationDashboard from './components/EvaluationDashboard'
import DesignStudio from './components/DesignStudio'
import './App.css'

type Tab = 'design' | 'studio' | 'evaluation'

function App() {
  const [yamlConfig, setYamlConfig] = useState('')
  const [prompt, setPrompt] = useState('')
  const [isRunning, setIsRunning] = useState(false)
  const [showWizard, setShowWizard] = useState(false)
  const [tab, setTab] = useState<Tab>('design')

  const handleConfigGenerated = (yaml: string) => {
    setYamlConfig(yaml)
    setShowWizard(false)
  }

  return (
    <div className="app-container">
      <header className="app-header">
        <h1>Agent Workbench</h1>
        <p>可插拔 · 可调参 · 可观察的 Agent 实验平台</p>
        <nav className="app-tabs">
          <button
            className={tab === 'design' ? 'tab active' : 'tab'}
            onClick={() => setTab('design')}
          >
            设计 & 运行
          </button>
          <button
            className={tab === 'studio' ? 'tab active' : 'tab'}
            onClick={() => setTab('studio')}
          >
            设计助手 & 自举
          </button>
          <button
            className={tab === 'evaluation' ? 'tab active' : 'tab'}
            onClick={() => setTab('evaluation')}
          >
            验证集 & 评测
          </button>
        </nav>
      </header>

      {tab === 'evaluation' ? (
        <main className="app-main-full">
          <EvaluationDashboard initialYaml={yamlConfig || undefined} />
        </main>
      ) : tab === 'studio' ? (
        <main className="app-main-full">
          <DesignStudio
            onUseConfig={(yaml) => {
              setYamlConfig(yaml)
              setTab('design')
            }}
          />
        </main>
      ) : (
      <main className="app-main">
        {!showWizard ? (
          <>
            <div className="editor-section">
              <div className="editor-header">
                <h2>YAML Configuration</h2>
                <button 
                  className="wizard-btn"
                  onClick={() => setShowWizard(true)}
                >
                  ✨ Use Agent Wizard
                </button>
              </div>
              <ConfigEditor 
                value={yamlConfig} 
                onChange={setYamlConfig} 
              />
              <textarea
                className="prompt-input"
                value={prompt}
                onChange={(e) => setPrompt(e.target.value)}
                placeholder="Prompt for the agent (optional — a self-test prompt is used if empty)"
                rows={3}
                spellCheck={false}
              />
              <button 
                className="run-button"
                onClick={() => setIsRunning(true)}
                disabled={!yamlConfig || isRunning}
              >
                {isRunning ? 'Running...' : 'Run Agent'}
              </button>
            </div>
            
            <div className="monitor-section">
              <h2>Execution Monitor</h2>
              <ExecutionMonitor 
                isActive={isRunning} 
                yamlConfig={yamlConfig}
                prompt={prompt}
                onFinished={() => setIsRunning(false)}
              />
            </div>
          </>
        ) : (
          <MetaAgentWizard 
            onConfigGenerated={handleConfigGenerated}
            onCancel={() => setShowWizard(false)}
          />
        )}
      </main>
      )}
    </div>
  )
}

export default App
