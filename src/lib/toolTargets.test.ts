import { describe, expect, it } from "vitest";
import type { ToolSettings } from "@/types";
import { getToolProjectionTargets } from "./toolTargets";

function tool(overrides: Partial<ToolSettings> = {}): ToolSettings {
  return {
    enabled: true,
    skillsPath: "/home/.codex/skills",
    agentsPath: "/home/.codex/agents",
    rulesPath: "/home/.codex/agentic-rules",
    instructionsPath: "/home/.codex/AGENTS.md",
    hooksEnabled: true,
    hooksFile: "/home/.codex/hooks.json",
    hooksDir: null,
    commandsPath: "/home/.codex/prompts",
    ...overrides,
  };
}

describe("getToolProjectionTargets", () => {
  it("returns five capability rows in order", () => {
    const rows = getToolProjectionTargets("codex", tool());
    expect(rows.map((r) => r.kind)).toEqual(["skill", "agent", "rule", "hook", "command"]);
  });

  it("uses AGENTS.md for kiro rules", () => {
    const ts = tool({
      skillsPath: "/home/.kiro/skills",
      agentsPath: "/home/.kiro/agents",
      rulesPath: "/home/.kiro/steering",
      instructionsPath: "/home/.kiro/steering/AGENTS.md",
      hooksFile: null,
      hooksDir: "/home/.kiro/hooks",
      commandsPath: null,
    });
    const rule = getToolProjectionTargets("kiro", ts).find((r) => r.kind === "rule");
    expect(rule?.path).toBe("/home/.kiro/steering/AGENTS.md");
    expect(rule?.detail).toContain("always included");
  });

  it("uses rules dir for cursor rules", () => {
    const ts = tool({
      instructionsPath: null,
      rulesPath: "/home/.cursor/rules",
    });
    const rule = getToolProjectionTargets("cursor", ts).find((r) => r.kind === "rule");
    expect(rule?.path).toBe("/home/.cursor/rules");
  });

  it("marks antigravity agents and commands unsupported", () => {
    const ts = tool({
      skillsPath: "/home/.gemini/config/skills",
      agentsPath: "/home/.gemini/antigravity/agents",
      rulesPath: "/home/.gemini/antigravity/rules",
      instructionsPath: "/home/.gemini/AGENTS.md",
      hooksFile: "/home/.gemini/config/hooks.json",
      commandsPath: null,
    });
    const rows = getToolProjectionTargets("antigravity", ts);
    expect(rows.find((r) => r.kind === "agent")?.unsupported).toBe(true);
    expect(rows.find((r) => r.kind === "command")?.unsupported).toBe(true);
    expect(rows.find((r) => r.kind === "rule")?.path).toBe("/home/.gemini/AGENTS.md");
  });

  it("uses hooks dir for kiro and copilot", () => {
    const kiro = tool({
      hooksFile: null,
      hooksDir: "/home/.kiro/hooks",
      commandsPath: null,
    });
    expect(getToolProjectionTargets("kiro", kiro).find((r) => r.kind === "hook")?.path).toBe(
      "/home/.kiro/hooks",
    );

    const copilot = tool({
      hooksFile: null,
      hooksDir: "/home/.copilot/hooks",
      rulesPath: "/home/.copilot/instructions",
      instructionsPath: "/home/.copilot/copilot-instructions.md",
      commandsPath: null,
    });
    expect(getToolProjectionTargets("copilot", copilot).find((r) => r.kind === "hook")?.path).toBe(
      "/home/.copilot/hooks",
    );
  });
});
