import { Check } from "lucide-react";
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

// Per-tool toggle cells for one capability row. Shared by the flat and tree
// renderers so toggle behaviour stays identical across views.
export function ToolCells(props: {
  item: CapabilityItem;
  tools: ToolDef[];
  adapterMap: Map<ToolId, AdapterStatus>;
  currentMap: Map<string, ToolCapabilityState>;
  desired: DesiredMap;
  onToggle: (tool: ToolId, itemId: string) => void;
}) {
  const { item, tools, adapterMap, currentMap, desired, onToggle } = props;
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
        const on = desired[k] ?? false;
        const modified = on !== (cur.state === "enabled");
        const abnormal = ABNORMAL[cur.state];
        return (
          <TableCell key={t.id} className="text-center">
            <Button
              variant="outline"
              size="icon-xs"
              className={cn(
                "relative size-[26px] rounded-md text-success",
                on && "border-success/40 bg-success/15",
                modified && "ring-2 ring-primary ring-inset",
                abnormal && "border-warning",
              )}
              title={`current: ${cur.state}${abnormal ? ` — ${abnormal}` : ""}`}
              onClick={() => onToggle(t.id, item.id)}
            >
              {on && <Check className="size-3" />}
              {abnormal && (
                <span className="absolute right-0.5 top-0.5 size-1.5 rounded-full bg-warning" />
              )}
            </Button>
          </TableCell>
        );
      })}
    </>
  );
}
