import { Button } from "@/components/ui/button";
import { Progress } from "@/components/ui/progress";
import { useManagerStore } from "@/state/manager";

// Sticky footer shown only when there are pending staged changes.
export function ActionBar() {
  const pending = useManagerStore((s) => s.pendingKeys.length);
  const applying = useManagerStore((s) => s.applying);
  const progress = useManagerStore((s) => s.progress);
  const requestApply = useManagerStore((s) => s.requestApply);
  const resetDesired = useManagerStore((s) => s.resetDesired);

  return (
    <div className="flex items-center justify-between gap-4 border-t bg-card px-6 py-3 shadow-[0_-6px_18px_#00000040]">
      <div className="flex items-center gap-4">
        <span className="font-semibold">
          {pending} pending change{pending === 1 ? "" : "s"}
          {applying && progress
            ? ` · applying ${progress.done}/${progress.total}`
            : applying
              ? " · applying…"
              : ""}
        </span>
        {applying && progress && (
          <Progress
            className="w-40"
            value={progress.total > 0 ? (progress.done / progress.total) * 100 : 0}
          />
        )}
      </div>
      <div className="flex gap-2.5">
        <Button variant="ghost" onClick={resetDesired} disabled={applying}>
          Reset
        </Button>
        <Button onClick={requestApply} disabled={applying}>
          Apply
        </Button>
      </div>
    </div>
  );
}
