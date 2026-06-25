// App-level confirm when applying a suite would remove manually-enabled extras.
import { useMemo, type RefObject } from "react";
import { useApplyStore } from "@/state/apply";
import { useManagerStore } from "@/state/manager";
import { usePaletteStore } from "@/state/palette";
import {
  AlertDialog,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog";
import { Button } from "@/components/ui/button";

type ApplySuiteConfirmDialogProps = {
  /** When set, the dialog content is measured so the palette window can grow. */
  contentRef?: RefObject<HTMLDivElement | null>;
};

export function ApplySuiteConfirmDialog({ contentRef }: ApplySuiteConfirmDialogProps = {}) {
  const pending = useApplyStore((s) => s.pending);
  const confirm = useApplyStore((s) => s.confirm);
  const cancel = useApplyStore((s) => s.cancel);
  const managerData = useManagerStore((s) => s.data);
  const paletteItems = usePaletteStore((s) => s.items);

  const labels = useMemo(() => {
    const managerItems = managerData?.items ?? [];
    const byId = new Map([...managerItems, ...paletteItems].map((it) => [it.id, it.name]));
    return (pending?.extras ?? []).map((id) => byId.get(id) ?? id);
  }, [managerData, paletteItems, pending?.extras]);

  return (
    <AlertDialog open={!!pending} onOpenChange={(o) => !o && cancel()}>
      <AlertDialogContent ref={contentRef}>
        <AlertDialogHeader>
          <AlertDialogTitle>Remove manually enabled capabilities?</AlertDialogTitle>
          <AlertDialogDescription asChild>
            <div className="space-y-2">
              <p>
                Applying &ldquo;{pending?.suiteName}&rdquo; to {pending?.tool} would remove{" "}
                {labels.length} capabilit{labels.length === 1 ? "y" : "ies"} you enabled outside
                this suite:
              </p>
              <ul className="max-h-40 list-disc overflow-auto pl-5 text-sm">
                {labels.map((label) => (
                  <li key={label}>{label}</li>
                ))}
              </ul>
            </div>
          </AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter className="flex-col gap-2 sm:flex-row">
          <AlertDialogCancel>Cancel</AlertDialogCancel>
          <Button variant="outline" onClick={() => void confirm(false)}>
            Remove extras
          </Button>
          <Button onClick={() => void confirm(true)}>Keep extras</Button>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}
