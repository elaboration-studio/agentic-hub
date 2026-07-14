import logoUrl from "@/assets/logo.png";
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { useManagerStore } from "@/state/manager";
import type { Route } from "@/shared";

export function Header(props: { route: Route; onNavigate: (route: Route) => void }) {
  const data = useManagerStore((s) => s.data);

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
      </div>
      <Tabs value={props.route} onValueChange={(v) => props.onNavigate(v as Route)}>
        <TabsList>
          <TabsTrigger value="manager">Manager</TabsTrigger>
          <TabsTrigger value="suites">Suites</TabsTrigger>
          <TabsTrigger value="skills">Resources</TabsTrigger>
          <TabsTrigger value="statistics">Statistics</TabsTrigger>
          <TabsTrigger value="config">Config</TabsTrigger>
        </TabsList>
      </Tabs>
    </header>
  );
}
