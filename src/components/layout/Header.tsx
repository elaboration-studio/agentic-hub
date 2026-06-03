import logoUrl from "@/assets/logo.png";
import { FolderGit2, Globe } from "lucide-react";
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { Toggle } from "@/components/ui/toggle";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { cn } from "@/lib/utils";
import { useManagerStore } from "@/state/manager";
import type { Route } from "@/shared";

export function Header(props: { route: Route; onNavigate: (route: Route) => void }) {
  const data = useManagerStore((s) => s.data);
  const scope = useManagerStore((s) => s.scope);
  const setScope = useManagerStore((s) => s.setScope);
  const watching = useManagerStore((s) => s.watching);
  const toggleWatching = useManagerStore((s) => s.toggleWatching);

  const count = data?.items.length ?? 0;
  const sources = data?.settings.sources.length ?? 0;

  return (
    <header className="flex items-center justify-between gap-4 border-b bg-card px-6 py-4">
      <div className="flex items-center gap-3.5">
        <img className="size-8 rounded-[9px] object-cover" src={logoUrl} alt="" aria-hidden />
        <div>
          <h1 className="text-base font-semibold tracking-[0.2px]">Agentic Hub</h1>
          <p className="mt-0.5 text-xs text-muted-foreground">
            {count} capabilities · {sources || 1} source{sources === 1 ? "" : "s"}
          </p>
        </div>
        {/* Scope switcher sits with the title context; rendering it here (only on
            the Manager route) keeps the right-side nav + watch pill from reflowing. */}
        {props.route === "manager" && (
          <div className="ml-1 border-l pl-3.5">
            <Select value={scope} onValueChange={(v) => v && setScope(v as typeof scope)}>
              <SelectTrigger size="sm" className="w-[148px]" title="Capability scope">
                <span className="flex min-w-0 items-center gap-2">
                  {scope === "global" ? <Globe /> : <FolderGit2 />}
                  <SelectValue />
                </span>
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="global">Global</SelectItem>
                <SelectItem value="workspace">Workspace</SelectItem>
              </SelectContent>
            </Select>
          </div>
        )}
      </div>
      <div className="flex items-center gap-3">
        <Tabs value={props.route} onValueChange={(v) => props.onNavigate(v as Route)}>
          <TabsList>
            <TabsTrigger value="manager">Manager</TabsTrigger>
            <TabsTrigger value="suites">Suites</TabsTrigger>
            <TabsTrigger value="config">Config</TabsTrigger>
          </TabsList>
        </Tabs>
        <Toggle
          pressed={watching}
          onPressedChange={(v) => void toggleWatching(v)}
          variant="outline"
          aria-label={watching ? "Watching — click to pause" : "Paused — click to watch"}
          title={
            watching
              ? "Watching source roots — changes sync automatically. Click to pause."
              : "Watcher paused. Click to watch source roots and auto-sync changes."
          }
        >
          <span
            className={cn(
              "size-2 rounded-full bg-muted-foreground transition-colors",
              watching && "bg-success shadow-[0_0_0_3px_rgba(34,197,94,0.25)]",
            )}
            aria-hidden
          />
          {watching ? "Watching" : "Paused"}
        </Toggle>
      </div>
    </header>
  );
}
