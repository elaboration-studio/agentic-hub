import { useCallback, useEffect, useMemo, useState } from "react";
import type { CapabilityItem, ToolId } from "@/types";
import { useApplyStore } from "@/state/apply";
import { useManagerStore } from "@/state/manager";
import { suiteRefMatchesItem, useSuitesStore } from "@/state/suites";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { TableCell } from "@/components/ui/table";
import { CapabilityTable } from "@/components/manager/CapabilityTable";
import { CapabilityRowActions } from "@/components/manager/CapabilityRowActions";

const SECTION_TITLE = "text-xs font-semibold uppercase tracking-[0.08em] text-muted-foreground";

interface Confirmation {
  title: string;
  body: string;
  action: () => void;
}

export function SuitesPage() {
  const data = useManagerStore((state) => state.data);
  const allItems = data?.items ?? [];
  const readOnlyItemIds = useManagerStore((state) => state.readOnlyItemIds);
  const tools = useManagerStore((state) => state.tools);
  const currentMap = useManagerStore((state) => state.currentMap);
  const lockedSkills = useManagerStore((state) => state.lockedSkills);
  const usageStats = useManagerStore((state) => state.usageStats);
  const managerStatus = useManagerStore((state) => state.status);
  const refresh = useManagerStore((state) => state.refresh);
  const items = useMemo(
    () => allItems.filter((item) => !readOnlyItemIds.has(item.id) && item.sourceId !== "agentic-hub"),
    [allItems, readOnlyItemIds],
  );

  const suites = useSuitesStore((state) => state.suites);
  const selectedId = useSuitesStore((state) => state.selectedId);
  const isCreating = useSuitesStore((state) => state.isCreating);
  const draft = useSuitesStore((state) => state.draft);
  const busy = useSuitesStore((state) => state.busy);
  const setDraft = useSuitesStore((state) => state.setDraft);
  const setCapabilities = useSuitesStore((state) => state.setCapabilities);
  const removeCapabilities = useSuitesStore((state) => state.removeCapabilities);
  const save = useSuitesStore((state) => state.save);
  const cancelEdit = useSuitesStore((state) => state.cancelEdit);
  const removeSuite = useSuitesStore((state) => state.remove);
  const setBase = useSuitesStore((state) => state.setBase);
  const applyBusy = useApplyStore((state) => state.busy);
  const requestApply = useApplyStore((state) => state.request);
  const runApply = useApplyStore((state) => state.runApply);

  const adapterStatuses = data?.result.adapterStatuses ?? [];
  const availableTools = useMemo(
    () => tools.filter((tool) => adapterStatuses.find((adapter) => adapter.tool === tool.id)?.available),
    [tools, adapterStatuses],
  );
  const [applyTool, setApplyTool] = useState<ToolId>(availableTools[0]?.id ?? "codex");
  const [confirmation, setConfirmation] = useState<Confirmation | null>(null);

  useEffect(() => {
    setApplyTool((current) =>
      availableTools.some((tool) => tool.id === current)
        ? current
        : (availableTools[0]?.id ?? current),
    );
  }, [availableTools]);

  const selectedSuite = suites.find((suite) => suite.id === selectedId);
  const included = useMemo(() => new Set(draft?.capabilities ?? []), [draft]);
  const liveIds = useMemo(() => new Set(items.map((item) => item.id)), [items]);
  const missingIds = useMemo(
    () => {
      const selectedRefs = selectedSuite?.capabilities ?? [];
      return (draft?.capabilities ?? []).filter((capability) => {
        const original = selectedRefs.find((ref) => ref.cap === capability);
        if (!original) return !liveIds.has(capability);
        return !items.some((item) => suiteRefMatchesItem(original, item));
      });
    },
    [draft, selectedSuite, items, liveIds],
  );

  const onApply = useCallback(() => {
    if (!selectedSuite) return;
    if (selectedSuite.capabilities.length === 0) {
      setConfirmation({
        title: "Apply empty suite?",
        body: "This suite is empty. Applying disables every capability for the tool. Continue?",
        action: () => void runApply(applyTool, selectedSuite.id, false),
      });
      return;
    }
    void requestApply(applyTool, selectedSuite.id, selectedSuite.name);
  }, [selectedSuite, applyTool, requestApply, runApply]);

  const onDelete = useCallback(() => {
    setConfirmation({
      title: `Delete suite "${selectedSuite?.name ?? selectedId}"?`,
      body: "This cannot be undone. Existing tool projections are left unchanged.",
      action: () => void removeSuite(),
    });
  }, [selectedSuite, selectedId, removeSuite]);

  if (!draft) {
    return (
      <Alert>
        <AlertTitle>Suites</AlertTitle>
        <AlertDescription>Select a suite from the rail, or create a new one.</AlertDescription>
      </Alert>
    );
  }

  return (
    <section className="flex min-w-0 flex-col gap-4">
      <div className="flex flex-wrap items-end gap-3 rounded-xl border bg-card p-4">
        <div className="grid min-w-[220px] flex-1 gap-1">
          <Label htmlFor="suite-name" className={SECTION_TITLE}>Name</Label>
          <Input id="suite-name" value={draft.name} placeholder="Suite name" onChange={(event) => setDraft({ name: event.target.value })} />
        </div>
        <div className="grid min-w-[260px] flex-[2] gap-1">
          <Label htmlFor="suite-description" className={SECTION_TITLE}>Description</Label>
          <Input id="suite-description" value={draft.description} placeholder="Optional description" onChange={(event) => setDraft({ description: event.target.value })} />
        </div>
        <div className="flex flex-wrap items-center gap-2">
          <Button onClick={() => void save(items)} disabled={busy || !draft.name.trim()}>Save</Button>
          <Button variant="outline" onClick={cancelEdit} disabled={busy}>Cancel</Button>
          {!isCreating && (
            <Button variant="ghost" className="text-destructive hover:text-destructive" onClick={onDelete} disabled={busy}>Delete</Button>
          )}
        </div>
      </div>

      {selectedSuite && !isCreating && (
        <div className="flex flex-wrap items-center gap-2.5 rounded-xl border bg-card p-3">
          <span className={SECTION_TITLE}>Apply</span>
          <Select value={applyTool} onValueChange={(value) => setApplyTool(value as ToolId)} disabled={busy || availableTools.length === 0}>
            <SelectTrigger className="w-[160px]"><SelectValue /></SelectTrigger>
            <SelectContent>
              {availableTools.map((tool) => <SelectItem key={tool.id} value={tool.id}>{tool.label}</SelectItem>)}
            </SelectContent>
          </Select>
          <Button onClick={onApply} disabled={busy || applyBusy || availableTools.length === 0}>Apply Suite</Button>
          <Button
            variant={selectedSuite.isBase ? "secondary" : "outline"}
            className="ml-auto"
            onClick={() => void setBase(selectedSuite.isBase ? null : selectedSuite.id)}
            disabled={busy}
            title="A base suite's capabilities merge into every applied suite"
          >
            {selectedSuite.isBase ? "Unset base" : "Set as base"}
          </Button>
        </div>
      )}

      {missingIds.length > 0 && (
        <Alert className="border-warning/40 bg-warning/10">
          <AlertTitle className="text-warning">{missingIds.length} missing suite reference{missingIds.length === 1 ? "" : "s"}</AlertTitle>
          <AlertDescription className="flex flex-wrap items-center justify-between gap-3">
            <span>These resources are no longer available from the current sources and will be skipped on apply.</span>
            <Button variant="outline" size="sm" onClick={() => removeCapabilities(missingIds)}>Remove missing references</Button>
          </AlertDescription>
        </Alert>
      )}

      <CapabilityTable
        items={items}
        stateColumns={[{ id: "included", label: "Included" }]}
        enabledItemIds={included}
        usageStats={usageStats}
        showEnabledOnly
        enabledOnlyLabel="Included only"
        onRefresh={() => void refresh()}
        refreshing={managerStatus === "loading"}
        renderRowActions={(item) =>
          data ? (
            <CapabilityRowActions
              item={item}
              settings={data.settings}
              tools={tools}
              currentMap={currentMap}
              locked={lockedSkills.get(item.id)}
            />
          ) : null
        }
        renderStateCells={(item) => (
          <TableCell className="text-center">
            <Checkbox
              checked={included.has(item.id)}
              onCheckedChange={(value) => setCapabilities([item.id], value === true)}
              aria-label={`${included.has(item.id) ? "Exclude" : "Include"} ${item.name}`}
            />
          </TableCell>
        )}
        renderAggregateCells={(rows: CapabilityItem[]) => {
          const includedCount = rows.filter((item) => included.has(item.id)).length;
          const checked = includedCount === rows.length;
          const state: boolean | "indeterminate" = includedCount > 0 && !checked ? "indeterminate" : checked;
          return (
            <TableCell className="text-center">
              <Checkbox
                checked={state}
                onCheckedChange={(value) => setCapabilities(rows.map((item) => item.id), value === true)}
                aria-label={`${includedCount}/${rows.length} included`}
              />
            </TableCell>
          );
        }}
      />

      <AlertDialog open={!!confirmation} onOpenChange={(open) => !open && setConfirmation(null)}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>{confirmation?.title}</AlertDialogTitle>
            <AlertDialogDescription>{confirmation?.body}</AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>Cancel</AlertDialogCancel>
            <AlertDialogAction onClick={() => { confirmation?.action(); setConfirmation(null); }}>Continue</AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </section>
  );
}
