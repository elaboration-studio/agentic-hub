// Resources panel: a three-pane view for Skills, the always-available Tools
// preflight, and local Sessions. Skills appears only when its source is enabled.

import { useEffect, useRef, useState } from "react";
import { useManagerStore } from "@/state/manager";
import { ResourcesRail } from "./ResourcesRail";
import {
  getDefaultResourcePane,
  resolveInitialLoadedResourcePane,
  resolveResourcePane,
  type ResourcePane,
} from "./resourcePanes";
import { ToolsPage } from "./ToolsPage";
import { SessionsPage } from "./SessionsPage";
import { SkillsPage } from "@/components/skills/SkillsPage";

export function ResourcesPage() {
  const skillsEnabled = useManagerStore((s) => s.data?.settings.skills.enabled);
  const [pane, setPane] = useState<ResourcePane>(() =>
    getDefaultResourcePane(skillsEnabled === true),
  );
  const hasResolvedInitialAvailability = useRef(false);
  const hasUserSelectedPane = useRef(false);

  useEffect(() => {
    if (skillsEnabled === undefined) {
      return;
    }

    setPane((current) => {
      if (!hasResolvedInitialAvailability.current) {
        hasResolvedInitialAvailability.current = true;
        return resolveInitialLoadedResourcePane(
          current,
          skillsEnabled,
          hasUserSelectedPane.current,
        );
      }

      return resolveResourcePane(current, skillsEnabled);
    });
  }, [skillsEnabled]);

  const selectPane = (nextPane: ResourcePane) => {
    hasUserSelectedPane.current = true;
    setPane(nextPane);
  };

  // Skills can be disabled while it is the selected pane; fall back to Tools so
  // the panel never shows an unavailable view.
  const active = resolveResourcePane(pane, skillsEnabled === true);

  return (
    <div className="flex flex-1 gap-5">
      <ResourcesRail
        pane={active}
        onSelect={selectPane}
        skillsEnabled={skillsEnabled === true}
      />
      <div className="flex min-w-0 flex-1 flex-col">
        {active === "tools" && <ToolsPage />}
        {active === "skills" && <SkillsPage />}
        {active === "sessions" && <SessionsPage />}
      </div>
    </div>
  );
}
