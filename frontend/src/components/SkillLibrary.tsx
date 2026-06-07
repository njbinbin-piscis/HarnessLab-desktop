import { useEffect, useMemo, useState } from "react";
import { api, ClawHubSkill, InstalledSkill } from "../api";
import { useT } from "../i18n/context";

export function SkillLibrary() {
  const t = useT();
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<ClawHubSkill[]>([]);
  const [installed, setInstalled] = useState<InstalledSkill[]>([]);
  const [selected, setSelected] = useState<string | null>(null);
  const [searching, setSearching] = useState(false);
  const [busy, setBusy] = useState<string | null>(null);
  const [err, setErr] = useState("");

  const refreshInstalled = () =>
    api
      .listSkills()
      .then(setInstalled)
      .catch((e) => setErr(String(e)));

  useEffect(() => {
    refreshInstalled();
    void search("");
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const installedSlugs = useMemo(
    () => new Set(installed.map((s) => s.slug)),
    [installed],
  );

  async function search(q: string) {
    setSearching(true);
    setErr("");
    try {
      const r = await api.clawhubSearch(q, 30);
      setResults(r.items);
    } catch (e) {
      setErr(String(e));
      setResults([]);
    } finally {
      setSearching(false);
    }
  }

  async function install(slug: string, version?: string) {
    setBusy(slug);
    setErr("");
    try {
      await api.clawhubInstall(slug, version);
      await refreshInstalled();
    } catch (e) {
      setErr(String(e));
    } finally {
      setBusy(null);
    }
  }

  async function uninstall(slug: string) {
    setBusy(slug);
    try {
      await api.uninstallSkill(slug);
      await refreshInstalled();
      if (selected === slug) setSelected(null);
    } catch (e) {
      setErr(String(e));
    } finally {
      setBusy(null);
    }
  }

  async function toggle(slug: string, enabled: boolean) {
    try {
      await api.setSkillEnabled(slug, enabled);
      await refreshInstalled();
    } catch (e) {
      setErr(String(e));
    }
  }

  const selectedSkill = useMemo(
    () => installed.find((s) => s.slug === selected) || null,
    [installed, selected],
  );

  return (
    <div className="lib page-fill">
      <aside className="lib-aside">
        <div className="lib-search">
          <input
            placeholder={t("skillLibrary.search")}
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") void search(query);
            }}
          />
        </div>
        {err && <div className="err">{err}</div>}

        <div className="lib-list">
          <div className="lib-group-label">{t("skillLibrary.installed")}</div>
          {installed.length === 0 && (
            <div className="empty">{t("skillLibrary.noInstalled")}</div>
          )}
          {installed.map((s) => {
            const active = selected === s.slug;
            return (
              <div
                key={s.slug}
                className={"lib-item" + (active ? " active" : "")}
                onClick={() => setSelected(s.slug)}
              >
                <span className="name">{s.name}</span>
                <span className="meta">
                  <span className={"badge " + (s.enabled ? "builtin" : "")}>
                    {s.enabled ? t("skillLibrary.enabled") : t("skillLibrary.disabled")}
                  </span>
                </span>
              </div>
            );
          })}

          <div className="lib-group-label" style={{ marginTop: 12 }}>
            {t("skillLibrary.clawhub")}
            {searching ? " …" : ""}
          </div>
          {results.map((r) => {
            const isInstalled = installedSlugs.has(r.slug);
            return (
              <div key={r.slug} className="lib-item" style={{ alignItems: "flex-start" }}>
                <span className="name">
                  {r.name}
                  {r.stars > 0 && <span className="muted"> ★ {r.stars}</span>}
                </span>
                <span className="meta">
                  <button
                    type="button"
                    className="btn-mini"
                    disabled={isInstalled || busy === r.slug}
                    onClick={(e) => {
                      e.stopPropagation();
                      void install(r.slug, r.version);
                    }}
                  >
                    {isInstalled
                      ? t("skillLibrary.installedShort")
                      : busy === r.slug
                        ? "…"
                        : t("skillLibrary.install")}
                  </button>
                </span>
              </div>
            );
          })}
          {!searching && results.length === 0 && (
            <div className="empty">{t("skillLibrary.noMatch", { query })}</div>
          )}
        </div>
      </aside>

      <section className="panel">
        {!selectedSkill && <div className="empty">{t("skillLibrary.emptySelect")}</div>}
        {selectedSkill && (
          <div className="lib-detail-scroll">
            <div className="detail-head">
              <span className="title">{selectedSkill.name}</span>
              <span className="kind">{selectedSkill.slug}</span>
              <span className="badge builtin">v{selectedSkill.version}</span>
            </div>
            <div className="detail-desc">{selectedSkill.description}</div>
            <div className="row" style={{ gap: 8, margin: "12px 0" }}>
              <label className="check-inline">
                <input
                  type="checkbox"
                  checked={selectedSkill.enabled}
                  onChange={(e) => void toggle(selectedSkill.slug, e.target.checked)}
                />
                {t("skillLibrary.injectPrompt")}
              </label>
              <button
                type="button"
                className="btn-mini danger"
                disabled={busy === selectedSkill.slug}
                onClick={() => void uninstall(selectedSkill.slug)}
              >
                {t("skillLibrary.uninstall")}
              </button>
            </div>
            <pre className="code-view">{selectedSkill.content}</pre>
          </div>
        )}
      </section>
    </div>
  );
}
