// Floating action button that opens the dedicated, live-streaming install
// window. A FAB (not a toolbar button) so it stays reachable while the
// inventory matrix scrolls and never crowds the toolbar. In workspace scope
// it opens targeting that workspace (toggleable to Library inside the
// window); in global scope there is no workspace to target, so it opens
// straight into Library scope.

import { Plus } from "lucide-react";
import { toast } from "sonner";
import { openInstallWindow, openLibraryInstallWindow } from "@/ipc";
import { messageOf } from "@/shared";
import { Button } from "@/components/ui/button";

export function InstallFab(props: { workspaceId?: string }) {
  const onClick = async () => {
    try {
      if (props.workspaceId) await openInstallWindow(props.workspaceId);
      else await openLibraryInstallWindow();
    } catch (e) {
      toast.error(messageOf(e));
    }
  };

  return (
    <Button
      size="icon-lg"
      className="fixed bottom-6 right-6 z-30 size-14 rounded-full shadow-lg"
      onClick={() => void onClick()}
      title="Install skill…"
      aria-label="Install skill…"
    >
      <Plus className="size-6" />
    </Button>
  );
}
