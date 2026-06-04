import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@/ipc", () => ({
  searchSkills: vi.fn(),
  listSkillFavorites: vi.fn(),
  addSkillFavorite: vi.fn(),
  removeSkillFavorite: vi.fn(),
  installSkill: vi.fn(),
}));
vi.mock("sonner", () => ({
  toast: { success: vi.fn(), error: vi.fn(), warning: vi.fn(), info: vi.fn() },
}));

import {
  addSkillFavorite,
  installSkill,
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
  installSkill: vi.mocked(installSkill),
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

describe("skills store — installMany", () => {
  it("installs a single skill via IPC and reports success", async () => {
    mocked.installSkill.mockResolvedValue({ ok: true, log: "done" });

    const summary = await useSkillsStore
      .getState()
      .installMany([{ favorite: makeFavorite("a"), toolIds: ["claude", "cursor"] }], "ws-1");

    expect(mocked.installSkill).toHaveBeenCalledWith({
      provider: SKILLS_SH_PROVIDER,
      installRef: "vercel-labs/agent-skills",
      workspaceId: "ws-1",
      toolIds: ["claude", "cursor"],
    });
    expect(summary).toEqual({ installed: 1, failed: 0 });
    expect(toast.success).toHaveBeenCalledTimes(1);
    expect(useSkillsStore.getState().installing).toBe(false);
  });

  it("installs several skills sequentially and tallies the outcomes", async () => {
    mocked.installSkill
      .mockResolvedValueOnce({ ok: true, log: "ok" })
      .mockResolvedValueOnce({ ok: false, log: "npx not found" });

    const summary = await useSkillsStore.getState().installMany(
      [
        { favorite: makeFavorite("a"), toolIds: ["cursor"] },
        { favorite: makeFavorite("b"), toolIds: ["claude"] },
      ],
      "ws-1",
    );

    expect(mocked.installSkill).toHaveBeenCalledTimes(2);
    expect(summary).toEqual({ installed: 1, failed: 1 });
    expect(toast.success).toHaveBeenCalledTimes(1);
    expect(toast.error).toHaveBeenCalledTimes(1);
  });

  it("counts a thrown install as a failure without aborting the batch", async () => {
    mocked.installSkill
      .mockRejectedValueOnce(new Error("workspace gone"))
      .mockResolvedValueOnce({ ok: true, log: "ok" });

    const summary = await useSkillsStore.getState().installMany(
      [
        { favorite: makeFavorite("a"), toolIds: ["cursor"] },
        { favorite: makeFavorite("b"), toolIds: ["cursor"] },
      ],
      "ws-1",
    );

    expect(summary).toEqual({ installed: 1, failed: 1 });
    expect(toast.error).toHaveBeenCalledWith("workspace gone");
    expect(useSkillsStore.getState().installing).toBe(false);
  });

  it("no-ops on an empty batch", async () => {
    const summary = await useSkillsStore.getState().installMany([], "ws-1");

    expect(mocked.installSkill).not.toHaveBeenCalled();
    expect(summary).toEqual({ installed: 0, failed: 0 });
  });

  it("captures the combined per-skill log for the output panel", async () => {
    mocked.installSkill
      .mockResolvedValueOnce({ ok: true, log: "added next-js" })
      .mockRejectedValueOnce(new Error("npx (Node.js) was not found on PATH"));

    await useSkillsStore.getState().installMany(
      [
        { favorite: makeFavorite("a"), toolIds: ["cursor"] },
        { favorite: makeFavorite("b"), toolIds: ["claude"] },
      ],
      "ws-1",
    );

    const log = useSkillsStore.getState().installLog ?? "";
    expect(log).toContain("✓ a → cursor");
    expect(log).toContain("added next-js");
    expect(log).toContain("✗ b → claude");
    expect(log).toContain("npx (Node.js) was not found on PATH");
  });

  it("clearInstallLog resets the captured output", async () => {
    useSkillsStore.setState({ installLog: "stale output" });

    useSkillsStore.getState().clearInstallLog();

    expect(useSkillsStore.getState().installLog).toBeNull();
  });
});
