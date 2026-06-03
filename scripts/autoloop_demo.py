"""End-to-end live autoloop demo on a smoke validation set.

Runs the real propose -> evaluate (live DeepSeek engine) -> read score -> revise
loop on ``smoke_gsm8k`` and prints the resulting lineage. Credentials come from
the environment (PISCIS_PROVIDER / PISCIS_API_KEY / PISCIS_BASE_URL), e.g. via
the workbench ``.env``.

    python scripts/autoloop_demo.py
"""

from __future__ import annotations

import argparse
import asyncio
import os
import sys
import tempfile
from pathlib import Path
from typing import List

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))

from backend.core.env import load_env  # noqa: E402

load_env()

from backend.core.autoloop import LineageEntry, _force_model, run_autoloop  # noqa: E402
from backend.core.engine import engine_available, missing_requirements  # noqa: E402
from backend.core.store import RunStore  # noqa: E402

MODEL = os.environ.get("PISCIS_DEFAULT_MODEL", "deepseek-v4-flash")

# A deliberately weak seed so round 0 under-performs and the loop must revise,
# producing a multi-round, parent-linked lineage.
_WEAK_SEED = f"""agent:
  name: WeakSeed
  model: {MODEL}
  context_window: 16000
  max_tokens: 64
strategy:
  type: react
  system_prompt_template: "Answer the math question with a single number."
"""


def _weak_proposer():
    """Round 0 = weak seed; revisions = deterministic refinement (parent-linked)."""
    from backend.core.meta_generator import refine_config

    async def proposer(round_idx: int, requirement: str, history: List[LineageEntry]) -> str:
        if round_idx == 0 or not history:
            return _force_model(_WEAK_SEED, MODEL)
        prev = history[-1]
        feedback = (
            f"The previous version scored accuracy {prev.accuracy:.2f}. Reason step by "
            "step, show each calculation, and output ONLY the final integer on the last line."
        )
        refined, _p, _v = refine_config(prev.config_yaml, feedback)
        return _force_model(refined, MODEL)

    return proposer


async def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--mode", choices=["agent", "weak"], default="agent",
        help="agent: Architect designs round 0; weak: weak seed + revise loop",
    )
    parser.add_argument("--rounds", type=int, default=2)
    parser.add_argument("--limit", type=int, default=3)
    args = parser.parse_args()

    print(f"=== Autoloop live demo (smoke_gsm8k, mode={args.mode}) ===")
    print(f"provider={os.environ.get('PISCIS_PROVIDER')} "
          f"model={os.environ.get('PISCIS_DEFAULT_MODEL')} "
          f"base_url={os.environ.get('PISCIS_BASE_URL')}")
    print(f"engine_available={engine_available()}  ({missing_requirements() or 'ok'})")

    workspace = tempfile.mkdtemp(prefix="autoloop_ws_")
    store = RunStore(Path(tempfile.mkdtemp(prefix="autoloop_db_")) / "runs.db")

    queue: asyncio.Queue = asyncio.Queue()

    async def drain() -> None:
        while True:
            event = await queue.get()
            etype = event.get("type")
            if etype == "autoloop_start":
                print(f"\n[start] rounds<= {event['rounds']}  target={event['target_accuracy']}")
            elif etype == "round_proposed":
                print(f"[round {event['round']}] proposed candidate, evaluating...")
            elif etype == "round_evaluated":
                print(f"[round {event['round']}] accuracy={event['accuracy']:.3f} "
                      f"run={str(event['run_id'])[:8]}")
            elif etype == "autoloop_complete":
                print(f"[complete] best_round={event['best_round']} "
                      f"best_accuracy={event['best_accuracy']:.3f} "
                      f"reason={event['stopped_reason']}")
                return

    drainer = asyncio.create_task(drain())

    result = await run_autoloop(
        requirement=(
            "Solve grade-school math word problems. Reason step by step, then on "
            "the final line output only the final integer answer with no extra text."
        ),
        dataset_id="smoke_gsm8k",
        rounds=args.rounds,
        target_accuracy=1.0,
        patience=args.rounds,  # let it run the full budget to show the lineage
        strategy_type="react",
        model=MODEL,
        seeds=1,
        limit=args.limit,
        workspace=workspace,
        out_queue=queue,
        store=store,
        proposer=_weak_proposer() if args.mode == "weak" else None,
    )

    await drainer

    print("\n=== Lineage ===")
    for entry in result.lineage:
        marker = " *BEST*" if entry.round == result.best_round else ""
        print(f"  round={entry.round} source={entry.source} "
              f"accuracy={entry.accuracy:.3f} run={str(entry.run_id)[:8]} "
              f"parent={str(entry.parent_run_id)[:8] if entry.parent_run_id else '-'}{marker}")

    print(f"\nbest config (round {result.best_round}, {result.best_accuracy:.3f}):")
    print("-" * 60)
    print(result.best_config_yaml)
    print("-" * 60)
    print(f"\nPersisted runs DB: {store.db_path}")
    return 0


if __name__ == "__main__":
    raise SystemExit(asyncio.run(main()))
