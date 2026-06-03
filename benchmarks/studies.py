"""Registry of reproduction studies.

Each study binds a paper to its arms (config diffs), a benchmark task set, and a
scorer. Running a study end-to-end reproduces the paper's headline ablation
using only configuration — the platform paper's central demonstration.
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import Dict, List

from benchmarks.types import Arm


@dataclass
class Study:
    key: str
    paper: str
    benchmark: str
    scorer: str
    trials: int
    arms: List[Arm]
    controlled_variable: str
    expected: str


REPRO = "configs/reproductions"

STUDIES: Dict[str, Study] = {
    "react": Study(
        key="react",
        paper="ReAct (Yao et al., ICLR 2023)",
        benchmark="hotpotqa",
        scorer="f1_match",
        trials=1,
        controlled_variable="reasoning channel (Thought) on/off + tool access",
        expected="ReAct > Act-only > CoT-only on multi-hop QA (paper Table 1).",
        arms=[
            Arm("react", f"{REPRO}/react_hotpotqa.yaml", "strategy=react+search",
                "Full ReAct: interleaved Thought/Action/Observation."),
            Arm("act_only", f"{REPRO}/react_actonly.yaml", "Thought channel removed",
                "Act-only: actions without reasoning traces."),
            Arm("cot_only", f"{REPRO}/react_cot_only.yaml", "tools removed",
                "CoT-only: reason then answer, no retrieval."),
        ],
    ),
    "plan_and_solve": Study(
        key="plan_and_solve",
        paper="Plan-and-Solve Prompting (Wang et al., ACL 2023)",
        benchmark="gsm8k",
        scorer="numeric_match",
        trials=1,
        controlled_variable="strategy.type (plan_and_solve vs react)",
        expected="Plan-and-Solve reduces missing-step/calc errors vs Zero-shot-CoT.",
        arms=[
            Arm("plan_and_solve", f"{REPRO}/plan_and_solve_gsm8k.yaml",
                "strategy=plan_and_solve", "Plan then execute step by step."),
            Arm("react", f"{REPRO}/react_hotpotqa.yaml",
                "strategy=react", "Direct reasoning control arm (same model)."),
        ],
    ),
    "reflexion": Study(
        key="reflexion",
        paper="Reflexion (Shinn et al., NeurIPS 2023)",
        benchmark="hotpotqa",
        scorer="f1_match",
        trials=5,
        controlled_variable="plugins.memory (InMemoryVectorStore vs NoOpMemory)",
        expected="Success rate rises across reflective retries (success@k curve).",
        arms=[
            Arm("reflexion", f"{REPRO}/reflexion_alfworld.yaml",
                "memory=InMemoryVectorStore", "Episodic verbal reflections on retry."),
            Arm("no_memory", f"{REPRO}/reflexion_noop_alfworld.yaml",
                "memory=NoOpMemory", "Control: no cross-trial reflection store."),
        ],
    ),
    "swebench": Study(
        key="swebench",
        paper="SWE-bench (Jimenez et al., ICLR 2024)",
        benchmark="swebench_verified",
        scorer="fail_to_pass",
        trials=1,
        controlled_variable="strategy.type (plan_and_solve vs react) on full tool set",
        expected="Planning helps multi-file patches (SWE-bench Pro regime).",
        arms=[
            Arm("plan_and_solve", f"{REPRO}/swebench_coding_agent.yaml",
                "strategy=plan_and_solve", "Reproduce->Plan->Edit->Verify loop."),
            Arm("react", f"{REPRO}/swebench_react_arm.yaml",
                "strategy=react", "Control: ReAct without explicit planning."),
        ],
    ),
    "gaia": Study(
        key="gaia",
        paper="GAIA (Mialon et al., ICLR 2024)",
        benchmark="gaia",
        scorer="exact_match",
        trials=1,
        controlled_variable="enabled tool set (reasoning-only vs +search vs +code)",
        expected="Performance collapses without retrieval; Level 1 >> Level 3.",
        arms=[
            Arm("gaia", f"{REPRO}/gaia_assistant.yaml",
                "tools=search+file+code", "Full general-assistant tool set."),
        ],
    ),
    "taubench": Study(
        key="taubench",
        paper="tau-bench / tau2-bench (Yao 2024; Barres 2025)",
        benchmark="taubench",
        scorer="exact_match",
        trials=8,
        controlled_variable="policy strictness + strategy.type",
        expected="pass^k degrades sharply; function-calling > ReAct/Act.",
        arms=[
            Arm("taubench", f"{REPRO}/taubench_policy_agent.yaml",
                "policy-gated tools", "Multi-turn policy-following customer agent."),
        ],
    ),
}


def get_study(key: str) -> Study:
    if key not in STUDIES:
        raise KeyError(f"unknown study '{key}'. Available: {', '.join(STUDIES)}")
    return STUDIES[key]
