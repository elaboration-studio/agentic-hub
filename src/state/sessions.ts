// Session Explorer state: the merged Codex + Claude + Cursor session list
// (read-only, re-read on each load — v1 has no persisted index yet) plus the
// on-demand transcript for whichever session is selected. Defaults to today
// (rolling 24h): reading every tool's full history on every open is
// expensive (see sessions.rs), so a narrow default range is what actually
// keeps this fast rather than just trimming an already-fully-read list.

import { create } from "zustand";
import { toast } from "sonner";
import { listSessions, getSession } from "../ipc";
import type { SessionSummary, SessionMessage, UsageDateRange } from "../types";
import { messageOf } from "../shared";

export const DEFAULT_SESSIONS_RANGE: UsageDateRange = "today";

interface SessionsState {
  sessions: SessionSummary[];
  range: UsageDateRange;
  loading: boolean;
  loaded: boolean;

  selectedKey: string | null;
  messages: SessionMessage[];
  messagesLoading: boolean;
  messagesError: string | null;

  load: () => Promise<void>;
  setRange: (range: UsageDateRange) => void;
  select: (sessionKey: string) => Promise<void>;
  clearSelection: () => void;
}

export const useSessionsStore = create<SessionsState>((set, get) => ({
  sessions: [],
  range: DEFAULT_SESSIONS_RANGE,
  loading: false,
  loaded: false,

  selectedKey: null,
  messages: [],
  messagesLoading: false,
  messagesError: null,

  load: async () => {
    set({ loading: true });
    try {
      const sessions = await listSessions({ range: get().range });
      set({ sessions, loaded: true });
    } catch (e) {
      toast.error(messageOf(e));
    } finally {
      set({ loading: false });
    }
  },

  setRange: (range) => {
    set({ range });
    void get().load();
  },

  select: async (sessionKey) => {
    set({
      selectedKey: sessionKey,
      messages: [],
      messagesLoading: true,
      messagesError: null,
    });
    try {
      const messages = await getSession(sessionKey);
      set({ messages });
    } catch (e) {
      set({ messagesError: messageOf(e) });
    } finally {
      set({ messagesLoading: false });
    }
  },

  clearSelection: () => set({ selectedKey: null, messages: [], messagesError: null }),
}));
