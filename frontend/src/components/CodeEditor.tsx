import { useLayoutEffect, useRef } from "react";

interface Props {
  value: string;
  onChange?: (next: string) => void;
  readOnly?: boolean;
  minRows?: number;
  ariaLabel?: string;
}

type Tok = { cls: string; text: string };

// Minimal JSON tokenizer → colored spans. Falls back to plain text on anything
// it doesn't recognize, so partially-typed (invalid) JSON still renders.
function tokenizeJson(src: string): Tok[] {
  const out: Tok[] = [];
  // Order matters: strings first, then literals, numbers, punctuation, ws.
  const re =
    /("(?:\\.|[^"\\])*"\s*:)|("(?:\\.|[^"\\])*")|(\btrue\b|\bfalse\b|\bnull\b)|(-?\d+(?:\.\d+)?(?:[eE][+-]?\d+)?)|([{}[\],:])|(\s+)|([^\s{}[\],:"]+)/g;
  let m: RegExpExecArray | null;
  let last = 0;
  while ((m = re.exec(src))) {
    if (m.index > last) out.push({ cls: "tok-plain", text: src.slice(last, m.index) });
    if (m[1]) out.push({ cls: "tok-key", text: m[1] });
    else if (m[2]) out.push({ cls: "tok-str", text: m[2] });
    else if (m[3]) out.push({ cls: "tok-lit", text: m[3] });
    else if (m[4]) out.push({ cls: "tok-num", text: m[4] });
    else if (m[5]) out.push({ cls: "tok-punc", text: m[5] });
    else if (m[6]) out.push({ cls: "tok-plain", text: m[6] });
    else out.push({ cls: "tok-plain", text: m[7] ?? "" });
    last = re.lastIndex;
  }
  if (last < src.length) out.push({ cls: "tok-plain", text: src.slice(last) });
  return out;
}

export function CodeEditor({ value, onChange, readOnly, minRows = 12, ariaLabel }: Props) {
  const taRef = useRef<HTMLTextAreaElement>(null);
  const preRef = useRef<HTMLPreElement>(null);

  // Keep the highlight layer scroll-synced with the textarea.
  const sync = () => {
    if (taRef.current && preRef.current) {
      preRef.current.scrollTop = taRef.current.scrollTop;
      preRef.current.scrollLeft = taRef.current.scrollLeft;
    }
  };
  useLayoutEffect(sync, [value]);

  const tokens = tokenizeJson(value);

  return (
    <div className="code-editor" style={{ minHeight: `${minRows * 1.5}em` }}>
      <pre className="code-editor-hl" ref={preRef} aria-hidden="true">
        <code>
          {tokens.map((tk, i) => (
            <span key={i} className={tk.cls}>
              {tk.text}
            </span>
          ))}
          {"\n"}
        </code>
      </pre>
      <textarea
        ref={taRef}
        className="code-editor-input"
        value={value}
        readOnly={readOnly}
        spellCheck={false}
        aria-label={ariaLabel}
        wrap="off"
        onScroll={sync}
        onChange={(e) => onChange?.(e.target.value)}
      />
    </div>
  );
}
