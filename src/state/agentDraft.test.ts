import { describe, expect, it } from "vitest";
import {
  AGENT_LIMITS,
  EMPTY_AGENT,
  agentDraftError,
  agentDraftFrom,
  agentPayload,
  toggleRequiredCli,
  utf8Bytes,
} from "./agentDraft";

describe("agentDraft — from a stored spec", () => {
  it("an absent agent block becomes an empty draft", () => {
    expect(agentDraftFrom(undefined)).toEqual(EMPTY_AGENT);
  });

  it("null fields become empty strings and the CLI list is copied", () => {
    const clis = ["gh"];
    const draft = agentDraftFrom({ emoji: null, instructions: "Be terse.", requiredClis: clis });

    expect(draft).toEqual({ emoji: "", instructions: "Be terse.", requiredClis: ["gh"] });
    expect(draft.requiredClis).not.toBe(clis);
  });
});

describe("agentDraft — payload", () => {
  it("an all-empty draft clears the block", () => {
    expect(agentPayload({ emoji: "  ", instructions: "\n", requiredClis: [] })).toBeNull();
  });

  it("trims emoji, keeps instructions verbatim, and nulls empty fields", () => {
    expect(
      agentPayload({ emoji: " 🛠️ ", instructions: "", requiredClis: ["gh", "jq"] }),
    ).toEqual({ emoji: "🛠️", instructions: null, requiredClis: ["gh", "jq"] });
    expect(
      agentPayload({ emoji: "", instructions: "  # CTO\n", requiredClis: [] }),
    ).toEqual({ emoji: null, instructions: "  # CTO\n", requiredClis: [] });
  });
});

describe("agentDraft — limits", () => {
  it("counts instructions in UTF-8 bytes like the Rust validator", () => {
    expect(utf8Bytes("abc")).toBe(3);
    expect(utf8Bytes("é")).toBe(2);
    expect(utf8Bytes("🛠")).toBe(4);
  });

  it("accepts a draft exactly at every limit", () => {
    const draft = {
      emoji: "🙂".repeat(AGENT_LIMITS.emojiChars),
      instructions: "a".repeat(AGENT_LIMITS.instructionsBytes),
      requiredClis: Array.from({ length: AGENT_LIMITS.requiredClis }, (_, i) => `cli-${i}`),
    };

    expect(agentDraftError(draft)).toBeNull();
  });

  it("rejects an emoji longer than 16 characters (code points)", () => {
    const draft = { ...EMPTY_AGENT, emoji: "🙂".repeat(AGENT_LIMITS.emojiChars + 1) };

    expect(agentDraftError(draft)).toMatch(/emoji/i);
  });

  it("rejects instructions over 32 KiB of UTF-8", () => {
    const draft = { ...EMPTY_AGENT, instructions: "é".repeat(AGENT_LIMITS.instructionsBytes / 2 + 1) };

    expect(agentDraftError(draft)).toMatch(/32 KiB/);
  });
});

describe("agentDraft — required CLIs", () => {
  it("adds a missing id and removes a present one", () => {
    const added = toggleRequiredCli({ ...EMPTY_AGENT, requiredClis: ["gh"] }, "jq");
    expect(added.requiredClis).toEqual(["gh", "jq"]);

    expect(toggleRequiredCli(added, "gh").requiredClis).toEqual(["jq"]);
  });

  it("refuses to add past the 20-CLI limit but still removes", () => {
    const full = {
      ...EMPTY_AGENT,
      requiredClis: Array.from({ length: AGENT_LIMITS.requiredClis }, (_, i) => `cli-${i}`),
    };

    expect(toggleRequiredCli(full, "extra")).toBe(full);
    expect(toggleRequiredCli(full, "cli-0").requiredClis).toHaveLength(AGENT_LIMITS.requiredClis - 1);
  });
});
