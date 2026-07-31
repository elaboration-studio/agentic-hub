import { useCallback, useMemo } from "react";
import { Check, Minus, RefreshCw } from "lucide-react";
import { toast } from "sonner";
import type {
  AdapterStatus,
  CapabilityItem,
  ToolCapabilityState,
  ToolId,
} from "@/types";
import { key, type ToolDef } from "@/shared";
import { useManagerStore, type OwnershipInfo } from "@/state/manager";
import { useManagerFiltersStore } from "@/state/managerFilters";
import { useWorkspaceStore } from "@/state/workspace";
import { Alert, AlertDescription } from "@/components/ui/alert";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { TableCell } from "@/components/ui/table";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { cn } from "@/lib/utils";
import { CapabilityTable, type CapabilityStateColumn } from "./CapabilityTable";
import { CapabilityRowActions } from "./CapabilityRowActions";
import { isReadOnlyAggregateComplete } from "./capabilityTableModel";
import { ToolCells } from "./ToolCells";

interface MatrixContext {
  tools: ToolDef[];
  adapterMap: Map<ToolId, AdapterStatus>;
  currentMap: Map<string, ToolCapabilityState>;
  desired: Record<string, boolean>;
  ownership: Map<string, OwnershipInfo>;
  onToggle: (tool: ToolId, itemId: string) => void;
  onToggleMany: (tool: ToolId, itemIds: string[], value: boolean) => void;
  readOnly: boolean;
  readOnlyItemIds: ReadonlySet<string>;
  lockedSkills: Map<string, { name: string; source: string; sourceId?: string; destSubpath?: string }>;
  workspaceId: string;
}

export function Matrix() {
  const data = useManagerStore((state) => state.data);
  const tools = useManagerStore((state) => state.tools);
  const currentMap = useManagerStore((state) => state.currentMap);
  const desired = useManagerStore((state) => state.desired);
  const ownership = useManagerStore((state) => state.ownership);
  const onToggle = useManagerStore((state) => state.toggle);
  const onToggleMany = useManagerStore((state) => state.toggleMany);
  const readOnly = useManagerStore((state) => state.readOnly);
  const readOnlyItemIds = useManagerStore((state) => state.readOnlyItemIds);
  const lockedSkills = useManagerStore((state) => state.lockedSkills);
  const usageStats = useManagerStore((state) => state.usageStats);
  const scope = useManagerStore((state) => state.scope);
  const status = useManagerStore((state) => state.status);
  const refresh = useManagerStore((state) => state.refresh);
  const loadWorkspace = useManagerStore((state) => state.loadWorkspace);
  const workspaceId = useWorkspaceStore((state) => state.activeId);
  const locateId = useManagerFiltersStore((state) => state.locateId);
  const clearLocate = useManagerFiltersStore((state) => state.clearLocate);

  const items = data?.items ?? [];
  const adapterMap = useMemo(() => {
    const map = new Map<ToolId, AdapterStatus>();
    for (const adapter of data?.result.adapterStatuses ?? []) map.set(adapter.tool, adapter);
    return map;
  }, [data?.result.adapterStatuses]);

  const enabledItemIds = useMemo(() => {
    const ids = new Set<string>();
    for (const item of items) {
      if (tools.some((tool) => currentMap.has(key(tool.id, item.id)) && desired[key(tool.id, item.id)])) {
        ids.add(item.id);
      }
    }
    return ids;
  }, [items, tools, currentMap, desired]);

  const onRefresh = useCallback(async () => {
    if (scope === "workspace" && workspaceId) await loadWorkspace(workspaceId);
    else await refresh();
    const next = useManagerStore.getState();
    if (next.status === "error") toast.error(next.error || "Refresh failed.");
    else toast.success("Resources and usage refreshed.");
  }, [scope, workspaceId, loadWorkspace, refresh]);

  if (!data || items.length === 0) {
    return (
      <div className="flex flex-col gap-2.5">
        <Alert>
          <AlertDescription>
            {readOnly
              ? "No agentic resources apply to this project — nothing installed locally or projected from a shared source yet."
              : "No capabilities found in the configured sources."}
          </AlertDescription>
        </Alert>
        <Button variant="outline" size="sm" className="w-fit gap-2" onClick={() => void onRefresh()} disabled={status === "loading"}>
          <RefreshCw className={cn("size-4", status === "loading" && "animate-spin")} />
          Refresh resources and usage
        </Button>
      </div>
    );
  }

  const context: MatrixContext = {
    tools,
    adapterMap,
    currentMap,
    desired,
    ownership,
    onToggle,
    onToggleMany,
    readOnly,
    readOnlyItemIds,
    lockedSkills,
    workspaceId,
  };
  const stateColumns: CapabilityStateColumn[] = tools.map((tool) => {
    const adapter = adapterMap.get(tool.id);
    return {
      id: tool.id,
      label: tool.label,
      title: adapter?.unavailableReason ?? undefined,
      unavailable: !!adapter && !adapter.available,
    };
  });

  return (
    <CapabilityTable
      items={items}
      stateColumns={stateColumns}
      enabledItemIds={enabledItemIds}
      usageStats={usageStats}
      showEnabledOnly={!readOnly}
      locateId={locateId}
      onLocateConsumed={clearLocate}
      onRefresh={() => void onRefresh()}
      refreshing={status === "loading"}
      renderStateCells={(item) => <ManagerStateCells item={item} context={context} />}
      renderAggregateCells={(rows) => <AggregateCells items={rows} context={context} />}
      renderRowActions={(item) => (
        <CapabilityRowActions
          item={item}
          settings={data.settings}
          tools={tools}
          currentMap={currentMap}
          locked={lockedSkills.get(item.id)}
          workspaceId={workspaceId}
        />
      )}
      renderItemMeta={(item) => <ItemMeta item={item} context={context} />}
    />
  );
}

