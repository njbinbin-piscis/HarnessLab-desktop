import { useEffect, useState } from "react";
import { api, ComponentSpec } from "../api";
import { useT } from "../i18n/context";
import { useDesigner } from "./store";

const KIND_ORDER = ["loop", "compaction", "memory", "context", "coordinator", "scorer"];
const KIND_LABEL: Record<string, string> = {
  loop: "Loops",
  compaction: "Compaction",
  memory: "Memory",
  context: "Context",
  coordinator: "Coordinators",
  scorer: "Scorers",
  tool: "Tools",
};

function startDrag(kind: string, name: string) {
  return (e: React.DragEvent) => {
    e.dataTransfer.setData("application/harnesslab", JSON.stringify({ kind, name }));
    e.dataTransfer.effectAllowed = "copy";
  };
}

export function Palette() {
  const t = useT();
  const catalog = useDesigner((s) => s.catalog);
  const [tools, setTools] = useState<{ name: string; description: string }[]>([]);
  const [q, setQ] = useState("");

  useEffect(() => {
    api.tools().then((r) => setTools(r.tools)).catch(() => {});
  }, []);

  const match = (s: string) => s.toLowerCase().includes(q.toLowerCase());
  const kinds = KIND_ORDER.filter((k) => catalog[k]?.length);

  return (
    <aside className="palette">
      <div className="palette-search">
        <input placeholder={t("palette.search")} value={q} onChange={(e) => setQ(e.target.value)} />
      </div>
      <p className="palette-hint">{t("palette.dragHint")}</p>
      {kinds.map((kind) => {
        const items = (catalog[kind] as ComponentSpec[]).filter((c) => match(c.name) || match(c.description));
        if (!items.length) return null;
        return (
          <div className="palette-group" key={kind}>
            <div className="palette-group-title">{KIND_LABEL[kind] ?? kind}</div>
            {items.map((c) => (
              <div key={c.name} className="palette-item" draggable onDragStart={startDrag(kind, c.name)} title={c.description}>
                <span className={"dot k-" + kind} />
                <span className="palette-item-name">{c.name}</span>
                {c.reference_langs?.length ? <span className="palette-ref">code</span> : null}
              </div>
            ))}
          </div>
        );
      })}
      <div className="palette-group">
        <div className="palette-group-title">{KIND_LABEL.tool}</div>
        {tools.filter((t) => match(t.name) || match(t.description)).map((t) => (
          <div key={t.name} className="palette-item" draggable onDragStart={startDrag("tool", t.name)} title={t.description}>
            <span className="dot k-tool" />
            <span className="palette-item-name">{t.name}</span>
          </div>
        ))}
      </div>
    </aside>
  );
}
