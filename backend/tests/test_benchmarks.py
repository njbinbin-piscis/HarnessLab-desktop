"""Unit tests for the reproduction benchmark harness (no LLM/API needed)."""

import pytest

from benchmarks.scorers import exact_match, f1_match, numeric_match, fail_to_pass
from benchmarks.runner import _tools_from_config
from benchmarks.studies import STUDIES, get_study
from benchmarks.types import Task


class TestScorers:
    def test_exact_match_normalizes(self):
        task = Task("t1", "capital of France?", "Paris")
        ok, score = exact_match(task, "FINAL ANSWER: paris")
        assert ok is True and score == 1.0

    def test_exact_match_finish_marker(self):
        task = Task("t2", "q", "ok")
        ok, _ = exact_match(task, "Action: finish[OK]")
        assert ok is True

    def test_f1_partial_credit(self):
        task = Task("t3", "q", "William Shakespeare")
        ok, score = f1_match(task, "FINAL ANSWER: Shakespeare")
        assert score > 0.0
        # one of two tokens overlaps -> F1 = 2*1*0.5/(1.5) ~ 0.667 >= 0.5
        assert ok is True

    def test_numeric_match(self):
        task = Task("t4", "2+2?", "4")
        ok, _ = numeric_match(task, "The answer is 4.")
        assert ok is True
        ok2, _ = numeric_match(task, "The answer is 5.")
        assert ok2 is False

    def test_numeric_handles_commas(self):
        task = Task("t5", "q", "1,000")
        ok, _ = numeric_match(task, "The answer is 1000")
        assert ok is True

    def test_fail_to_pass_reads_metadata(self):
        task = Task("t6", "q", None, metadata={"resolved": True})
        ok, score = fail_to_pass(task, "")
        assert ok is True and score == 1.0


class TestToolWhitelist:
    def test_no_reproduction_section_returns_none(self):
        assert _tools_from_config("agent:\n  name: A\n  model: m\n") is None

    def test_empty_tools_list(self):
        cfg = "agent:\n  name: A\n  model: m\nreproduction:\n  tools: []\n"
        assert _tools_from_config(cfg) == []

    def test_tools_whitelist(self):
        cfg = (
            "agent:\n  name: A\n  model: m\n"
            "reproduction:\n  tools: [web_search, file_read]\n"
        )
        assert _tools_from_config(cfg) == ["web_search", "file_read"]


class TestStudies:
    def test_all_studies_resolve(self):
        for key in STUDIES:
            study = get_study(key)
            assert study.arms, f"study {key} has no arms"

    def test_react_has_three_arms(self):
        study = get_study("react")
        names = {a.name for a in study.arms}
        assert {"react", "act_only", "cot_only"} <= names

    def test_unknown_study_raises(self):
        with pytest.raises(KeyError):
            get_study("does_not_exist")
