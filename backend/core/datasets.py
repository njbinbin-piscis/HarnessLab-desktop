"""Validation-set (dataset) management for the experiment platform.

Brings the offline ``benchmarks/`` dataset + scorer machinery into the live
platform so an experiment can actually *run against a validation set* and be
observed. A dataset is a list of tasks (``{task_id, prompt, answer, metadata}``)
plus a default scorer name (resolved via the component registry / benchmarks
``SCORERS``).

Sources:

- **builtin** — the dependency-free smoke sets from ``benchmarks.datasets`` so
  the platform is usable with zero downloads.
- **file** — user-uploaded JSONL stored under ``data/`` with a ``.meta.json``
  sidecar (name / description / scorer).
"""

from __future__ import annotations

import json
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any, Dict, List, Optional

from benchmarks.datasets import builtin_smoke_set, load_jsonl
from benchmarks.types import Task

# Default data directory (mirrors the benchmarks README's ``data/`` convention).
WORKBENCH_ROOT = Path(__file__).resolve().parents[2]
DEFAULT_DATA_DIR = WORKBENCH_ROOT / "data"

# Built-in smoke datasets exposed through the platform, with their paper scorer.
_BUILTIN_DATASETS = {
    "smoke_hotpotqa": {"benchmark": "hotpotqa", "scorer": "f1_match",
                       "name": "HotpotQA smoke set", "description": "2 multi-hop QA tasks (no download)."},
    "smoke_gsm8k": {"benchmark": "gsm8k", "scorer": "numeric_match",
                    "name": "GSM8K smoke set", "description": "2 grade-school math tasks (no download)."},
    "smoke_gaia": {"benchmark": "gaia", "scorer": "exact_match",
                   "name": "GAIA smoke set", "description": "1 general-assistant task (no download)."},
}


@dataclass
class DatasetSpec:
    """Metadata describing one validation set."""

    id: str
    name: str
    scorer: str
    source: str  # "builtin" | "file"
    num_tasks: int
    description: str = ""
    path: Optional[str] = None
    metadata: Dict[str, Any] = field(default_factory=dict)

    def to_dict(self) -> Dict[str, Any]:
        return {
            "id": self.id,
            "name": self.name,
            "scorer": self.scorer,
            "source": self.source,
            "num_tasks": self.num_tasks,
            "description": self.description,
            "path": self.path,
            "metadata": self.metadata,
        }


def _task_to_dict(task: Task) -> Dict[str, Any]:
    return {
        "task_id": task.task_id,
        "prompt": task.prompt,
        "answer": task.answer,
        "metadata": task.metadata,
    }


class DatasetError(ValueError):
    """Raised on invalid dataset content or unknown dataset id."""


