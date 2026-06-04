// Skills-source state: skills.sh search results and the local "starred"
// favorites. Search runs through Rust (keyless public index, no API key);
// starring and installing go through typed IPC. Favorites are a local reference
// list reused across projects — there is no remote sync. Errors surface as toasts.

import { create } from "zustand";
import { toast } from "sonner";
import {
  addSkillFavorite,
  installSkill,
  listSkillFavorites,
  removeSkillFavorite,
  searchSkills,
} from "../ipc";
import type {
  SkillFavorite,
  SkillInstallResult,
  SkillSearchHit,
  ToolId,
} from "../types";
import { messageOf } from "../shared";

/// The only source provider today. Centralized so the seam is obvious when more
/// are added.
export const SKILLS_SH_PROVIDER = "skills.sh";

/// Build a favorite from a search hit. Links are derived in Rust, so this is a
/// straight projection; `starredAt` is stamped by the store.
export function favoriteFromResult(r: SkillSearchHit): SkillFavorite {
  return {
    provider: SKILLS_SH_PROVIDER,
    id: r.id,
    slug: r.skillId,
    name: r.name,
    source: r.source,
    installRef: r.installRef,
    githubUrl: r.githubUrl,
    pageUrl: r.pageUrl,
    starredAt: "",
  };
}

/// One row of a batch install: a starred skill and the tools to target.
export interface InstallItem {
  favorite: SkillFavorite;
  toolIds: ToolId[];
}

/// Outcome of a batch install: how many of the requested skills landed.
export interface InstallSummary {
  installed: number;
  failed: number;
}

interface SkillsState {
  query: string;
  results: SkillSearchHit[];
  favorites: SkillFavorite[];
  searching: boolean;
  installing: boolean;
  /// Combined, human-readable output of the last install batch (per-skill CLI
  /// logs / errors). `null` until a batch runs; shown in the install dialog so a
  /// failure isn't a dead-end toast.
  installLog: string | null;

  setQuery: (q: string) => void;
  loadFavorites: () => Promise<void>;
  search: () => Promise<void>;
  star: (result: SkillSearchHit) => Promise<void>;
  unstar: (provider: string, id: string) => Promise<void>;
  installMany: (items: InstallItem[], workspaceId: string) => Promise<InstallSummary>;
  clearInstallLog: () => void;
}

/// Run a single install through IPC, toasting the outcome. Returns the outcome
/// plus its CLI log so the batch caller can both tally and assemble a combined
/// output panel. A thrown IPC error becomes a failed result whose log is the
/// error message (so the cause is visible, not just a transient toast).
async function runInstall(
  item: InstallItem,
  workspaceId: string,
): Promise<{ ok: boolean; log: string }> {
  try {
    const result: SkillInstallResult = await installSkill({
      provider: item.favorite.provider,
      installRef: item.favorite.installRef,
      workspaceId,
      toolIds: item.toolIds,
    });
    if (result.ok) toast.success(`Installed ${item.favorite.name}`);
    else toast.error(`Install failed: ${item.favorite.name}`);
    return { ok: result.ok, log: result.log };
  } catch (e) {
    const msg = messageOf(e);
    toast.error(msg);
    return { ok: false, log: msg };
  }
}

export const useSkillsStore = create<SkillsState>((set, get) => ({
  query: "",
  results: [],
  favorites: [],
  searching: false,
  installing: false,
  installLog: null,

  setQuery: (query) => set({ query }),

  clearInstallLog: () => set({ installLog: null }),

  loadFavorites: async () => {
    try {
      const state = await listSkillFavorites();
      set({ favorites: state.favorites });
    } catch (e) {
      toast.error(messageOf(e));
    }
  },

  search: async () => {
    const query = get().query.trim();
    // The index ignores 1-char queries; skip the round trip and clear stale hits.
    if (query.length < 2) {
      set({ results: [] });
      return;
    }
    set({ searching: true });
    try {
      const results = await searchSkills(SKILLS_SH_PROVIDER, query);
      set({ results });
    } catch (e) {
      toast.error(messageOf(e));
    } finally {
      set({ searching: false });
    }
  },

  star: async (result) => {
    try {
      const stored = await addSkillFavorite(favoriteFromResult(result));
      // Upsert newest-first to mirror the store's own ordering.
      set((s) => ({
        favorites: [
          stored,
          ...s.favorites.filter(
            (f) => !(f.provider === stored.provider && f.id === stored.id),
          ),
        ],
      }));
    } catch (e) {
      toast.error(messageOf(e));
    }
  },

  unstar: async (provider, id) => {
    try {
      await removeSkillFavorite(provider, id);
      set((s) => ({
        favorites: s.favorites.filter(
          (f) => !(f.provider === provider && f.id === id),
        ),
      }));
    } catch (e) {
      toast.error(messageOf(e));
    }
  },

  // Install one or more starred skills sequentially into the active workspace.
  // The busy flag spans the whole batch so a single Install click can't overlap;
  // each skill toasts its own outcome and the summary lets the caller decide
  // whether to keep the dialog open (partial failure) or close it (all landed).
  installMany: async (items, workspaceId) => {
    if (items.length === 0) return { installed: 0, failed: 0 };
    set({ installing: true, installLog: null });
    let installed = 0;
    let failed = 0;
    const logs: string[] = [];
    try {
      for (const item of items) {
        const { ok, log } = await runInstall(item, workspaceId);
        if (ok) installed += 1;
        else failed += 1;
        const head = `${ok ? "✓" : "✗"} ${item.favorite.name} → ${item.toolIds.join(", ")}`;
        logs.push(log.trim() ? `${head}\n${log.trim()}` : head);
      }
    } finally {
      set({ installing: false, installLog: logs.join("\n\n") });
    }
    return { installed, failed };
  },
}));
