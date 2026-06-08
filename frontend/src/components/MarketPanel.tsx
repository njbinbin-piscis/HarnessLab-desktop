import { useEffect, useState } from "react";
import { api } from "../api";
import { useT } from "../i18n/context";

type CatalogEntry = Awaited<ReturnType<typeof api.marketCatalog>>[number];

export function MarketPanel() {
  const t = useT();
  const [installed, setInstalled] = useState<any[]>([]);
  const [source, setSource] = useState("");
  const [registries, setRegistries] = useState<string[]>([]);
  const [registryDraft, setRegistryDraft] = useState("");
  const [catalog, setCatalog] = useState<CatalogEntry[]>([]);
  const [search, setSearch] = useState("");
  const [inspectResult, setInspectResult] = useState<any | null>(null);
  const [msg, setMsg] = useState("");
  const [err, setErr] = useState("");
  const [tab, setTab] = useState<"catalog" | "advanced">("catalog");

  const refresh = () => api.marketInstalled().then(setInstalled);

  const loadCatalog = async (query?: string) => {
    try {
      setCatalog(await api.marketCatalog(query ?? search));
      setErr("");
    } catch (e) {
      setErr(String(e));
    }
  };

  useEffect(() => {
    refresh();
    api.marketSampleSource().then(setSource).catch(() => {
      setSource("registry:cool-loop@0.1.0");
    });
    api.marketGetRegistries().then((r) => {
      setRegistries(r);
      setRegistryDraft(r.join("\n"));
    }).then(() => loadCatalog());
  }, []);

  const saveRegistries = async () => {
    const urls = registryDraft.split("\n").map((s) => s.trim()).filter(Boolean);
    try {
      await api.marketSetRegistries(urls);
      setRegistries(urls);
      setMsg(t("market.registriesSaved"));
      await loadCatalog();
    } catch (e) {
      setErr(String(e));
    }
  };

  const inspect = async () => {
    setErr(""); setMsg(""); setInspectResult(null);
    try { setInspectResult(await api.marketInspect(source)); }
    catch (e) { setErr(String(e)); }
  };

  const install = async (spec: string, consent = false) => {
    setErr(""); setMsg("");
    try {
      const info = await api.marketInstall(spec, consent);
      setMsg(t("market.installedMsg", { name: info.name, version: info.version, wired: info.wired }));
      refresh();
    } catch (e: any) {
      const m = String(e);
      if (m.includes("needs_consent") || m.includes("capabilities")) {
        if (confirm(t("market.consentConfirm"))) {
          install(spec, true);
          return;
        }
      }
      setErr(m);
    }
  };

  const installFromCatalog = (entry: CatalogEntry, version?: string) => {
    const ver = version ?? entry.latest ?? entry.versions[entry.versions.length - 1];
    const spec = `registry:${entry.name}@${ver}`;
    setSource(spec);
    install(spec, false);
  };

  return (
    <div className="panel panel-stack page-fill">
      <h2>{t("market.title")}</h2>
      <p className="muted panel-intro">{t("market.intro")}</p>

      <div className="seg panel-intro" style={{ marginBottom: 12 }}>
        <button type="button" className={tab === "catalog" ? "active" : ""} onClick={() => setTab("catalog")}>{t("market.catalog")}</button>
        <button type="button" className={tab === "advanced" ? "active" : ""} onClick={() => setTab("advanced")}>{t("market.advanced")}</button>
      </div>

      <div className="panel-body-scroll">
        {tab === "catalog" && (
          <>
            <h3>{t("market.registryIndexes")}</h3>
            <textarea
              rows={3}
              value={registryDraft}
              onChange={(e) => setRegistryDraft(e.target.value)}
              placeholder={t("market.registryPlaceholder")}
              style={{ width: "100%", fontFamily: "monospace", fontSize: 12 }}
            />
            <div className="panel-actions" style={{ marginTop: 8 }}>
              <button type="button" className="btn" onClick={saveRegistries}>{t("market.saveRegistries")}</button>
              <button type="button" className="btn" onClick={() => loadCatalog()}>{t("market.refreshCatalog")}</button>
            </div>
            {registries.length === 0 && (
              <p className="muted" style={{ marginTop: 8 }}>{t("market.noRegistries")}</p>
            )}

            <h3 style={{ marginTop: 16 }}>{t("market.availablePackages")}</h3>
            <div className="control-row" style={{ marginBottom: 8 }}>
              <input
                style={{ flex: 1 }}
                value={search}
                placeholder={t("market.searchPlaceholder")}
                onChange={(e) => setSearch(e.target.value)}
                onKeyDown={(e) => { if (e.key === "Enter") loadCatalog(); }}
              />
              <button type="button" className="btn" onClick={() => loadCatalog()}>{t("market.search")}</button>
              {search && (
                <button type="button" className="btn" onClick={() => { setSearch(""); loadCatalog(""); }}>{t("common.dash")}</button>
              )}
            </div>
            <div className="table-scroll">
              <table>
                <thead>
                  <tr><th>{t("market.registry")}</th><th>{t("common.name")}</th><th>{t("market.latest")}</th><th>{t("market.type")}</th><th>{t("common.description")}</th><th></th></tr>
                </thead>
                <tbody>
                  {catalog.map((e) => (
                    <tr key={`${e.registry}:${e.name}`}>
                      <td>{e.registry}</td>
                      <td>{e.name}</td>
                      <td>{e.latest ?? t("common.dash")}</td>
                      <td>{e.type ?? e.category ?? t("common.dash")}</td>
                      <td>{e.description ?? t("common.dash")}</td>
                      <td>
                        <button type="button" className="btn btn-sm btn-primary" onClick={() => installFromCatalog(e)}>{t("common.install")}</button>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
            {catalog.length === 0 && registries.length > 0 && (
              <p className="muted">{t("market.noPackages")}</p>
            )}
          </>
        )}

        {tab === "advanced" && (
          <>
            <label>{t("market.sourceSpec")}</label>
            <input value={source} onChange={(e) => setSource(e.target.value)}
              placeholder={t("market.sourcePlaceholder")} />
            <div className="panel-actions">
              <button type="button" className="btn" onClick={inspect}>{t("common.inspect")}</button>
              <button type="button" className="btn btn-primary" onClick={() => install(source, false)}>{t("common.install")}</button>
            </div>
          </>
        )}

        {msg && <div className="muted" style={{ marginTop: 8 }}>{msg}</div>}
        {err && <div className="err">{err}</div>}

        {inspectResult && (
          <div style={{ marginTop: 12 }}>
            <h3>{t("market.manifest")}</h3>
            <div className="code code-in-panel">{JSON.stringify(inspectResult.manifest, null, 2)}</div>
            {inspectResult.problems?.length > 0 && (
              <div className="err">problems: {inspectResult.problems.join("; ")}</div>
            )}
          </div>
        )}

        <h3>{t("market.installed")}</h3>
        <div className="table-scroll">
          <table>
            <thead><tr><th>{t("common.name")}</th><th>version</th><th>{t("market.type")}</th><th>{t("market.category")}</th><th></th></tr></thead>
            <tbody>
              {installed.map((p) => (
                <tr key={p.name}>
                  <td>{p.name}</td><td>{p.version}</td><td>{p.type}</td><td>{p.category}</td>
                  <td><button type="button" className="btn btn-sm" onClick={() => api.marketUninstall(p.name).then(refresh)}>{t("common.remove")}</button></td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </div>
    </div>
  );
}
