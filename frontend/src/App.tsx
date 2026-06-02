import { useState } from 'react'
import ConfigEditor from './components/ConfigEditor'
import ExecutionMonitor from './components/ExecutionMonitor'
import MetaAgentWizard from './components/MetaAgentWizard'
import './App.css'

function App() {
  const [yamlConfig, setYamlConfig] = useState('')
  const [isRunning, setIsRunning] = useState(false)
  const [showWizard, setShowWizard] = useState(false)

  const handleConfigGenerated = (yaml: string) => {
    setYamlConfig(yaml)
    setShowWizard(false)
  }

  return (
    <div className="app-container">
      <header className="app-header">
        <h1>Agent Workbench</h1>
        <p>0-Code Agent Design & Visualization Platform</p>
      </header>
      
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
    </div>
  )
}

export default App
