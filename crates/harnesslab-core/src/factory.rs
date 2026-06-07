//! Code generation (Lab → Factory): turn a [`Project`] into a runnable,
//! standalone multi-agent codebase in Python / TypeScript / Rust.
//!
//! A language-neutral [`ProjectIr`] is built from the project, then a per-target
//! emitter produces a `{relative_path: contents}` file map (namespaced under the
//! project slug). Generated code is self-contained — it does **not** depend on
//! HarnessLab or `piscis-kernel` at runtime — and ships an offline deterministic
//! mock LLM so it compiles / type-checks and runs out of the box.
//!
//! Ported from HarnessLab's Python `factory` (ir + emitters + snippets).

use std::collections::BTreeMap;
use std::io::Write;

use serde::Serialize;
use serde_json::Value;

use crate::project::Project;

/// Supported target languages (stable order for the UI dropdown).
pub fn languages() -> Vec<String> {
    vec!["python".into(), "typescript".into(), "rust".into()]
}

/// The generated file map plus the language it targets.
#[derive(Debug, Clone, Serialize)]
pub struct GenerateOutput {
    pub language: String,
    pub files: BTreeMap<String, String>,
}

// ── IR ───────────────────────────────────────────────────────────────────────

struct AgentIr {
    name: String,
    role: String,
    model: String,
    system_prompt: String,
    loop_name: String,
}

struct ProjectIr {
    name: String,
    slug: String,
    description: String,
    agents: Vec<AgentIr>,
    mode: String,
    coordinator: Option<String>,
}

fn slug(name: &str) -> String {
    let s: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect();
    let trimmed = s.trim_matches('_').to_string();
    if trimmed.is_empty() {
        "agent_project".to_string()
    } else {
        trimmed
    }
}

fn build_ir(project: &Project) -> ProjectIr {
    let agents = project
        .ordered_agents()
        .into_iter()
        .map(|spec| AgentIr {
            name: spec.name.clone(),
            role: spec.role.clone(),
            model: spec.model.clone(),
            system_prompt: spec.system_prompt.clone(),
            loop_name: spec
                .r#loop
                .as_ref()
                .map(|c| c.name.clone())
                .unwrap_or_else(|| "react".to_string()),
        })
        .collect();
    ProjectIr {
        name: project.name.clone(),
        slug: slug(&project.name),
        description: project.description.clone(),
        agents,
        mode: project.topology.mode.clone(),
        coordinator: project
            .topology
            .coordinator
            .as_ref()
            .map(|c| c.name.clone()),
    }
}

