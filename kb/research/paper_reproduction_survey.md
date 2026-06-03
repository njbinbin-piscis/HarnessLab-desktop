# Agent Paper Reproduction Survey

This survey maps representative agent papers and their benchmarks onto Agent
Workbench's configuration knobs, establishing that each paper's agent design can
be reproduced as a **configuration diff** rather than bespoke code. This is the
empirical backbone of the platform paper.

## 1. Workbench knobs (the configuration surface)

| Knob (YAML)              | Values                                             | Research lever it exposes          |
|--------------------------|----------------------------------------------------|------------------------------------|
| `strategy.type`          | `react`, `plan_and_solve`                          | reasoning pattern / planning phase |
| `strategy.system_prompt_template` | free text                                 | prompt-method (CoT, ReAct, P&S)    |
| `plugins.memory`         | `NoOpMemory`, `InMemoryVectorStore`, `SqliteMemory`| episodic / long-term memory        |
| `plugins.context`        | `ProjectContextManager`, `WorkspaceRootManager`, `DynamicContextManager` | context injection |
| tool registry (`exec`)   | file_read/write/edit, code_run, shell, web_search  | action space                       |
| `policy.confirm_*`       | bool                                               | policy gating / autonomy           |
| `policy.auto_compact_threshold` + tiers | tokens / %                          | long-horizon context management    |
| `hooks`                  | FileJournal, AuditLog, PlanTracker                 | observability / reflection / plans |

## 2. Reproduction targets

### 2.1 ReAct — Yao et al., ICLR 2023 (arXiv:2210.03629)
- **Idea**: interleave free-form reasoning (Thought) with tool actions (Act) and
  observations.
- **Benchmarks**: HotpotQA, FEVER (knowledge); ALFWorld, WebShop (decision).
- **Workbench mapping**: `strategy=react` + `web_search` tool, `memory=NoOpMemory`.
- **Ablation (config diff only)**: ReAct vs **Act-only** (remove Thought channel)
  vs **CoT-only** (remove tools). See `configs/reproductions/react_hotpotqa.yaml`
  and `cot_actonly_baselines.yaml`.
- **Expected**: ReAct > Act-only > CoT-only on multi-hop QA.

### 2.2 Plan-and-Solve — Wang et al., ACL 2023 (arXiv:2305.04091)
- **Idea**: devise a plan, then execute it step-by-step; reduces missing-step and
  calculation errors over Zero-shot-CoT.
- **Benchmarks**: GSM8K, SVAMP, AQuA, MultiArith.
- **Workbench mapping**: `strategy=plan_and_solve` + `PlanTracker` hook.
- **Ablation**: `plan_and_solve` vs `react` on identical model/temperature.
- **Config**: `configs/reproductions/plan_and_solve_gsm8k.yaml`.

### 2.3 Reflexion — Shinn et al., NeurIPS 2023 (arXiv:2303.11366)
- **Idea**: verbal self-reflection stored in episodic memory; injected on retry
  to improve success across trials (verbal reinforcement learning).
- **Benchmarks**: ALFWorld, HotpotQA, HumanEval.
- **Workbench mapping**: `strategy=react` + `memory=InMemoryVectorStore` +
  reflection journal hook; harness drives `trials_per_task` and feeds prior
  reflections back in.
- **Ablation**: `InMemoryVectorStore` vs `NoOpMemory` → isolates the memory
  mechanism; metric is the success@k curve.
- **Config**: `configs/reproductions/reflexion_alfworld.yaml`.

### 2.4 SWE-bench — Jimenez et al., ICLR 2024 (arXiv:2310.06770)
- **Idea**: resolve real GitHub issues; localize, edit, run tests, iterate.
- **Benchmarks**: SWE-bench Verified (500 Python tasks); **SWE-bench Pro**
  (1,865 multi-language tasks, contamination-resistant — the 2026 standard).
- **2026 status**: Verified is saturated/contaminated (top ~94%); Pro is the
  credible target (top systems ~58–78%, frontier single models ~46–58%).
- **Workbench mapping**: full neutral tool set
  (file_read/search/list/edit/write, code_run, shell); `strategy` ablation.
