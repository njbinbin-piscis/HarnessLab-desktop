import { useEffect, useMemo, useState } from "react";
import { api, ToolSpec } from "../api";
import { useT } from "../i18n/context";

const PLATFORM_ORDER = ["any", "linux", "windows", "macos"];

export function ToolLibrary() {
  const t = useT();
  const [tools, setTools] = useState<ToolSpec[]>([]);
  const [query, setQuery] = useState("");
  const [platformFilter, setPlatformFilter] = useState<string>("all");
  const [selected, setSelected] = useState<string | null>(null);
  const [err, setErr] = useState("");

  useEffect(() => {
    api
      .tools()
      .then((r) => setTools(r.tools))
      .catch((e) => setErr(String(e)));
  }, []);

  const platformLabel = (p: string) => {
    switch (p) {
      case "any":
        return t("toolLibrary.platformAny");
      case "linux":
        return t("toolLibrary.platformLinux");
      case "windows":
        return t("toolLibrary.platformWindows");
      case "macos":
        return t("toolLibrary.platformMacos");
      default:
        return p;
    }
  };

  const counts = useMemo(() => {
    const c: Record<string, number> = {};
    for (const tool of tools) c[tool.platform] = (c[tool.platform] || 0) + 1;
    return c;
  }, [tools]);

  const groups = useMemo(() => {
    const q = query.trim().toLowerCase();
    const platforms = Array.from(
      new Set([...PLATFORM_ORDER, ...tools.map((tl) => tl.platform)]),
    );
    return platforms
      .filter((p) => platformFilter === "all" || platformFilter === p)
      .map((platform) => {
        const items = tools.filter(
          (tl) =>
            tl.platform === platform &&
            (!q ||
              tl.name.toLowerCase().includes(q) ||
              (tl.description || "").toLowerCase().includes(q)),
        );
        return { platform, items };
      })
      .filter((g) => g.items.length > 0);
  }, [tools, query, platformFilter]);

  const selectedSpec = useMemo(
    () => tools.find((tl) => tl.name === selected) || null,
    [tools, selected],
  );

  return (
    <div className="lib page-fill">
      <aside className="lib-aside">
        <div className="lib-search">
          <input
            placeholder={t("toolLibrary.search")}
            value={query}
            onChange={(e) => setQuery(e.target.value)}
          />
        </div>
        <div className="kind-filter">
          <span
            className={"chip" + (platformFilter === "all" ? " active" : "")}
            onClick={() => setPlatformFilter("all")}
          >
            {t("toolLibrary.all")}
          </span>
          {PLATFORM_ORDER.filter((p) => counts[p]).map((p) => (
            <span
              key={p}
              className={"chip" + (platformFilter === p ? " active" : "")}
              onClick={() => setPlatformFilter(p)}
            >
              {platformLabel(p)} · {counts[p]}
            </span>
          ))}
        </div>
        {err && <div className="err">{err}</div>}

        <div className="lib-list">
          {groups.map((g) => (
            <div key={g.platform}>
              <div className="lib-group-label">{platformLabel(g.platform)}</div>
              {g.items.map((tl) => {
                const active = selected === tl.name;
                return (
                  <div
                    key={tl.name}
                    className={"lib-item" + (active ? " active" : "")}
                    onClick={() => setSelected(tl.name)}
                  >
                    <span className="name">{tl.name}</span>
                    <span className="meta">
                      <span className="badge builtin">{tl.category}</span>
                    </span>
                  </div>
                );
              })}
            </div>
          ))}
          {groups.length === 0 && (
            <div className="empty">{t("toolLibrary.noMatch", { query })}</div>
          )}
        </div>
      </aside>

      <section className="panel">
        {!selectedSpec && <div className="empty">{t("toolLibrary.emptySelect")}</div>}
        {selectedSpec && (
          <div className="lib-detail-scroll">
            <div className="detail-head">
              <span className="title">{selectedSpec.name}</span>
              <span className="kind">{selectedSpec.category}</span>
              <span className="badge builtin">{platformLabel(selectedSpec.platform)}</span>
            </div>
            <div className="detail-desc">{selectedSpec.description}</div>
            <table className="params-table" style={{ marginTop: 12 }}>
              <tbody>
                <tr>
                  <td>{t("toolLibrary.category")}</td>
                  <td className="muted">{selectedSpec.category}</td>
                </tr>
                <tr>
                  <td>{t("toolLibrary.platform")}</td>
                  <td className="muted">{platformLabel(selectedSpec.platform)}</td>
                </tr>
              </tbody>
            </table>
          </div>
        )}
      </section>
    </div>
  );
}
