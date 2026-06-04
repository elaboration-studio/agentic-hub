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

  setQuery: (q: string) => void;
  loadFavorites: () => Promise<void>;
  search: () => Promise<void>;
  star: (result: SkillSearchHit) => Promise<void>;
  unstar: (provider: string, id: string) => Promise<void>;
  installMany: (items: InstallItem[], workspaceId: string) => Promise<InstallSummary>;
}

/// Run a single install through IPC, toasting the outcome. Returns whether it
/// landed so the batch caller can tally without re-inspecting the result.
async function runInstall(item: InstallItem, workspaceId: string): Promise<boolean> {
  try {
    const result: SkillInstallResult = await installSkill({
      provider: item.favorite.provider,
      installRef: item.favorite.installRef,
      workspaceId,
      toolIds: item.toolIds,
    });
    if (result.ok) {
      toast.success(`Installed ${item.favorite.name}`);
      return true;
    }
    toast.error(`Install failed: ${item.favorite.name}`);
    return false;
  } catch (e) {
    toast.error(messageOf(e));
    return false;
  }
}

export const useSkillsStore = create<SkillsState>((set, get) => ({
  query: "",
  results: [],
  favorites: [],
  searching: false,
  installing: false,

  setQuery: (query) => set({ query }),

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
    set({ installing: true });
    let installed = 0;
    let failed = 0;
    try {
      for (const item of items) {
        if (await runInstall(item, workspaceId)) installed += 1;
        else failed += 1;
      }
    } finally {
      set({ installing: false });
    }
    return { installed, failed };
  },
}));