function ManagerStateCells({ item, context }: { item: CapabilityItem; context: MatrixContext }) {
  return (
    <ToolCells
      item={item}
      tools={context.tools}
      adapterMap={context.adapterMap}
      currentMap={context.currentMap}
      desired={context.desired}
      ownership={context.ownership}
      onToggle={context.onToggle}
      readOnly={context.readOnly}
      readOnlyItemIds={context.readOnlyItemIds}
    />
  );
}

function AggregateCells({ items, context }: { items: CapabilityItem[]; context: MatrixContext }) {
  return (
    <>
      {context.tools.map((tool) => {
        const present = items.filter((item) => context.currentMap.has(key(tool.id, item.id)));
        const togglable = present.filter((item) => !context.readOnlyItemIds.has(item.id));
        if (present.length === 0) return <TableCell key={tool.id} className="text-center text-muted-foreground/50">—</TableCell>;
        if (context.readOnly) {
          const complete = isReadOnlyAggregateComplete(items.length, togglable.length);
          return (
            <TableCell
              key={tool.id}
              className={cn(
                "text-center text-xs tabular-nums",
                complete ? "text-success" : "text-muted-foreground",
              )}
            >
              {togglable.length}
            </TableCell>
          );
        }
        if (togglable.length === 0) return <TableCell key={tool.id} className="text-center text-xs tabular-nums text-success">{present.length}</TableCell>;
        const onCount = togglable.filter((item) => context.desired[key(tool.id, item.id)]).length;
        const allOn = onCount === togglable.length;
        const mixed = onCount > 0 && !allOn;
        return (
          <TableCell key={tool.id} className="text-center">
            <Button
              variant="outline"
              size="icon-xs"
              className={cn("size-[26px] rounded-md text-primary", allOn && "border-success/40 bg-success/15 text-success", mixed && "border-primary bg-primary/15")}
              title={`${onCount}/${togglable.length} on — click to ${allOn ? "disable" : "enable"} all`}
              onClick={() => context.onToggleMany(tool.id, togglable.map((item) => item.id), !allOn)}
            >
              {allOn ? <Check className="size-3" /> : mixed ? <Minus className="size-3" /> : null}
            </Button>
          </TableCell>
        );
      })}
    </>
  );
}

function ItemMeta({ item, context }: { item: CapabilityItem; context: MatrixContext }) {
  if (!context.lockedSkills.has(item.id)) return null;
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <Badge variant="outline" className="mr-2 gap-1 text-muted-foreground"><RefreshCw className="size-3" />skills.sh</Badge>
      </TooltipTrigger>
      <TooltipContent>Installed via skills.sh — update from the row menu</TooltipContent>
    </Tooltip>
  );
}
