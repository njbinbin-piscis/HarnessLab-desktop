"""Download and normalize benchmark datasets into the harness JSONL format.

Each produced JSONL line is: {"task_id", "prompt", "answer", "metadata"}.
Datasets are fetched from their public HuggingFace mirrors via the `datasets`
library when available; otherwise the script prints the manual download URL.

Usage:
    python -m benchmarks.prepare_datasets --dataset gsm8k    --split test  --limit 200
    python -m benchmarks.prepare_datasets --dataset hotpotqa --split validation --limit 500
    python -m benchmarks.prepare_datasets --dataset gaia     --split validation

Output goes to data/<dataset>_<split>.jsonl by default.
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path
from typing import Dict, Iterable, List

DATA_DIR = Path(__file__).resolve().parents[1] / "data"

# HuggingFace dataset coordinates + the manual URL fallback.
SOURCES: Dict[str, Dict[str, str]] = {
    "gsm8k": {
        "hf_path": "openai/gsm8k",
        "hf_config": "main",
        "url": "https://huggingface.co/datasets/openai/gsm8k",
    },
    "hotpotqa": {
        "hf_path": "hotpotqa/hotpot_qa",
        "hf_config": "distractor",
        "url": "https://huggingface.co/datasets/hotpotqa/hotpot_qa",
    },
    "gaia": {
        "hf_path": "gaia-benchmark/GAIA",
        "hf_config": "2023_all",
        "url": "https://huggingface.co/datasets/gaia-benchmark/GAIA",
    },
}


def _gsm8k_rows(ds) -> Iterable[dict]:
    for i, row in enumerate(ds):
        # GSM8K answers end with "#### <number>".
        raw = row["answer"]
        gold = raw.split("####")[-1].strip() if "####" in raw else raw.strip()
        yield {
            "task_id": f"gsm8k-{i}",
            "prompt": row["question"].strip() + "\nThe answer is a number.",
            "answer": gold,
            "metadata": {"source": "gsm8k"},
        }


def _hotpotqa_rows(ds) -> Iterable[dict]:
    for i, row in enumerate(ds):
        yield {
            "task_id": str(row.get("id", f"hotpotqa-{i}")),
            "prompt": row["question"].strip(),
            "answer": row["answer"].strip(),
            "metadata": {"level": row.get("level"), "type": row.get("type")},
        }


def _gaia_rows(ds) -> Iterable[dict]:
    for i, row in enumerate(ds):
        yield {
            "task_id": str(row.get("task_id", f"gaia-{i}")),
            "prompt": row["Question"].strip(),
            "answer": (row.get("Final answer") or "").strip(),
            "metadata": {"level": row.get("Level"), "file_name": row.get("file_name", "")},
        }


ROW_ADAPTERS = {
    "gsm8k": _gsm8k_rows,
    "hotpotqa": _hotpotqa_rows,
    "gaia": _gaia_rows,
}


def prepare(dataset: str, split: str, limit: int, out: Path) -> int:
    if dataset not in SOURCES:
        print(f"unknown dataset '{dataset}'. Known: {', '.join(SOURCES)}", file=sys.stderr)
        return 2

    try:
        from datasets import load_dataset  # type: ignore
    except ImportError:
        src = SOURCES[dataset]
        print(
            "The `datasets` package is not installed. Install it with:\n"
            "    pip install datasets\n"
            f"Or download {dataset} manually from: {src['url']}\n"
            "and format each line as {\"task_id\",\"prompt\",\"answer\",\"metadata\"}.",
            file=sys.stderr,
        )
        return 1

    src = SOURCES[dataset]
    ds = load_dataset(src["hf_path"], src.get("hf_config"), split=split)
    rows = ROW_ADAPTERS[dataset](ds)

    out.parent.mkdir(parents=True, exist_ok=True)
    written = 0
    with open(out, "w", encoding="utf-8") as handle:
        for row in rows:
            if limit and written >= limit:
                break
            handle.write(json.dumps(row, ensure_ascii=False) + "\n")
            written += 1

    print(f"Wrote {written} {dataset}/{split} tasks to {out}")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description="Prepare reproduction datasets")
    parser.add_argument("--dataset", required=True, choices=sorted(SOURCES))
    parser.add_argument("--split", default="test")
    parser.add_argument("--limit", type=int, default=0, help="cap rows (0 = all)")
    parser.add_argument("--out", help="output JSONL path")
    args = parser.parse_args()

    out = Path(args.out) if args.out else DATA_DIR / f"{args.dataset}_{args.split}.jsonl"
    return prepare(args.dataset, args.split, args.limit, out)


if __name__ == "__main__":
    raise SystemExit(main())