class DatasetRegistry:
    """Catalogue + loader for validation sets (built-in + user JSONL files)."""

    def __init__(self, data_dir: Optional[Path] = None) -> None:
        self.data_dir = Path(data_dir) if data_dir else DEFAULT_DATA_DIR
        self.data_dir.mkdir(parents=True, exist_ok=True)

    # -- discovery ---------------------------------------------------------- #

    def _builtin_spec(self, dataset_id: str) -> DatasetSpec:
        info = _BUILTIN_DATASETS[dataset_id]
        tasks = builtin_smoke_set(info["benchmark"])
        return DatasetSpec(
            id=dataset_id,
            name=info["name"],
            scorer=info["scorer"],
            source="builtin",
            num_tasks=len(tasks),
            description=info["description"],
        )

    def _file_spec(self, jsonl_path: Path) -> DatasetSpec:
        dataset_id = jsonl_path.stem
        meta_path = jsonl_path.with_suffix(".meta.json")
        meta: Dict[str, Any] = {}
        if meta_path.exists():
            try:
                meta = json.loads(meta_path.read_text(encoding="utf-8"))
            except json.JSONDecodeError:
                meta = {}
        try:
            num = sum(1 for line in jsonl_path.read_text(encoding="utf-8").splitlines() if line.strip())
        except OSError:
            num = 0
        return DatasetSpec(
            id=dataset_id,
            name=meta.get("name", dataset_id),
            scorer=meta.get("scorer", "exact_match"),
            source="file",
            num_tasks=num,
            description=meta.get("description", ""),
            path=str(jsonl_path),
            metadata={k: v for k, v in meta.items() if k not in ("name", "scorer", "description")},
        )

    def list(self) -> List[DatasetSpec]:
        specs = [self._builtin_spec(d) for d in _BUILTIN_DATASETS]
        for jsonl_path in sorted(self.data_dir.glob("*.jsonl")):
            specs.append(self._file_spec(jsonl_path))
        return specs

    def get_spec(self, dataset_id: str) -> DatasetSpec:
        if dataset_id in _BUILTIN_DATASETS:
            return self._builtin_spec(dataset_id)
        jsonl_path = self.data_dir / f"{dataset_id}.jsonl"
        if jsonl_path.exists():
            return self._file_spec(jsonl_path)
        raise DatasetError(f"unknown dataset '{dataset_id}'")

    # -- task access -------------------------------------------------------- #

    def load(self, dataset_id: str) -> List[Task]:
        if dataset_id in _BUILTIN_DATASETS:
            return builtin_smoke_set(_BUILTIN_DATASETS[dataset_id]["benchmark"])
        jsonl_path = self.data_dir / f"{dataset_id}.jsonl"
        if not jsonl_path.exists():
            raise DatasetError(f"unknown dataset '{dataset_id}'")
        return load_jsonl(str(jsonl_path))

    def get_tasks(
        self, dataset_id: str, limit: Optional[int] = None, offset: int = 0
    ) -> List[Dict[str, Any]]:
        tasks = self.load(dataset_id)
        window = tasks[offset : offset + limit if limit is not None else None]
        return [_task_to_dict(t) for t in window]

    # -- mutation ----------------------------------------------------------- #

    @staticmethod
    def _validate_jsonl(content: str) -> int:
        """Validate JSONL task content; return the task count or raise."""
        count = 0
        for i, line in enumerate(content.splitlines(), start=1):
            line = line.strip()
            if not line:
                continue
            try:
                row = json.loads(line)
            except json.JSONDecodeError as exc:
                raise DatasetError(f"line {i}: invalid JSON ({exc})")
            if not isinstance(row, dict):
                raise DatasetError(f"line {i}: each row must be a JSON object")
            if "prompt" not in row and "question" not in row:
                raise DatasetError(f"line {i}: missing 'prompt' (or 'question')")
            count += 1
        if count == 0:
            raise DatasetError("dataset is empty")
        return count

    def register_jsonl(
        self,
        dataset_id: str,
        content: str,
        *,
        name: Optional[str] = None,
        scorer: str = "exact_match",
        description: str = "",
        overwrite: bool = False,
    ) -> DatasetSpec:
        """Persist a user-provided JSONL dataset + metadata sidecar."""
        safe_id = "".join(c if c.isalnum() or c in "-_" else "_" for c in dataset_id).strip("_")
        if not safe_id:
            raise DatasetError("dataset id must contain alphanumeric characters")
        if safe_id in _BUILTIN_DATASETS:
            raise DatasetError(f"'{safe_id}' is a reserved built-in dataset id")
        jsonl_path = self.data_dir / f"{safe_id}.jsonl"
        if jsonl_path.exists() and not overwrite:
            raise DatasetError(f"dataset '{safe_id}' already exists (set overwrite=true)")

        self._validate_jsonl(content)
        jsonl_path.write_text(content if content.endswith("\n") else content + "\n", encoding="utf-8")
        meta = {"name": name or safe_id, "scorer": scorer, "description": description}
        jsonl_path.with_suffix(".meta.json").write_text(
            json.dumps(meta, ensure_ascii=False, indent=2), encoding="utf-8"
        )
        return self._file_spec(jsonl_path)

    def delete(self, dataset_id: str) -> bool:
        if dataset_id in _BUILTIN_DATASETS:
            raise DatasetError(f"cannot delete built-in dataset '{dataset_id}'")
        jsonl_path = self.data_dir / f"{dataset_id}.jsonl"
        if not jsonl_path.exists():
            return False
        jsonl_path.unlink(missing_ok=True)
        jsonl_path.with_suffix(".meta.json").unlink(missing_ok=True)
        return True


_global_dataset_registry: Optional[DatasetRegistry] = None


def get_dataset_registry() -> DatasetRegistry:
    global _global_dataset_registry
    if _global_dataset_registry is None:
        _global_dataset_registry = DatasetRegistry()
    return _global_dataset_registry
