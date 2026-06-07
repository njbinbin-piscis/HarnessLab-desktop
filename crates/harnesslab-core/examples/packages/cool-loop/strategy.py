"""Example community algorithm package: a concise-answer loop strategy."""
from harnesslab.library.base import LoopStrategy


class CoolLoop(LoopStrategy):
    NAME = "cool_loop"
    DESCRIPTION = "Community loop strategy: think briefly, answer concisely."

    def system_addendum(self) -> str:
        return "Be concise. Think briefly, then emit 'FINAL ANSWER:' with a short answer."

    def should_stop(self, turn: int, last_text: str, used_tool: bool) -> bool:
        return "final answer:" in last_text.lower()


COMPONENTS = [CoolLoop]
