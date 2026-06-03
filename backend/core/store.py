"""Persistent store for evaluation runs, task results, and trajectories.

The live experiment manager (`manager.py`) keeps run state in memory and drops
it on restart. For the platform to be *observable* — replay a run, compare arms,
drill into a single task's trajectory after the fact — runs must be persisted.

This is a small, dependency-free SQLite store (stdlib ``sqlite3``) with four
tables:

- ``eval_runs``      — one row per (agent config × dataset) evaluation run.
- ``task_results``   — one row per task in a run (success, score, tokens, …).
- ``trajectories``   — ordered events per (run, task) for replay.
- (aggregate metrics live as columns on ``eval_runs``.)

Writes are guarded by a lock so it is safe to call from the asyncio evaluator.
"""

from __future__ import annotations

import json
import sqlite3
import threading
import time
import uuid
from dataclasses import asdict, dataclass, field
from pathlib import Path
from typing import Any, Dict, List, Optional

WORKBENCH_ROOT = Path(__file__).resolve().parents[2]
DEFAULT_DB_PATH = WORKBENCH_ROOT / ".pisci" / "eval_runs.db"

_SCHEMA = """
CREATE TABLE IF NOT EXISTS eval_runs (
    id TEXT PRIMARY KEY,
    created_at REAL NOT NULL,
    updated_at REAL NOT NULL,
    status TEXT NOT NULL,
    label TEXT,
    agent_name TEXT,
    config_yaml TEXT,
    dataset_id TEXT,
    scorer TEXT,
    seeds INTEGER DEFAULT 1,
    num_tasks INTEGER DEFAULT 0,
    completed_tasks INTEGER DEFAULT 0,
    accuracy REAL DEFAULT 0.0,
    total_input_tokens INTEGER DEFAULT 0,
    total_output_tokens INTEGER DEFAULT 0,
    total_latency_s REAL DEFAULT 0.0,
    error TEXT,
    lineage TEXT,
    metadata TEXT
);

CREATE TABLE IF NOT EXISTS task_results (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    run_id TEXT NOT NULL,
    task_id TEXT NOT NULL,
    success INTEGER DEFAULT 0,
    score REAL DEFAULT 0.0,
    trials INTEGER DEFAULT 1,
    events INTEGER DEFAULT 0,
    input_tokens INTEGER DEFAULT 0,
    output_tokens INTEGER DEFAULT 0,
    latency_s REAL DEFAULT 0.0,
    final_text TEXT,
    error TEXT,
    FOREIGN KEY (run_id) REFERENCES eval_runs(id)
);

CREATE TABLE IF NOT EXISTS trajectories (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    run_id TEXT NOT NULL,
    task_id TEXT NOT NULL,
    seq INTEGER NOT NULL,
    type TEXT,
    content TEXT,
    metadata TEXT,
    FOREIGN KEY (run_id) REFERENCES eval_runs(id)
);

CREATE INDEX IF NOT EXISTS idx_task_results_run ON task_results(run_id);
CREATE INDEX IF NOT EXISTS idx_trajectories_run_task ON trajectories(run_id, task_id, seq);
"""


@dataclass
class TaskRecord:
    """A persisted per-task evaluation result."""

    task_id: str
    success: bool = False
    score: float = 0.0
    trials: int = 1
    events: int = 0
    input_tokens: int = 0
    output_tokens: int = 0
    latency_s: float = 0.0
    final_text: str = ""
    error: Optional[str] = None


