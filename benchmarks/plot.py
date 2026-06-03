"""Render figures from one or more benchmark result JSON files.

Produces a grouped bar chart of accuracy-per-arm (with std error bars when a run
used multiple seeds). Falls back to an ASCII bar chart if matplotlib is absent,
so the script is useful even in a headless minimal environment.

Usage:
    python -m benchmarks.plot results/react.json results/swebench.json --out figs/
"""

from __future__ import annotations

import argparse
import json
from pathlib import Path
from typing import Dict, List, Tuple


def _load(path: Path) -> Tuple[str, List[str], List[float], List[float]]:
    data = json.loads(path.read_text(encoding="utf-8"))
    title = f"{data.get('study', path.stem)} — {data.get('benchmark', '')}"
    acc = data.get("accuracy_by_arm")
    if acc:
        arms = list(acc.keys())
        means = [acc[a]["mean"] for a in arms]
        stdevs = [acc[a].get("stdev", 0.0) for a in arms]
    else:  # older single-run schema
        results = data.get("results", [])
        arms = [r["arm"] for r in results]
        means = [r["accuracy"] for r in results]
        stdevs = [0.0] * len(arms)
    return title, arms, means, stdevs


def _ascii_chart(title: str, arms: List[str], means: List[float]) -> str:
    lines = [f"\n{title}"]
    width = 40
    for arm, mean in zip(arms, means):
        filled = int(round(mean * width))
        bar = "#" * filled + "." * (width - filled)
        lines.append(f"  {arm:<18} |{bar}| {mean:.3f}")
    return "\n".join(lines)


def main() -> int:
    parser = argparse.ArgumentParser(description="Plot benchmark results")
    parser.add_argument("inputs", nargs="+", help="result JSON file(s)")
    parser.add_argument("--out", default="figs", help="output directory for figures")
    args = parser.parse_args()

    loaded = [_load(Path(p)) for p in args.inputs]

    try:
        import matplotlib

        matplotlib.use("Agg")
        import matplotlib.pyplot as plt  # type: ignore
    except ImportError:
        for title, arms, means, _ in loaded:
            print(_ascii_chart(title, arms, means))
        print("\n(matplotlib not installed; printed ASCII charts. `pip install matplotlib` for PNGs.)")
        return 0

    out_dir = Path(args.out)
    out_dir.mkdir(parents=True, exist_ok=True)
    for title, arms, means, stdevs in loaded:
        fig, ax = plt.subplots(figsize=(max(4, len(arms) * 1.4), 4))
        ax.bar(arms, means, yerr=stdevs, capsize=4, color="#3b7dd8")
        ax.set_ylim(0, 1.0)
        ax.set_ylabel("accuracy")
        ax.set_title(title)
        for i, m in enumerate(means):
            ax.text(i, m + 0.02, f"{m:.2f}", ha="center", fontsize=9)
        fig.tight_layout()
        slug = title.split(" ")[0].replace("/", "_")
        path = out_dir / f"{slug}.png"
        fig.savefig(path, dpi=150)
        plt.close(fig)
        print(f"Wrote {path}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
