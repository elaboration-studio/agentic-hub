import { MoreHorizontal, RefreshCw } from "lucide-react";
import { toast } from "sonner";
import type {
  CapabilityItem,
  Settings,
  ToolCapabilityState,
} from "@/types";
import {
  openLibraryUpdateWindow,
  openPath,
  openUpdateWindow,
  revealPath,
} from "@/ipc";
import {
  editorApp,
  key,
  messageOf,
  originalFile,
  type ToolDef,
} from "@/shared";
import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";

interface LockedSkill {
  name: string;
  source: string;
  sourceId?: string;
  destSubpath?: string;
}

interface CapabilityRowActionsProps {
  item: CapabilityItem;
  settings: Settings;
  tools: ToolDef[];
  currentMap: ReadonlyMap<string, ToolCapabilityState>;
  locked?: LockedSkill;
  workspaceId?: string;
}

export function CapabilityRowActions(props: CapabilityRowActionsProps) {
  const app = editorApp(props.settings);
  const locked = props.locked;
  const lockedSourceId = locked?.sourceId;
  const workspaceId = props.workspaceId;
  const run = (operation: Promise<void>) =>
    void operation.catch((error) => toast.error(messageOf(error)));
  const updateLocked = locked && lockedSourceId
    ? () =>
        openLibraryUpdateWindow(
          "skills.sh",
          locked.source,
          locked.name,
          lockedSourceId,
          locked.destSubpath ?? "",
        )
    : locked && workspaceId
      ? () =>
          openUpdateWindow(
            workspaceId,
            "skills.sh",
            locked.source,
            locked.name,
          )
      : undefined;
  const projected = props.tools
    .map((tool) => ({
      tool,
      state: props.currentMap.get(key(tool.id, props.item.id)),
    }))
    .filter(
      (entry): entry is { tool: ToolDef; state: ToolCapabilityState } =>
        !!entry.state &&
        entry.state.state === "enabled" &&
        !!entry.state.targetPath,
    );

  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button
          variant="ghost"
          size="icon-xs"
          className="ml-2 align-middle opacity-0 transition-opacity group-hover:opacity-100 focus-visible:opacity-100 data-[state=open]:opacity-100"
          title="More actions"
          aria-label={`More actions for ${props.item.name}`}
        >
          <MoreHorizontal className="size-3.5" />
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="start">
        {updateLocked && (
          <>
            <DropdownMenuItem onClick={() => run(updateLocked())}>
              <RefreshCw className="size-3.5" />Update via skills.sh
            </DropdownMenuItem>
            <DropdownMenuSeparator />
          </>
        )}
        <DropdownMenuItem
          onClick={() => run(openPath(originalFile(props.item), app))}
        >
          Open original
        </DropdownMenuItem>
        <DropdownMenuItem onClick={() => run(revealPath(props.item.sourcePath))}>
          Reveal in Finder
        </DropdownMenuItem>
        {projected.length > 0 && <DropdownMenuSeparator />}
        {projected.map(({ tool, state }) => (
          <DropdownMenuItem
            key={tool.id}
            onClick={() => run(openPath(state.targetPath))}
          >
            Open in {tool.label}
          </DropdownMenuItem>
        ))}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
