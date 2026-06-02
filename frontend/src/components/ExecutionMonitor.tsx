/* eslint-disable react-hooks/set-state-in-effect */
import React, { useState, useEffect, useRef } from 'react';

interface LogEntry {
  timestamp: string;
  level: string;
  message: string;
}

interface ExecutionMonitorProps {
  isActive: boolean;
  yamlConfig: string;
  onFinished: () => void;
}

const ExecutionMonitor: React.FC<ExecutionMonitorProps> = ({ isActive, yamlConfig, onFinished }) => {
  // yamlConfig can be used for backend API calls in future iterations
  void yamlConfig;
  const [logs, setLogs] = useState<LogEntry[]>([]);
  const [status, setStatus] = useState<string>('Idle');
  const logEndRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!isActive) {
      setStatus('Idle');
      return;
    }

    // Reset state when activation starts
    setLogs([]);
    setStatus('Starting...');

    let cancelled = false;

    // Simulate a stream for now since backend might not be fully ready or CORS might be an issue in prototype
    // In a real scenario, we'd connect to the backend SSE endpoint
    const mockStream = async () => {
      if (cancelled) return;
      setStatus('Running');
      const steps = [
        "Initializing Agent Context...",
        "Loading YAML Configuration...",
        "Parsing Strategy: ReAct",
        "Step 1: Analyzing user request...",
        "Tool Call: search_web(query='Agent Workbench')",
        "Observation: Found 3 relevant results.",
        "Step 2: Synthesizing information...",
        "Final Answer: Agent Workbench is a platform for...",
        "Execution Completed."
      ];

      for (const step of steps) {
        if (cancelled) return;
        await new Promise(r => setTimeout(r, 1000));
        setLogs(prev => [...prev, {
          timestamp: new Date().toLocaleTimeString(),
          level: 'INFO',
          message: step
        }]);
      }
      if (!cancelled) {
        setStatus('Finished');
        onFinished();
      }
    };

    mockStream();

    return () => {
      cancelled = true;
    };
  }, [isActive, onFinished]);

  useEffect(() => {
    logEndRef.current?.scrollIntoView({ behavior: 'smooth' });
  }, [logs]);

  return (
    <div className="execution-monitor">
      <div className={`status-indicator ${status.toLowerCase()}`}>
        Status: {status}
      </div>
      <div className="log-container">
        {logs.map((log, index) => (
          <div key={index} className={`log-entry ${log.level.toLowerCase()}`}>
            <span className="log-time">[{log.timestamp}]</span>
            <span className="log-level">{log.level}</span>
            <span className="log-message">{log.message}</span>
          </div>
        ))}
        <div ref={logEndRef} />
      </div>
    </div>
  );
};

export default ExecutionMonitor;
