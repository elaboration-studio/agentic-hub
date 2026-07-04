import { Check, Lock } from "lucide-react";
import { TableCell } from "@/components/ui/table";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import type {
  AdapterStatus,
  CapabilityItem,
  ToolCapabilityState,
  ToolId,
} from "@/types";
import type { DesiredMap } from "@/ipc";
import { ABNORMAL, key, type ToolDef } from "@/shared";
import type { OwnershipInfo } from "@/state/manager";

// Per-tool toggle cells for one capability row. Shared by the flat and tree
// renderers so toggle behaviour stays identical across views.
export function ToolCells(props: {
  item: CapabilityItem;
  tools: ToolDef[];
  adapterMap: Map<ToolId, AdapterStatus>;
  currentMap: Map<string, ToolCapabilityState>;
  desired: DesiredMap;
  ownership: Map<string, OwnershipInfo>;
  onToggle: (tool: ToolId, itemId: string) => void;
  // Workspace scope: render a static present/blank indicator, never a toggle.
  readOnly?: boolean;
  // Global installed rows are individually read-only.
  readOnlyItemIds?: ReadonlySet<string>;
}) {
  const {
    item,
    tools,
    adapterMap,
    currentMap,
    desired,
    ownership,
    onToggle,
    readOnly,
    readOnlyItemIds,
  } = props;
  const itemReadOnly = readOnly || readOnlyItemIds?.has(item.id) === true;
  return (
    <>
      {tools.map((t) => {
        const adapter = adapterMap.get(t.id);
        const k = key(t.id, item.id);
        const cur = currentMap.get(k);
        if ((adapter && !adapter.available) || !cur) {
          return (
            <TableCell key={t.id} className="text-center">
              <span className="text-muted-foreground/50">—</span>
            </TableCell>
          );
        }
        // Read-only inventory: presence is the whole story. A plain green check
        // marks an installed resource; absence already fell through to "—".
        if (itemReadOnly) {
          return (
            <TableCell key={t.id} className="text-center">
              <Check className="mx-auto size-3.5 text-success" aria-label="present" />
            </TableCell>
          );
        }
        const on = desired[k] ?? false;
        const modified = on !== (cur.state === "enabled");
        const abnormal = ABNORMAL[cur.state];
        // Suite-managed cells are locked: a binding (or the base suite) owns
        // them. We use `aria-disabled` (not `disabled`) so the cell keeps
        // pointer events — the cursor reads not-allowed and the owning-suite
        // tooltip works on hover. Clicks are no-ops: the store guards owned
        // keys in `toggle`. Locked cells read as an indigo dashed lock, which
        // is visually distinct from a manually enabled green check.
        const owner = ownership.get(k);
        return (
          <TableCell key={t.id} className="text-center">
            <Button
              variant="outline"
              size="icon-xs"
              aria-disabled={owner ? true : undefined}
              className={cn(
                "relative size-[26px] rounded-md",
                !owner && "text-success",
                !owner && on && "border-success/50 bg-success/25",
                !owner && modified && "ring-2 ring-primary ring-inset",
                !owner && abnormal && "border-warning",
                owner &&
                  "cursor-not-allowed border-dashed border-primary/50 bg-primary/10 text-primary hover:bg-primary/10 hover:text-primary",
              )}
              title={
                owner
                  ? `Locked by suite: ${owner.suiteName}${owner.fromBase ? " (base)" : ""}`
                  : `current: ${cur.state}${abnormal ? ` — ${abnormal}` : ""}`
              }
              onClick={() => onToggle(t.id, item.id)}
            >
              {owner ? (
                <Lock className="size-3" />
              ) : (
                on && <Check className="size-3" />
              )}
              {abnormal && !owner && (
                <span className="absolute right-0.5 top-0.5 size-1.5 rounded-full bg-warning" />
              )}
            </Button>
          </TableCell>
        );
      })}
    </>
  );
}
