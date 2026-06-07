import { useEffect, useMemo, useState } from "react";
import { api, ComponentReference, ComponentSpec } from "../api";
import { CodeBlock } from "./CodeBlock";
import { EmbeddingStatus } from "./EmbeddingStatus";

const KIND_ORDER = ["loop", "compaction", "memory", "context", "coordinator", "scorer"];

type Selected = { kind: string; name: string };

export function LibraryBrowser() {
  const [catalog, setCatalog] = useState<Record<string, ComponentSpec[]>>({});
  const [query, setQuery] = useState("");
  const [kindFilter, setKindFilter] = useState<string>("all");
  const [selected, setSelected] = useState<Selected | null>(null);
  const [err, setErr] = useState("");

  const load = () => api.catalog().then(setCatalog).catch((e) => setErr(String(e)));
  useEffect(() => { load(); }, []);

  const counts = useMemo(() => {
    const c: Record<string, number> = {};
    for (const k of KIND_ORDER) c[k] = (catalog[k] || []).length;
    return c;
  }, [catalog]);

  const groups = useMemo(() => {
    const q = query.trim().toLowerCase();
    return KIND_ORDER.filter((k) => kindFilter === "all" || kindFilter === k)
      .map((kind) => {
        const items = (catalog[kind] || []).filter(
          (c) => !q || c.name.toLowerCase().includes(q) || (c.description || "").toLowerCase().includes(q),
        );
        return { kind, items };
      })
      .filter((g) => g.items.length > 0);
  }, [catalog, query, kindFilter]);

  const selectedSpec = useMemo(() => {
    if (!selected) return null;
    return (catalog[selected.kind] || []).find((c) => c.name === selected.name) || null;
  }, [catalog, selected]);

  return (
    <div className="lib">
      <aside className="lib-aside">
        <div className="lib-search">
          <input
            placeholder="Search algorithms…"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
          />
        </div>
        <div className="kind-filter">
          <span className={"chip" + (kindFilter === "all" ? " active" : "")} onClick={() => setKindFilter("all")}>
            all
          </span>
          {KIND_ORDER.map((k) => (
            <span key={k} className={"chip" + (kindFilter === k ? " active" : "")} onClick={() => setKindFilter(k)}>
              {k} {counts[k] ? `· ${counts[k]}` : ""}
            </span>
          ))}
        </div>
        <button className="ghost" style={{ width: "100%", marginBottom: 8 }} onClick={() => api.reloadLibrary().then(load)}>
          ↻ Hot-reload plugins
        </button>
        <EmbeddingStatus />
        {err && <div className="err">{err}</div>}

        <div className="lib-list">
          {groups.map((g) => (
            <div key={g.kind}>
              <div className="lib-group-label">{g.kind}</div>
              {g.items.map((c) => {
                const active = selected?.kind === g.kind && selected?.name === c.name;
                return (
                  <div
                    key={c.name}
                    className={"lib-item" + (active ? " active" : "")}
                    onClick={() => setSelected({ kind: g.kind, name: c.name })}
                  >
                    <span className="name">{c.name}</span>
                    <span className="meta">
                      {c.reference_langs && c.reference_langs.length > 0 && (
                        <span className="dot-langs">{c.reference_langs.map((l) => l[0].toUpperCase()).join("")}</span>
                      )}
                      <span className={"badge " + (c.source || "builtin")}>{c.source || "builtin"}</span>
                    </span>
                  </div>
                );
              })}
            </div>
          ))}
          {groups.length === 0 && <div className="empty">No components match “{query}”.</div>}
        </div>
      </aside>

      <section className="panel">
        {!selectedSpec && <div className="empty">Select an algorithm to view its details and reference code.</div>}
        {selectedSpec && selected && <ComponentDetail kind={selected.kind} spec={selectedSpec} />}
      </section>
    </div>
  );
}

function ComponentDetail({ kind, spec }: { kind: string; spec: ComponentSpec }) {
  const [ref, setRef] = useState<ComponentReference | null>(null);
  const [lang, setLang] = useState<string>("");
  const [loading, setLoading] = useState(false);

  useEffect(() => {
    setRef(null);
    setLang("");
    if (!spec.reference_langs || spec.reference_langs.length === 0) return;
    setLoading(true);
    api.componentReference(kind, spec.name)
      .then((r) => { setRef(r); setLang(r.languages[0] || ""); })
      .finally(() => setLoading(false));
  }, [kind, spec.name]);

  const params = Object.entries(spec.params_schema || {});

  return (
    <div>
      <div className="detail-head">
        <span className="title">{spec.name}</span>
        <span className="kind">{kind}</span>
        <span className={"badge " + (spec.source || "builtin")}>{spec.source || "builtin"}</span>
      </div>
      <div className="detail-desc">{spec.description || "No description."}</div>

      {params.length > 0 && (
        <>
          <h3>Parameters</h3>
          <table className="params-table">
            <thead><tr><th>name</th><th>schema</th></tr></thead>
            <tbody>
              {params.map(([k, v]) => (
                <tr key={k}>
                  <td>{k}</td>
                  <td className="muted">{JSON.stringify(v)}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </>
      )}

      <h3>Reference implementation</h3>
      {loading && <div className="muted">Loading reference code…</div>}
      {!loading && !ref && (
        <div className="muted">
          No curated reference snippet yet for this component. It still runs in the engine and can be
          generated via the Factory.
        </div>
      )}
      {ref && (
        <>
          <div className="seg" style={{ marginBottom: 10 }}>
            {ref.languages.map((l) => (
              <button key={l} className={lang === l ? "active" : ""} onClick={() => setLang(l)}>
                {l}
              </button>
            ))}
          </div>
          {lang && <CodeBlock code={ref.snippets[lang]} title={`${spec.name}.${lang}`} />}
          <div className="muted" style={{ marginTop: 8 }}>
            Copy this into your project, or let the Factory emit a full codebase wired with this component.
          </div>
        </>
      )}
    </div>
  );
}
