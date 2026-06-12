// Skills-source state: skills.sh search results and the local "starred"
// favorites. Search runs through Rust (keyless public index, no API key);
// starring goes through typed IPC. Favorites are a local reference list reused
// across projects — there is no remote sync. Errors surface as toasts. The
// install flow lives in its own window (see components/install/), so it owns its
// selection + streaming state; this store only serves the starred list.

import { create } from "zustand";
import { toast } from "sonner";
import {
  addSkillFavorite,
  listSkillFavorites,
  removeSkillFavorite,
  searchSkills,
} from "../ipc";
import type { SkillFavorite, SkillSearchHit } from "../types";
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

/// Case-insensitive local filter over the starred list. Matches name, source,
/// slug, and install ref so typing a skill name, repo, or owner all work.
export function filterFavorites(
  favorites: SkillFavorite[],
  query: string,
): SkillFavorite[] {
  const q = query.trim().toLowerCase();
  if (!q) return favorites;
  return favorites.filter(
    (f) =>
      f.name.toLowerCase().includes(q) ||
      f.source.toLowerCase().includes(q) ||
      f.slug.toLowerCase().includes(q) ||
      f.installRef.toLowerCase().includes(q),
  );
}

interface SkillsState {
  query: string;
  results: SkillSearchHit[];
  favorites: SkillFavorite[];
  searching: boolean;

  setQuery: (q: string) => void;
  loadFavorites: () => Promise<void>;
  search: () => Promise<void>;
  star: (result: SkillSearchHit) => Promise<void>;
  unstar: (provider: string, id: string) => Promise<void>;
}

export const useSkillsStore = create<SkillsState>((set, get) => ({
  query: "",
  results: [],
  favorites: [],
  searching: false,

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
}));
