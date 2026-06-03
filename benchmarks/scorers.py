"""Scorers used to reproduce paper metrics.

Each scorer takes a Task and the agent's final answer text and returns
(success: bool, score: float in [0, 1]). These intentionally mirror the
metrics reported in the source papers so reproduced numbers are comparable.
"""

from __future__ import annotations

import re
import string
from typing import Tuple

from benchmarks.types import Task


def _normalize(text: str) -> str:
    """SQuAD/HotpotQA-style normalization: lowercase, strip punctuation/articles."""
    text = text.lower()
    text = "".join(ch for ch in text if ch not in set(string.punctuation))
    text = re.sub(r"\b(a|an|the)\b", " ", text)
    return " ".join(text.split())


def _extract_final(text: str) -> str:
    """Pull the agent's final answer from common terminal markers."""
    for pattern in (
        r"FINAL ANSWER:\s*(.+)",
        r"finish\[(.+?)\]",
        r"[Tt]he answer is\s*:?\s*(.+)",
    ):
        match = re.search(pattern, text, re.DOTALL)
        if match:
            return match.group(1).strip().splitlines()[0].strip()
    return text.strip().splitlines()[-1].strip() if text.strip() else ""


def exact_match(task: Task, final_text: str) -> Tuple[bool, float]:
    """GAIA / closed-form QA: normalized exact string match."""
    if task.answer is None:
        return (False, 0.0)
    pred = _normalize(_extract_final(final_text))
    gold = _normalize(task.answer)
    ok = pred == gold
    return (ok, 1.0 if ok else 0.0)


def f1_match(task: Task, final_text: str) -> Tuple[bool, float]:
    """HotpotQA token-level F1 (success = F1 >= 0.5)."""
    if task.answer is None:
        return (False, 0.0)
    pred_tokens = _normalize(_extract_final(final_text)).split()
    gold_tokens = _normalize(task.answer).split()
    if not pred_tokens or not gold_tokens:
        return (False, 0.0)
    common = 0
    gold_pool = list(gold_tokens)
    for tok in pred_tokens:
        if tok in gold_pool:
            common += 1
            gold_pool.remove(tok)
    if common == 0:
        return (False, 0.0)
    precision = common / len(pred_tokens)
    recall = common / len(gold_tokens)
    f1 = 2 * precision * recall / (precision + recall)
    return (f1 >= 0.5, f1)


def numeric_match(task: Task, final_text: str) -> Tuple[bool, float]:
    """GSM8K-style: compare the last number in the answer to the gold number."""
    if task.answer is None:
        return (False, 0.0)

    def last_number(s: str):
        nums = re.findall(r"-?\d[\d,]*\.?\d*", s.replace(",", ""))
        return float(nums[-1]) if nums else None

    pred = last_number(_extract_final(final_text))
    gold = last_number(task.answer)
    if pred is None or gold is None:
        return (False, 0.0)
    ok = abs(pred - gold) < 1e-4
    return (ok, 1.0 if ok else 0.0)


def fail_to_pass(task: Task, final_text: str) -> Tuple[bool, float]:
    """SWE-bench placeholder: success is decided by the external test harness.

    The runner sets task.metadata['resolved'] after executing the official
    SWE-bench evaluation in the task's Docker image. This scorer simply reads it.
    """
    resolved = bool(task.metadata.get("resolved", False))
    return (resolved, 1.0 if resolved else 0.0)


SCORERS = {
    "exact_match": exact_match,
    "f1_match": f1_match,
    "numeric_match": numeric_match,
    "fail_to_pass": fail_to_pass,
}
