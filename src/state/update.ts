// App self-update state. A "scan" checks the R2-hosted feed for a newer
// minisign-signed release; the Tauri updater verifies the signature on install.
// Checks run on launch, on each app re-open, and on a weekly safety-net timer,
// all throttled to at most once per `UPDATE_CHECK_INTERVAL_MS` via a persisted
// `lastCheckedAt`. A found update surfaces a prompt regardless of the throttle.

import { create } from "zustand";
import { toast } from "sonner";
import { checkForUpdate, installUpdate, type AvailableUpdate } from "../ipc";
import { messageOf } from "../shared";

/// Weekly cadence for the background scan. A check runs at most once per window.
export const UPDATE_CHECK_INTERVAL_MS = 7 * 24 * 60 * 60 * 1000;

// Versioned, minimal localStorage key (see vercel-react-best-practices 4.4).
const LAST_CHECKED_KEY = "agentic-hub:update:lastCheckedAt:v1";

function loadLastCheckedAt(): number | null {
  try {
    const raw = localStorage.getItem(LAST_CHECKED_KEY);
    const n = raw === null ? NaN : Number(raw);
    return Number.isFinite(n) ? n : null;
  } catch {
    // No storage (Node tests, private browsing) — treat as "never checked".
    return null;
  }
}

function saveLastCheckedAt(ts: number): void {
  try {
    localStorage.setItem(LAST_CHECKED_KEY, String(ts));
  } catch {
    // A missed persist only means the next check happens sooner — harmless.
  }
}

type UpdatePhase =
  | "idle"
  | "checking"
  | "available"
  | "uptodate"
  | "downloading"
  | "error";

interface UpdateState {
  phase: UpdatePhase;
  available: AvailableUpdate | null;
  error: string | null;
  lastCheckedAt: number | null;

  /// Check the feed now. `silent` suppresses the up-to-date / error toasts
  /// (background checks); a found update always surfaces in state.
  check: (opts?: { silent?: boolean }) => Promise<void>;
  /// Run `check` (silent) only if the weekly window has elapsed.
  maybeCheck: (now?: number) => Promise<void>;
  /// Download + install the pending update, then relaunch.
  install: () => Promise<void>;
  /// Dismiss the available-update prompt without installing.
  dismiss: () => void;
}

export const useUpdateStore = create<UpdateState>((set, get) => ({
  phase: "idle",
  available: null,
  error: null,
  lastCheckedAt: loadLastCheckedAt(),

  check: async ({ silent = false } = {}) => {
    // Stamp the check time up front (even on failure) so a flaky network can't
    // turn the throttle into a tight retry loop.
    const now = Date.now();
    set({ phase: "checking", error: null, lastCheckedAt: now });
    saveLastCheckedAt(now);
    try {
      const update = await checkForUpdate();
      if (update) {
        set({ phase: "available", available: update });
        return;
      }
      set({ phase: "uptodate", available: null });
      if (!silent) toast.success("Agentic Hub is up to date.");
    } catch (e) {
      set({ phase: "error", error: messageOf(e) });
      if (!silent) toast.error(messageOf(e));
    }
  },

  maybeCheck: async (now = Date.now()) => {
    const { lastCheckedAt, phase } = get();
    if (phase === "checking" || phase === "downloading") return;
    if (lastCheckedAt !== null && now - lastCheckedAt < UPDATE_CHECK_INTERVAL_MS) {
      return;
    }
    await get().check({ silent: true });
  },

  install: async () => {
    const { available } = get();
    if (!available) return;
    set({ phase: "downloading" });
    try {
      await installUpdate(available);
      // On success the app relaunches, so execution does not continue past here.
    } catch (e) {
      set({ phase: "error", error: messageOf(e) });
      toast.error(messageOf(e));
    }
  },

  dismiss: () => set({ phase: "idle", available: null }),
}));
