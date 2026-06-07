import { useEffect, useState } from "react";
import { AgentSpec, ComponentSpec } from "../api";
import { useT } from "../i18n/context";
import { Select } from "./Select";

const ALGO_SLOTS = ["loop", "compaction", "memory", "context"] as const;

interface Props {
  name: string;
  description: string;
  spec: AgentSpec;
  catalog: Record<string, ComponentSpec[]>;
  onName: (v: string) => void;
  onDescription: (v: string) => void;
  onSpec: (spec: AgentSpec) => void;
}

function credParam(spec: AgentSpec, key: string): string {
  const v = spec.credentials?.params?.[key];
  return typeof v === "string" ? v : "";
}

function setCredParam(spec: AgentSpec, key: string, value: string): AgentSpec {
  const cred = spec.credentials ?? { name: "env", params: {} };
  return {
    ...spec,
    credentials: {
      ...cred,
      params: { ...(cred.params ?? {}), [key]: value },
    },
  };
}

export function AgentEditorForm({
  name,
  description,
  spec,
  catalog,
  onName,
  onDescription,
  onSpec,
}: Props) {
  const t = useT();
  const names = (kind: string) => (catalog[kind] || []).map((c) => c.name);

  const patch = (p: Partial<AgentSpec>) => onSpec({ ...spec, ...p });

  const setSlot = (kind: string, slotName: string, params?: Record<string, unknown>) => {
    patch({ [kind]: slotName ? { name: slotName, params: params ?? {} } : undefined } as Partial<AgentSpec>);
  };

  const credKind = spec.credentials?.name ?? "env";

  return (
    <div className="agent-editor-form">
      <label>{t("common.name")}</label>
      <input value={name} onChange={(e) => onName(e.target.value)} />

      <label>{t("common.description")}</label>
      <input value={description} onChange={(e) => onDescription(e.target.value)} />

      <label>{t("common.model")}</label>
      <input value={spec.model ?? ""} onChange={(e) => patch({ model: e.target.value })} />

      <label>{t("agentLibrary.credentials")}</label>
      <Select
        block
        value={credKind}
        onChange={(v) => setSlot("credentials", v, v === "static" ? spec.credentials?.params ?? {} : {})}
        options={names("credentials").map((n) => ({ value: n, label: n }))}
      />
      {credKind === "static" && (
        <div className="cred-static-fields">
          <label>{t("agentLibrary.apiKey")}</label>
          <input
            type="password"
            value={credParam(spec, "api_key")}
            onChange={(e) => onSpec(setCredParam(spec, "api_key", e.target.value))}
            placeholder="sk-…"
          />
          <label>{t("agentLibrary.baseUrl")}</label>
          <input
            value={credParam(spec, "base_url")}
            onChange={(e) => onSpec(setCredParam(spec, "base_url", e.target.value))}
            placeholder="https://api.openai.com/v1"
          />
          <label>{t("agentLibrary.provider")}</label>
          <input
            value={credParam(spec, "provider") || credParam(spec, "name")}
            onChange={(e) => onSpec(setCredParam(spec, "provider", e.target.value))}
            placeholder="openai / qwen / custom"
          />
        </div>
      )}
      {credKind === "env" && (
        <p className="muted" style={{ fontSize: 12, marginTop: 4 }}>{t("agentLibrary.envHint")}</p>
      )}

      <label>{t("projects.systemPrompt")}</label>
      <textarea
        rows={4}
        value={spec.system_prompt ?? ""}
        onChange={(e) => patch({ system_prompt: e.target.value })}
      />

      <h3>{t("agentLibrary.algorithmSlots")}</h3>
      <div className="row">
        {ALGO_SLOTS.map((kind) => (
          <div className="col" key={kind}>
            <label>{kind}</label>
            <Select
              block
              value={(spec[kind] as { name?: string } | undefined)?.name ?? ""}
              onChange={(n) => setSlot(kind, n)}
              options={names(kind).map((n) => ({ value: n, label: n }))}
            />
          </div>
        ))}
      </div>
    </div>
  );
}

export function defaultLibraryAgentSpec(name = "New Agent"): AgentSpec {
  return {
    name,
    model: "qwen-plus",
    system_prompt: "You are a helpful agent. Reply with FINAL ANSWER: when done.",
    loop: { name: "react", params: {} },
    compaction: { name: "sliding_window", params: {} },
    memory: { name: "noop", params: {} },
    context: { name: "none", params: {} },
    credentials: { name: "env", params: {} },
  };
}