/// Quote + escape a string as a source-code string literal. The escape set
/// (`\\ " \n \r \t`) is valid for Python, TypeScript and Rust alike; non-ASCII
/// is emitted verbatim (all three accept UTF-8 in string literals), avoiding
/// JSON's `\uXXXX` which Rust does not accept.
fn esc(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            _ => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Loop strategy → system-prompt steering, mirrored from the library so a
/// generated agent behaves like its experimented counterpart.
const LOOP_ADDENDA: &[(&str, &str)] = &[
    ("react", "Think briefly, then act or give FINAL ANSWER:."),
    (
        "plan_and_solve",
        "Write a numbered PLAN, execute it, then FINAL ANSWER:.",
    ),
    (
        "reflexion",
        "Attempt, self-critique, improve, then FINAL ANSWER:.",
    ),
    (
        "tree_of_thought",
        "Explore candidate branches, pick the best, then FINAL ANSWER:.",
    ),
    (
        "rewoo",
        "Plan tool calls up front, gather evidence, synthesize FINAL ANSWER:.",
    ),
];

fn agent_list_md(ir: &ProjectIr) -> String {
    let mut s = String::new();
    for a in &ir.agents {
        let role = if a.role.is_empty() { "agent" } else { &a.role };
        s.push_str(&format!(
            "- **{}** ({}) — model `{}`, loop `{}`\n",
            a.name, role, a.model, a.loop_name
        ));
    }
    s
}

// ── Coordinator bodies (ported verbatim from snippets.py) ────────────────────

fn coord_py(name: &str) -> String {
    let body = match name {
        "sequential_pipeline" => r#"    context = ""
    result = ""
    for agent in agents:
        result = agent(task, context=context)
        context = (context + "\n[" + agent.name + "] " + result).strip()
    return result"#,
        "majority_vote" => r#"    from collections import Counter
    answers = [a(task) for a in agents]
    norm = [x.strip().lower() for x in answers]
    winner, _ = Counter(norm).most_common(1)[0]
    for a in answers:
        if a.strip().lower() == winner:
            return a
    return answers[0]"#,
        "supervisor" => r#"    supervisor = agents[0]
    workers = agents[1:] or agents
    plan = supervisor("Break down and delegate: " + task)
    outputs = [w(task, context=plan) for w in workers]
    joined = "\n".join("[" + w.name + "] " + o for w, o in zip(workers, outputs))
    return supervisor("Synthesize a final answer for: " + task, context=joined)"#,
        "debate" => r#"    answers = [a(task) for a in agents]
    board = "\n".join("[" + a.name + "] " + ans for a, ans in zip(agents, answers))
    refined = [a("Critique others and refine for: " + task, context=board) for a in agents]
    board2 = "\n".join("[" + a.name + "] " + c for a, c in zip(agents, refined))
    return agents[0]("Reconcile the debate into one answer for: " + task, context=board2)"#,
        "group_chat" => r#"    transcript = ""
    for _ in range(2):
        for agent in agents:
            transcript += "\n[" + agent.name + "] " + agent(task, context=transcript)
    return agents[-1]("Summarize the group's conclusion for: " + task, context=transcript)"#,
        "map_reduce" => r#"    if len(agents) == 1:
        return agents[0](task)
    mappers, reducer = agents[:-1], agents[-1]
    partials = [m("Produce your partial analysis for: " + task) for m in mappers]
    mapped = "\n".join("[" + m.name + "] " + p for m, p in zip(mappers, partials))
    return reducer("Reduce these partial results into a final answer for: " + task, context=mapped)"#,
        "hierarchical" => r#"    if len(agents) == 1:
        return agents[0](task)
    manager, workers = agents[0], agents[1:]
    plan = manager("Decompose into subtasks, one per line, for: " + task)
    results = [w(task, context="Your assignment:\n" + plan) for w in workers]
    joined = "\n".join("[" + w.name + "] " + r for w, r in zip(workers, results))
    return manager("Integrate the workers' results into one final answer for: " + task, context=plan + "\n" + joined)"#,
        "tournament" => r#"    candidates = [a(task) for a in agents]
    if len(candidates) == 1:
        return candidates[0]
    judge = agents[0]
    wins = [0] * len(candidates)
    for i in range(len(candidates)):
        for j in range(i + 1, len(candidates)):
            verdict = judge("Compare A and B for: " + task, context="[A]\n%s\n[B]\n%s" % (candidates[i], candidates[j])).strip().lower()
            win_i = verdict.endswith(" a") or verdict == "a" or "answer: a" in verdict or len(candidates[i]) >= len(candidates[j])
            wins[i if win_i else j] += 1
    best = max(range(len(candidates)), key=lambda x: wins[x])
    return candidates[best]"#,
        other => {
            return format!(
                "    # TODO: port the '{other}' coordinator. Falling back to sequential.\n    context = \"\"\n    result = \"\"\n    for agent in agents:\n        result = agent(task, context=context)\n        context = (context + \"\\n\" + result).strip()\n    return result"
            )
        }
    };
    body.to_string()
}

fn coord_ts(name: &str) -> String {
    let body = match name {
        "sequential_pipeline" => r#"  let context = "";
  let result = "";
  for (const agent of agents) {
    result = await agent.run(task, context);
    context = `${context}\n[${agent.name}] ${result}`.trim();
  }
  return result;"#,
        "majority_vote" => r#"  const answers = await Promise.all(agents.map((a) => a.run(task)));
  const counts = new Map<string, number>();
  for (const a of answers) counts.set(a.trim().toLowerCase(), (counts.get(a.trim().toLowerCase()) ?? 0) + 1);
  let winner = answers[0];
  let best = -1;
  for (const a of answers) {
    const c = counts.get(a.trim().toLowerCase()) ?? 0;
    if (c > best) { best = c; winner = a; }
  }
  return winner;"#,
        "supervisor" => r#"  const supervisor = agents[0];
  const workers = agents.slice(1).length ? agents.slice(1) : agents;
  const plan = await supervisor.run("Break down and delegate: " + task);
  const outputs = await Promise.all(workers.map((w) => w.run(task, plan)));
  const joined = workers.map((w, i) => `[${w.name}] ${outputs[i]}`).join("\n");
  return supervisor.run("Synthesize a final answer for: " + task, joined);"#,
        "debate" => r#"  const answers = await Promise.all(agents.map((a) => a.run(task)));
  const board = agents.map((a, i) => `[${a.name}] ${answers[i]}`).join("\n");
  const refined = await Promise.all(agents.map((a) => a.run("Critique others and refine for: " + task, board)));
  const board2 = agents.map((a, i) => `[${a.name}] ${refined[i]}`).join("\n");
  return agents[0].run("Reconcile the debate into one answer for: " + task, board2);"#,
        "tournament" => r#"  const candidates = await Promise.all(agents.map((a) => a.run(task)));
  if (candidates.length === 1) return candidates[0];
  const judge = agents[0];
  const wins = new Array(candidates.length).fill(0);
  for (let i = 0; i < candidates.length; i++) {
    for (let j = i + 1; j < candidates.length; j++) {
      const verdict = (await judge.run("Compare A and B for: " + task, `[A]\n${candidates[i]}\n[B]\n${candidates[j]}`)).trim().toLowerCase();
      const winI = verdict.endsWith(" a") || verdict === "a" || verdict.includes("answer: a") || candidates[i].length >= candidates[j].length;
      wins[winI ? i : j] += 1;
    }
  }
  let best = 0;
  for (let k = 1; k < wins.length; k++) if (wins[k] > wins[best]) best = k;
  return candidates[best];"#,
        other => {
            return format!(
                "  // TODO: port the '{other}' coordinator. Falling back to sequential.\n  let context = \"\";\n  let result = \"\";\n  for (const agent of agents) {{\n    result = await agent.run(task, context);\n    context = `${{context}}\\n${{result}}`.trim();\n  }}\n  return result;"
            )
        }
    };
    body.to_string()
}

fn coord_rs(name: &str) -> String {
    let body = match name {
        "sequential_pipeline" => r#"    let mut context = String::new();
    let mut result = String::new();
    for agent in agents {
        result = agent.run(task, &context);
        context = format!("{}\n[{}] {}", context, agent.name, result).trim().to_string();
    }
    result"#,
        "majority_vote" => r#"    use std::collections::HashMap;
    let answers: Vec<String> = agents.iter().map(|a| a.run(task, "")).collect();
    let mut counts: HashMap<String, usize> = HashMap::new();
    for a in &answers {
        *counts.entry(a.trim().to_lowercase()).or_insert(0) += 1;
    }
    let mut winner = answers[0].clone();
    let mut best = 0usize;
    for a in &answers {
        let c = counts[&a.trim().to_lowercase()];
        if c > best { best = c; winner = a.clone(); }
    }
    winner"#,
        "supervisor" => r#"    let supervisor = &agents[0];
    let workers = if agents.len() > 1 { &agents[1..] } else { &agents[..] };
    let plan = supervisor.run(&format!("Break down and delegate: {}", task), "");
    let mut joined = String::new();
    for w in workers {
        let o = w.run(task, &plan);
        joined.push_str(&format!("[{}] {}\n", w.name, o));
    }
    supervisor.run(&format!("Synthesize a final answer for: {}", task), &joined)"#,
        "tournament" => r#"    let candidates: Vec<String> = agents.iter().map(|a| a.run(task, "")).collect();
    if candidates.len() == 1 { return candidates[0].clone(); }
    let judge = &agents[0];
    let mut wins = vec![0usize; candidates.len()];
    for i in 0..candidates.len() {
        for j in (i + 1)..candidates.len() {
            let ctx = format!("[A]\n{}\n[B]\n{}", candidates[i], candidates[j]);
            let verdict = judge.run(&format!("Compare A and B for: {}", task), &ctx).trim().to_lowercase();
            let win_i = verdict.ends_with(" a") || verdict == "a" || verdict.contains("answer: a") || candidates[i].len() >= candidates[j].len();
            if win_i { wins[i] += 1; } else { wins[j] += 1; }
        }
    }
    let mut best = 0usize;
    for k in 1..wins.len() { if wins[k] > wins[best] { best = k; } }
    candidates[best].clone()"#,
        other => {
            return format!(
                "    // TODO: port the '{other}' coordinator. Falling back to sequential.\n    let mut context = String::new();\n    let mut result = String::new();\n    for agent in agents {{\n        result = agent.run(task, &context);\n        context = format!(\"{{}}\\n{{}}\", context, result).trim().to_string();\n    }}\n    result"
            )
        }
    };
    body.to_string()
}

// ── Python emitter ───────────────────────────────────────────────────────────

fn emit_python(ir: &ProjectIr) -> BTreeMap<String, String> {
    let mut files = BTreeMap::new();
    let desc = if ir.description.is_empty() {
        ir.name.clone()
    } else {
        ir.description.clone()
    };
    let default_model = ir
        .agents
        .first()
        .map(|a| a.model.clone())
        .unwrap_or_else(|| "mock".to_string());

    // README.md
    let mut readme = format!("# {}\n\n{}\n\nGenerated by **HarnessLab** (Lab → Factory). Standalone Python multi-agent scaffold; it does not depend on HarnessLab at runtime.\n\n## Agents\n", ir.name, if ir.description.is_empty() { "Generated multi-agent system." } else { &ir.description });
    readme.push_str(&agent_list_md(ir));
    readme.push_str(&format!(
        "\n## Coordination\n- mode: `{}`{}\n\n## Run\n\n```bash\npip install -e .\npython -m {}.main \"your task here\"\n```\n\nSet `OPENAI_API_KEY` to use a real model; otherwise a deterministic mock is used.\n",
        ir.mode,
        ir.coordinator.as_ref().map(|c| format!(", coordinator: `{c}`")).unwrap_or_default(),
        ir.slug
    ));
    files.insert("README.md".into(), readme);

    // pyproject.toml
    files.insert(
        "pyproject.toml".into(),
        format!("[build-system]\nrequires = [\"setuptools>=68\", \"wheel\"]\nbuild-backend = \"setuptools.build_meta\"\n\n[project]\nname = \"{}\"\nversion = \"0.1.0\"\ndescription = \"{}\"\nrequires-python = \">=3.10\"\n\n[tool.setuptools.packages.find]\ninclude = [\"{}*\"]\n", ir.slug, desc, ir.slug),
    );

    files.insert(
        format!("{}/__init__.py", ir.slug),
        "\"\"\"Generated multi-agent system.\"\"\"\n".into(),
    );

    // llm.py
    files.insert(
        format!("{}/llm.py", ir.slug),
        format!(
            r#""""Minimal stdlib LLM client with an offline deterministic mock fallback."""
from __future__ import annotations

import hashlib
import json
import os
import re
import urllib.error
import urllib.request

_ANSWER_RE = re.compile(r"(?:final answer|answer)\s*[:：]\s*(.+)", re.IGNORECASE)


def complete(system: str, user: str, model: str = {model}) -> str:
    key = os.environ.get("OPENAI_API_KEY")
    base = os.environ.get("OPENAI_BASE_URL", "https://api.openai.com/v1")
    if key and model != "mock":
        payload = json.dumps({{
            "model": model,
            "messages": [
                {{"role": "system", "content": system}},
                {{"role": "user", "content": user}},
            ],
        }}).encode("utf-8")
        req = urllib.request.Request(
            f"{{base}}/chat/completions",
            data=payload,
            headers={{"Authorization": f"Bearer {{key}}", "Content-Type": "application/json"}},
            method="POST",
        )
        try:
            with urllib.request.urlopen(req, timeout=120) as resp:
                data = json.loads(resp.read().decode("utf-8"))
            return data["choices"][0]["message"]["content"]
        except (urllib.error.URLError, KeyError, ValueError) as exc:
            return f"[llm-error] {{exc}}"
    m = _ANSWER_RE.search(user)
    if m:
        return "FINAL ANSWER: " + m.group(1).strip().splitlines()[0]
    digest = hashlib.sha1(user.encode("utf-8")).hexdigest()[:6]
    return f"FINAL ANSWER: mock-response-{{digest}}"
"#,
            model = esc(&default_model)
        ),
    );

    // agents.py
    let mut agents_py = String::from("\"\"\"Agent definitions generated from the HarnessLab project.\"\"\"\nfrom __future__ import annotations\n\nfrom . import llm\n\nLOOP_ADDENDA = {\n");
    for (k, v) in LOOP_ADDENDA {
        agents_py.push_str(&format!("    {}: {},\n", esc(k), esc(v)));
    }
    agents_py.push_str("}\n\n\nclass Agent:\n    def __init__(self, name: str, role: str, model: str, system_prompt: str, loop: str):\n        self.name = name\n        self.role = role\n        self.model = model\n        self.loop = loop\n        addendum = LOOP_ADDENDA.get(loop, \"\")\n        self.system = (system_prompt + (\"\\n\" + addendum if addendum else \"\")).strip()\n\n    def run(self, task: str, context: str = \"\") -> str:\n        user = task if not context else f\"{task}\\n\\n## Shared context\\n{context}\"\n        return llm.complete(self.system, user, self.model)\n\n    def __call__(self, task: str, context: str = \"\") -> str:\n        return self.run(task, context)\n\n\ndef build_agents() -> list[Agent]:\n    return [\n");
    for a in &ir.agents {
        agents_py.push_str(&format!(
            "        Agent({}, {}, {}, {}, {}),\n",
            esc(&a.name),
            esc(&a.role),
            esc(&a.model),
            esc(&a.system_prompt),
            esc(&a.loop_name)
        ));
    }
    agents_py.push_str("    ]\n");
    files.insert(format!("{}/agents.py", ir.slug), agents_py);

    // coordinator.py
    let mut coord = String::from("\"\"\"Coordination logic generated from the project topology.\"\"\"\nfrom __future__ import annotations\n\nfrom .agents import Agent\n\n\ndef coordinate(task: str, agents: list[Agent]) -> str:\n");
    if ir.mode == "single" || ir.coordinator.is_none() {
        coord.push_str("    # Single-agent topology.\n    return agents[0].run(task)\n");
    } else {
        let name = ir.coordinator.as_ref().unwrap();
        coord.push_str(&format!("    # Coordinator: {name}\n"));
        coord.push_str(&coord_py(name));
        coord.push('\n');
    }
    files.insert(format!("{}/coordinator.py", ir.slug), coord);

    // main.py
    files.insert(
        format!("{}/main.py", ir.slug),
        format!("\"\"\"Entrypoint for the {} multi-agent system.\"\"\"\nfrom __future__ import annotations\n\nimport sys\n\nfrom .agents import build_agents\nfrom .coordinator import coordinate\n\n\ndef main() -> None:\n    task = \" \".join(sys.argv[1:]) or \"Hello, introduce the system.\"\n    agents = build_agents()\n    result = coordinate(task, agents)\n    print(result)\n\n\nif __name__ == \"__main__\":\n    main()\n", ir.name),
    );

    files.insert(
        "data/tasks.jsonl".into(),
        "{\"id\": \"t1\", \"question\": \"What is 2+2? Answer: 4\", \"answer\": \"4\"}\n{\"id\": \"t2\", \"question\": \"Capital of France? Answer: Paris\", \"answer\": \"Paris\"}\n".into(),
    );

    files
}

// ── TypeScript emitter ───────────────────────────────────────────────────────

fn emit_typescript(ir: &ProjectIr, options: &Value) -> BTreeMap<String, String> {
    let mut files = BTreeMap::new();
    let openai = options.get("ts_client").and_then(|v| v.as_str()) == Some("openai");
    let desc = if ir.description.is_empty() {
        ir.name.clone()
    } else {
        ir.description.clone()
    };

    // README.md
    let mut readme = format!("# {}\n\n{}\n\nGenerated by **HarnessLab** (Lab → Factory). Standalone TypeScript (Node) multi-agent scaffold.\n\n## Agents\n", ir.name, if ir.description.is_empty() { "Generated multi-agent system." } else { &ir.description });
    readme.push_str(&agent_list_md(ir));
    readme.push_str(
        "\n## Run\n\n```bash\nnpm install\nnpm run build && npm start -- \"your task here\"\n```\n",
    );
    files.insert("README.md".into(), readme);

    // package.json
    let dep = if openai {
        "\n    \"openai\": \"^4.0.0\"\n  "
    } else {
        ""
    };
    files.insert(
        "package.json".into(),
        format!("{{\n  \"name\": \"{}\",\n  \"version\": \"0.1.0\",\n  \"description\": \"{}\",\n  \"type\": \"module\",\n  \"scripts\": {{\n    \"build\": \"tsc\",\n    \"start\": \"node dist/main.js\"\n  }},\n  \"dependencies\": {{{}}},\n  \"devDependencies\": {{\n    \"typescript\": \"^5.4.0\",\n    \"@types/node\": \"^20.0.0\"\n  }}\n}}\n", ir.slug, desc, dep),
    );

    // tsconfig.json
    files.insert(
        "tsconfig.json".into(),
        "{\n  \"compilerOptions\": {\n    \"target\": \"ES2020\",\n    \"module\": \"ES2020\",\n    \"moduleResolution\": \"node\",\n    \"outDir\": \"dist\",\n    \"rootDir\": \"src\",\n    \"strict\": true,\n    \"esModuleInterop\": true,\n    \"skipLibCheck\": true\n  },\n  \"include\": [\"src/**/*.ts\"]\n}\n".into(),
    );

    // src/llm.ts
    let llm_call = if openai {
        "      const client = new OpenAI({ apiKey: key, baseURL: base });\n      const resp = await client.chat.completions.create({\n        model,\n        messages: [\n          { role: \"system\", content: system },\n          { role: \"user\", content: user },\n        ],\n      });\n      return resp.choices?.[0]?.message?.content ?? \"\";"
    } else {
        "      const resp = await fetch(`${base}/chat/completions`, {\n        method: \"POST\",\n        headers: { Authorization: `Bearer ${key}`, \"Content-Type\": \"application/json\" },\n        body: JSON.stringify({\n          model,\n          messages: [\n            { role: \"system\", content: system },\n            { role: \"user\", content: user },\n          ],\n        }),\n      });\n      const data: any = await resp.json();\n      return data.choices?.[0]?.message?.content ?? \"\";"
    };
    let openai_import = if openai {
        "import OpenAI from \"openai\";\n"
    } else {
        ""
    };
    files.insert(
        "src/llm.ts".into(),
        format!("// LLM client with an offline deterministic mock fallback.\nimport {{ createHash }} from \"crypto\";\n{import}\nconst ANSWER_RE = /(?:final answer|answer)\\s*[:：]\\s*(.+)/i;\n\nexport async function complete(system: string, user: string, model: string): Promise<string> {{\n  const key = process.env.OPENAI_API_KEY;\n  const base = process.env.OPENAI_BASE_URL ?? \"https://api.openai.com/v1\";\n  if (key && model !== \"mock\") {{\n    try {{\n{call}\n    }} catch (e) {{\n      return `[llm-error] ${{e}}`;\n    }}\n  }}\n  const m = user.match(ANSWER_RE);\n  if (m) return \"FINAL ANSWER: \" + m[1].trim().split(\"\\n\")[0];\n  const digest = createHash(\"sha1\").update(user).digest(\"hex\").slice(0, 6);\n  return `FINAL ANSWER: mock-response-${{digest}}`;\n}}\n", import = openai_import, call = llm_call),
    );

    // src/agents.ts
    let mut agents_ts = String::from("// Agent definitions generated from the HarnessLab project.\nimport { complete } from \"./llm.js\";\n\nconst LOOP_ADDENDA: Record<string, string> = {\n");
    for (k, v) in LOOP_ADDENDA {
        agents_ts.push_str(&format!("  {}: {},\n", esc(k), esc(v)));
    }
    agents_ts.push_str("};\n\nexport class Agent {\n  name: string;\n  role: string;\n  model: string;\n  loop: string;\n  system: string;\n\n  constructor(name: string, role: string, model: string, systemPrompt: string, loop: string) {\n    this.name = name;\n    this.role = role;\n    this.model = model;\n    this.loop = loop;\n    const addendum = LOOP_ADDENDA[loop] ?? \"\";\n    this.system = (systemPrompt + (addendum ? \"\\n\" + addendum : \"\")).trim();\n  }\n\n  async run(task: string, context = \"\"): Promise<string> {\n    const user = context ? `${task}\\n\\n## Shared context\\n${context}` : task;\n    return complete(this.system, user, this.model);\n  }\n}\n\nexport function buildAgents(): Agent[] {\n  return [\n");
    for a in &ir.agents {
        agents_ts.push_str(&format!(
            "    new Agent({}, {}, {}, {}, {}),\n",
            esc(&a.name),
            esc(&a.role),
            esc(&a.model),
            esc(&a.system_prompt),
            esc(&a.loop_name)
        ));
    }
    agents_ts.push_str("  ];\n}\n");
    files.insert("src/agents.ts".into(), agents_ts);

    // src/coordinator.ts
    let mut coord = String::from("// Coordination logic generated from the project topology.\nimport { Agent } from \"./agents.js\";\n\nexport async function coordinate(task: string, agents: Agent[]): Promise<string> {\n");
    if ir.mode == "single" || ir.coordinator.is_none() {
        coord.push_str("  // Single-agent topology.\n  return agents[0].run(task);\n");
    } else {
        let name = ir.coordinator.as_ref().unwrap();
        coord.push_str(&format!("  // Coordinator: {name}\n"));
        coord.push_str(&coord_ts(name));
        coord.push('\n');
    }
    coord.push_str("}\n");
    files.insert("src/coordinator.ts".into(), coord);

    // src/main.ts
    files.insert(
        "src/main.ts".into(),
        format!("// Entrypoint for the {} multi-agent system.\nimport {{ buildAgents }} from \"./agents.js\";\nimport {{ coordinate }} from \"./coordinator.js\";\n\nasync function main(): Promise<void> {{\n  const task = process.argv.slice(2).join(\" \") || \"Hello, introduce the system.\";\n  const agents = buildAgents();\n  const result = await coordinate(task, agents);\n  console.log(result);\n}}\n\nmain();\n", ir.name),
    );

    files
}

// ── Rust emitter ─────────────────────────────────────────────────────────────

fn emit_rust(ir: &ProjectIr, options: &Value) -> BTreeMap<String, String> {
    let mut files = BTreeMap::new();
    let reqwest = options.get("rust_client").and_then(|v| v.as_str()) == Some("reqwest");
    let desc = if ir.description.is_empty() {
        ir.name.clone()
    } else {
        ir.description.clone()
    };

    // README.md
    let mut readme = format!("# {}\n\n{}\n\nGenerated by **HarnessLab** (Lab → Factory). Standalone Rust multi-agent scaffold (std-only; choose the `reqwest` client for a real LLM provider).\n\n## Agents\n", ir.name, if ir.description.is_empty() { "Generated multi-agent system." } else { &ir.description });
    readme.push_str(&agent_list_md(ir));
    readme.push_str("\n## Run\n\n```bash\ncargo run -- \"your task here\"\n```\n");
    files.insert("README.md".into(), readme);

    // Cargo.toml
    let deps = if reqwest {
        "reqwest = { version = \"0.12\", features = [\"blocking\", \"json\"] }\nserde = { version = \"1\", features = [\"derive\"] }\nserde_json = \"1\"\n"
    } else {
        ""
    };
    files.insert(
        "Cargo.toml".into(),
        format!("[package]\nname = \"{}\"\nversion = \"0.1.0\"\nedition = \"2021\"\ndescription = \"{}\"\n\n[dependencies]\n{}\n[[bin]]\nname = \"{}\"\npath = \"src/main.rs\"\n", ir.slug, desc, deps, ir.slug),
    );

    // src/llm.rs
    let llm_rs = if reqwest {
        r#"//! LLM client: a real OpenAI-compatible call via `reqwest` (blocking), with an
//! offline deterministic mock fallback.

use serde_json::json;

pub fn complete(system: &str, user: &str, model: &str) -> String {
    if let Ok(key) = std::env::var("OPENAI_API_KEY") {
        if model != "mock" {
            let base = std::env::var("OPENAI_BASE_URL")
                .unwrap_or_else(|_| "https://api.openai.com/v1".to_string());
            match call_api(&base, &key, system, user, model) {
                Ok(text) => return text,
                Err(e) => return format!("[llm-error] {}", e),
            }
        }
    }
    mock(user)
}

fn call_api(base: &str, key: &str, system: &str, user: &str, model: &str)
    -> Result<String, Box<dyn std::error::Error>> {
    let body = json!({
        "model": model,
        "messages": [
            {"role": "system", "content": system},
            {"role": "user", "content": user}
        ]
    });
    let client = reqwest::blocking::Client::new();
    let resp: serde_json::Value = client
        .post(format!("{}/chat/completions", base.trim_end_matches('/')))
        .bearer_auth(key)
        .json(&body)
        .send()?
        .json()?;
    Ok(resp["choices"][0]["message"]["content"]
        .as_str()
        .unwrap_or("")
        .to_string())
}

fn mock(user: &str) -> String {
    let lower = user.to_lowercase();
    if let Some(idx) = lower.find("answer:") {
        let rest = &user[idx + "answer:".len()..];
        let line = rest.trim().lines().next().unwrap_or("").trim();
        return format!("FINAL ANSWER: {}", line);
    }
    let h = fnv1a(user) & 0xFF_FFFF;
    format!("FINAL ANSWER: mock-response-{:06x}", h)
}

fn fnv1a(s: &str) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in s.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}
"#
    } else {
        r#"//! Minimal LLM client with an offline deterministic mock fallback.
//!
//! For a real provider, regenerate with the `reqwest` client option.

pub fn complete(_system: &str, user: &str, _model: &str) -> String {
    let lower = user.to_lowercase();
    if let Some(idx) = lower.find("answer:") {
        let rest = &user[idx + "answer:".len()..];
        let line = rest.trim().lines().next().unwrap_or("").trim();
        return format!("FINAL ANSWER: {}", line);
    }
    let h = fnv1a(user) & 0xFF_FFFF;
    format!("FINAL ANSWER: mock-response-{:06x}", h)
}

fn fnv1a(s: &str) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in s.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}
"#
    };
    files.insert("src/llm.rs".into(), llm_rs.to_string());

    // src/agents.rs
    let mut agents_rs = String::from("//! Agent definitions generated from the HarnessLab project.\n\nuse crate::llm;\n\npub struct Agent {\n    pub name: String,\n    pub role: String,\n    pub model: String,\n    pub system: String,\n    pub loop_name: String,\n}\n\nfn loop_addendum(name: &str) -> &'static str {\n    match name {\n");
    for (k, v) in LOOP_ADDENDA {
        agents_rs.push_str(&format!("        {} => {},\n", esc(k), esc(v)));
    }
    agents_rs.push_str("        _ => \"\",\n    }\n}\n\nimpl Agent {\n    pub fn new(name: &str, role: &str, model: &str, system_prompt: &str, loop_name: &str) -> Self {\n        let add = loop_addendum(loop_name);\n        let system = if add.is_empty() {\n            system_prompt.to_string()\n        } else {\n            format!(\"{}\\n{}\", system_prompt, add)\n        };\n        Agent {\n            name: name.to_string(),\n            role: role.to_string(),\n            model: model.to_string(),\n            system,\n            loop_name: loop_name.to_string(),\n        }\n    }\n\n    pub fn run(&self, task: &str, context: &str) -> String {\n        let user = if context.is_empty() {\n            task.to_string()\n        } else {\n            format!(\"{}\\n\\n## Shared context\\n{}\", task, context)\n        };\n        llm::complete(&self.system, &user, &self.model)\n    }\n}\n\npub fn build_agents() -> Vec<Agent> {\n    vec![\n");
    for a in &ir.agents {
        agents_rs.push_str(&format!(
            "        Agent::new({}, {}, {}, {}, {}),\n",
            esc(&a.name),
            esc(&a.role),
            esc(&a.model),
            esc(&a.system_prompt),
            esc(&a.loop_name)
        ));
    }
    agents_rs.push_str("    ]\n}\n");
    // Touch every field so the scaffold compiles warning-clean regardless of
    // which coordinator body the topology selected.
    agents_rs.push_str("\n#[allow(dead_code)]\nfn _fields(a: &Agent) -> (&str, &str, &str, &str, &str) {\n    (&a.name, &a.role, &a.model, &a.system, &a.loop_name)\n}\n");
    files.insert("src/agents.rs".into(), agents_rs);

    // src/coordinator.rs
    let mut coord = String::from("//! Coordination logic generated from the project topology.\n\nuse crate::agents::Agent;\n\npub fn coordinate(task: &str, agents: &[Agent]) -> String {\n");
    if ir.mode == "single" || ir.coordinator.is_none() {
        coord.push_str("    // Single-agent topology.\n    agents[0].run(task, \"\")\n");
    } else {
        let name = ir.coordinator.as_ref().unwrap();
        coord.push_str(&format!("    // Coordinator: {name}\n"));
        coord.push_str(&coord_rs(name));
        coord.push('\n');
    }
    coord.push_str("}\n");
    files.insert("src/coordinator.rs".into(), coord);

    // src/main.rs
    files.insert(
        "src/main.rs".into(),
        format!("//! Entrypoint for the {} multi-agent system.\n\nmod agents;\nmod coordinator;\nmod llm;\n\nfn main() {{\n    let args: Vec<String> = std::env::args().skip(1).collect();\n    let task = if args.is_empty() {{\n        \"Hello, introduce the system.\".to_string()\n    }} else {{\n        args.join(\" \")\n    }};\n    let agents = agents::build_agents();\n    let result = coordinator::coordinate(&task, &agents);\n    println!(\"{{}}\", result);\n}}\n", ir.name),
    );

    files
}

