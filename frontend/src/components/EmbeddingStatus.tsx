import { useEffect, useState } from "react";
import { api } from "../api";

type Status = Awaited<ReturnType<typeof api.embeddingStatus>>;

export function EmbeddingStatus() {
  const [s, setS] = useState<Status | null>(null);
  const [probing, setProbing] = useState(false);

  useEffect(() => {
    api.embeddingStatus(false).then(setS).catch(() => {});
  }, []);

  const probe = async () => {
    setProbing(true);
    try {
      setS(await api.embeddingStatus(true));
    } finally {
      setProbing(false);
    }
  };

  if (!s) return null;
  const probeOk = s.probe?.ok;
  const cls = !s.api_configured ? "off" : probeOk === false ? "warn" : probeOk ? "ok" : "on";

  return (
    <div className={"embed-status " + cls}>
      <div className="es-head">
        <span className="dot" />
        <span className="es-title">Embeddings</span>
        <span className="spacer" />
        <button className="btn" onClick={probe} disabled={probing || !s.api_configured}>
          {probing ? "Probing…" : "Test"}
        </button>
      </div>
      <div className="es-body">
        <span className="es-model">{s.model}</span>
        {s.api_configured ? (
          <span className="muted" title={s.base_url}>{s.base_url}</span>
        ) : (
          <span className="muted">local fallback (set HARNESSLAB_EMBED_BASE_URL)</span>
        )}
        {s.probe ? (
          <span className={"es-probe " + (probeOk ? "ok" : "warn")}>
            {probeOk ? `✓ live · ${s.probe.dim}d` : `✗ ${s.probe.note || s.probe.reason}`}
          </span>
        ) : null}
      </div>
    </div>
  );
}
