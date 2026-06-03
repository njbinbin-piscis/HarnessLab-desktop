"""CLI entrypoint for paper-reproduction studies.

Examples
--------
List available studies:
    python -m benchmarks.run --list

Smoke-test the harness wiring (uses tiny built-in tasks, requires PISCIS_API_KEY
for a live run; otherwise the engine reports the missing requirement):
    python -m benchmarks.run --study react --limit 2

Run a real reproduction against a downloaded dataset:
    python -m benchmarks.run --study gsm8k --tasks data/gsm8k_test.jsonl \
        --workspace /tmp/ws --out results/gsm8k.json
"""

from __future__ import annotations

import argparse
import asyncio
import json
import statistics
from pathlib import Path

from backend.core.env import load_env

load_env()

from benchmarks.datasets import load_tasks
from benchmarks.runner import run_ablation
from benchmarks.studies import STUDIES, get_study


def _print_table(results, seed_accuracies=None) -> None:
    print(f"\n{'arm':<20}{'tasks':>6}{'accuracy':>10}{'in_tok':>9}{'out_tok':>9}{'lat(s)':>9}")
    print("-" * 63)
    for r in results:
        acc_str = f"{r.accuracy:>10.3f}"
        if seed_accuracies and r.arm in seed_accuracies and len(seed_accuracies[r.arm]) > 1:
            vals = seed_accuracies[r.arm]
            acc_str = f"{statistics.mean(vals):>7.3f}±{statistics.pstdev(vals):.3f}"
        print(
            f"{r.arm:<20}{r.num_tasks:>6}{acc_str}"
            f"{r.total_input_tokens:>9}{r.total_output_tokens:>9}{r.total_latency_s:>9.1f}"
        )
        for k, v in r.pass_hat_k.items():
            print(f"   pass^{k:<3}{'':<11}{v:>10.3f}")


async def _main_async(args: argparse.Namespace) -> int:
    study = get_study(args.study)
    tasks = load_tasks(study.benchmark, args.tasks)
    if args.limit:
        tasks = tasks[: args.limit]

    print(f"Study : {study.paper}")
    print(f"Bench : {study.benchmark}  |  scorer={study.scorer}  trials={study.trials}")
    print(f"Vary  : {study.controlled_variable}")
    print(f"Expect: {study.expected}")
    print(f"Tasks : {len(tasks)}  |  seeds: {args.seeds}")

    # Run the ablation once per seed; aggregate accuracy mean ± std across seeds.
    seed_accuracies: dict[str, list[float]] = {}
    all_runs = []
    last_results = []
    for seed in range(max(1, args.seeds)):
        results = await run_ablation(
            study.benchmark,
            study.arms,
            tasks,
            study.scorer,
            trials=study.trials,
            workspace=args.workspace,
        )
        last_results = results
        for r in results:
            seed_accuracies.setdefault(r.arm, []).append(r.accuracy)
        all_runs.append({"seed": seed, "results": [r.summary() for r in results]})

    _print_table(last_results, seed_accuracies)

    if args.out:
        payload = {
            "study": study.key,
            "paper": study.paper,
            "benchmark": study.benchmark,
            "controlled_variable": study.controlled_variable,
            "expected": study.expected,
            "seeds": args.seeds,
            "accuracy_by_arm": {
                arm: {
                    "mean": statistics.mean(vals),
                    "stdev": statistics.pstdev(vals),
                    "runs": vals,
                }
                for arm, vals in seed_accuracies.items()
            },
            "runs": all_runs,
            "per_task": [
                {
                    "arm": t.arm,
                    "task_id": t.task_id,
                    "success": t.success,
                    "score": t.score,
                    "trials": t.trials,
                    "input_tokens": t.input_tokens,
                    "output_tokens": t.output_tokens,
                    "latency_s": round(t.latency_s, 3),
                    "error": t.error,
                }
                for r in last_results
                for t in r.per_task
            ],
        }
        Path(args.out).parent.mkdir(parents=True, exist_ok=True)
        Path(args.out).write_text(json.dumps(payload, indent=2), encoding="utf-8")
        print(f"\nWrote results to {args.out}")

    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description="Agent Workbench paper-reproduction harness")
    parser.add_argument("--study", help="study key (see --list)")
    parser.add_argument("--tasks", help="path to a JSONL task file (defaults to built-in smoke set)")
    parser.add_argument("--workspace", help="workspace root for tool execution")
    parser.add_argument("--limit", type=int, default=0, help="cap number of tasks")
    parser.add_argument("--seeds", type=int, default=1, help="repeat the ablation N times for mean±std")
    parser.add_argument("--out", help="write JSON results to this path")
    parser.add_argument("--list", action="store_true", help="list available studies and exit")
    args = parser.parse_args()

    if args.list or not args.study:
        print("Available reproduction studies:\n")
        for key, study in STUDIES.items():
            print(f"  {key:<16} {study.paper}")
            print(f"  {'':<16} bench={study.benchmark} scorer={study.scorer} trials={study.trials}")
        return 0

    return asyncio.run(_main_async(args))


if __name__ == "__main__":
    raise SystemExit(main())
