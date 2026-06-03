"""Meta-Agent configuration generation from natural-language requirements."""

import re
import time
from typing import Any, Dict, List, Optional, Tuple

import yaml

from backend.models.schemas import MetaAgentGenerateRequest, ValidationResult


TOOL_LABELS = {
    "file_io": "File I/O",
    "shell": "Shell",
    "web_search": "Web Search",
    "code_execution": "Code Execution",
}

STRATEGY_PROMPTS = {
    "react": (
        "You are a ReAct agent. For each step, think about what to do, then use a tool. "
        "Format: Thought / Action / Action Input / Observation."
    ),
    "plan_and_solve": (
        "You are a Plan-and-Solve agent. First produce a structured plan, then execute "
        "each step with tools and revise the plan when observations require it."
    ),
}


def _slugify(text: str, max_len: int = 32) -> str:
    slug = re.sub(r"[^a-zA-Z0-9]+", "-", text.strip()).strip("-").lower()
    if not slug:
        return "custom-agent"
    return slug[:max_len].rstrip("-")


def _infer_agent_name(description: str) -> str:
    lowered = description.lower()
    if any(k in lowered for k in ("review", "审查", "code review")):
        return "CodeReviewer"
    if any(k in lowered for k in ("data", "分析", "csv", "pandas")):
        return "DataAnalyst"
    if any(k in lowered for k in ("research", "调研", "search")):
        return "ResearchAgent"
    if any(k in lowered for k in ("doc", "文档", "api doc")):
        return "DocGenerator"
    return f"Agent-{_slugify(description[:24]).title()}"


def _build_system_prompt(request: MetaAgentGenerateRequest) -> str:
    base = STRATEGY_PROMPTS.get(request.strategy_type, STRATEGY_PROMPTS["react"])
    tools_text = ", ".join(TOOL_LABELS.get(t, t) for t in request.allowed_tools) or "general tools"
    return (
        f"{base}\n\n"
        f"Role & goal: {request.goal_description.strip()}\n"
        f"Allowed tools: {tools_text}.\n"
        f"Follow project policies and ask for confirmation before destructive actions."
    )


def _build_config_dict(request: MetaAgentGenerateRequest) -> Dict[str, Any]:
    confirm_shell = "shell" in request.allowed_tools
    confirm_file_write = "file_io" in request.allowed_tools
    agent_name = _infer_agent_name(request.goal_description)
    system_prompt = _build_system_prompt(request)

    auto_compact = 150000 if request.strategy_type == "plan_and_solve" else 100000
    compaction = (
        {"micro_percent": 60, "auto_percent": 80, "full_percent": 95}
        if request.strategy_type == "plan_and_solve"
        else {"micro_percent": 50, "auto_percent": 70, "full_percent": 90}
    )

    hooks: List[Dict[str, Any]] = [
        {"type": "FileJournal", "path": ".pisci/journal.db", "enabled": True}
    ]
    if request.strategy_type == "plan_and_solve":
        hooks.append({"type": "PlanTracker", "enabled": True})

    return {
        "agent": {
            "name": agent_name,
            "model": request.model_preference,
            "context_window": 200000,
            "max_tokens": 8192,
            "enable_streaming": True,
        },
        "plugins": {
            "memory": "InMemoryVectorStore",
            "context": "ProjectContextManager",
        },
        "policy": {
            "confirm_shell": confirm_shell,
            "confirm_file_write": confirm_file_write,
            "auto_compact_threshold": auto_compact,
            "compaction_tiers": compaction,
            "max_tool_result_tokens": 4000,
        },
        "strategy": {
            "type": request.strategy_type,
            "system_prompt_template": system_prompt,
        },
        "hooks": hooks,
    }


def generate_agent_config(
    request: MetaAgentGenerateRequest,
) -> Tuple[str, str, ValidationResult]:
    """Generate YAML config and system prompt, then validate."""
    from backend.api.config import validate_config_dict

    config_dict = _build_config_dict(request)
    config_yaml = yaml.dump(config_dict, default_flow_style=False, allow_unicode=True)
    system_prompt = _build_system_prompt(request)
    validation = validate_config_dict(config_dict)
    return config_yaml, system_prompt, validation


def list_templates() -> List[Dict[str, str]]:
    """Return built-in agent design patterns."""
    return [
        {
            "id": "react_basic",
            "name": "Basic ReAct Agent",
            "description": "Fast reasoning + tool loop for straightforward tasks.",
            "strategy_type": "react",
        },
        {
            "id": "plan_and_solve",
            "name": "Plan-and-Solve Agent",
            "description": "Explicit planning with todo tracking for multi-step work.",
            "strategy_type": "plan_and_solve",
        },
        {
            "id": "coder_expert",
            "name": "Python Coding Expert",
            "description": "Code review and refactoring with file I/O and shell tools.",
            "strategy_type": "react",
        },
        {
            "id": "researcher",
            "name": "Research Assistant",
            "description": "Web search plus synthesis for investigation tasks.",
            "strategy_type": "react",
        },
    ]


def refine_config(
    config_yaml: str,
    feedback: str,
    updated_tools: Optional[List[str]] = None,
) -> Tuple[str, str, ValidationResult]:
    """Apply lightweight refinements to an existing draft."""
    from backend.api.config import validate_config_dict

    try:
        config_dict = yaml.safe_load(config_yaml) or {}
    except yaml.YAMLError as exc:
        result = ValidationResult(valid=False, errors=[f"YAML parsing error: {exc}"])
        return config_yaml, "", result

    if not isinstance(config_dict, dict):
        result = ValidationResult(valid=False, errors=["Configuration must be a mapping"])
        return config_yaml, "", result

    strategy = config_dict.setdefault("strategy", {})
    if isinstance(strategy, dict):
        existing = strategy.get("system_prompt_template", "")
        strategy["system_prompt_template"] = f"{existing}\n\nAdditional requirement: {feedback.strip()}"

    if updated_tools is not None:
        policy = config_dict.setdefault("policy", {})
        if isinstance(policy, dict):
            policy["confirm_shell"] = "shell" in updated_tools
            policy["confirm_file_write"] = "file_io" in updated_tools

    refined_yaml = yaml.dump(config_dict, default_flow_style=False, allow_unicode=True)
    system_prompt = ""
    if isinstance(strategy, dict):
        system_prompt = str(strategy.get("system_prompt_template", ""))
    validation = validate_config_dict(config_dict)
    return refined_yaml, system_prompt, validation
