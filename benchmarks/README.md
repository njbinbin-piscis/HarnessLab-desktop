# Agent Workbench — Paper Reproduction Suite

This package turns published agent designs into controlled, reproducible
experiments on top of the `piscis-kernel`. Each *study* corresponds to a paper;
each *arm* is one YAML config in `configs/reproductions/` that differs from the
others by a single controlled knob (strategy, memory plugin, tool availability,
prompt). The same kernel execution path powers both the live web platform and
these benchmarks, so results transfer directly.

## Studies

| key              | paper                                   | benchmark         | controlled knob                          |
|------------------|-----------------------------------------|-------------------|------------------------------------------|
| `react`          | ReAct (Yao et al., 2023)                | HotpotQA          | reasoning channel + tool access          |
| `plan_and_solve` | Plan-and-Solve (Wang et al., 2023)      | GSM8K             | `strategy.type`                          |
| `reflexion`      | Reflexion (Shinn et al., 2023)          | HotpotQA/ALFWorld | `plugins.memory`                         |
| `swebench`       | SWE-bench (Jimenez et al., 2024)        | SWE-bench Verified| `strategy.type` (plan vs react)          |
| `gaia`           | GAIA (Mialon et al., 2024)              | GAIA              | tool availability by level               |
| `taubench`       | tau-bench / tau2 (Yao 2024; Barres 2025)| tau-bench         | `pass^k` reliability                     |

```bash
python -m benchmarks.run --list          # show all studies and their arms
```

## 1. Prepare datasets

Built-in smoke sets (a handful of tasks) run with no download. For real numbers,
fetch the public datasets into the harness JSONL format
(`{task_id, prompt, answer, metadata}`):

```bash
pip install datasets   # one-time, for the HuggingFace loaders
python -m benchmarks.prepare_datasets --dataset gsm8k    --split test       --limit 200
python -m benchmarks.prepare_datasets --dataset hotpotqa --split validation --limit 500
python -m benchmarks.prepare_datasets --dataset gaia     --split validation
```

Outputs land in `data/<dataset>_<split>.jsonl`.

## 2. Configure the model

The kernel reads LLM credentials from the environment:

```bash
export PISCIS_PROVIDER=qwen          # or openai, anthropic, ...
export PISCIS_API_KEY=sk-...         # required
export PISCIS_BASE_URL=https://...   # optional, openai-compatible endpoints
```

Without a key, every study still runs end-to-end in **dry-run mode** (the wiring,
scoring, and aggregation are exercised; accuracy is reported as 0). This is what
CI uses.

## 3. Run a study

```bash
# Smoke set, single seed:
python -m benchmarks.run --study react --out results/react.json

# Real dataset, 3 seeds (accuracy reported as mean ± std), capped at 100 tasks:
python -m benchmarks.run \
    --study react \
    --tasks data/hotpotqa_validation.jsonl \
    --limit 100 --seeds 3 \
    --out results/react.json
```

The console table reports accuracy, `pass^k`, total input/output tokens, and
wall-clock latency per arm. The JSON artifact additionally stores per-task
results and per-seed accuracy for downstream analysis.

## 4. Plot

```bash
python -m benchmarks.plot results/react.json results/swebench.json --out figs/
```

Renders grouped accuracy bar charts (with std error bars across seeds) to
`figs/<study>.png`. Without matplotlib it prints ASCII bars instead.

## Reproduce everything

```bash
for study in react plan_and_solve reflexion swebench gaia taubench; do
  python -m benchmarks.run --study "$study" --seeds 3 --out "results/${study}.json"
done
python -m benchmarks.plot results/*.json --out figs/
```

## How an arm maps to a paper lever

- **ReAct**: `react_hotpotqa.yaml` (full) vs `react_actonly.yaml` (no Thought)
  vs `react_cot_only.yaml` (`reproduction.tools: []` → kernel runs `--no-tools`).
- **Reflexion**: `reflexion_alfworld.yaml` (`InMemoryVectorStore`) vs
  `reflexion_noop_alfworld.yaml` (`NoOpMemory`).
- **SWE-bench**: `swebench_coding_agent.yaml` (`plan_and_solve`) vs
  `swebench_react_arm.yaml` (`react`), same tool set.

See `kb/research/paper_reproduction_survey.md` for the full knob→lever mapping
and `kb/paper/platform_paper_outline.md` for the platform paper draft.
