"""Self-bootstrapping design loop (Phase 5, Pillar 2 + 3 closed loop).

Ties the design assistant to the validation layer so the platform can *improve a
design automatically*:

    propose → evaluate on validation set → read score → revise → repeat

Each round persists an evaluation run (with lineage linking it to the round and
its parent), and the loop tracks the best-scoring candidate. It stops when a
target accuracy is reached, when there is no improvement for ``patience`` rounds,
or when the round budget is exhausted.

The proposer and evaluator are injected so the whole loop is testable without an
engine: the default proposer uses the Architect (agent) for round 0 and the
deterministic refiner for revisions; the default evaluator is
:func:`backend.core.evaluator.evaluate`.
"""

from __future__ import annotations

import asyncio
from dataclasses import dataclass, field
from typing import Any, Awaitable, Callable, Dict, List, Optional

from backend.core.datasets import DatasetRegistry, get_dataset_registry
from backend.core.store import RunStore, get_run_store
from backend.models.schemas import MetaAgentGenerateRequest

# proposer(round_idx, requirement, history) -> config_yaml
Proposer = Callable[[int, str, List["LineageEntry"]], Awaitable[str]]
# evaluator(config_yaml, lineage) -> summary dict with run_id + accuracy
EvaluatorFn = Callable[..., Awaitable[Dict[str, Any]]]


@dataclass
class LineageEntry:
    round: int
    config_yaml: str
    source: str
    run_id: Optional[str] = None
    accuracy: float = 0.0
    parent_run_id: Optional[str] = None

    def to_dict(self) -> Dict[str, Any]:
        return {
            "round": self.round,
            "source": self.source,
            "run_id": self.run_id,
            "accuracy": self.accuracy,
            "parent_run_id": self.parent_run_id,
            "config_yaml": self.config_yaml,
        }


@dataclass
class AutoLoopResult:
    requirement: str
    dataset_id: str
    lineage: List[LineageEntry] = field(default_factory=list)
    best_round: Optional[int] = None
    best_accuracy: float = 0.0
    best_run_id: Optional[str] = None
    best_config_yaml: Optional[str] = None
    rounds_run: int = 0
    stopped_reason: str = ""

    def to_dict(self) -> Dict[str, Any]:
        return {
            "requirement": self.requirement,
            "dataset_id": self.dataset_id,
            "rounds_run": self.rounds_run,
            "best_round": self.best_round,
            "best_accuracy": self.best_accuracy,
            "best_run_id": self.best_run_id,
            "best_config_yaml": self.best_config_yaml,
            "stopped_reason": self.stopped_reason,
            "lineage": [e.to_dict() for e in self.lineage],
        }


def _force_model(config_yaml: str, model: Optional[str]) -> str:
    """Pin ``agent.model`` in a candidate config so it runs on an available model.

    The Architect (or template) may emit any model name; when the loop is bound
    to a specific provider/model we override it so evaluation actually runs.
    """
    if not model:
        return config_yaml
    import yaml as _yaml

    try:
        cfg = _yaml.safe_load(config_yaml) or {}
    except _yaml.YAMLError:
        return config_yaml
    if not isinstance(cfg, dict):
        return config_yaml
    agent = cfg.setdefault("agent", {})
    if isinstance(agent, dict):
        agent["model"] = model
        agent.pop("fallback_models", None)
    return _yaml.dump(cfg, default_flow_style=False, allow_unicode=True)


def _default_proposer(
    *,
    strategy_type: str,
    model: Optional[str] = None,
    engine_check: Optional[Callable[[], bool]] = None,
) -> Proposer:
    """Architect (agent) for round 0, deterministic refinement for revisions."""
    from backend.core.design import generate_design
    from backend.core.meta_generator import refine_config

    async def proposer(round_idx: int, requirement: str, history: List[LineageEntry]) -> str:
        if round_idx == 0 or not history:
            req = MetaAgentGenerateRequest(
                goal_description=requirement,
                strategy_type=strategy_type,
                model_preference=model or "claude-sonnet-4.5",
            )
            result = await generate_design(req, engine_check=engine_check)
            return _force_model(result["config_yaml"], model)
        prev = history[-1]
        feedback = (
            f"The previous version scored accuracy {prev.accuracy:.2f} on the validation set. "
            "Tighten the system prompt and tool/strategy choices to raise accuracy."
        )
        refined_yaml, _prompt, _validation = refine_config(prev.config_yaml, feedback)
        return _force_model(refined_yaml, model)

    return proposer


