import { useCallback, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { toast } from "sonner";
import { addSource, scaffoldDemo } from "@/ipc";
import { messageOf } from "@/shared";
import { useManagerStore } from "@/state/manager";
import { Button } from "@/components/ui/button";

export function EmptyState() {
  const data = useManagerStore((s) => s.data);
  const refresh = useManagerStore((s) => s.refresh);
  const destination = data?.settings.sources[0]?.path ?? data?.settings.sharedRoot ?? "";
  const [busy, setBusy] = useState(false);
  const [summary, setSummary] = useState("");

  const onScaffold = useCallback(async () => {
    setBusy(true);
    setSummary("");
    try {
      const r = await scaffoldDemo("merge");
      setSummary(`Wrote ${r.written}, skipped ${r.skipped} into ${r.destinationRoot}.`);
      await refresh();
    } catch (e) {
      toast.error(messageOf(e));
    } finally {
      setBusy(false);
    }
  }, [refresh]);

  const onAddSource = useCallback(async () => {
    setBusy(true);
    try {
      const picked = await open({ directory: true, multiple: false, title: "Choose a resources root" });
      if (typeof picked !== "string") return;
      const label = picked.split("/").filter(Boolean).pop() ?? picked;
      await addSource(label, picked);
      await refresh();
    } catch (e) {
      toast.error(messageOf(e));
    } finally {
      setBusy(false);
    }
  }, [refresh]);

  return (
    <section className="mx-auto mt-[8vh] max-w-[520px] rounded-2xl border bg-card px-7 py-8 text-center">
      <h2 className="mb-2 text-lg font-semibold tracking-[0.2px]">No capabilities yet</h2>
      <p className="mb-4 text-muted-foreground">
        Bootstrap a starter shared root with example skills, agents, rules, and a hook — then enable
        them per tool from the manager.
      </p>
      <code className="mb-4 inline-block rounded-lg border bg-secondary px-3 py-1.5 font-mono text-xs text-muted-foreground">
        {destination}
      </code>
      <div className="mb-2.5 flex justify-center gap-2.5">
        <Button onClick={() => void onScaffold()} disabled={busy}>
          {busy ? "Working…" : "Scaffold demo resources"}
        </Button>
        <Button variant="ghost" onClick={() => void onAddSource()} disabled={busy}>
          Add a source…
        </Button>
      </div>
      {summary && <p className="text-xs text-muted-foreground">{summary}</p>}
    </section>
  );
}
