import type { CapabilityKind, ToolId, ToolSettings } from "@/types";
import { KIND_LABEL } from "@/shared";

export interface ToolProjectionTarget {
  kind: CapabilityKind;
  label: string;
  path: string | null;
  detail?: string;
  unsupported?: boolean;
}

const NO_PROJECTION: Partial<Record<ToolId, ReadonlySet<CapabilityKind>>> = {
  openclaw: new Set(["hook", "command"]),
  kiro: new Set(["command"]),
  copilot: new Set(["command"]),
  antigravity: new Set(["agent", "command"]),
};

function unsupported(kind: CapabilityKind): ToolProjectionTarget {
  return {
    kind,
    label: KIND_LABEL[kind],
    path: null,
    unsupported: true,
  };
}

function skillTarget(ts: ToolSettings): ToolProjectionTarget {
  return { kind: "skill", label: KIND_LABEL.skill, path: ts.skillsPath };
}

function agentTarget(toolId: ToolId, ts: ToolSettings): ToolProjectionTarget {
  if (NO_PROJECTION[toolId]?.has("agent")) return unsupported("agent");
  return {
    kind: "agent",
    label: KIND_LABEL.agent,
    path: ts.agentsPath,
    detail: toolId === "codex" ? "rendered as `.toml`" : undefined,
  };
}

function ruleTarget(toolId: ToolId, ts: ToolSettings): ToolProjectionTarget {
  if (toolId === "cursor") {
    return {
      kind: "rule",
      label: KIND_LABEL.rule,
      path: ts.rulesPath,
      detail: "symlinks under rules dir",
    };
  }
  if (toolId === "grok") {
    return {
      kind: "rule",
      label: KIND_LABEL.rule,
      path: ts.rulesPath,
      detail: "symlinks under rules dir",
    };
  }
  if (toolId === "copilot") {
    return {
      kind: "rule",
      label: KIND_LABEL.rule,
      path: ts.rulesPath,
      detail: ts.instructionsPath
        ? "`.instructions.md` symlinks; global instructions file separate"
        : "`.instructions.md` symlinks",
    };
  }
  if (ts.instructionsPath) {
    let detail = "managed block";
    if (toolId === "kiro") detail = "managed block in steering (always included)";
    if (toolId === "codex" && ts.rulesPath) {
      detail = `managed block; optional mirror at ${ts.rulesPath}`;
    }
    return {
      kind: "rule",
      label: KIND_LABEL.rule,
      path: ts.instructionsPath,
      detail,
    };
  }
  return { kind: "rule", label: KIND_LABEL.rule, path: ts.rulesPath };
}

function hookTarget(toolId: ToolId, ts: ToolSettings): ToolProjectionTarget {
  if (NO_PROJECTION[toolId]?.has("hook")) return unsupported("hook");
  const path = ts.hooksDir ?? ts.hooksFile;
  if (!path) return unsupported("hook");
  let detail = ts.hooksDir ? "one JSON file per hook id" : "shared hooks JSON file";
  if (!ts.hooksEnabled) detail = `${detail}; projection off in settings`;
  return { kind: "hook", label: KIND_LABEL.hook, path, detail };
}

function commandTarget(toolId: ToolId, ts: ToolSettings): ToolProjectionTarget {
  if (NO_PROJECTION[toolId]?.has("command")) return unsupported("command");
  if (!ts.commandsPath) return unsupported("command");
  return {
    kind: "command",
    label: KIND_LABEL.command,
    path: ts.commandsPath,
    detail: toolId === "codex" ? "Codex prompts dir" : undefined,
  };
}

/** Resolved global projection targets for one tool — one row per capability kind. */
export function getToolProjectionTargets(
  toolId: ToolId,
  ts: ToolSettings,
): ToolProjectionTarget[] {
  return [
    skillTarget(ts),
    agentTarget(toolId, ts),
    ruleTarget(toolId, ts),
    hookTarget(toolId, ts),
    commandTarget(toolId, ts),
  ];
}
