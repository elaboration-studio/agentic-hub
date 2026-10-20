// The editable form of a suite's agent block. Limits mirror
// `agentic-core::agent_spec` so the editor rejects what Rust would reject.

import type { AgentSpec } from "../types";

export const AGENT_LIMITS = {
  emojiChars: 16,
  instructionsBytes: 32 * 1024,
  requiredClis: 20,
} as const;

export interface AgentDraft {
  emoji: string;
  instructions: string;
  requiredClis: string[];
}

export const EMPTY_AGENT: AgentDraft = { emoji: "", instructions: "", requiredClis: [] };

const encoder = new TextEncoder();

export function utf8Bytes(text: string): number {
  return encoder.encode(text).length;
}

export function agentDraftFrom(spec: AgentSpec | undefined): AgentDraft {
  return {
    emoji: spec?.emoji ?? "",
    instructions: spec?.instructions ?? "",
    requiredClis: [...(spec?.requiredClis ?? [])],
  };
}

/// The IPC value: `null` clears the block when every field is empty.
export function agentPayload(draft: AgentDraft): AgentSpec | null {
  const emoji = draft.emoji.trim();
  const instructions = draft.instructions.trim() ? draft.instructions : "";
  if (!emoji && !instructions && draft.requiredClis.length === 0) return null;
  return {
    emoji: emoji || null,
    instructions: instructions || null,
    requiredClis: [...draft.requiredClis],
  };
}

/// First limit the draft breaks, or `null` when it is saveable.
export function agentDraftError(draft: AgentDraft): string | null {
  // Rust counts `chars()` (code points); `Array.from` does the same.
  if (Array.from(draft.emoji.trim()).length > AGENT_LIMITS.emojiChars) {
    return `Agent emoji must be at most ${AGENT_LIMITS.emojiChars} characters`;
  }
  if (utf8Bytes(draft.instructions) > AGENT_LIMITS.instructionsBytes) {
    return `Agent instructions must be at most ${AGENT_LIMITS.instructionsBytes / 1024} KiB`;
  }
  return null;
}

export function toggleRequiredCli(draft: AgentDraft, id: string): AgentDraft {
  if (draft.requiredClis.includes(id)) {
    return { ...draft, requiredClis: draft.requiredClis.filter((c) => c !== id) };
  }
  if (draft.requiredClis.length >= AGENT_LIMITS.requiredClis) return draft;
  return { ...draft, requiredClis: [...draft.requiredClis, id] };
}
