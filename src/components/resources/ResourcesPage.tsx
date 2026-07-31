// Resources panel: a two-pane view whose left rail switches between the Tools
// preflight (always available) and the skills.sh browser (only when that source
// is enabled). Tools are a pre-flight independent of skills.sh, so the panel is
// reachable even when the skills source is off — it simply shows Tools alone.

import { useState } from "react";
import { useManagerStore } from "@/state/manager";
import { ResourcesRail } from "./ResourcesRail";
import {
  getDefaultResourcePane,
  resolveResourcePane,
  type ResourcePane,
} from "./resourcePanes";
import { ToolsPage } from "./ToolsPage";
import { SessionsPage } from "./SessionsPage";
import { SkillsPage } from "@/components/skills/SkillsPage";

export function ResourcesPage() {
  const skillsEnabled = useManagerStore(
    (s) => s.data?.settings.skills.enabled ?? false,
  );
  const [pane, setPane] = useState<ResourcePane>(() =>
    getDefaultResourcePane(skillsEnabled),
  );

  // Skills can be disabled while it is the selected pane; fall back to Tools so
  // the panel never shows an unavailable view.
  const active = resolveResourcePane(pane, skillsEnabled);

  return (
    <div className="flex flex-1 gap-5">
      <ResourcesRail pane={active} onSelect={setPane} skillsEnabled={skillsEnabled} />
      <div className="flex min-w-0 flex-1 flex-col">
        {active === "tools" && <ToolsPage />}
        {active === "skills" && <SkillsPage />}
        {active === "sessions" && <SessionsPage />}
      </div>
    </div>
  );
}