- **Config**: `configs/reproductions/swebench_coding_agent.yaml`.

### 2.5 GAIA — Mialon et al., ICLR 2024 (arXiv:2311.12983)
- **Idea**: real-world assistant questions, easy for humans (92%) hard for AI;
  need reasoning + browsing + files + code; exact-match answers.
- **2026 status**: top orchestrated systems ~92% (e.g. OPS-Agentic-Search);
  single scaffolds ~70–75% (Claude Sonnet 4.5 ~74.5% on the validation set).
- **Workbench mapping**: `react` + `web_search`/`file_read`/`code_run`; ablate
  tool availability across the 3 difficulty levels.
- **Config**: `configs/reproductions/gaia_assistant.yaml`.

### 2.6 tau-bench / tau2-bench — Yao et al. 2024 (2406.12045); Barres et al. 2025 (2506.07982)
- **Idea**: tool-agent-user interaction; follow domain policy across multi-turn
  conversations with a simulated user; measure **pass^k** reliability.
- **2026 status**: even strong models < 50% on harder domains; pass^8 < 25% in
  retail; native function-calling > ReAct/Act; tau2 adds dual-control (user also
  has tools) and shows large drops from coordination/communication load.
- **Workbench mapping**: domain policy → PolicyGate + `confirm_*`; domain tools →
  user-defined tools; harness drives the user simulator and computes pass^k.
- **Config**: `configs/reproductions/taubench_policy_agent.yaml`.

## 3. Coverage matrix (paper × knob exercised)

| Paper           | strategy | memory | tools | policy | context-mgmt | metric        |
|-----------------|:--------:|:------:|:-----:|:------:|:------------:|---------------|
| ReAct           | ●        | ○      | ●     | ○      | ○            | EM / F1       |
| Plan-and-Solve  | ●        | ○      | ○     | ○      | ○            | acc (numeric) |
| Reflexion       | ●        | ●      | ●     | ○      | ○            | success@k     |
| SWE-bench       | ●        | ●      | ●     | ●      | ●            | resolve %     |
| GAIA            | ●        | ●      | ●     | ○      | ●            | exact-match   |
| tau-bench       | ●        | ○      | ●     | ●      | ○            | pass^k        |

● = primary controlled variable / heavily exercised; ○ = held constant.

Together the six studies exercise **every** configuration knob, demonstrating the
platform's expressiveness across reasoning, memory, tools, policy, and
long-horizon context management.

## 4. Running a study

```bash
# List studies
python -m benchmarks.run --list

# Reproduce (needs PISCIS_API_KEY; point --tasks at the real dataset JSONL)
python -m benchmarks.run --study plan_and_solve \
    --tasks data/gsm8k_test.jsonl --workspace /tmp/ws \
    --out results/plan_and_solve.json
```

Each study runs all arms over the **same** task set through the identical
`piscis-kernel` execution path; only the YAML differs between arms. That
controlled-diff property is what makes the resulting numbers attributable to the
studied mechanism rather than harness differences.

## 5. References

- Yao et al. "ReAct: Synergizing Reasoning and Acting in Language Models." ICLR 2023. arXiv:2210.03629.
- Wang et al. "Plan-and-Solve Prompting." ACL 2023. arXiv:2305.04091.
- Shinn et al. "Reflexion: Language Agents with Verbal Reinforcement Learning." NeurIPS 2023. arXiv:2303.11366.
- Jimenez et al. "SWE-bench: Can Language Models Resolve Real-World GitHub Issues?" ICLR 2024. arXiv:2310.06770.
- Mialon et al. "GAIA: A Benchmark for General AI Assistants." ICLR 2024. arXiv:2311.12983.
- Yao et al. "tau-bench: A Benchmark for Tool-Agent-User Interaction in Real-World Domains." 2024. arXiv:2406.12045.
- Barres et al. "tau2-bench: Evaluating Conversational Agents in a Dual-Control Environment." 2025. arXiv:2506.07982.
- SWE-bench Pro (SEAL leaderboard), 2026 — multi-language, contamination-resistant successor to Verified.
