// Workspace-scope panel: pick project folders and hard-copy a suite into one.

import { useEffect } from "react";
import { useManagerStore } from "@/state/manager";
import { useWorkspaceStore } from "@/state/workspace";
import { Alert, AlertDescription } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { RadioGroup, RadioGroupItem } from "@/components/ui/radio-group";
import { Label } from "@/components/ui/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { cn } from "@/lib/utils";

const sectionTitle = "text-xs font-semibold uppercase tracking-[0.08em] text-muted-foreground";

export function WorkspacePanel() {
  const tools = useManagerStore((s) => s.workspaceTools);

  const targets = useWorkspaceStore((s) => s.targets);
  const activeId = useWorkspaceStore((s) => s.activeId);
  const suites = useWorkspaceStore((s) => s.suites);
  const suiteId = useWorkspaceStore((s) => s.suiteId);
  const tool = useWorkspaceStore((s) => s.tool);
  const busy = useWorkspaceStore((s) => s.busy);
  const result = useWorkspaceStore((s) => s.result);
  const reload = useWorkspaceStore((s) => s.reload);
  const pick = useWorkspaceStore((s) => s.pick);
  const activate = useWorkspaceStore((s) => s.activate);
  const remove = useWorkspaceStore((s) => s.remove);
  const apply = useWorkspaceStore((s) => s.apply);
  const setSuiteId = useWorkspaceStore((s) => s.setSuiteId);
  const setTool = useWorkspaceStore((s) => s.setTool);

  useEffect(() => {
    void reload();
  }, [reload]);

  useEffect(() => {
    if (!tools.some((t) => t.id === tool)) setTool(tools[0]?.id ?? "codex");
  }, [tools, tool, setTool]);

  return (
    <Card className="p-4">
      <CardHeader className="flex-row items-center justify-between p-0">
        <CardTitle className={sectionTitle}>Workspace Targets</CardTitle>
        <Button variant="ghost" size="sm" onClick={() => void pick()} disabled={busy}>
          Add workspace…
        </Button>
      </CardHeader>
      <CardContent className="flex flex-col gap-4 p-0">
        {targets.length === 0 ? (
          <Alert>
            <AlertDescription>
              No workspace folders yet. Add one to project a suite into it.
            </AlertDescription>
          </Alert>
        ) : (
          <RadioGroup
            value={activeId}
            onValueChange={(id) => void activate(id)}
            className="flex flex-col gap-1.5"
          >
            {targets.map((t) => (
              <div
                key={t.id}
                className={cn(
                  "flex items-center justify-between gap-3 rounded-lg border bg-secondary px-2.5 py-2",
                  t.id === activeId && "border-primary",
                )}
              >
                <Label className="flex min-w-0 flex-1 cursor-pointer items-center gap-2.5">
                  <RadioGroupItem value={t.id} />
                  <span className="font-semibold">{t.label}</span>
                  <code className="truncate font-mono text-xs text-muted-foreground">{t.dir}</code>
                </Label>
                <Button
                  variant="ghost"
                  size="xs"
                  className="text-destructive hover:text-destructive"
                  onClick={() => void remove(t.id)}
                  disabled={busy}
                >
                  Remove
                </Button>
              </div>
            ))}
          </RadioGroup>
        )}

        <div className="flex flex-wrap items-center gap-2.5">
          <Select
            value={suiteId}
            onValueChange={setSuiteId}
            disabled={busy || suites.length === 0}
          >
            <SelectTrigger className="min-w-[200px]">
              <SelectValue placeholder="No suites yet" />
            </SelectTrigger>
            <SelectContent>
              {suites.map((s) => (
                <SelectItem key={s.id} value={s.id}>
                  {s.name} ({s.capabilities.length})
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
          <span className="text-muted-foreground">→</span>
          <Select value={tool} onValueChange={(v) => setTool(v as typeof tool)} disabled={busy}>
            <SelectTrigger className="w-[140px]">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {tools.map((t) => (
                <SelectItem key={t.id} value={t.id}>
                  {t.label}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
          <Button onClick={() => void apply()} disabled={busy || !activeId || !suiteId}>
            {busy ? "Applying…" : "Apply Patch"}
          </Button>
        </div>

        {result && (
          <div className="border-t pt-3 text-[13px]">
            <p>
              Applied <strong>{result.suiteName}</strong> to {result.tool} · {result.applied.length}{" "}
              written, {result.removed.length} cleaned
              {result.skippedStaleIds.length > 0
                ? `, ${result.skippedStaleIds.length} stale skipped`
                : ""}
            </p>
            {result.notes.map((n, i) => (
              <p key={`n${i}`} className="text-warning">
                {n}
              </p>
            ))}
            {result.errors.map((er, i) => (
              <p key={`e${i}`} className="text-destructive">
                {er}
              </p>
            ))}
          </div>
        )}
      </CardContent>
    </Card>
  );
}
