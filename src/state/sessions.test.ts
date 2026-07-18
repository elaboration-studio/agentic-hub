import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@/ipc", () => ({
  listSessions: vi.fn(),
  getSession: vi.fn(),
}));
vi.mock("sonner", () => ({
  toast: { success: vi.fn(), error: vi.fn(), warning: vi.fn(), info: vi.fn() },
}));

import { listSessions, getSession } from "../ipc";
import { toast } from "sonner";
import type { SessionMessage, SessionSummary } from "@/types";
import { DEFAULT_SESSIONS_RANGE, useSessionsStore } from "./sessions";

const mocked = {
  listSessions: vi.mocked(listSessions),
  getSession: vi.mocked(getSession),
};

function makeSession(overrides: Partial<SessionSummary> = {}): SessionSummary {
  return {
    sessionKey: "claude:abc",
    tool: "claude",
    title: "Fix the login bug",
    workspace: "~/dev/agentic-hub",
    gitBranch: null,
    model: null,
    startedAt: "2026-07-18T10:00:00Z",
    updatedAt: "2026-07-18T11:00:00Z",
    messageCount: 2,
    sourcePath: "/Users/arno/.claude/projects/x/abc.jsonl",
    ...overrides,
  };
}

const MESSAGES: SessionMessage[] = [
  { role: "user", text: "Why does login fail?", toolName: null, timestamp: null },
];

beforeEach(() => {
  vi.clearAllMocks();
  useSessionsStore.setState({
    sessions: [],
    range: DEFAULT_SESSIONS_RANGE,
    loading: false,
    loaded: false,
    selectedKey: null,
    messages: [],
    messagesLoading: false,
    messagesError: null,
  });
});

describe("useSessionsStore", () => {
  it("load() populates sessions and marks loaded on success", async () => {
    const session = makeSession();
    mocked.listSessions.mockResolvedValue([session]);

    await useSessionsStore.getState().load();

    expect(mocked.listSessions).toHaveBeenCalledWith({ range: DEFAULT_SESSIONS_RANGE });
    expect(useSessionsStore.getState().sessions).toEqual([session]);
    expect(useSessionsStore.getState().loaded).toBe(true);
    expect(useSessionsStore.getState().loading).toBe(false);
  });

  it("load() toasts an error and leaves sessions empty on failure", async () => {
    mocked.listSessions.mockRejectedValue(new Error("boom"));

    await useSessionsStore.getState().load();

    expect(toast.error).toHaveBeenCalledWith("boom");
    expect(useSessionsStore.getState().sessions).toEqual([]);
    expect(useSessionsStore.getState().loading).toBe(false);
  });

  it("setRange() updates range and triggers a reload", async () => {
    mocked.listSessions.mockResolvedValue([]);

    useSessionsStore.getState().setRange("allTime");

    expect(useSessionsStore.getState().range).toBe("allTime");
    await vi.waitFor(() => expect(mocked.listSessions).toHaveBeenCalledWith({ range: "allTime" }));
  });

  it("select() populates messages on success", async () => {
    mocked.getSession.mockResolvedValue(MESSAGES);

    await useSessionsStore.getState().select("claude:abc");

    expect(mocked.getSession).toHaveBeenCalledWith("claude:abc");
    expect(useSessionsStore.getState().selectedKey).toBe("claude:abc");
    expect(useSessionsStore.getState().messages).toEqual(MESSAGES);
    expect(useSessionsStore.getState().messagesLoading).toBe(false);
    expect(useSessionsStore.getState().messagesError).toBeNull();
  });

  it("select() sets messagesError (not a toast) on failure", async () => {
    mocked.getSession.mockRejectedValue(new Error("session_source_unavailable"));

    await useSessionsStore.getState().select("claude:abc");

    expect(useSessionsStore.getState().messagesError).toBe("session_source_unavailable");
    expect(useSessionsStore.getState().messages).toEqual([]);
    expect(useSessionsStore.getState().messagesLoading).toBe(false);
    expect(toast.error).not.toHaveBeenCalled();
  });

  it("clearSelection() resets selection, messages, and error", async () => {
    mocked.getSession.mockRejectedValue(new Error("boom"));
    await useSessionsStore.getState().select("claude:abc");
    expect(useSessionsStore.getState().messagesError).toBe("boom");

    useSessionsStore.getState().clearSelection();

    expect(useSessionsStore.getState().selectedKey).toBeNull();
    expect(useSessionsStore.getState().messages).toEqual([]);
    expect(useSessionsStore.getState().messagesError).toBeNull();
  });
});
