"""Tests for the Layer-3 multi-agent swarm coordination."""

import asyncio
import textwrap

import pytest

from backend.core.swarm import (
    AuctionCoordinator,
    BlackboardCoordinator,
    CoordinationContext,
    MajorityVoteCoordinator,
    MessageBus,
    builtin_coordinators,
    extract_bid,
    extract_final,
    load_coordinator,
)
from backend.core.manager import MultiAgentSession, SwarmOrchestrator


# --------------------------------------------------------------------------- #
# Marker parsing
# --------------------------------------------------------------------------- #


def test_extract_final_and_bid():
    assert extract_final("blah\nFINAL: 42") == "42"
    assert extract_final("no marker here") is None
    assert extract_bid("I am confident BID: 0.8 yes") == 0.8
    assert extract_bid("no bid") is None


# --------------------------------------------------------------------------- #
# Coordinators (pure)
# --------------------------------------------------------------------------- #


def test_majority_vote_dispatch_then_tally():
    coord = MajorityVoteCoordinator()
    agents = ["a", "b", "c"]

    # First round: nobody has spoken -> dispatch first agent.
    ctx = CoordinationContext(task="2+2?", agents=agents, transcript=[])
    d = coord.coordinate(ctx)
    assert d.next_agent == "a" and not d.done

    # All voted, majority wins (case/space-insensitive).
    transcript = [
        {"agent": "a", "text": "FINAL: 4"},
        {"agent": "b", "text": "FINAL:  4"},
        {"agent": "c", "text": "FINAL: five"},
    ]
    ctx = CoordinationContext(task="2+2?", agents=agents, transcript=transcript)
    d = coord.coordinate(ctx)
    assert d.done and d.final == "4"


def test_auction_two_phases():
    coord = AuctionCoordinator()
    agents = ["a", "b"]

    # Phase 1: collect bids.
    ctx = CoordinationContext(task="t", agents=agents, transcript=[])
    assert coord.coordinate(ctx).next_agent == "a"

    # Bids in; highest bidder asked to solve.
    transcript = [
        {"agent": "a", "text": "BID: 0.3"},
        {"agent": "b", "text": "BID: 0.9"},
    ]
    ctx = CoordinationContext(task="t", agents=agents, transcript=transcript)
    d = coord.coordinate(ctx)
    assert d.next_agent == "b" and not d.done

    # Winner produced a final answer -> done.
    transcript.append({"agent": "b", "text": "FINAL: done"})
    ctx = CoordinationContext(task="t", agents=agents, transcript=transcript)
    d = coord.coordinate(ctx)
    assert d.done and d.final == "done" and d.metadata["winner"] == "b"


def test_blackboard_round_robin_and_final():
    coord = BlackboardCoordinator()
    agents = ["a", "b"]

    # Empty board: pick least-contributing (first).
    ctx = CoordinationContext(task="t", agents=agents, transcript=[])
    assert coord.coordinate(ctx).next_agent == "a"

    # a contributed; next should be b.
    ctx = CoordinationContext(
        task="t", agents=agents, transcript=[{"agent": "a", "text": "idea"}]
    )
    assert coord.coordinate(ctx).next_agent == "b"

    # Final posted -> done.
    ctx = CoordinationContext(
        task="t",
        agents=agents,
        transcript=[{"agent": "a", "text": "idea"}, {"agent": "b", "text": "FINAL: x"}],
    )
    d = coord.coordinate(ctx)
    assert d.done and d.final == "x"


def test_blackboard_max_rounds_stop():
    coord = BlackboardCoordinator()
    ctx = CoordinationContext(
        task="t",
        agents=["a"],
        transcript=[{"agent": "a", "text": "partial"}],
        round=8,
        max_rounds=8,
    )
    d = coord.coordinate(ctx)
    assert d.done and d.final == "partial"


# --------------------------------------------------------------------------- #
# Coordinator loading (built-in, dynamic, error)
# --------------------------------------------------------------------------- #