/// Generate a runnable project in `language`, returning files namespaced under
/// the project slug directory.
pub fn generate(
    project: &Project,
    language: &str,
    options: &Value,
) -> Result<GenerateOutput, String> {
    let ir = build_ir(project);
    let raw = match language {
        "python" => emit_python(&ir),
        "typescript" => emit_typescript(&ir, options),
        "rust" => emit_rust(&ir, options),
        other => {
            return Err(format!(
                "unsupported language {other:?}; choose from python, typescript, rust"
            ))
        }
    };
    let files = raw
        .into_iter()
        .map(|(path, content)| (format!("{}/{}", ir.slug, path), content))
        .collect();
    Ok(GenerateOutput {
        language: language.to_string(),
        files,
    })
}

/// Filesystem-safe slug derived from a project display name.
pub fn project_slug(name: &str) -> String {
    slug(name)
}

/// Generate a zip archive of the factory output (paths relative to archive root).
pub fn generate_zip(project: &Project, language: &str, options: &Value) -> Result<Vec<u8>, String> {
    let out = generate(project, language, options)?;
    let mut buf = Vec::new();
    {
        let mut writer = zip::ZipWriter::new(std::io::Cursor::new(&mut buf));
        let opts = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        for (path, content) in out.files {
            writer
                .start_file(&path, opts)
                .map_err(|e| format!("zip start_file: {e}"))?;
            writer
                .write_all(content.as_bytes())
                .map_err(|e| format!("zip write: {e}"))?;
        }
        writer.finish().map_err(|e| format!("zip finish: {e}"))?;
    }
    Ok(buf)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::{AgentSpec, ComponentRef, Topology};

    fn sample() -> Project {
        let mut solver = AgentSpec::new("solver");
        solver.model = "gpt-4o".into();
        let mut critic = AgentSpec::new("critic");
        critic.r#loop = Some(ComponentRef::new("reflexion"));
        Project {
            id: "p".into(),
            name: "My Demo".into(),
            description: "demo".into(),
            agents: vec![solver, critic],
            topology: Topology {
                mode: "coordinated".into(),
                coordinator: Some(ComponentRef::new("debate")),
                agent_order: vec!["solver".into(), "critic".into()],
            },
            datasets: vec![],
            scorer: None,
            rev: 0,
            created_at: 0.0,
            updated_at: 0.0,
        }
    }

    #[test]
    fn slug_is_filesystem_safe() {
        assert_eq!(slug("My Demo!"), "my_demo");
        assert_eq!(slug("   "), "agent_project");
    }

    #[test]
    fn generate_python_has_expected_files() {
        let out = generate(&sample(), "python", &Value::Null).unwrap();
        assert!(out.files.contains_key("my_demo/my_demo/agents.py"));
        assert!(out.files.contains_key("my_demo/pyproject.toml"));
        let agents = &out.files["my_demo/my_demo/agents.py"];
        assert!(agents.contains("Agent(\"solver\""));
        // debate coordinator body is woven in
        assert!(out.files["my_demo/my_demo/coordinator.py"].contains("Reconcile the debate"));
    }

    #[test]
    fn generate_rust_and_ts_and_unknown() {
        let rs = generate(&sample(), "rust", &Value::Null).unwrap();
        assert!(rs.files.contains_key("my_demo/src/main.rs"));
        assert!(rs.files["my_demo/Cargo.toml"].contains("name = \"my_demo\""));
        let ts = generate(&sample(), "typescript", &Value::Null).unwrap();
        assert!(ts.files.contains_key("my_demo/src/agents.ts"));
        assert!(generate(&sample(), "cobol", &Value::Null).is_err());
    }

    #[test]
    fn generate_zip_is_non_empty_and_has_local_header() {
        let out = generate_zip(&sample(), "python", &Value::Null).unwrap();
        assert!(out.len() > 100);
        assert_eq!(&out[0..2], b"PK");
    }
}
