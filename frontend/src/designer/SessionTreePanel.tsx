import { useEffect, useMemo, useRef, useState } from "react";
import { SessionNode } from "../api";
import { useT } from "../i18n/context";
import { useDesigner } from "./store";

/** Compact session tree overlay on the main canvas (no Dock required). */
export function SessionTreePanel({ onClose }: { onClose: () => void }) {
  const t = useT();
  const {
    session, sessionRoots, loadSession, loadRuns,
    forkParent, forkTurn, setForkParent,
    overlayRunId, showRunOnCanvas, runTask, running,
    sessionExpanded, toggleSessionExpanded, setSessionExpanded,
    sessionPanelPosition, setSessionPanelPosition,
  } = useDesigner();
  const [task, setTask] = useState("Continue from here — reply with FINAL ANSWER:");
  const dragRef = useRef<{ sx: number; sy: number; ox: number; oy: number } | null>(null);

  useEffect(() => {
    loadSession();
    loadRuns();
  }, [loadSession, loadRuns]);

  // Default-expand roots when nothing persisted yet.
  useEffect(() => {
    if (sessionRoots.length && sessionExpanded.length === 0) {
      setSessionExpanded(sessionRoots);
    }
  }, [sessionRoots, sessionExpanded.length, setSessionExpanded]);

  const expanded = useMemo(() => new Set(sessionExpanded), [sessionExpanded]);

  useEffect(() => {
    const onMove = (e: MouseEvent) => {
      if (!dragRef.current) return;
      const { sx, sy, ox, oy } = dragRef.current;
      setSessionPanelPosition(ox + e.clientX - sx, oy + e.clientY - sy);
    };
    const onUp = () => { dragRef.current = null; };
    window.addEventListener("mousemove", onMove);
    window.addEventListener("mouseup", onUp);
    return () => {
      window.removeEventListener("mousemove", onMove);
      window.removeEventListener("mouseup", onUp);
    };
  }, [setSessionPanelPosition]);

  const onHeadMouseDown = (e: React.MouseEvent) => {
    if ((e.target as HTMLElement).closest("button")) return;
    dragRef.current = {
      sx: e.clientX,
      sy: e.clientY,
      ox: sessionPanelPosition.x,
      oy: sessionPanelPosition.y,
    };
    e.preventDefault();
  };

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

  const toggleExpand = (id: string) => toggleSessionExpanded(id);

  const selectRun = (runId: string) => {
    showRunOnCanvas(runId);
    if (!expanded.has(runId)) toggleSessionExpanded(runId);
  };

  const selectTurn = (runId: string, turn: number) => {
    showRunOnCanvas(runId);
    setForkParent(runId, turn);
  };

  const forkFromRun = (runId: string) => {
    setForkParent(runId, null);
    selectRun(runId);
  };

  const forkFromTurn = (runId: string, turn: number) => {
    setForkParent(runId, turn);
    selectRun(runId);
  };

  const renderNode = (n: SessionNode, depth: number) => {
    const isTurn = n.type === "turn";
    const isRun = !isTurn;
    const hasChildren = (childrenOf[n.id]?.length ?? 0) > 0;
    const isExpanded = isRun && (expanded.has(n.id) || (hasChildren && sessionRoots.includes(n.id) && sessionExpanded.length === 0));
    const active = isTurn
      ? forkParent === n.run_id && forkTurn === n.turn
      : overlayRunId === n.id || (forkParent === n.id && forkTurn == null);

    return (
      <div key={n.id} className="cst-node-wrap">
        <div
          className={"cst-node" + (active ? " active" : "") + (isTurn ? " turn" : " run")}
          style={{ paddingLeft: 8 + depth * 12 }}
          onClick={() => (isTurn ? selectTurn(n.run_id!, n.turn!) : selectRun(n.id))}
        >
          {isRun && hasChildren ? (
            <button
              className="cst-exp"
              onClick={(e) => { e.stopPropagation(); toggleExpand(n.id); }}
              title={isExpanded ? "collapse" : "expand turns"}
            >
              {isExpanded ? "▾" : "▸"}
            </button>
          ) : (
            <span className="cst-exp placeholder" />
          )}
          {isTurn ? (
            <>
              <span className="cst-label">T{n.turn}</span>
              <span className="cst-detail" title={n.assistant}>{n.assistant || "—"}</span>
              <button
                className="cst-act"
                onClick={(e) => { e.stopPropagation(); forkFromTurn(n.run_id!, n.turn!); }}
                title={t("session.forkFromTurn")}
              >
                {t("common.fork")}
              </button>
            </>
          ) : (
            <>
              <span className="cst-label">{n.id.slice(0, 6)}</span>
              <span className="cst-detail" title={n.task}>{n.task || "—"}</span>
              <button
                className="cst-act"
                onClick={(e) => { e.stopPropagation(); forkFromRun(n.id); }}
                title={t("session.forkFromRun")}
              >
                {t("common.fork")}
              </button>
            </>
          )}
        </div>
        {isRun && isExpanded ? (childrenOf[n.id] ?? []).map((c) => renderNode(c, depth + 1)) : null}
      </div>
    );
  };

  return (
    <div
      className="canvas-session-panel"
      style={{ left: sessionPanelPosition.x, top: sessionPanelPosition.y, right: "auto" }}
    >
      <div className="cst-head" onMouseDown={onHeadMouseDown}>
        <span>{t("session.title")}</span>
        <span className="muted" style={{ fontSize: 11 }}>{t("session.hint")}</span>
        <span className="spacer" />
        {overlayRunId ? (
          <button type="button" className="btn btn-sm" onClick={() => showRunOnCanvas(null)}>{t("session.clearOverlay")}</button>
        ) : null}
        <button type="button" className="btn btn-sm" onClick={onClose}>×</button>
      </div>
      <div className="cst-body">
        {session.length === 0 ? (
          <span className="muted" style={{ fontSize: 12 }}>{t("session.noRuns")}</span>
        ) : (
          (sessionRoots.length ? sessionRoots : Object.keys(childrenOf))
            .map((rid) => session.find((n) => n.id === rid && n.type !== "turn"))
            .filter((n): n is SessionNode => !!n)
            .map((n) => renderNode(n, 0))
        )}
      </div>
      {forkParent ? (
        <div className="cst-fork-bar">
          <span className="muted" style={{ fontSize: 11 }}>
            {t("session.forkedFrom")} {forkParent.slice(0, 8)}{forkTurn != null ? ` · T${forkTurn}` : ""}
          </span>
          <input
            value={task}
            onChange={(e) => setTask(e.target.value)}
            placeholder={t("common.taskPlaceholder")}
            onKeyDown={(e) => e.key === "Enter" && !running && runTask(task)}
          />
          <button type="button" className="btn btn-primary" disabled={running} onClick={() => runTask(task)}>
            {running ? "…" : t("common.run")}
          </button>
          <button type="button" className="btn" onClick={() => setForkParent(null)}>{t("session.cancel")}</button>
        </div>
      ) : null}
    </div>
  );
}