class RunStore:
    """SQLite-backed persistence for evaluation runs."""

    def __init__(self, db_path: Optional[Path] = None) -> None:
        self.db_path = Path(db_path) if db_path else DEFAULT_DB_PATH
        self.db_path.parent.mkdir(parents=True, exist_ok=True)
        self._lock = threading.Lock()
        self._conn = sqlite3.connect(str(self.db_path), check_same_thread=False)
        self._conn.row_factory = sqlite3.Row
        with self._lock:
            self._conn.executescript(_SCHEMA)
            self._conn.commit()

    # -- run lifecycle ------------------------------------------------------ #

    def create_run(
        self,
        *,
        agent_name: str,
        config_yaml: str,
        dataset_id: str,
        scorer: str,
        seeds: int = 1,
        num_tasks: int = 0,
        label: Optional[str] = None,
        lineage: Optional[Dict[str, Any]] = None,
        metadata: Optional[Dict[str, Any]] = None,
    ) -> str:
        run_id = f"run_{uuid.uuid4().hex[:10]}"
        now = time.time()
        with self._lock:
            self._conn.execute(
                """INSERT INTO eval_runs
                   (id, created_at, updated_at, status, label, agent_name, config_yaml,
                    dataset_id, scorer, seeds, num_tasks, lineage, metadata)
                   VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?)""",
                (
                    run_id, now, now, "pending", label, agent_name, config_yaml,
                    dataset_id, scorer, seeds, num_tasks,
                    json.dumps(lineage) if lineage else None,
                    json.dumps(metadata) if metadata else None,
                ),
            )
            self._conn.commit()
        return run_id

    def set_status(self, run_id: str, status: str, error: Optional[str] = None) -> None:
        with self._lock:
            self._conn.execute(
                "UPDATE eval_runs SET status=?, error=?, updated_at=? WHERE id=?",
                (status, error, time.time(), run_id),
            )
            self._conn.commit()

    def add_task_result(self, run_id: str, rec: TaskRecord) -> None:
        with self._lock:
            self._conn.execute(
                """INSERT INTO task_results
                   (run_id, task_id, success, score, trials, events,
                    input_tokens, output_tokens, latency_s, final_text, error)
                   VALUES (?,?,?,?,?,?,?,?,?,?,?)""",
                (
                    run_id, rec.task_id, int(rec.success), rec.score, rec.trials, rec.events,
                    rec.input_tokens, rec.output_tokens, rec.latency_s, rec.final_text, rec.error,
                ),
            )
            self._conn.execute(
                "UPDATE eval_runs SET completed_tasks = completed_tasks + 1, updated_at=? WHERE id=?",
                (time.time(), run_id),
            )
            self._conn.commit()

    def add_trajectory(
        self, run_id: str, task_id: str, events: List[Dict[str, Any]]
    ) -> None:
        if not events:
            return
        rows = [
            (
                run_id, task_id, seq,
                ev.get("type", ""),
                ev.get("content") or ev.get("delta") or ev.get("message") or "",
                json.dumps({k: v for k, v in ev.items() if k not in ("type", "content", "delta", "message")}),
            )
            for seq, ev in enumerate(events)
        ]
        with self._lock:
            self._conn.executemany(
                "INSERT INTO trajectories (run_id, task_id, seq, type, content, metadata) VALUES (?,?,?,?,?,?)",
                rows,
            )
            self._conn.commit()

    def finalize_run(
        self,
        run_id: str,
        *,
        accuracy: float,
        total_input_tokens: int,
        total_output_tokens: int,
        total_latency_s: float,
        status: str = "completed",
        error: Optional[str] = None,
    ) -> None:
        with self._lock:
            self._conn.execute(
                """UPDATE eval_runs SET status=?, accuracy=?, total_input_tokens=?,
                   total_output_tokens=?, total_latency_s=?, error=?, updated_at=? WHERE id=?""",
                (status, accuracy, total_input_tokens, total_output_tokens,
                 total_latency_s, error, time.time(), run_id),
            )
            self._conn.commit()

    # -- queries ------------------------------------------------------------ #

    def get_run(self, run_id: str) -> Optional[Dict[str, Any]]:
        with self._lock:
            row = self._conn.execute("SELECT * FROM eval_runs WHERE id=?", (run_id,)).fetchone()
        return _run_row_to_dict(row) if row else None

    def list_runs(self, limit: int = 100) -> List[Dict[str, Any]]:
        with self._lock:
            rows = self._conn.execute(
                "SELECT * FROM eval_runs ORDER BY created_at DESC LIMIT ?", (limit,)
            ).fetchall()
        return [_run_row_to_dict(r) for r in rows]

    def get_task_results(self, run_id: str) -> List[Dict[str, Any]]:
        with self._lock:
            rows = self._conn.execute(
                "SELECT * FROM task_results WHERE run_id=? ORDER BY id", (run_id,)
            ).fetchall()
        return [dict(r) for r in rows]

    def get_trajectory(self, run_id: str, task_id: str) -> List[Dict[str, Any]]:
        with self._lock:
            rows = self._conn.execute(
                "SELECT seq, type, content, metadata FROM trajectories WHERE run_id=? AND task_id=? ORDER BY seq",
                (run_id, task_id),
            ).fetchall()
        out = []
        for r in rows:
            d = dict(r)
            if d.get("metadata"):
                try:
                    d["metadata"] = json.loads(d["metadata"])
                except json.JSONDecodeError:
                    pass
            out.append(d)
        return out

    def compare(self, run_ids: List[str]) -> List[Dict[str, Any]]:
        """Side-by-side aggregate metrics for a set of runs."""
        out = []
        for rid in run_ids:
            run = self.get_run(rid)
            if run:
                out.append({
                    "id": run["id"],
                    "label": run["label"],
                    "agent_name": run["agent_name"],
                    "dataset_id": run["dataset_id"],
                    "scorer": run["scorer"],
                    "status": run["status"],
                    "num_tasks": run["num_tasks"],
                    "accuracy": run["accuracy"],
                    "total_input_tokens": run["total_input_tokens"],
                    "total_output_tokens": run["total_output_tokens"],
                    "total_latency_s": run["total_latency_s"],
                })
        return out

    def close(self) -> None:
        with self._lock:
            self._conn.close()


def _run_row_to_dict(row: sqlite3.Row) -> Dict[str, Any]:
    d = dict(row)
    for key in ("lineage", "metadata"):
        if d.get(key):
            try:
                d[key] = json.loads(d[key])
            except (json.JSONDecodeError, TypeError):
                pass
    return d


_global_store: Optional[RunStore] = None


def get_run_store() -> RunStore:
    global _global_store
    if _global_store is None:
        _global_store = RunStore()
    return _global_store
