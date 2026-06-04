// Skills route: search skills.sh and manage the local "starred" favorites.
// Search runs through Rust against the keyless public index (no API key);
// starring goes through IPC. Favorites are reused from the Workspace scope to
// install skills.

import { useEffect, useMemo } from "react";
import { ExternalLink, GitBranch, Search, Star } from "lucide-react";
import { toast } from "sonner";
import { openUrl } from "@/ipc";
import { useManagerStore } from "@/state/manager";
import { useSkillsStore, SKILLS_SH_PROVIDER } from "@/state/skills";
import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Badge } from "@/components/ui/badge";
import { Alert, AlertDescription } from "@/components/ui/alert";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/dialog";
import { cn } from "@/lib/utils";

const sectionTitle =
  "text-xs font-semibold uppercase tracking-[0.08em] text-muted-foreground";

// Anchor navigation is a no-op inside the WebView; open external links via Rust.
function openExternal(url: string): void {
  void openUrl(url).catch((e) =>
    toast.error(e instanceof Error ? e.message : String(e)),
  );
}

export function SkillsPage() {
  const skills = useManagerStore((s) => s.data?.settings.skills);
  const query = useSkillsStore((s) => s.query);
  const results = useSkillsStore((s) => s.results);
  const favorites = useSkillsStore((s) => s.favorites);
  const searching = useSkillsStore((s) => s.searching);
  const setQuery = useSkillsStore((s) => s.setQuery);
  const search = useSkillsStore((s) => s.search);
  const star = useSkillsStore((s) => s.star);
  const unstar = useSkillsStore((s) => s.unstar);
  const loadFavorites = useSkillsStore((s) => s.loadFavorites);

  useEffect(() => {
    void loadFavorites();
  }, [loadFavorites]);

  // Debounce: search as the user types, once they pause.
  useEffect(() => {
    const id = setTimeout(() => void search(), 300);
    return () => clearTimeout(id);
  }, [query, search]);

  const starredIds = useMemo(
    () => new Set(favorites.filter((f) => f.provider === SKILLS_SH_PROVIDER).map((f) => f.id)),
    [favorites],
  );

  if (!skills?.enabled) {
    return (
      <Alert>
        <AlertDescription>
          The skills.sh source is disabled. Enable it in Config to search and star skills.
        </AlertDescription>
      </Alert>
    );
  }

  return (
    <div className="flex h-full flex-col gap-[18px]">
      <div className="flex items-center justify-between">
        <h2 className={sectionTitle}>Starred skills</h2>
        <Dialog>
          <DialogTrigger asChild>
            <Button variant="outline" size="sm" className="gap-2">
              <Search className="size-4" />
              Search skills.sh…
            </Button>
          </DialogTrigger>
          <DialogContent className="flex max-h-[85vh] flex-col sm:max-w-[600px]">
            <DialogHeader>
              <DialogTitle>Search skills.sh</DialogTitle>
            </DialogHeader>
            <div className="relative shrink-0">
              <Search className="absolute left-3 top-1/2 size-4 -translate-y-1/2 text-muted-foreground" />
              <Input
                value={query}
                placeholder="Type to search skills.sh…"
                className="pl-9"
                onChange={(e) => setQuery(e.target.value)}
              />
            </div>
            {searching && (
              <p className="shrink-0 text-xs text-muted-foreground">Searching…</p>
            )}
            <ul className="flex min-h-0 flex-col gap-1.5 overflow-y-auto">
              {results.map((r) => (
                <SkillRow
                  key={r.id}
                  name={r.name}
                  source={r.source}
                  installs={r.installs}
                  githubUrl={r.githubUrl}
                  pageUrl={r.pageUrl}
                  starred={starredIds.has(r.id)}
                  onToggle={() =>
                    starredIds.has(r.id)
                      ? void unstar(SKILLS_SH_PROVIDER, r.id)
                      : void star(r)
                  }
                />
              ))}
            </ul>
          </DialogContent>
        </Dialog>
      </div>

      <Card className="flex min-h-0 flex-1 flex-col p-4">
        <CardContent className="flex min-h-0 flex-col gap-2 overflow-y-auto p-0">
          {favorites.length === 0 ? (
            <p className="rounded-lg border border-dashed px-3 py-6 text-center text-sm text-muted-foreground">
              No starred skills yet. Search and star a result to reuse it across projects.
            </p>
          ) : (
            <ul className="flex flex-col gap-1.5">
              {favorites.map((f) => (
                <SkillRow
                  key={`${f.provider}:${f.id}`}
                  name={f.name}
                  source={f.source}
                  githubUrl={f.githubUrl}
                  pageUrl={f.pageUrl}
                  starred
                  onToggle={() => void unstar(f.provider, f.id)}
                />
              ))}
            </ul>
          )}
        </CardContent>
      </Card>
    </div>
  );
}

interface SkillRowProps {
  name: string;
  source: string;
  installs?: number;
  githubUrl: string | null;
  pageUrl: string | null;
  starred: boolean;
  onToggle: () => void;
}

function SkillRow(props: SkillRowProps) {
  return (
    <li className="flex items-center gap-2.5 rounded-lg border bg-secondary px-3 py-2">
      <div className="flex min-w-0 flex-1 flex-col">
        <span className="truncate font-semibold">{props.name}</span>
        <code className="truncate font-mono text-[11px] text-muted-foreground">
          {props.source}
        </code>
      </div>
      {typeof props.installs === "number" && (
        <Badge variant="secondary" className="font-mono text-[11px]">
          {props.installs.toLocaleString()} installs
        </Badge>
      )}
      {props.pageUrl && (
        <Button
          variant="ghost"
          size="icon-sm"
          onClick={() => void openExternal(props.pageUrl!)}
          title="Open on skills.sh"
          aria-label="Open on skills.sh"
        >
          <ExternalLink className="size-4" />
        </Button>
      )}
      {props.githubUrl && (
        <Button
          variant="ghost"
          size="icon-sm"
          onClick={() => void openExternal(props.githubUrl!)}
          title="Open on GitHub"
          aria-label="Open source repo"
        >
          <GitBranch className="size-4" />
        </Button>
      )}
      <Button
        variant="ghost"
        size="icon-sm"
        onClick={props.onToggle}
        title={props.starred ? "Unstar" : "Star"}
        aria-label={props.starred ? "Unstar" : "Star"}
      >
        <Star className={cn("size-4", props.starred && "fill-warning text-warning")} />
      </Button>
    </li>
  );
}
