import { useState } from "react";
import { useT } from "../i18n/context";
import { AgentLibrary } from "./AgentLibrary";
import { LibraryBrowser } from "./LibraryBrowser";
import { ToolLibrary } from "./ToolLibrary";
import { SkillLibrary } from "./SkillLibrary";
import { AlgorithmStudio } from "./AlgorithmStudio";

type LibTab = "agents" | "algorithms" | "algoStudio" | "tools" | "skills";

export function LibraryHub() {
  const t = useT();
  const [tab, setTab] = useState<LibTab>("agents");

  return (
    <div className="page-fill lib-hub">
      <div className="seg panel-intro" style={{ marginBottom: 0 }}>
        <button type="button" className={tab === "agents" ? "active" : ""} onClick={() => setTab("agents")}>
          {t("agentLibrary.tab")}
        </button>
        <button type="button" className={tab === "algorithms" ? "active" : ""} onClick={() => setTab("algorithms")}>
          {t("library.tab")}
        </button>
        <button type="button" className={tab === "algoStudio" ? "active" : ""} onClick={() => setTab("algoStudio")}>
          {t("algoStudio.tab")}
        </button>
        <button type="button" className={tab === "tools" ? "active" : ""} onClick={() => setTab("tools")}>
          {t("toolLibrary.tab")}
        </button>
        <button type="button" className={tab === "skills" ? "active" : ""} onClick={() => setTab("skills")}>
          {t("skillLibrary.tab")}
        </button>
      </div>
      {tab === "agents" && <AgentLibrary />}
      {tab === "algorithms" && <LibraryBrowser />}
      {tab === "algoStudio" && <AlgorithmStudio />}
      {tab === "tools" && <ToolLibrary />}
      {tab === "skills" && <SkillLibrary />}
    </div>
  );
}
