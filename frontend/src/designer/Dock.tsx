import { useCallback, useEffect, useMemo, useState } from "react";
import { api, CanvasRun, SessionNode } from "../api";
import { CodeBlock } from "../components/CodeBlock";
import { useT } from "../i18n/context";
import { RuntimeMonitorPanel } from "./RuntimeMonitor";
import { useDesigner } from "./store";

type DockTab = "run" | "history" | "session" | "monitor" | "copilot" | "code";

function ago(ts: number): string {
  const s = Math.max(0, Math.round(Date.now() / 1000 - ts));
  if (s < 60) return `${s}s ago`;
  if (s < 3600) return `${Math.round(s / 60)}m ago`;
  if (s < 86400) return `${Math.round(s / 3600)}h ago`;
  return `${Math.round(s / 86400)}d ago`;
}

function summarize(v: unknown): string {
  if (v == null) return "—";
  if (Array.isArray(v)) {
    const names = v.map((x) => (x && typeof x === "object" && "name" in x ? (x as { name: string }).name : x));
    return names.length ? names.join(", ") : "[]";
  }
  if (typeof v === "object") {
    const o = v as Record<string, unknown>;
    if ("name" in o) return String(o.name) + ("mode" in o ? ` (${o.mode})` : "");
    if ("mode" in o) return String(o.mode);
    return JSON.stringify(v).slice(0, 60);
  }
  return String(v);
}

function RunPanel() {
  const t = useT();
  const { running, runLog, runResult, runTask, forkParent, forkTurn, setForkParent } = useDesigner();
  const [task, setTask] = useState("What is 2 + 2? Reply with FINAL ANSWER:");
  return (
    <div className="dock-pane">
      {forkParent ? (
        <div className="fork-banner">
          ↳ {t("dock.forkedFrom")} <code>{forkParent.slice(0, 8)}</code>
          {forkTurn != null ? <> · turn <code>{forkTurn}</code></> : null}
          <button type="button" className="btn" onClick={() => setForkParent(null)}>{t("dock.cancelFork")}</button>
        </div>
      ) : null}
      <div className="dock-row">
        <input value={task} onChange={(e) => setTask(e.target.value)} placeholder={t("common.taskPlaceholder")} />
        <button className="btn btn-primary" onClick={() => runTask(task)} disabled={running}>
          {running ? t("experiments.running") : forkParent ? `${t("common.run")} (${t("common.fork")})` : t("dock.runTask")}
        </button>
      </div>
      {runResult ? <div className="run-result"><b>Result:</b> {runResult}</div> : null}
      <div className="run-log">
        {runLog.length === 0 ? (
          <span className="muted" style={{ fontSize: 12 }}>Run the project to stream events and overlay per-agent metrics on the canvas.</span>
        ) : (
          runLog.map((e, i) => (
            <div key={i} className={"log-line k-" + e.kind}>
              <span className="log-agent">{e.agent}</span>
              <span className="log-kind">{e.kind}</span>
              <span className="log-text">{e.text || (e.data?.name as string) || ""}</span>
            </div>
          ))
        )}
      </div>
    </div>
  );
}

function RunComparison({ a, b }: { a: CanvasRun; b: CanvasRun }) {
  const agents = Array.from(new Set([...Object.keys(a.metrics), ...Object.keys(b.metrics)])).sort();
  const fields: (keyof CanvasRun["metrics"][string])[] = ["turns", "tools", "tokens", "ms"];
  const cell = (av: number, bv: number) => {
    const d = bv - av;
    const cls = d > 0 ? "delta up" : d < 0 ? "delta down" : "delta";
    return (
      <td>
        {bv.toLocaleString()} <span className={cls}>{d === 0 ? "±0" : (d > 0 ? "+" : "") + d.toLocaleString()}</span>
      </td>
    );
  };
  return (
    <div className="run-compare">
      <div className="rc-head">
        <span>A · {ago(a.created_at)} · {a.total_tokens.toLocaleString()} tok</span>
        <span>vs</span>
        <span>B · {ago(b.created_at)} · {b.total_tokens.toLocaleString()} tok</span>
      </div>
      <table className="rc-table">
        <thead>
          <tr><th>agent</th>{fields.map((f) => <th key={f}>{f} (A → B)</th>)}</tr>
        </thead>
        <tbody>
          {agents.map((ag) => {
            const am = a.metrics[ag] ?? { turns: 0, tools: 0, tokens: 0, ms: 0 };
            const bm = b.metrics[ag] ?? { turns: 0, tools: 0, tokens: 0, ms: 0 };
            return (
              <tr key={ag}>
                <td className="rc-agent">{ag}</td>
                {fields.map((f) => cell(am[f] as number, bm[f] as number))}
              </tr>
            );
          })}
        </tbody>
      </table>
      <div className="rc-results">
        <div><b>A result:</b> {a.result || "—"}</div>
        <div><b>B result:</b> {b.result || "—"}</div>
      </div>
    </div>
  );
}