async def run_autoloop(
    *,
    requirement: str,
    dataset_id: str,
    rounds: int = 3,
    target_accuracy: float = 1.0,
    patience: int = 2,
    strategy_type: str = "react",
    model: Optional[str] = None,
    scorer_name: Optional[str] = None,
    seeds: int = 1,
    limit: Optional[int] = None,
    workspace: Optional[str] = None,
    proposer: Optional[Proposer] = None,
    evaluator_fn: Optional[EvaluatorFn] = None,
    engine_check: Optional[Callable[[], bool]] = None,
    out_queue: Optional[asyncio.Queue] = None,
    store: Optional[RunStore] = None,
    datasets: Optional[DatasetRegistry] = None,
) -> AutoLoopResult:
    """Run the propose→evaluate→revise loop and return the lineage + best design."""
    store = store or get_run_store()
    datasets = datasets or get_dataset_registry()
    proposer = proposer or _default_proposer(
        strategy_type=strategy_type, model=model, engine_check=engine_check
    )
    if evaluator_fn is None:
        from backend.core.evaluator import evaluate as evaluator_fn  # type: ignore

    result = AutoLoopResult(requirement=requirement, dataset_id=dataset_id)

    async def emit(event: Dict[str, Any]) -> None:
        if out_queue is not None:
            await out_queue.put(event)

    await emit({"type": "autoloop_start", "requirement": requirement, "dataset_id": dataset_id,
                "rounds": rounds, "target_accuracy": target_accuracy})

    no_improve = 0
    for round_idx in range(rounds):
        result.rounds_run = round_idx + 1
        parent_run_id = result.lineage[-1].run_id if result.lineage else None

        config_yaml = await proposer(round_idx, requirement, result.lineage)
        entry = LineageEntry(
            round=round_idx, config_yaml=config_yaml,
            source="agent" if round_idx == 0 else "refine",
            parent_run_id=parent_run_id,
        )
        await emit({"type": "round_proposed", "round": round_idx})

        summary = await evaluator_fn(
            config_yaml=config_yaml,
            dataset_id=dataset_id,
            agent_name=f"autoloop-r{round_idx}",
            scorer_name=scorer_name,
            seeds=seeds,
            limit=limit,
            workspace=workspace,
            label=f"autoloop r{round_idx}",
            lineage={"round": round_idx, "parent_run_id": parent_run_id, "requirement": requirement},
            store=store,
            datasets=datasets,
        )
        entry.run_id = summary.get("run_id")
        entry.accuracy = float(summary.get("accuracy", 0.0))
        result.lineage.append(entry)

        await emit({"type": "round_evaluated", "round": round_idx,
                    "run_id": entry.run_id, "accuracy": entry.accuracy})

        if entry.accuracy > result.best_accuracy:
            result.best_accuracy = entry.accuracy
            result.best_round = round_idx
            result.best_run_id = entry.run_id
            result.best_config_yaml = config_yaml
            no_improve = 0
        else:
            no_improve += 1

        if entry.accuracy >= target_accuracy:
            result.stopped_reason = "target_reached"
            break
        if no_improve >= patience:
            result.stopped_reason = "no_improvement"
            break
    else:
        result.stopped_reason = "rounds_exhausted"

    await emit({"type": "autoloop_complete", "best_round": result.best_round,
                "best_accuracy": result.best_accuracy, "best_run_id": result.best_run_id,
                "stopped_reason": result.stopped_reason})
    return result
