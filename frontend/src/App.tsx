import { useState } from "react";
import { LibraryHub } from "./components/LibraryHub";
import { ProjectWorkspace } from "./components/ProjectWorkspace";
import { VisualDesigner } from "./components/VisualDesigner";
import { ErrorBoundary } from "./components/ErrorBoundary";
import { DesignStudio } from "./components/DesignStudio";
import { ExperimentDashboard } from "./components/ExperimentDashboard";
import { FactoryPanel } from "./components/FactoryPanel";
import { MarketPanel } from "./components/MarketPanel";
import { LanguageSwitch } from "./components/LanguageSwitch";
import { Icon } from "./components/icons";
import { useT } from "./i18n/context";

type Tab = "projects" | "designer" | "design" | "library" | "experiments" | "factory" | "market";

type NavItem = {
  id: Tab;
  labelKey: string;
  blurbKey: string;
  icon: keyof typeof Icon;
};

const NAV: { groupKey: string; items: NavItem[] }[] = [
  {
    groupKey: "nav.groups.build",
    items: [
      { id: "projects", labelKey: "nav.projects", blurbKey: "nav.projectsBlurb", icon: "projects" },
      { id: "design", labelKey: "nav.design", blurbKey: "nav.designBlurb", icon: "design" },
      { id: "designer", labelKey: "nav.designer", blurbKey: "nav.designerBlurb", icon: "design" },
    ],
  },
  {
    groupKey: "nav.groups.library",
    items: [
      { id: "library", labelKey: "nav.library", blurbKey: "nav.libraryBlurb", icon: "library" },
    ],
  },
  {
    groupKey: "nav.groups.evaluate",
    items: [
      { id: "experiments", labelKey: "nav.experiments", blurbKey: "nav.experimentsBlurb", icon: "experiments" },
    ],
  },
  {
    groupKey: "nav.groups.ship",
    items: [
      { id: "factory", labelKey: "nav.factory", blurbKey: "nav.factoryBlurb", icon: "factory" },
      { id: "market", labelKey: "nav.market", blurbKey: "nav.marketBlurb", icon: "market" },
    ],
  },
];

export function App() {
  const t = useT();
  const [tab, setTab] = useState<Tab>("projects");
  const [activeProject, setActiveProject] = useState<string | null>(null);
  const current = NAV.flatMap((g) => g.items).find((i) => i.id === tab)!;

  return (
    <div className="shell">
      <div className="brand">
        <span className="logo">H</span>
        <h1>{t("app.brand")}</h1>
      </div>

      <div className="topbar">
        <span className="crumb">
          {t(current.labelKey)}
          {activeProject && <> · <b>{t("common.project")} {activeProject.slice(0, 8)}</b></>}
        </span>
        <span className="spacer" />
        <LanguageSwitch />
        <span className="env"><span className="dot" />{t("app.env")}</span>
      </div>

      <nav className="sidebar">
        {NAV.map((g) => (
          <div key={g.groupKey} className="group">
            <div className="group-label">{t(g.groupKey)}</div>
            {g.items.map((it) => {
              const Glyph = Icon[it.icon];
              return (
                <button
                  key={it.id}
                  type="button"
                  className={"nav-item" + (tab === it.id ? " active" : "")}
                  onClick={() => setTab(it.id)}
                >
                  <Glyph />
                  {t(it.labelKey)}
                </button>
              );
            })}
          </div>
        ))}
      </nav>

      <main className="content">
        {tab !== "designer" && (
          <div className="content-head">
            <h2>{t(current.labelKey)}</h2>
            <p>{t(current.blurbKey)}</p>
          </div>
        )}

        <div className="content-body">
          {tab === "projects" && (
            <ProjectWorkspace activeProject={activeProject} setActiveProject={setActiveProject} />
          )}
          {tab === "designer" && (
            <ErrorBoundary>
              <VisualDesigner activeProject={activeProject} setActiveProject={setActiveProject} />
            </ErrorBoundary>
          )}
          {tab === "design" && (
            <DesignStudio
              onOpen={(id, dest) => {
                setActiveProject(id);
                setTab(dest === "designer" ? "designer" : "experiments");
              }}
            />
          )}
          {tab === "library" && <LibraryHub />}
          {tab === "experiments" && <ExperimentDashboard activeProject={activeProject} setActiveProject={setActiveProject} />}
          {tab === "factory" && <FactoryPanel activeProject={activeProject} setActiveProject={setActiveProject} />}
          {tab === "market" && <MarketPanel />}
        </div>
      </main>
    </div>
  );
}
