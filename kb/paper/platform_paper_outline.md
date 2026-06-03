# Platform Paper Outline

**Working title:** *Agent Workbench: A Configuration-Driven Platform for
Designing, Reproducing, and Ablating LLM-Agent Experiments*

**Target venues:** systems/ML demo + research tracks — NeurIPS Datasets &
Benchmarks, ICLR, EMNLP System Demonstrations, or MLSys. Artifact-evaluation
friendly (reproducibility is the core thesis).

---

## Abstract (draft)

LLM-agent research moves fast, but its empirical claims are hard to reproduce:
each paper ships a bespoke scaffold tangling the reasoning strategy, memory,
tool set, and policy together with the evaluation harness. We present **Agent
Workbench**, a platform that decouples *agent design* from *agent runtime*. An
agent is specified declaratively in YAML — strategy, memory plugin, context
manager, tool set, policy, and hooks — and compiled onto a single shared
execution kernel (`piscis-kernel`). Because every experiment runs through the
identical kernel, an ablation becomes a one-line configuration diff, and
observed performance deltas are attributable to the studied mechanism rather
than to harness drift. We demonstrate the platform by reproducing six landmark
agent designs — ReAct, Plan-and-Solve, Reflexion, SWE-bench coding agents, GAIA
assistants, and tau-bench policy agents — entirely through configuration, and
recovering their headline qualitative findings. We release the platform, the
reproduction configs, and a benchmark harness.

---

## 1. Introduction

- **Problem**: agent papers couple design + harness; reproductions diverge; it
  is unclear whether a reported gain comes from the *mechanism* or the *plumbing*.
- **Observation**: most agent innovations are re-parameterizations along a small
  set of axes — reasoning pattern, memory, tools, policy, context management.
- **Thesis**: if those axes are first-class configuration over a shared kernel,
  then (a) designs become reproducible diffs and (b) ablations become controlled.
- **Contributions**:
  1. A configuration schema + kernel that expresses the design axes of modern
     agents (Section 3).
  2. A 0-code workflow (web UI + meta-agent wizard) for authoring/validating
     agents and streaming live execution (Section 4).
  3. A reproduction harness and six case studies recovering published findings
     via configuration only (Section 5–6).

## 2. Related Work

- Agent scaffolds/frameworks: LangChain, AutoGPT, AutoGen, OpenHands, smolagents.
- Benchmarks: SWE-bench (+Pro), GAIA, tau-bench/tau2, AgentBench, WebArena.
- Reproducibility & evaluation: HAL (Holistic Agent Leaderboard), contamination
  studies (SWE-bench Verified→Pro), pass^k reliability.
- **Gap**: existing frameworks optimize *building* agents; few treat the *design
  space itself* as a controlled, declarative, reproducible experimental surface.

## 3. System Design

- **3.1 Configuration schema** — `agent / plugins / policy / strategy / hooks`
  (cite `src/config.rs`, README). Map each field to a research lever (Table:
  knob → lever; see `kb/research/paper_reproduction_survey.md` §1).
- **3.2 Execution kernel** — `piscis-kernel`: OS/UI-neutral agent loop, LLM
  clients, neutral tool registry, policy gate, layered prompt, compaction.
- **3.3 Compilation** — `build_harness_config()` lowers YAML → `HarnessConfig` →
  `AgentLoop`; context managers/hooks resolved by name; `exec` streams kernel
  `AgentEvent`s as NDJSON (cite `src/loader.rs`, `src/main.rs`).
- **3.4 Web platform** — FastAPI gateway + React UI; SSE event stream;
  meta-agent wizard turns NL requirements into validated configs.

## 4. The 0-Code Workflow

- Author via wizard or YAML; live validation (Python structural + Rust harness
  build); run; watch thought/action/observation/permission events stream over
  SSE; inspect metrics (tokens, tool calls, context usage).

## 5. Reproduction Methodology

- **5.1 Controlled-diff principle** — arms share everything but one knob; same
  kernel, same task set, same scorer.
- **5.2 Scorers** — exact-match (GAIA), token-F1 (HotpotQA), numeric (GSM8K),
  fail-to-pass (SWE-bench), pass^k (tau-bench). Cite `benchmarks/scorers.py`.
- **5.3 Harness** — `benchmarks/` package; studies registry binds paper → arms →
  benchmark → scorer (`benchmarks/studies.py`).
- **5.4 Validity** — because the only inter-arm difference is the config,
  deltas isolate the mechanism. Discuss contamination (use SWE-bench Pro) and
  multi-trial reliability (pass^k).

## 6. Case Studies / Experiments

For each: the config diff, the metric, the expected published finding, and the
reproduced result (table + plot).

1. **ReAct ablation** (HotpotQA F1): ReAct vs Act-only vs CoT-only.
2. **Plan-and-Solve** (GSM8K accuracy): planning vs direct CoT.
3. **Reflexion** (HotpotQA/ALFWorld success@k): memory on/off; success curve.
4. **SWE-bench** (Verified + Pro resolve %): strategy effect on multi-file patch.
5. **GAIA** (validation EM by level): tool-availability ablation.
6. **tau-bench** (pass^k by domain): policy strictness + strategy.

**Headline result table**: paper × {published finding, reproduced?, our number,
config diff size (lines)}. Emphasize median diff is a handful of lines.

## 7. Analysis

- Which knobs matter most per benchmark (sensitivity analysis).
- Cost/latency vs accuracy (the platform logs tokens + tool calls).
- Failure modes surfaced by the shared harness (e.g. coordination in tau2).

## 8. Limitations

- Live LLM nondeterminism; provider drift; we report seeds/temperatures and
  multiple trials. Some benchmarks (browser/multimodal) need tools beyond the
  current neutral registry. Reproductions are *qualitative-finding* level, not
  bit-exact.

## 9. Conclusion

Treating the agent design space as declarative configuration over a shared
kernel makes agent research reproducible and ablations controlled. Agent
Workbench operationalizes this and recovers six landmark findings via config.

---

## Appendix / Artifacts checklist

- [ ] All six reproduction configs (`configs/reproductions/*.yaml`) — DONE, validated.
- [ ] Benchmark harness (`benchmarks/`) — DONE (wiring verified on smoke set).
- [ ] Dataset adapters (JSONL) + download scripts for HotpotQA/GSM8K/GAIA/SWE-bench/tau-bench — TODO.
- [ ] Result JSON + plotting notebook — TODO.
- [ ] Seeds, model versions, provider/base-url, cost per study — TODO (record in results).
- [ ] Repro README: one command per study — TODO.

## Open tasks to make results paper-ready

1. Wire real datasets (JSONL loaders exist; need download/format scripts).
2. Add Act-only / CoT-only arms as concrete configs (templates exist in
   `cot_actonly_baselines.yaml`) so the ReAct ablation runs automatically.
3. Implement the SWE-bench Docker scorer (`fail_to_pass` reads `resolved`; needs
   the official test-execution bridge).
4. Implement the tau-bench user simulator bridge (multi-turn + pass^k already in
   the runner).
5. Run with a real `PISCIS_API_KEY` across ≥3 seeds; record cost/latency.
