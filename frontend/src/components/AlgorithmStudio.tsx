import { useEffect, useRef, useState } from "react";
import { api, DeclarativeAlgorithm, AlgorithmTrialMsg, RUNTIME_ALGO_KINDS } from "../api";
import { useT } from "../i18n/context";
import { CodeEditor } from "./CodeEditor";

interface ChatMsg {
  role: "user" | "assistant";
  text: string;
}

interface TrialLine {
  kind: string;
  agent: string;
  text: string;
}

function parseAlgo(code: string): { algo: DeclarativeAlgorithm | null; error: string } {
  try {
    const obj = JSON.parse(code) as DeclarativeAlgorithm;
    if (!obj || typeof obj !== "object") return { algo: null, error: "not an object" };
    if (!obj.name?.trim()) return { algo: null, error: "missing name" };
    if (!obj.kind?.trim()) obj.kind = "loop";
    // Scorers are configured via params; everything else needs prompt guidance.
    if (obj.kind !== "scorer" && !obj.addendum?.trim()) {
      return { algo: null, error: "missing addendum" };
    }
    return { algo: obj, error: "" };
  } catch (e) {
    return { algo: null, error: String(e) };
  }
}

export function AlgorithmStudio() {
  const t = useT();
  const [messages, setMessages] = useState<ChatMsg[]>([]);
  const [input, setInput] = useState("A debate-style strategy: argue two positions, then reconcile.");
  const [kind, setKind] = useState<string>("loop");
  const [authoring, setAuthoring] = useState(false);

  const [code, setCode] = useState("");
  const [task, setTask] = useState("What is 17 × 23? Show your reasoning, then give the final number.");
  const [trial, setTrial] = useState<TrialLine[]>([]);
  const [running, setRunning] = useState(false);
  const [trialOk, setTrialOk] = useState(false);

  const [saving, setSaving] = useState(false);
  const [saved, setSaved] = useState("");
  const [err, setErr] = useState("");

  const [savedAlgos, setSavedAlgos] = useState<DeclarativeAlgorithm[]>([]);

  const trialEndRef = useRef<HTMLDivElement>(null);

  const refreshSaved = () => api.listAlgorithms().then(setSavedAlgos).catch(() => {});
  useEffect(() => {
    refreshSaved();
  }, []);

  const loadSaved = (a: DeclarativeAlgorithm) => {
    setCode(JSON.stringify(a, null, 2));
    if (a.kind) setKind(a.kind);
    setTrialOk(false);
    setSaved("");
    setErr("");
    setMessages((m) => [...m, { role: "assistant", text: t("algoStudio.loadedMsg", { name: a.name }) }]);
  };

  const removeSaved = async (name: string) => {
    try {
      await api.deleteAlgorithm(name);
      refreshSaved();
    } catch (e) {
      setErr(String(e));
    }
  };

  const { algo, error: parseError } = parseAlgo(code);
  const hasCode = code.trim().length > 0;

  const author = async () => {
    const instruction = input.trim();
    if (!instruction || authoring) return;
    setAuthoring(true);
    setErr("");
    setMessages((m) => [...m, { role: "user", text: instruction }]);
    setInput("");
    try {
      // When refining an existing algorithm keep its kind; otherwise use the picker.
      const res = await api.authorAlgorithm(instruction, algo, algo ? algo.kind : kind);
      setCode(JSON.stringify(res.algorithm, null, 2));
      if (res.algorithm.kind) setKind(res.algorithm.kind);
      setTrialOk(false);
      setSaved("");
      setMessages((m) => [...m, { role: "assistant", text: res.explanation }]);
    } catch (e) {
      setErr(String(e));
      setMessages((m) => [...m, { role: "assistant", text: String(e) }]);
    } finally {
      setAuthoring(false);
    }
  };

  // Any manual edit invalidates the last successful trial → save greys out.
  const onEditCode = (next: string) => {
    setCode(next);
    setTrialOk(false);
    setSaved("");
  };

  const runTrial = async () => {
    if (!algo || running) return;
    setRunning(true);
    setErr("");
    setSaved("");
    setTrial([]);
    setTrialOk(false);
    try {
      await api.runAlgorithmTrialChannel(algo, task, (msg: AlgorithmTrialMsg) => {
        if (msg.type === "event") {
          setTrial((l) => [
            ...l,
            { kind: msg.event.kind, agent: msg.event.agent, text: msg.event.text },
          ]);
        } else if (msg.type === "result") {
          setTrial((l) => [...l, { kind: "result", agent: "trial", text: msg.result }]);
          setTrialOk(msg.success);
          if (!msg.success) setErr(t("algoStudio.trialEmpty"));
        } else if (msg.type === "error") {
          setTrial((l) => [...l, { kind: "error", agent: "trial", text: msg.message }]);
          setErr(msg.message);
          setTrialOk(false);
        }
        setTimeout(() => trialEndRef.current?.scrollIntoView({ block: "end" }), 0);
      });
    } catch (e) {
      setErr(String(e));
    } finally {
      setRunning(false);
    }
  };

  const save = async () => {
    if (!algo || !trialOk || saving) return;
    setSaving(true);
    setErr("");
    try {
      await api.registerAlgorithm(algo);
      setSaved(t("algoStudio.savedMsg", { name: algo.name }));
      refreshSaved();
    } catch (e) {
      setErr(String(e));
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="algo-studio">
      <div className="panel col panel-stack algo-chat">
        <h3>{t("algoStudio.assistant")}</h3>
        <p className="muted">{t("algoStudio.assistantHint")}</p>
        <div className="algo-kind-row">
          <label>{t("algoStudio.kind")}</label>
          <div className="seg">
            {RUNTIME_ALGO_KINDS.map((k) => (
              <button
                type="button"
                key={k}
                className={kind === k ? "active" : ""}
                disabled={authoring}
                onClick={() => setKind(k)}
              >
                {t(`algoStudio.kind_${k}`)}
              </button>
            ))}
          </div>
        </div>
        <div className="algo-chat-log">
          {messages.length === 0 && <div className="muted">{t("algoStudio.chatEmpty")}</div>}
          {messages.map((m, i) => (
            <div key={i} className={"chat-msg chat-" + m.role}>
              <span className="chat-role">{m.role === "user" ? t("algoStudio.you") : t("algoStudio.assistantShort")}</span>
              <span className="chat-text">{m.text}</span>
            </div>
          ))}
        </div>
        <textarea
          rows={3}
          value={input}
          onChange={(e) => setInput(e.target.value)}
          placeholder={t("algoStudio.chatPlaceholder")}
          onKeyDown={(e) => {
            if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) author();
          }}
        />
        <div className="panel-actions" style={{ marginTop: 8 }}>
          <button type="button" className="btn btn-primary" disabled={authoring || !input.trim()} onClick={author}>
            {authoring ? t("algoStudio.authoring") : messages.length ? t("algoStudio.refine") : t("algoStudio.generate")}
          </button>
        </div>

        <div className="algo-saved">
          <h4>{t("algoStudio.savedTitle")}</h4>
          {savedAlgos.length === 0 ? (
            <div className="muted">{t("algoStudio.noSaved")}</div>
          ) : (
            <div className="algo-saved-list">
              {savedAlgos.map((a) => (
                <div key={a.name} className="algo-saved-item">
                  <button type="button" className="algo-saved-load" title={a.description || a.name} onClick={() => loadSaved(a)}>
                    <span className="algo-saved-name">{a.name}</span>
                    <span className="muted algo-saved-desc">{a.description}</span>
                  </button>
                  <button type="button" className="btn-mini" onClick={() => removeSaved(a.name)}>
                    {t("common.remove")}
                  </button>
                </div>
              ))}
            </div>
          )}
        </div>
      </div>

      <div className="panel col panel-stack algo-editor">
        <div className="algo-editor-head">
          <h3>{t("algoStudio.editorTitle")}</h3>
          {algo && <span className="muted">{algo.kind} · {algo.name}</span>}
        </div>

        {!hasCode ? (
          <div className="designer-empty">{t("algoStudio.editorEmpty")}</div>
        ) : (
          <>
            <CodeEditor value={code} onChange={onEditCode} ariaLabel="algorithm JSON" minRows={12} />
            {parseError ? <div className="err">{t("algoStudio.invalidJson")}: {parseError}</div> : null}

            <div className="algo-run-row">
              <label>{t("algoStudio.trialTask")}</label>
              <textarea rows={2} value={task} onChange={(e) => setTask(e.target.value)} />
              <div className="panel-actions" style={{ marginTop: 8 }}>
                <button type="button" className="btn" disabled={!algo || running} onClick={runTrial}>
                  {running ? t("algoStudio.running") : t("algoStudio.runTrial")}
                </button>
                <button
                  type="button"
                  className="btn btn-primary"
                  disabled={!algo || !trialOk || saving}
                  title={trialOk ? t("algoStudio.saveTitleReady") : t("algoStudio.saveTitleLocked")}
                  onClick={save}
                >
                  {saving ? t("common.loading") : t("algoStudio.saveHotReload")}
                </button>
                {trialOk && <span className="trial-ok">✓ {t("algoStudio.trialPassed")}</span>}
              </div>
            </div>

            {trial.length > 0 && (
              <div className="algo-trial-log">
                {trial.map((l, i) => (
                  <div key={i} className={"trial-line trial-" + l.kind}>
                    <span className="trial-kind">{l.kind}</span>
                    <span className="trial-text">{l.text}</span>
                  </div>
                ))}
                <div ref={trialEndRef} />
              </div>
            )}

            {saved && <div className="muted" style={{ marginTop: 8 }}>{saved}</div>}
            {err && <div className="err">{err}</div>}
          </>
        )}
      </div>
    </div>
  );
}
