import React, { useState, useEffect, useRef } from 'react';
import axios from 'axios';

interface LogEntry {
  timestamp: string;
  level: string;
  message: string;
}

interface ExecutionMonitorProps {
  isActive: boolean;
  yamlConfig: string;
  prompt?: string;
  onFinished: () => void;
}

const EVENT_LEVELS: Record<string, string> = {
  thought: 'INFO',
  action: 'ACTION',
  observation: 'OBS',
  error: 'ERROR',
  complete: 'DONE',
};

const ExecutionMonitor: React.FC<ExecutionMonitorProps> = ({ isActive, yamlConfig, prompt, onFinished }) => {
  const [logs, setLogs] = useState<LogEntry[]>([]);
  const [status, setStatus] = useState<string>('Idle');
  const [experimentId, setExperimentId] = useState<string | null>(null);
  const logEndRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!isActive || !yamlConfig.trim()) {
      return;
    }

    let cancelled = false;
    let eventSource: EventSource | null = null;

    const appendLog = (level: string, message: string) => {
      setLogs((prev) => [
        ...prev,
        {
          timestamp: new Date().toLocaleTimeString(),
          level,
          message,
        },
      ]);
    };

    const startExperiment = async () => {
      setLogs([]);
      setStatus('Starting...');
      setExperimentId(null);

      try {
        const response = await axios.post('/api/v1/experiments', {
          config_yaml: yamlConfig,
          prompt: prompt?.trim() || undefined,
        });
        if (cancelled) return;

        const id = response.data.experiment_id as string;
        setExperimentId(id);
        setStatus('Running');
        appendLog('INFO', `Experiment ${id} started`);

        eventSource = new EventSource(`/api/v1/experiments/${id}/stream`);

        const handleEvent = (eventType: string) => (event: MessageEvent) => {
          if (cancelled) return;
          let content = event.data;
          try {
            const parsed = JSON.parse(event.data);
            content = parsed.content ?? parsed.status ?? event.data;
          } catch {
            // keep raw payload
          }

          const level = EVENT_LEVELS[eventType] ?? 'INFO';
          appendLog(level, content);

          if (eventType === 'complete' || eventType === 'error') {
            setStatus(eventType === 'complete' ? 'Finished' : 'Failed');
            eventSource?.close();
            onFinished();
          }
        };

        eventSource.addEventListener('thought', handleEvent('thought'));
        eventSource.addEventListener('action', handleEvent('action'));
        eventSource.addEventListener('observation', handleEvent('observation'));
        eventSource.addEventListener('error', handleEvent('error'));
        eventSource.addEventListener('complete', handleEvent('complete'));

        eventSource.onerror = () => {
          if (cancelled) return;
          setStatus('Failed');
          appendLog('ERROR', 'Lost connection to experiment stream');
          eventSource?.close();
          onFinished();
        };
      } catch (error) {
        if (cancelled) return;
        const message = axios.isAxiosError(error)
          ? error.response?.data?.detail ?? error.message
          : 'Failed to start experiment';
        setStatus('Failed');
        appendLog('ERROR', String(message));
        onFinished();
      }
    };

    startExperiment();

    return () => {
      cancelled = true;
      eventSource?.close();
    };
  }, [isActive, yamlConfig, prompt, onFinished]);

  useEffect(() => {
    logEndRef.current?.scrollIntoView({ behavior: 'smooth' });
  }, [logs]);

  return (
    <div className="execution-monitor">
      <div className={`status-indicator ${status.toLowerCase()}`}>
        Status: {status}
        {experimentId ? ` · ${experimentId}` : ''}
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
