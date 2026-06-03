"""User-codeable multi-agent coordination (Layer 3).

This module lifts the *coordination algorithm* out of the Rust kernel
(`pool/coordinator.rs`, which stays as the low-level "run one koi turn"
primitive) and into Python, where an experiment author can pick a built-in
algorithm or drop in their own.

Three pieces:

- :class:`Coordinator` — the protocol a coordination algorithm implements:
  given the task, the participating agents and the shared bus transcript so
  far, return a :class:`Decision` (who to activate next, what to say, or that
  the swarm is done with a final answer).
- Built-in coordinators: :class:`MajorityVoteCoordinator`,
  :class:`AuctionCoordinator`, :class:`BlackboardCoordinator`.
- :func:`load_coordinator` — resolves a coordinator from a built-in name or a
  user Python module (loaded at run time).

The coordinators are deliberately pure (no I/O): they read a transcript and
return a decision. The :class:`~backend.core.manager.SwarmOrchestrator`
executes those decisions against live agent sessions and the message bus, so
the algorithms here are unit-testable with plain dictionaries.
"""

from __future__ import annotations

import importlib
import importlib.util
import re
from collections import Counter
from dataclasses import dataclass, field
from pathlib import Path
from typing import Dict, List, Optional, Protocol, runtime_checkable


# Message markers agents use on the bus so coordinators can parse intent.
_FINAL_RE = re.compile(r"FINAL\s*:\s*(.+)", re.IGNORECASE | re.DOTALL)
_BID_RE = re.compile(r"BID\s*:\s*([0-9]*\.?[0-9]+)", re.IGNORECASE)


@dataclass
class CoordinationContext:
    """Read-only snapshot handed to a coordinator each round."""

    task: str
    agents: List[str]
    # Ordered bus transcript: each entry is {"agent": str, "text": str}.
    transcript: List[Dict[str, str]] = field(default_factory=list)
    round: int = 0
    max_rounds: int = 8

    def messages_by(self, agent: str) -> List[str]:
        """All texts posted by ``agent``, in order."""
        return [m["text"] for m in self.transcript if m.get("agent") == agent]

    def latest_by(self, agent: str) -> Optional[str]:
        msgs = self.messages_by(agent)
        return msgs[-1] if msgs else None

    def agents_without_message(self) -> List[str]:
        spoken = {m.get("agent") for m in self.transcript}
        return [a for a in self.agents if a not in spoken]


@dataclass
class Decision:
    """A coordinator's instruction for the next step."""

    next_agent: Optional[str] = None
    message: Optional[str] = None
    done: bool = False
    final: Optional[str] = None
    metadata: Dict[str, str] = field(default_factory=dict)


@runtime_checkable
class Coordinator(Protocol):
    """Protocol for a multi-agent coordination algorithm."""

    name: str

    def coordinate(self, ctx: CoordinationContext) -> Decision:
        """Inspect the transcript and decide the next step."""
        ...


def extract_final(text: Optional[str]) -> Optional[str]:
    """Pull the answer out of a ``FINAL: <answer>`` message, if present."""
    if not text:
        return None
    m = _FINAL_RE.search(text)
    return m.group(1).strip() if m else None


def extract_bid(text: Optional[str]) -> Optional[float]:
    """Pull the numeric bid out of a ``BID: <n>`` message, if present."""
    if not text:
        return None
    m = _BID_RE.search(text)
    if not m:
        return None
    try:
        return float(m.group(1))
    except ValueError:
        return None


class MajorityVoteCoordinator:
    """Ask every agent to answer, then return the majority ``FINAL`` answer.

    Each agent is prompted once (in declaration order) to produce a
    ``FINAL: <answer>`` line. Once all have voted, answers are tallied
    (case-insensitive, whitespace-normalised) and the plurality wins; ties are
    broken by first-seen order.
    """

    name = "majority_vote"

    def coordinate(self, ctx: CoordinationContext) -> Decision:
        pending = ctx.agents_without_message()
        if pending:
            return Decision(
                next_agent=pending[0],
                message=(
                    f"Task: {ctx.task}\n\n"
                    "Give your best answer on a single line prefixed with "
                    "`FINAL:` (e.g. `FINAL: 42`)."
                ),
            )

        votes: List[str] = []
        normalized_to_display: Dict[str, str] = {}
        for agent in ctx.agents:
            answer = extract_final(ctx.latest_by(agent))
            if answer:
                key = " ".join(answer.lower().split())
                votes.append(key)
                normalized_to_display.setdefault(key, answer)

        if not votes:
            return Decision(done=True, final=None, metadata={"reason": "no_votes"})

        winner_key, count = Counter(votes).most_common(1)[0]
        return Decision(
            done=True,
            final=normalized_to_display[winner_key],
            metadata={"votes": str(len(votes)), "winning_count": str(count)},
        )


