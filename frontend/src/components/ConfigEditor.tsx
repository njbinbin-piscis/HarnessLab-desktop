import React from 'react';

interface ConfigEditorProps {
  value: string;
  onChange: (value: string) => void;
}

const ConfigEditor: React.FC<ConfigEditorProps> = ({ value, onChange }) => {
  return (
    <div className="config-editor">
      <textarea
        value={value}
        onChange={(e) => onChange(e.target.value)}
        placeholder="# Enter your Agent YAML configuration here..."
        spellCheck={false}
      />
    </div>
  );
};

export default ConfigEditor;
