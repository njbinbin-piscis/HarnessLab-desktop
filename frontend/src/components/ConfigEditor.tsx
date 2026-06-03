import React, { useEffect, useState } from 'react';
import axios from 'axios';

interface ConfigEditorProps {
  value: string;
  onChange: (value: string) => void;
}

interface ValidationState {
  valid: boolean;
  errors: string[];
  warnings: string[];
}

const ConfigEditor: React.FC<ConfigEditorProps> = ({ value, onChange }) => {
  const [validation, setValidation] = useState<ValidationState | null>(null);
  const [isValidating, setIsValidating] = useState(false);

  const trimmed = value.trim();

  useEffect(() => {
    if (!trimmed) {
      return;
    }

    const timer = window.setTimeout(async () => {
      setIsValidating(true);
      try {
        const response = await axios.post('/api/v1/config/validate', value, {
          headers: { 'Content-Type': 'text/plain' },
        });
        setValidation({
          valid: response.data.valid,
          errors: response.data.errors ?? [],
          warnings: response.data.warnings ?? [],
        });
      } catch {
        setValidation({ valid: false, errors: ['Validation request failed'], warnings: [] });
      } finally {
        setIsValidating(false);
      }
    }, 600);

    return () => window.clearTimeout(timer);
  }, [trimmed]);

  return (
    <div className="config-editor">
      <textarea
        value={value}
        onChange={(e) => onChange(e.target.value)}
        placeholder="# Enter your Agent YAML configuration here..."
        spellCheck={false}
      />
      {isValidating && <p className="validation-hint">Validating...</p>}
      {trimmed && validation && !isValidating && (
        <div className={`validation-status ${validation.valid ? 'valid' : 'invalid'}`}>
          {validation.valid ? (
            <p>Configuration looks valid</p>
          ) : (
            <ul>
              {validation.errors.map((err, idx) => (
                <li key={idx}>{err}</li>
              ))}
            </ul>
          )}
          {validation.warnings.length > 0 && (
            <ul className="warnings">
              {validation.warnings.map((warn, idx) => (
                <li key={idx}>{warn}</li>
              ))}
            </ul>
          )}
        </div>
      )}
    </div>
  );
};

export default ConfigEditor;