function HistoryPanel() {
  const t = useT();
  const { runs, overlayRunId, showRunOnCanvas, loadRuns } = useDesigner();
  const [sel, setSel] = useState<string[]>([]); // up to 2 run ids for comparison

  useEffect(() => {
    loadRuns();
  }, [loadRuns]);

  const toggleCompare = (id: string) =>
    setSel((s) => (s.includes(id) ? s.filter((x) => x !== id) : [...s, id].slice(-2)));

  const [a, b] = useMemo(() => {
    const ra = runs.find((r) => r.id === sel[0]);
    const rb = runs.find((r) => r.id === sel[1]);
    return [ra, rb];
  }, [runs, sel]);

  return (
    <div className="dock-pane">
      <div className="dock-row">
        <span className="muted" style={{ fontSize: 12 }}>
          {runs.length} run{runs.length === 1 ? "" : "s"}. Click a run to overlay its metrics on the canvas; tick two to compare.
        </span>
        <span className="spacer" />
        {overlayRunId ? <button className="btn" onClick={() => showRunOnCanvas(null)}>{t("dock.clearOverlay")}</button> : null}
      </div>
      {runs.length === 0 ? (
        <span className="muted" style={{ fontSize: 12 }}>No runs yet — run the project to record history.</span>
      ) : (
        <div className="run-history">
          {runs.map((r) => (
            <div key={r.id} className={"hist-row" + (r.id === overlayRunId ? " active" : "")}>
              <input
                type="checkbox"
                checked={sel.includes(r.id)}
                onChange={() => toggleCompare(r.id)}
                title="select for comparison"
              />
              <button className="hist-main" onClick={() => showRunOnCanvas(r.id)}>
                <span className="hist-task">{r.task}</span>
                <span className="hist-meta">
                  {ago(r.created_at)} · {r.total_tokens.toLocaleString()} tok · {r.duration_ms.toLocaleString()} ms
                  {r.project_rev != null ? ` · rev ${r.project_rev}` : ""}
                </span>
              </button>
            </div>
          ))}
        </div>
      )}
      {a && b ? <RunComparison a={a} b={b} /> : null}
    </div>
  );
}

function SessionPanel({ onForked }: { onForked: () => void }) {
  const t = useT();
  const { session, sessionRoots, loadSession, setForkParent, forkParent, forkTurn } = useDesigner();

  useEffect(() => {
    loadSession();
  }, [loadSession]);

  const childrenOf = useMemo(() => {
    const m: Record<string, SessionNode[]> = {};
    for (const n of session) {
      const key = n.parent ?? "__root__";
      (m[key] ||= []).push(n);
    }
    for (const k of Object.keys(m)) {
      m[k].sort((a, b) => {
        if (a.type === "turn" && b.type === "turn") return (a.turn ?? 0) - (b.turn ?? 0);
        return 0;
      });
    }
    return m;
  }, [session]);

  const forkFromRun = (runId: string) => {
    setForkParent(runId, null);
    onForked();
  };

  const forkFromTurn = (runId: string, turn: number) => {
    setForkParent(runId, turn);
    onForked();
  };

  const renderNode = (n: SessionNode, depth: number) => {
    const isTurn = n.type === "turn";
    const active = isTurn
      ? forkParent === n.run_id && forkTurn === n.turn
      : forkParent === n.id && forkTurn == null;
    return (
      <div key={n.id}>
        <div
          className={"sess-node" + (active ? " active" : "") + (isTurn ? " sess-turn" : "")}
          style={{ marginLeft: depth * 16 }}
        >
          {isTurn ? (
            <>
              <span className="sess-id">T{n.turn}</span>
              <span className="sess-result" title={n.assistant}>{n.assistant || "—"}</span>
              <button type="button" className="btn btn-sm sess-fork" onClick={() => forkFromTurn(n.run_id!, n.turn!)} title={t("dock.forkFromTurn")}>{t("common.fork")}</button>
            </>
          ) : (
            <>
              <span className="sess-id">{n.id.slice(0, 8)}</span>
              <span className="sess-task" title={n.task}>{n.task || "—"}</span>
              <span className="sess-result" title={n.result}>{n.result || "—"}</span>
              <button type="button" className="btn btn-sm sess-fork" onClick={() => forkFromRun(n.id)} title={t("dock.forkFromRun")}>{t("common.fork")}</button>
            </>
          )}
        </div>
        {(childrenOf[n.id] ?? []).map((c) => renderNode(c, depth + 1))}
      </div>
    );
  };

  return (
    <div className="dock-pane">
      <div className="dock-row">
        <span className="muted" style={{ fontSize: 12 }}>{t("dock.sessionHint")}</span>
      </div>
      {session.length === 0 ? (
        <span className="muted" style={{ fontSize: 12 }}>{t("dock.noRuns")}</span>
      ) : (
        <div className="sess-tree">
          {(sessionRoots.length ? sessionRoots : Object.keys(childrenOf))
            .map((rid) => session.find((n) => n.id === rid))
            .filter((n): n is SessionNode => !!n)
            .map((n) => renderNode(n, 0))}
        </div>
      )}
    </div>
  );
}

