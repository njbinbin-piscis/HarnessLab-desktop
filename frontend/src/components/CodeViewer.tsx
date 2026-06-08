interface Props {
  code: string;
  path?: string;
}

type Tok = { cls: string; text: string };

const KEYWORDS: Record<string, string[]> = {
  python: ["def","class","return","if","elif","else","for","while","import","from","as","with","try","except","finally","raise","yield","lambda","pass","break","continue","global","nonlocal","assert","async","await","in","is","not","and","or","del"],
  rust: ["fn","let","mut","pub","struct","enum","impl","trait","use","mod","match","if","else","for","while","loop","return","self","crate","super","async","await","move","ref","where","type","const","static","dyn","as","in","break","continue","unsafe"],
  typescript: ["function","const","let","var","return","if","else","for","while","do","switch","case","break","continue","class","extends","implements","interface","type","enum","import","export","from","as","new","await","async","yield","try","catch","finally","throw","typeof","instanceof","in","of","void","this","super","public","private","protected","readonly","static","default"],
};
KEYWORDS.javascript = KEYWORDS.typescript;

const LITERALS = new Set(["true", "false", "null", "None", "True", "False", "undefined", "self"]);

function langFor(path?: string): string {
  const ext = (path ?? "").split(".").pop()?.toLowerCase() ?? "";
  switch (ext) {
    case "py": return "python";
    case "rs": return "rust";
    case "ts":
    case "tsx": return "typescript";
    case "js":
    case "jsx":
    case "mjs": return "javascript";
    case "json": return "json";
    case "toml": return "toml";
    case "yaml":
    case "yml": return "yaml";
    case "sh":
    case "bash": return "shell";
    default: return "plain";
  }
}

function isIdentStart(c: string) {
  return /[A-Za-z_$]/.test(c);
}
function isIdent(c: string) {
  return /[A-Za-z0-9_$]/.test(c);
}

function tokenize(src: string, lang: string): Tok[] {
  const out: Tok[] = [];
  const kws = new Set(KEYWORDS[lang] ?? []);
  const hashComment = ["python", "shell", "yaml", "toml"].includes(lang);
  const slashComment = ["rust", "typescript", "javascript"].includes(lang);
  const blockComment = ["rust", "typescript", "javascript"].includes(lang);
  let i = 0;
  const n = src.length;
  const push = (cls: string, text: string) => text && out.push({ cls, text });

  while (i < n) {
    const c = src[i];

    if (hashComment && c === "#") {
      let j = i + 1;
      while (j < n && src[j] !== "\n") j++;
      push("tok-comment", src.slice(i, j));
      i = j;
      continue;
    }
    if (slashComment && c === "/" && src[i + 1] === "/") {
      let j = i + 2;
      while (j < n && src[j] !== "\n") j++;
      push("tok-comment", src.slice(i, j));
      i = j;
      continue;
    }
    if (blockComment && c === "/" && src[i + 1] === "*") {
      let j = i + 2;
      while (j < n && !(src[j] === "*" && src[j + 1] === "/")) j++;
      j = Math.min(n, j + 2);
      push("tok-comment", src.slice(i, j));
      i = j;
      continue;
    }
    if (c === '"' || c === "'" || c === "`") {
      let j = i + 1;
      while (j < n) {
        if (src[j] === "\\") { j += 2; continue; }
        if (src[j] === c) { j++; break; }
        j++;
      }
      push("tok-str", src.slice(i, j));
      i = j;
      continue;
    }
    if (/[0-9]/.test(c)) {
      let j = i + 1;
      while (j < n && /[0-9._eExXa-fA-F]/.test(src[j])) j++;
      push("tok-num", src.slice(i, j));
      i = j;
      continue;
    }
    if (isIdentStart(c)) {
      let j = i + 1;
      while (j < n && isIdent(src[j])) j++;
      const word = src.slice(i, j);
      if (LITERALS.has(word)) push("tok-lit", word);
      else if (kws.has(word)) push("tok-kw", word);
      else push("tok-plain", word);
      i = j;
      continue;
    }
    // Run of plain / whitespace / punctuation until the next "interesting" char.
    let j = i + 1;
    while (j < n && !isIdentStart(src[j]) && !/[0-9"'`]/.test(src[j])
           && src[j] !== "#" && src[j] !== "/") {
      j++;
    }
    const chunk = src.slice(i, j);
    push(/[{}[\]().,;:]/.test(chunk.trim()) ? "tok-punc" : "tok-plain", chunk);
    i = j;
  }
  return out;
}

export function CodeViewer({ code, path }: Props) {
  const lang = langFor(path);
  const tokens = lang === "plain" ? [{ cls: "tok-plain", text: code }] : tokenize(code, lang);
  const lineCount = code.length ? code.split("\n").length : 1;
  const gutter = Array.from({ length: lineCount }, (_, i) => i + 1).join("\n");

  return (
    <div className="code-viewer">
      <pre className="cv-gutter" aria-hidden="true">{gutter}</pre>
      <pre className="cv-code">
        <code>
          {tokens.map((tk, i) => (
            <span key={i} className={tk.cls}>{tk.text}</span>
          ))}
        </code>
      </pre>
    </div>
  );
}
