import { useState } from "react";

interface Props {
  code: string;
  lang?: string;
  title?: string;
}

export function CodeBlock({ code, lang, title }: Props) {
  const [copied, setCopied] = useState(false);

  const copy = async () => {
    try {
      await navigator.clipboard.writeText(code);
    } catch {
      // Fallback for non-secure contexts.
      const ta = document.createElement("textarea");
      ta.value = code;
      document.body.appendChild(ta);
      ta.select();
      document.execCommand("copy");
      document.body.removeChild(ta);
    }
    setCopied(true);
    setTimeout(() => setCopied(false), 1400);
  };

  return (
    <div className="codeblock">
      <div className="cb-head">
        <span className="cb-lang">{title ?? lang ?? "code"}</span>
        <button className={"copy-btn" + (copied ? " ok" : "")} onClick={copy}>
          {copied ? "Copied" : "Copy"}
        </button>
      </div>
      <pre>
        <code>{code}</code>
      </pre>
    </div>
  );
}
