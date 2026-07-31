export const RESOURCE_PANES = ["skills", "tools", "sessions"] as const;

export type ResourcePane = (typeof RESOURCE_PANES)[number];

export function getDefaultResourcePane(skillsEnabled: boolean): ResourcePane {
  return skillsEnabled ? "skills" : "tools";
}

export function resolveResourcePane(
  pane: ResourcePane,
  skillsEnabled: boolean,
): ResourcePane {
  return pane === "skills" && !skillsEnabled ? "tools" : pane;
}

export function resolveInitialLoadedResourcePane(
  pane: ResourcePane,
  skillsEnabled: boolean,
  hasUserSelectedPane: boolean,
): ResourcePane {
  if (!hasUserSelectedPane) {
    return getDefaultResourcePane(skillsEnabled);
  }

  return resolveResourcePane(pane, skillsEnabled);
}
