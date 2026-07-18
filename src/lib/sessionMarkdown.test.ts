import { describe, expect, it } from "vitest";

import { sessionRoleLabel, sessionToMarkdown } from "./sessionMarkdown";
import type { SessionMessage, SessionSummary } from "@/types";

const session: SessionSummary = {
  sessionKey: "claude:abc-123",
  tool: "claude",
  title: "Fix the login bug",
  workspace: "~/dev/agentic-hub",
  gitBranch: "fix/login-20260718",
  model: "claude-sonnet-5",
  startedAt: "2026-07-18T10:00:00Z",
  updatedAt: "2026-07-18T11:00:00Z",
  messageCount: 2,
  sourcePath: "/Users/arno/.claude/projects/x/abc-123.jsonl",
};

describe("sessionRoleLabel", () => {
  it("labels user/assistant/system by role", () => {
    expect(sessionRoleLabel({ role: "user", toolName: null })).toBe("User");
    expect(sessionRoleLabel({ role: "assistant", toolName: null })).toBe("Assistant");
    expect(sessionRoleLabel({ role: "system", toolName: null })).toBe("System");
  });

  it("prefers the tool name for tool turns, falling back to 'Tool'", () => {
    expect(sessionRoleLabel({ role: "tool", toolName: "Bash" })).toBe("Bash");
    expect(sessionRoleLabel({ role: "tool", toolName: null })).toBe("Tool");
  });
});

describe("sessionToMarkdown", () => {
  it("renders a title, metadata line, and role-headed turns separated by rules", () => {
    const messages: SessionMessage[] = [
      { role: "user", text: "Why does login fail?", toolName: null, timestamp: "2026-07-18T10:00:00Z" },
      { role: "assistant", text: "Because the token expires early.", toolName: null, timestamp: "2026-07-18T10:01:00Z" },
    ];

    const md = sessionToMarkdown(session, messages);

    expect(md).toBe(
      [
        "# Fix the login bug",
        "",
        "**Tool:** claude · **Workspace:** ~/dev/agentic-hub · **Branch:** fix/login-20260718 · **Model:** claude-sonnet-5 · **Started:** 2026-07-18T10:00:00Z · **Updated:** 2026-07-18T11:00:00Z",
        "",
        "---",
        "",
        "### User — 2026-07-18T10:00:00Z",
        "",
        "Why does login fail?",
        "",
        "---",
        "",
        "### Assistant — 2026-07-18T10:01:00Z",
        "",
        "Because the token expires early.",
      ].join("\n"),
    );
  });

  it("fences tool-turn text as a code block", () => {
    const messages: SessionMessage[] = [
      { role: "tool", text: "$ ls\nfoo.txt", toolName: "Bash", timestamp: null },
    ];

    const md = sessionToMarkdown(session, messages);

    expect(md).toContain("### Bash\n\n```\n$ ls\nfoo.txt\n```");
  });

  it("omits absent optional metadata instead of rendering empty labels", () => {
    const bareSession: SessionSummary = {
      ...session,
      workspace: null,
      gitBranch: null,
      model: null,
      startedAt: null,
      updatedAt: null,
    };

    const md = sessionToMarkdown(bareSession, []);

    expect(md).toContain("**Tool:** claude · **Workspace:** unknown workspace");
    expect(md).not.toContain("**Branch:**");
    expect(md).not.toContain("**Model:**");
    expect(md).not.toContain("**Started:**");
    expect(md).not.toContain("**Updated:**");
  });

  it("renders a placeholder body when there are no readable messages", () => {
    const md = sessionToMarkdown(session, []);

    expect(md).toContain("_No readable messages in this session._");
  });
});
