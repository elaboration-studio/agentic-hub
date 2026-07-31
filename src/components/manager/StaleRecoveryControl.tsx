import { TriangleAlert } from "lucide-react";
import { toast } from "sonner";
import type {
  CapabilityItem,
  Settings,
  ToolCapabilityState,
} from "@/types";
import { openPath, revealPath } from "@/ipc";
import { editorApp, messageOf, originalFile, type ToolDef } from "@/shared";
import type { OwnershipInfo } from "@/state/manager";
import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { staleRecoveryActions } from "./staleRecoveryModel";

interface StaleRecoveryControlProps {
  item: CapabilityItem;
  tool: ToolDef;
  state: ToolCapabilityState;
  owner?: OwnershipInfo;
  settings: Settings;
  onStageRefresh: (toolId: ToolDef["id"], itemId: string) => void;
  onResyncSuite: (toolId: ToolDef["id"]) => Promise<void>;
}

export function StaleRecoveryControl({
  item,
  tool,
  state,
  owner,
  settings,
  onStageRefresh,
  onResyncSuite,
}: StaleRecoveryControlProps) {
  const actions = staleRecoveryActions(item.kind, state.state, !!owner);
  const run = (operation: Promise<void>) =>
    void operation.catch((error) => toast.error(messageOf(error)));
  const label = `Stale copy of ${item.name} in ${tool.label}. Open recovery actions.`;

  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button
          variant="outline"
          size="icon-xs"
          className="size-[26px] rounded-md border-warning bg-warning/10 text-warning-foreground hover:bg-warning/20"
          aria-label={label}
          title={label}
        >
          <TriangleAlert className="size-3.5" />
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="center" className="w-72">
        <DropdownMenuLabel className="space-y-1 whitespace-normal">
          <span className="block text-foreground">Stale managed copy</span>
          <span className="block text-xs font-normal text-muted-foreground">
            The target no longer matches its source.
            {owner
              ? ` It is managed by ${owner.suiteName}${owner.fromBase ? " (base)" : ""}.`
              : " Stage a safe refresh, then review and Apply it normally."}
          </span>
        </DropdownMenuLabel>
        <DropdownMenuSeparator />
        {actions.includes("refresh") && (
          <DropdownMenuItem
            onClick={() => {
              onStageRefresh(tool.id, item.id);
              toast.info("Refresh staged. Review the pending change, then Apply.");
            }}
          >
            Refresh from source
          </DropdownMenuItem>
        )}
        {actions.includes("resyncSuite") && (
          <DropdownMenuItem onClick={() => void onResyncSuite(tool.id)}>
            Re-sync current suite binding
          </DropdownMenuItem>
        )}
        <DropdownMenuSeparator />
        {actions.includes("openSource") && (
          <DropdownMenuItem
            onClick={() => run(openPath(originalFile(item), editorApp(settings)))}
          >
            Open source
          </DropdownMenuItem>
        )}
        {actions.includes("revealTarget") && (
          <DropdownMenuItem onClick={() => run(revealPath(state.targetPath))}>
            Reveal target
          </DropdownMenuItem>
        )}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
