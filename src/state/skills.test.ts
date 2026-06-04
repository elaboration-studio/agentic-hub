import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@/ipc", () => ({
  searchSkills: vi.fn(),
  listSkillFavorites: vi.fn(),
  addSkillFavorite: vi.fn(),
  removeSkillFavorite: vi.fn(),
}));
vi.mock("sonner", () => ({
  toast: { success: vi.fn(), error: vi.fn(), warning: vi.fn(), info: vi.fn() },
}));

import {
  addSkillFavorite,
  listSkillFavorites,
  removeSkillFavorite,
  searchSkills,
} from "@/ipc";
import { toast } from "sonner";
import type { SkillFavorite, SkillSearchHit } from "@/types";
import { SKILLS_SH_PROVIDER, favoriteFromResult, useSkillsStore } from "./skills";

const mocked = {
  searchSkills: vi.mocked(searchSkills),
  listSkillFavorites: vi.mocked(listSkillFavorites),
  addSkillFavorite: vi.mocked(addSkillFavorite),
  removeSkillFavorite: vi.mocked(removeSkillFavorite),
};

function makeResult(slug: string): SkillSearchHit {
  return {
    id: `vercel-labs/agent-skills/${slug}`,
    skillId: slug,
    name: slug,
    source: "vercel-labs/agent-skills",
    installs: 100,
    installRef: "vercel-labs/agent-skills",
    githubUrl: "https://github.com/vercel-labs/agent-skills",
    pageUrl: `https://skills.sh/vercel-labs/agent-skills/${slug}`,
  };
}

function makeFavorite(id: string): SkillFavorite {
  return {
    provider: SKILLS_SH_PROVIDER,
    id,
    slug: id,
    name: id,
    source: "vercel-labs/agent-skills",
    installRef: "vercel-labs/agent-skills",
    githubUrl: null,
    pageUrl: null,
    starredAt: "2026-01-01",
  };
}

beforeEach(() => {
  vi.clearAllMocks();
  useSkillsStore.setState(useSkillsStore.getInitialState(), true);
});

describe("favoriteFromResult", () => {
  it("maps a search result to the owner/repo install ref", () => {
    const fav = favoriteFromResult(makeResult("next-js"));
    expect(fav.provider).toBe(SKILLS_SH_PROVIDER);
    expect(fav.installRef).toBe("vercel-labs/agent-skills");
    expect(fav.pageUrl).toBe("https://skills.sh/vercel-labs/agent-skills/next-js");
    expect(fav.githubUrl).toBe("https://github.com/vercel-labs/agent-skills");
  });
});

describe("skills store — search", () => {
  it("queries the keyless index and stores the results", async () => {
    mocked.searchSkills.mockResolvedValue([makeResult("a"), makeResult("b")]);
    useSkillsStore.getState().setQuery("react");

    await useSkillsStore.getState().search();

    expect(mocked.searchSkills).toHaveBeenCalledWith(SKILLS_SH_PROVIDER, "react");
    expect(useSkillsStore.getState().results).toHaveLength(2);
    expect(useSkillsStore.getState().searching).toBe(false);
  });

  it("clears results and skips the call for a too-short query", async () => {
    useSkillsStore.setState({ results: [makeResult("stale")] });
    useSkillsStore.getState().setQuery(" a ");

    await useSkillsStore.getState().search();

    expect(mocked.searchSkills).not.toHaveBeenCalled();
    expect(useSkillsStore.getState().results).toEqual([]);
  });

  it("toasts and resets the busy flag when search throws", async () => {
    mocked.searchSkills.mockRejectedValue(new Error("skills.sh returned 503"));
    useSkillsStore.getState().setQuery("react");

    await useSkillsStore.getState().search();

    expect(toast.error).toHaveBeenCalledWith("skills.sh returned 503");
    expect(useSkillsStore.getState().searching).toBe(false);
  });
});

describe("skills store — favorites", () => {
  it("loadFavorites populates from the store", async () => {
    mocked.listSkillFavorites.mockResolvedValue({
      favorites: [makeFavorite("a"), makeFavorite("b")],
    });

    await useSkillsStore.getState().loadFavorites();

    expect(useSkillsStore.getState().favorites).toHaveLength(2);
  });

  it("star adds the favorite newest-first and dedupes", async () => {
    useSkillsStore.setState({ favorites: [makeFavorite("old")] });
    mocked.addSkillFavorite.mockResolvedValue(makeFavorite("new"));

    await useSkillsStore.getState().star(makeResult("new"));

    const favs = useSkillsStore.getState().favorites;
    expect(favs[0].id).toBe("new");
    expect(favs).toHaveLength(2);
  });

  it("unstar removes the matching favorite", async () => {
    useSkillsStore.setState({
      favorites: [makeFavorite("a"), makeFavorite("b")],
    });
    mocked.removeSkillFavorite.mockResolvedValue(undefined);

    await useSkillsStore.getState().unstar(SKILLS_SH_PROVIDER, "a");

    expect(mocked.removeSkillFavorite).toHaveBeenCalledWith(SKILLS_SH_PROVIDER, "a");
    expect(useSkillsStore.getState().favorites.map((f) => f.id)).toEqual(["b"]);
  });
});