def test_load_builtins_and_unknown():
    assert set(builtin_coordinators()) == {"majority_vote", "auction", "blackboard"}
    assert load_coordinator("majority_vote").name == "majority_vote"
    with pytest.raises(ValueError):
        load_coordinator("does_not_exist")


def test_load_user_module_from_file(tmp_path):
    mod = tmp_path / "my_coord.py"
    mod.write_text(
        textwrap.dedent(
            """
            class MyCoordinator:
                name = "custom"
                def coordinate(self, ctx):
                    from backend.core.swarm import Decision
                    return Decision(done=True, final="custom-answer")
            """
        )
    )
    coord = load_coordinator(f"{mod}:MyCoordinator")
    assert coord.name == "custom"
    d = coord.coordinate(CoordinationContext(task="t", agents=["a"]))
    assert d.done and d.final == "custom-answer"


def test_load_user_module_rejects_non_coordinator(tmp_path):
    mod = tmp_path / "bad.py"
    mod.write_text("class NotACoordinator:\n    pass\n")
    with pytest.raises(TypeError):
        load_coordinator(f"{mod}:NotACoordinator")


# --------------------------------------------------------------------------- #
# Message bus
# --------------------------------------------------------------------------- #


def test_message_bus_post_history_mentions():
    bus = MessageBus()
    bus.post("a", "hello @b please review")
    bus.post("b", "ack")
    hist = bus.history()
    assert [m["agent"] for m in hist] == ["a", "b"]
    assert bus.mentions("ping @b and @c", ["a", "b", "c"]) == ["b", "c"]


# --------------------------------------------------------------------------- #
# Orchestrator end-to-end with fake sessions
# --------------------------------------------------------------------------- #


class FakeSession:
    """Minimal InteractiveSession stand-in scripted with per-prompt answers."""

    def __init__(self, answers):
        self._answers = list(answers)
        self._idx = 0
        self._q: asyncio.Queue = asyncio.Queue()

    async def start(self):
        await self._q.put({"type": "session_ready"})

    async def events(self):
        while True:
            ev = await self._q.get()
            if ev is None:
                return
            yield ev

    async def send_prompt(self, text):
        answer = self._answers[self._idx] if self._idx < len(self._answers) else "FINAL: none"
        self._idx += 1
        await self._q.put({"type": "turn_start"})
        await self._q.put({"type": "text_delta", "delta": answer})
        await self._q.put(
            {"type": "turn_complete", "total_input_tokens": 1, "total_output_tokens": 1}
        )

    async def inject(self, text):
        pass

    async def close(self):
        await self._q.put(None)


@pytest.mark.asyncio
async def test_swarm_orchestrator_majority_vote():
    sessions = {
        "a": FakeSession(["FINAL: 42"]),
        "b": FakeSession(["FINAL: 42"]),
        "c": FakeSession(["FINAL: 7"]),
    }
    multi = MultiAgentSession(sessions)
    bus = MessageBus()
    out_queue: asyncio.Queue = asyncio.Queue()
    orch = SwarmOrchestrator(
        multi, MajorityVoteCoordinator(), bus, task="what is the answer?",
        max_rounds=8, out_queue=out_queue,
    )
    await multi.start_all()
    result = await orch.run()
    await multi.close_all()

    assert result["final"] == "42"
    # Every agent answered exactly once and posted to the bus.
    posted = {m["agent"] for m in bus.history()}
    assert posted == {"a", "b", "c"}


@pytest.mark.asyncio
async def test_swarm_orchestrator_blackboard_finals_early():
    sessions = {
        "a": FakeSession(["thinking..."]),
        "b": FakeSession(["FINAL: solved"]),
    }
    multi = MultiAgentSession(sessions)
    bus = MessageBus()
    orch = SwarmOrchestrator(
        multi, BlackboardCoordinator(), bus, task="solve", max_rounds=8
    )
    await multi.start_all()
    result = await orch.run()
    await multi.close_all()

    assert result["final"] == "solved"
