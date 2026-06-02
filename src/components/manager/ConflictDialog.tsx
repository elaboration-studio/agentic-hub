import { useManagerStore } from "@/state/manager";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";

// Confirms the destructive take-over of `foreign_file` targets. Skip applies
// everything else (those enables become skip_conflict in the plan).
export function ConflictDialog() {
  const rows = useManagerStore((s) => s.conflicts);
  const resolve = useManagerStore((s) => s.resolveConflicts);
  const cancel = useManagerStore((s) => s.cancelConflicts);

  const open = !!rows && rows.length > 0;

  return (
    <Dialog open={open} onOpenChange={(o) => !o && cancel()}>
      <DialogContent className="border-destructive/50">
        <DialogHeader>
          <DialogTitle>
            Real files block {rows?.length ?? 0} target{rows?.length === 1 ? "" : "s"}
          </DialogTitle>
          <DialogDescription>
            A real file or folder already exists where these capabilities would be projected. Taking
            over <strong>deletes</strong> the existing file/folder and replaces it. This cannot be
            undone.
          </DialogDescription>
        </DialogHeader>
        <ul className="flex max-h-[40vh] flex-col gap-2 overflow-auto">
          {rows?.map((r, i) => (
            <li key={i} className="flex flex-col gap-0.5 rounded-lg border bg-secondary px-2.5 py-2">
              <span className="font-semibold">
                {r.name} <span className="text-xs text-muted-foreground">in {r.toolLabel}</span>
              </span>
              <code className="break-all font-mono text-[11px] text-muted-foreground">
                {r.targetPath}
              </code>
            </li>
          ))}
        </ul>
        <DialogFooter>
          <Button variant="ghost" onClick={() => resolve(false)}>
            Skip these
          </Button>
          <Button variant="destructive" onClick={() => resolve(true)}>
            Delete &amp; take over
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
