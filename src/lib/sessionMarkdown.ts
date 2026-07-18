import type { SessionMessage, SessionRole, SessionSummary } from "@/types";

const ROLE_LABEL: Record<SessionRole, string> = {
  user: "User",
  assistant: "Assistant",
  tool: "Tool",
  system: "System",
};

/** Heading label for a transcript message — "Tool" turns prefer their `toolName`. */
export function sessionRoleLabel(message: Pick<SessionMessage, "role" | "toolName">): string {
  if (message.role === "tool") return message.toolName ?? ROLE_LABEL.tool;
  return ROLE_LABEL[message.role];
}

function messageToMarkdown(message: SessionMessage): string {
  const heading = `### ${sessionRoleLabel(message)}${message.timestamp ? ` — ${message.timestamp}` : ""}`;
  // Tool output is raw/preformatted; fence it so it renders as a block rather
  // than being reflowed as prose. User/assistant text is already natural-language
  // (and may itself contain markdown) so it's left unfenced.
  const body = message.role === "tool" ? `\`\`\`\n${message.text}\n\`\`\`` : message.text;
  return `${heading}\n\n${body}`;
}

/** Render a session's summary + full transcript as a single Markdown document. */
export function sessionToMarkdown(session: SessionSummary, messages: SessionMessage[]): string {
  const metaLines = [
    `**Tool:** ${session.tool}`,
    `**Workspace:** ${session.workspace ?? "unknown workspace"}`,
    session.gitBranch ? `**Branch:** ${session.gitBranch}` : null,
    session.model ? `**Model:** ${session.model}` : null,
    session.startedAt ? `**Started:** ${session.startedAt}` : null,
    session.updatedAt ? `**Updated:** ${session.updatedAt}` : null,
  ].filter((line): line is string => line !== null);

  const header = `# ${session.title}\n\n${metaLines.join(" · ")}`;

  if (messages.length === 0) return `${header}\n\n---\n\n_No readable messages in this session._`;

  const body = messages.map(messageToMarkdown).join("\n\n---\n\n");
  return `${header}\n\n---\n\n${body}`;
}