function opLabel(op: Record<string, unknown>): string {
  const k = op.op as string;
  const a = op.agent ? ` ${op.agent}` : "";
  switch (k) {
    case "add_agent": return `add agent ${op.name}${op.loop ? ` (loop=${op.loop})` : ""}`;
    case "remove_agent": return `remove agent ${op.name}`;
    case "rename_agent": return `rename ${op.name} → ${op.to}`;
    case "set_slot": return `set ${op.kind} of${a} → ${op.ref}`;
    case "clear_slot": return `clear ${op.kind} of${a}`;
    case "attach_tool": return `attach ${op.tool} to${a}`;
    case "detach_tool": return `detach ${op.tool} from${a}`;
    case "set_param": return `set${a} ${op.key} = ${JSON.stringify(op.value)}`;
    case "set_topology": return `topology → ${op.mode}${op.coordinator ? ` / ${op.coordinator}` : ""}`;
    case "set_scorer": return `scorer → ${op.ref}`;
    case "clear_scorer": return "clear scorer";
    case "add_dataset": return `add dataset ${op.dataset}`;
    case "remove_dataset": return `remove dataset ${op.dataset}`;
    default: return JSON.stringify(op);
  }
}

function CopilotPanel() {
  const t = useT();
  const { proposing, proposal, propose, clearProposal, applyOps } = useDesigner();
  const [text, setText] = useState("");
  const [picked, setPicked] = useState<Set<number>>(new Set());
  const [showJson, setShowJson] = useState(false);

  useEffect(() => {
    setPicked(new Set(proposal ? proposal.ops.map((_, i) => i) : []));
  }, [proposal]);

  const toggle = (i: number) =>
    setPicked((s) => {
      const n = new Set(s);
      n.has(i) ? n.delete(i) : n.add(i);
      return n;
    });

  const applySelected = async () => {
    if (!proposal) return;
    const ops = proposal.ops.filter((_, i) => picked.has(i));
    if (ops.length && (await applyOps(ops))) clearProposal();
  };

  return (
    <div className="dock-pane">
      <div className="dock-row">
        <input
          value={text}
          onChange={(e) => setText(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && propose(text)}
          placeholder="e.g. add a critic agent and switch to coordinated with debate"
        />
        <button className="btn btn-primary" onClick={() => propose(text)} disabled={proposing}>
          {proposing ? "Thinking…" : "Propose"}
        </button>
      </div>
      {proposal ? (
        <div className="cop-proposal">
          <p className="cop-explain">{proposal.explanation} <span className="badge">{proposal.source}</span></p>
          {proposal.ops.length ? (
            <>
              <div className="cop-ops">
                {proposal.ops.map((op, i) => (
                  <label key={i} className="cop-op">
                    <input type="checkbox" checked={picked.has(i)} onChange={() => toggle(i)} />
                    <span className="cop-op-kind">{op.op as string}</span>
                    <span className="cop-op-text">{opLabel(op)}</span>
                  </label>
                ))}
              </div>
              {Object.keys(proposal.preview.diff).length ? (
                <div className="cop-diff">
                  {Object.entries(proposal.preview.diff).map(([field, ba]) => (
                    <div className="cop-diff-row" key={field}>
                      <span className="cop-diff-field">{field}</span>
                      <span className="cop-diff-before">{summarize(ba.before)}</span>
                      <span className="cop-diff-arrow">→</span>
                      <span className="cop-diff-after">{summarize(ba.after)}</span>
                    </div>
                  ))}
                </div>
              ) : null}
              {!proposal.preview.valid ? (
                <ul className="cop-problems">
                  {proposal.preview.problems.map((p, i) => <li key={i}>{p}</li>)}
                </ul>
              ) : null}
              <div className="cop-actions">
                <button className="cp-tab" onClick={() => setShowJson((v) => !v)}>{showJson ? t("dock.hideJson") : t("dock.json")}</button>
                <span className="cop-ok">{picked.size}/{proposal.ops.length} selected</span>
                <span className="spacer" />
                <button className="btn" onClick={clearProposal}>{t("dock.discard")}</button>
                <button className="btn btn-primary" onClick={applySelected} disabled={picked.size === 0}>{t("dock.applySelected")}</button>
              </div>
              {showJson ? <CodeBlock code={JSON.stringify(proposal.ops, null, 2)} title="edit ops" /> : null}
            </>
          ) : (
            <p className="muted" style={{ fontSize: 12 }}>No concrete change proposed.</p>
          )}
        </div>
      ) : (
        <span className="muted" style={{ fontSize: 12 }}>
          Describe a change in plain language. The copilot proposes validated edit ops for you to review before applying.
        </span>
      )}
    </div>
  );
}

function CodePanel() {
  const t = useT();
  const { project, projectId } = useDesigner();
  const [code, setCode] = useState<Record<string, string>>({});
  const [active, setActive] = useState("");
  const [loading, setLoading] = useState(false);

  const refresh = useCallback(async () => {
    if (!projectId) return;
    setLoading(true);
    try {
      const res = await api.generate(projectId, "python");
      setCode(res.files);
      setActive((a) => (a && res.files[a] ? a : Object.keys(res.files)[0] ?? ""));
    } finally {
      setLoading(false);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [projectId, project]);

  const files = Object.keys(code);
  return (
    <div className="dock-pane code-pane">
      <div className="dock-row">
        <button className="btn" onClick={refresh} disabled={loading}>{loading ? t("dock.generating") : t("dock.regenerate")}</button>
        <span className="cp-tabs">
          {files.map((f) => (
            <button key={f} className={"cp-tab" + (f === active ? " active" : "")} onClick={() => setActive(f)}>{f}</button>
          ))}
        </span>
      </div>
      {active ? <CodeBlock code={code[active]} title={active} /> : <div className="cp-empty">Regenerate to preview the runnable Python for this project.</div>}
    </div>
  );
}

export function Dock({ onClose }: { onClose: () => void }) {
  const t = useT();
  const [tab, setTab] = useState<DockTab>("run");
  const proposal = useDesigner((s) => s.proposal);
  const runs = useDesigner((s) => s.runs);
  const runtimeActive = useDesigner((s) => s.runtimeActive);
  const tabs: { id: DockTab; label: string }[] = [
    { id: "run", label: t("dock.runTab") },
    { id: "monitor", label: runtimeActive > 0 ? `Monitor (${runtimeActive})` : "Monitor" },
    { id: "history", label: runs.length ? `History (${runs.length})` : "History" },
    { id: "session", label: t("dock.sessionTab") },
    { id: "copilot", label: proposal ? `${t("dock.copilot")} ●` : t("dock.copilot") },
    { id: "code", label: t("dock.code") },
  ];
  return (
    <div className="dock">
      <div className="dock-head">
        <div className="seg">
          {tabs.map((tabItem) => (
            <button key={tabItem.id} className={tabItem.id === tab ? "active" : ""} onClick={() => setTab(tabItem.id)}>{tabItem.label}</button>
          ))}
        </div>
        <span className="spacer" />
        <button className="btn" onClick={onClose}>{t("dock.close")}</button>
      </div>
      {tab === "run" && <RunPanel />}
      {tab === "history" && <HistoryPanel />}
      {tab === "session" && <SessionPanel onForked={() => setTab("run")} />}
      {tab === "monitor" && <RuntimeMonitorPanel />}
      {tab === "copilot" && <CopilotPanel />}
      {tab === "code" && <CodePanel />}
    </div>
  );
}