class AuctionCoordinator:
    """Two-phase auction: agents bid, the highest bidder solves the task.

    Phase 1 — each agent posts ``BID: <0..1>`` (self-assessed fitness).
    Phase 2 — the highest bidder is asked to produce ``FINAL: <answer>``; once
    they do, the swarm is done.
    """

    name = "auction"

    def coordinate(self, ctx: CoordinationContext) -> Decision:
        # Phase 1: collect a bid from each agent.
        bids: Dict[str, float] = {}
        for agent in ctx.agents:
            for text in ctx.messages_by(agent):
                bid = extract_bid(text)
                if bid is not None:
                    bids[agent] = bid
                    break

        missing_bids = [a for a in ctx.agents if a not in bids]
        if missing_bids:
            return Decision(
                next_agent=missing_bids[0],
                message=(
                    f"Task: {ctx.task}\n\n"
                    "How well can you handle this? Reply with a single line "
                    "`BID: <0.0-1.0>` reflecting your confidence."
                ),
            )

        winner = max(bids, key=lambda a: bids[a])

        # Phase 2: has the winner already produced a final answer?
        final = None
        for text in ctx.messages_by(winner):
            final = extract_final(text) or final
        if final:
            return Decision(done=True, final=final, metadata={"winner": winner})

        return Decision(
            next_agent=winner,
            message=(
                f"You won the auction (bid {bids[winner]:.2f}). Solve the task "
                f"and reply with `FINAL: <answer>`.\n\nTask: {ctx.task}"
            ),
            metadata={"winner": winner, "bid": f"{bids[winner]:.2f}"},
        )


class BlackboardCoordinator:
    """Shared-blackboard round-robin until someone posts a final answer.

    Agents take turns appending to the shared transcript (the "blackboard"),
    each seeing all prior contributions. The swarm finishes when any agent
    posts ``FINAL: <answer>`` or the round budget is exhausted (in which case
    the most recent contribution is returned as the result).
    """

    name = "blackboard"

    def coordinate(self, ctx: CoordinationContext) -> Decision:
        # Finished if anyone has posted a FINAL answer.
        for entry in ctx.transcript:
            final = extract_final(entry.get("text"))
            if final:
                return Decision(
                    done=True,
                    final=final,
                    metadata={"by": entry.get("agent", "?")},
                )

        if ctx.round >= ctx.max_rounds:
            last = ctx.transcript[-1]["text"] if ctx.transcript else None
            return Decision(done=True, final=last, metadata={"reason": "max_rounds"})

        # Round-robin: pick the agent who has contributed least, then earliest.
        counts = Counter(m.get("agent") for m in ctx.transcript)
        next_agent = min(ctx.agents, key=lambda a: (counts.get(a, 0), ctx.agents.index(a)))
        return Decision(
            next_agent=next_agent,
            message=(
                f"Task: {ctx.task}\n\n"
                "Shared blackboard so far is in the conversation. Add your "
                "contribution. When the task is fully solved, post the answer "
                "as `FINAL: <answer>`."
            ),
        )


_BUILTINS = {
    "majority_vote": MajorityVoteCoordinator,
    "auction": AuctionCoordinator,
    "blackboard": BlackboardCoordinator,
}


def builtin_coordinators() -> List[str]:
    return sorted(_BUILTINS)


def load_coordinator(spec: str) -> Coordinator:
    """Resolve a coordinator from a built-in name or a user module.

    ``spec`` forms:
    - built-in name: ``"majority_vote"`` / ``"auction"`` / ``"blackboard"``
    - dotted import: ``"my_pkg.my_mod:MyCoordinator"``
    - file path:    ``"/abs/path/to/file.py:MyCoordinator"``

    The named attribute may be a class (instantiated with no args) or a
    factory callable returning a coordinator. The result must satisfy the
    :class:`Coordinator` protocol (``name`` attribute + ``coordinate`` method).
    """
    if spec in _BUILTINS:
        return _BUILTINS[spec]()

    if ":" not in spec:
        raise ValueError(
            f"unknown coordinator '{spec}'. Use a built-in "
            f"({', '.join(builtin_coordinators())}) or 'module:Attr' / 'file.py:Attr'."
        )

    target, attr = spec.rsplit(":", 1)
    if target.endswith(".py") or "/" in target:
        path = Path(target).expanduser().resolve()
        if not path.exists():
            raise FileNotFoundError(f"coordinator module not found: {path}")
        module_spec = importlib.util.spec_from_file_location(f"_swarm_user_{path.stem}", path)
        if module_spec is None or module_spec.loader is None:
            raise ImportError(f"cannot load coordinator module: {path}")
        module = importlib.util.module_from_spec(module_spec)
        module_spec.loader.exec_module(module)
    else:
        module = importlib.import_module(target)

    if not hasattr(module, attr):
        raise AttributeError(f"module '{target}' has no attribute '{attr}'")
    obj = getattr(module, attr)
    coordinator = obj() if callable(obj) else obj

    if not isinstance(coordinator, Coordinator):
        raise TypeError(
            f"loaded coordinator '{spec}' does not satisfy the Coordinator protocol "
            "(needs a `name` attribute and a `coordinate(ctx)` method)"
        )
    return coordinator


class MessageBus:
    """In-memory message bus shared by a swarm.

    Mirrors the semantics of the kernel's ``pool_chat`` (append-only,
    readable history with @mention addressing) so a swarm can use it as the
    inter-agent communication channel. It is intentionally simple and
    in-process; a ``pool_chat``-backed implementation can be substituted
    behind the same ``post`` / ``history`` interface.
    """

    def __init__(self) -> None:
        self._messages: List[Dict[str, str]] = []

    def post(self, agent: str, text: str) -> Dict[str, str]:
        entry = {"agent": agent, "text": text}
        self._messages.append(entry)
        return entry

    def history(self) -> List[Dict[str, str]]:
        return list(self._messages)

    def mentions(self, text: str, agents: List[str]) -> List[str]:
        """Return agent names @mentioned in ``text``."""
        found = []
        for a in agents:
            if re.search(rf"@{re.escape(a)}\b", text):
                found.append(a)
        return found
