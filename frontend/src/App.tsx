import { useState } from "react";
import { LibraryBrowser } from "./components/LibraryBrowser";
import { ProjectWorkspace } from "./components/ProjectWorkspace";
import { VisualDesigner } from "./components/VisualDesigner";
import { ErrorBoundary } from "./components/ErrorBoundary";
import { DesignStudio } from "./components/DesignStudio";
import { ExperimentDashboard } from "./components/ExperimentDashboard";
import { FactoryPanel } from "./components/FactoryPanel";
import { MarketPanel } from "./components/MarketPanel";
import { Icon } from "./components/icons";

type Tab = "projects" | "designer" | "design" | "library" | "experiments" | "factory" | "market";

type NavItem = { id: Tab; label: string; icon: keyof typeof Icon; blurb: string };

const NAV: { group: string; items: NavItem[] }[] = [
  {
    group: "Build",
    items: [
      { id: "projects", label: "Projects", icon: "projects", blurb: "Compose agents, topologies and algorithm slots." },
      { id: "designer", label: "Visual Designer", icon: "design", blurb: "Wire agents, slots and tools on a live canvas; edits are validated and round-trip to runnable code." },
      { id: "design", label: "Design Assistant", icon: "design", blurb: "Describe a system; get a project scaffold and author new components." },
    ],
  },
  {
    group: "Library",
    items: [
      { id: "library", label: "Algorithm Library", icon: "library", blurb: "Browse components and copy reference code in Python, TypeScript and Rust." },
    ],
  },
  {
    group: "Evaluate",
    items: [
      { id: "experiments", label: "Experiments", icon: "experiments", blurb: "Run evaluations, track cost/latency and compare on the leaderboard." },
    ],
  },
  {
    group: "Ship",
    items: [
      { id: "factory", label: "Factory · Codegen", icon: "factory", blurb: "Generate a standalone, runnable codebase from a project." },
      { id: "market", label: "Market", icon: "market", blurb: "Install and publish decentralized algorithm & dataset packages." },
    ],
  },
];

const ALL = NAV.flatMap((g) => g.items);

export function App() {
  const [tab, setTab] = useState<Tab>("projects");
  const [activeProject, setActiveProject] = useState<string | null>(null);
  const current = ALL.find((i) => i.id === tab)!;

  return (
    <div className="shell">
      <div className="brand">
        <span className="logo">H</span>
        <h1>HarnessLab</h1>
      </div>

      <div className="topbar">
        <span className="crumb">
          {current.label}
          {activeProject && <> · <b>project {activeProject.slice(0, 8)}</b></>}
        </span>
        <span className="spacer" />
        <span className="env"><span className="dot" />Python engine · hot-reload</span>
      </div>

      <nav className="sidebar">
        {NAV.map((g) => (
          <div key={g.group} className="group">
            <div className="group-label">{g.group}</div>
            {g.items.map((it) => {
              const Glyph = Icon[it.icon];
              return (
                <button
                  key={it.id}
                  className={"nav-item" + (tab === it.id ? " active" : "")}
                  onClick={() => setTab(it.id)}
                >
                  <Glyph />
                  {it.label}
                </button>
              );
            })}
          </div>
        ))}
      </nav>

      <main className="content">
        <div className="content-head">
          <h2>{current.label}</h2>
          <p>{current.blurb}</p>
        </div>

        {tab === "projects" && (
          <ProjectWorkspace activeProject={activeProject} setActiveProject={setActiveProject} />
        )}
        {tab === "designer" && (
          <ErrorBoundary>
            <VisualDesigner activeProject={activeProject} setActiveProject={setActiveProject} />
          </ErrorBoundary>
        )}
        {tab === "design" && <DesignStudio onSaved={(id) => { setActiveProject(id); setTab("experiments"); }} />}
        {tab === "library" && <LibraryBrowser />}
        {tab === "experiments" && <ExperimentDashboard activeProject={activeProject} setActiveProject={setActiveProject} />}
        {tab === "factory" && <FactoryPanel activeProject={activeProject} setActiveProject={setActiveProject} />}
        {tab === "market" && <MarketPanel />}
      </main>
    </div>
  );
}
