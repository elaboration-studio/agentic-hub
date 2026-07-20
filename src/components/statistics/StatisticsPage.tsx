import { useEffect, useMemo, type ReactNode } from "react";
import { ArrowDown, BarChart3, RefreshCw } from "lucide-react";
import { ActivityChart } from "@/components/statistics/charts/ActivityChart";
import { KindBreakdownChart } from "@/components/statistics/charts/KindBreakdownChart";
import { SourceToolChart } from "@/components/statistics/charts/SourceToolChart";
import { Alert, AlertDescription } from "@/components/ui/alert";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { TOOL_LABELS, KIND_LABEL, KIND_ORDER, type UsageSort } from "@/shared";
import { formatLocalTimestamp } from "@/lib/format";
import { cn } from "@/lib/utils";
import { compareUsageTopRows } from "@/state/managerSort";
import { useStatisticsStore } from "@/state/statistics";
import type { CapabilityKind, ToolId, UsageDateRange, UsageTopRow } from "@/types";

const sectionTitle = "text-xs font-semibold uppercase tracking-[0.08em] text-muted-foreground";
const hint = "text-xs text-muted-foreground";

const RANGE_OPTIONS: { value: UsageDateRange; label: string }[] = [
  { value: "last7Days", label: "Last 7 days" },
  { value: "last30Days", label: "Last 30 days" },
  { value: "last90Days", label: "Last 90 days" },
  { value: "allTime", label: "All time" },
];

const KIND_BADGE_COLOR: Record<CapabilityKind, string> = {
  skill: "text-primary",
  agent: "text-kind-agent",
  rule: "text-success",
  hook: "text-warning",
  command: "text-kind-command",
};

function sourceToolLabel(sourceTool: string): string {
  if (sourceTool === "agentic-hub") return "Palette";
  return TOOL_LABELS[sourceTool as ToolId] ?? sourceTool;
}

function formatBuckets(row: UsageTopRow): string {
  if (row.toolBuckets.length === 0) return "—";
  return row.toolBuckets
    .map((bucket) => `${sourceToolLabel(bucket.sourceTool)} ${bucket.executionCount}`)
    .join(" · ");
}

function SortableUsageHead(props: {
  label: string;
  active: boolean;
  align?: "left" | "right";
  onClick: () => void;
}) {
  return (
    <TableHead className={props.align === "right" ? "text-right" : undefined}>
      <button
        type="button"
        className={cn(
          "inline-flex items-center gap-1 font-medium transition-colors",
          props.align === "right" && "ml-auto",
          props.active ? "text-foreground" : "text-muted-foreground hover:text-foreground",
        )}
        onClick={props.onClick}
        aria-sort={props.active ? "descending" : "none"}
      >
        {props.label}
        {props.active && <ArrowDown className="size-3.5 shrink-0" aria-hidden />}
      </button>
    </TableHead>
  );
}

function UsageCapabilityTable(props: {
  rows: UsageTopRow[];
  usesLabel: string;
  emptyMessage: string;
  sort: UsageSort;
  onSortChange: (sort: UsageSort) => void;
}) {
  const sortedRows = useMemo(
    () => [...props.rows].sort((a, b) => compareUsageTopRows(a, b, props.sort)),
    [props.rows, props.sort],
  );

  if (sortedRows.length === 0) {
    return <p className="text-sm text-muted-foreground">{props.emptyMessage}</p>;
  }
  return (
    <Table>
      <TableHeader>
        <TableRow>
          <TableHead>Capability</TableHead>
          <TableHead>Source</TableHead>
          <TableHead>Workspace</TableHead>
          <TableHead>Path</TableHead>
          <SortableUsageHead
            label={props.usesLabel}
            active={props.sort === "usageCount"}
            align="right"
            onClick={() => props.onSortChange("usageCount")}
          />
          <SortableUsageHead
            label="Last used"
            active={props.sort === "lastUsed"}
            onClick={() => props.onSortChange("lastUsed")}
          />
          <TableHead>By tool</TableHead>
        </TableRow>
      </TableHeader>
      <TableBody>
        {sortedRows.map((row) => (
          <TableRow
            key={`${row.capabilityScope}:${row.workspaceRoot ?? "global"}:${row.capabilityId}`}
          >
            <TableCell>
              <Badge
                variant="outline"
                className={cn("mr-2 uppercase", KIND_BADGE_COLOR[row.kind as CapabilityKind])}
              >
                {row.kind}
              </Badge>
              {row.name}
            </TableCell>
            <TableCell className="text-muted-foreground">{row.sourceLabel}</TableCell>
            <TableCell className="max-w-52 truncate font-mono text-xs text-muted-foreground">
              {row.workspaceRoot ?? "Global"}
            </TableCell>
            <TableCell className="font-mono text-xs text-muted-foreground">
              {row.relativePath}
            </TableCell>
            <TableCell className="text-right tabular-nums">{row.executionCount}</TableCell>
            <TableCell className="text-xs text-muted-foreground">
              {row.lastUsedAt ? formatLocalTimestamp(row.lastUsedAt) : "—"}
            </TableCell>
            <TableCell className="text-xs text-muted-foreground">{formatBuckets(row)}</TableCell>
          </TableRow>
        ))}
      </TableBody>
    </Table>
  );
}

function StatTile(props: {
  label: string;
  value: string | number;
  hint?: ReactNode;
  labelClassName?: string;
}) {
  return (
    <div className="rounded-lg border bg-secondary/40 px-4 py-3">
      <p className={cn(sectionTitle, props.labelClassName)}>{props.label}</p>
      <p className="mt-1 text-2xl font-semibold tabular-nums">{props.value}</p>
      {props.hint && <p className={cn("mt-1", hint)}>{props.hint}</p>}
    </div>
  );
}

export function StatisticsPage() {
  const inventory = useStatisticsStore((s) => s.inventory);
  const dashboard = useStatisticsStore((s) => s.dashboard);
  const range = useStatisticsStore((s) => s.range);
  const tableSort = useStatisticsStore((s) => s.tableSort);
  const tracingStatus = useStatisticsStore((s) => s.tracingStatus);
  const loading = useStatisticsStore((s) => s.loading);
  const setRange = useStatisticsStore((s) => s.setRange);
  const setTableSort = useStatisticsStore((s) => s.setTableSort);
  const reload = useStatisticsStore((s) => s.reload);

  useEffect(() => {
    void reload();
  }, [reload]);

  const tracingEnabled = tracingStatus?.enabled ?? false;
  const overview = dashboard?.overview;

  return (
    <div className="flex flex-col gap-[18px]">
      <Card className="p-4">
        <div className="flex flex-wrap items-center justify-between gap-3">
          <div>
            <CardTitle className="flex items-center gap-2 text-sm font-semibold">
              <BarChart3 className="size-4 text-primary" />
              Usage statistics
            </CardTitle>
            <p className={cn("mt-1", hint)}>
              Resource inventory and local usage from traced events.
            </p>
          </div>
          <div className="flex items-center gap-2">
            <Select value={range} onValueChange={(v) => setRange(v as UsageDateRange)}>
              <SelectTrigger className="w-[150px]" aria-label="Usage date range">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {RANGE_OPTIONS.map((option) => (
                  <SelectItem key={option.value} value={option.value}>
                    {option.label}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
            <Button
              variant="outline"
              size="sm"
              disabled={loading}
              onClick={() => void reload()}
            >
              <RefreshCw className={cn("size-3.5", loading && "animate-spin")} />
              Refresh
            </Button>
          </div>
        </div>
      </Card>

      {inventory && (
        <Card className="p-4">
          <CardHeader className="p-0 pb-3">
            <CardTitle className={sectionTitle}>Resource inventory</CardTitle>
          </CardHeader>
          <CardContent className="flex flex-col gap-3 p-0">
            <div className="grid gap-3 sm:grid-cols-2 xl:grid-cols-4">
              <StatTile label="Total resources" value={inventory.total} />
              <StatTile
                label="Configured sources"
                value={inventory.sourceCount}
                hint={`${inventory.sourceCount} source${inventory.sourceCount === 1 ? "" : "s"}`}
              />
              <StatTile
                label="Enabled tools"
                value={inventory.enabledTools}
                hint={`${inventory.enabledTools} of ${inventory.totalTools}`}
              />
              <StatTile
                label="Starred skills"
                value={inventory.favoritesCount}
                hint={
                  <a
                    href="#/skills"
                    className="text-primary underline-offset-2 hover:underline"
                  >
                    View in Resources
                  </a>
                }
              />
            </div>
            <div className="grid gap-3 sm:grid-cols-2 md:grid-cols-3 xl:grid-cols-5">
              {KIND_ORDER.map((kind) => (
                <StatTile
                  key={kind}
                  label={KIND_LABEL[kind]}
                  value={inventory.byKind[kind]}
                  labelClassName={KIND_BADGE_COLOR[kind]}
                />
              ))}
            </div>
          </CardContent>
        </Card>
      )}

      {!tracingEnabled && (
        <Alert>
          <AlertDescription>
            Local usage tracing is off. Enable it in{" "}
            <a href="#/config" className="text-primary underline-offset-2 hover:underline">
              Config
            </a>{" "}
            to collect and chart skill, command, and agent usage.
          </AlertDescription>
        </Alert>
      )}

      {tracingEnabled && overview && (
        <Tabs defaultValue="overview">
          <TabsList>
            <TabsTrigger value="overview">Overview</TabsTrigger>
            <TabsTrigger value="activity">Activity</TabsTrigger>
            <TabsTrigger value="top-usage">Top usage</TabsTrigger>
            <TabsTrigger value="unused">Unused</TabsTrigger>
          </TabsList>

          <TabsContent value="overview" className="flex flex-col gap-[18px]">
            <Card className="p-4">
              <CardHeader className="p-0 pb-3">
                <CardTitle className={sectionTitle}>Usage overview</CardTitle>
              </CardHeader>
              <CardContent className="grid gap-3 p-0 sm:grid-cols-2 xl:grid-cols-4">
                <StatTile label="Terminal events" value={overview.terminalEvents} />
                <StatTile
                  label="Used capabilities"
                  value={overview.tracedCapabilities}
                  hint={`${overview.installedCountable} installed`}
                />
                <StatTile label="Unused installed" value={overview.unusedCountable} />
                <StatTile
                  label="Unresolved events"
                  value={overview.unresolvedEvents}
                  hint={
                    tracingStatus?.collectorRunning
                      ? "Collector running"
                      : "Collector stopped"
                  }
                />
              </CardContent>
            </Card>

            <Card className="p-4">
              <CardHeader className="p-0 pb-3">
                <CardTitle className={sectionTitle}>Today's usage</CardTitle>
                <p className={cn("mt-1", hint)}>
                  Capabilities used so far today, regardless of the selected date range.
                </p>
              </CardHeader>
              <CardContent className="p-0">
                <UsageCapabilityTable
                  rows={dashboard.todayTopCapabilities}
                  usesLabel="Uses today"
                  emptyMessage="No usage recorded yet today."
                  sort={tableSort}
                  onSortChange={setTableSort}
                />
              </CardContent>
            </Card>
          </TabsContent>

          <TabsContent value="activity" className="flex flex-col gap-[18px]">
            <div className="grid gap-[18px] lg:grid-cols-2">
              <Card className="p-4">
                <CardHeader className="p-0 pb-3">
                  <CardTitle className={sectionTitle}>Activity over time</CardTitle>
                </CardHeader>
                <CardContent className="p-0">
                  <ActivityChart data={dashboard.byDay} />
                </CardContent>
              </Card>
              <Card className="p-4">
                <CardHeader className="p-0 pb-3">
                  <CardTitle className={sectionTitle}>By source tool</CardTitle>
                </CardHeader>
                <CardContent className="p-0">
                  <SourceToolChart data={dashboard.bySourceTool} />
                </CardContent>
              </Card>
              <Card className="p-4">
                <CardHeader className="p-0 pb-3">
                  <CardTitle className={sectionTitle}>By kind</CardTitle>
                </CardHeader>
                <CardContent className="p-0">
                  <KindBreakdownChart data={dashboard.byKind} />
                </CardContent>
              </Card>
              <Card className="p-4">
                <CardHeader className="p-0 pb-3">
                  <CardTitle className={sectionTitle}>By workspace</CardTitle>
                </CardHeader>
                <CardContent className="p-0">
                  {dashboard.byWorkspace.length === 0 ? (
                    <p className="text-sm text-muted-foreground">No workspace-tagged events.</p>
                  ) : (
                    <Table>
                      <TableHeader>
                        <TableRow>
                          <TableHead>Workspace</TableHead>
                          <TableHead className="text-right">Uses</TableHead>
                        </TableRow>
                      </TableHeader>
                      <TableBody>
                        {dashboard.byWorkspace.map((row) => (
                          <TableRow key={row.workspace}>
                            <TableCell className="font-mono text-xs">{row.workspace}</TableCell>
                            <TableCell className="text-right tabular-nums">
                              {row.executionCount}
                            </TableCell>
                          </TableRow>
                        ))}
                      </TableBody>
                    </Table>
                  )}
                </CardContent>
              </Card>
            </div>
          </TabsContent>

          <TabsContent value="top-usage" className="flex flex-col gap-[18px]">
            <Card className="p-4">
              <CardHeader className="p-0 pb-3">
                <CardTitle className={sectionTitle}>Most used</CardTitle>
              </CardHeader>
              <CardContent className="p-0">
                <UsageCapabilityTable
                  rows={dashboard.topCapabilities}
                  usesLabel="Uses"
                  emptyMessage="No attributed usage yet."
                  sort={tableSort}
                  onSortChange={setTableSort}
                />
              </CardContent>
            </Card>
          </TabsContent>

          <TabsContent value="unused" className="flex flex-col gap-[18px]">
            <Card className="p-4">
              <CardHeader className="p-0 pb-3">
                <CardTitle className={sectionTitle}>Installed but unused</CardTitle>
                <p className={cn("mt-1", hint)}>
                  Skills, commands, and agents with zero attributed usage in this range.
                </p>
              </CardHeader>
              <CardContent className="p-0">
                {dashboard.unusedCapabilities.length === 0 ? (
                  <p className="text-sm text-muted-foreground">Everything countable has been used.</p>
                ) : (
                  <Table>
                    <TableHeader>
                      <TableRow>
                        <TableHead>Capability</TableHead>
                        <TableHead>Source</TableHead>
                        <TableHead>Path</TableHead>
                      </TableRow>
                    </TableHeader>
                    <TableBody>
                      {dashboard.unusedCapabilities.map((row) => (
                        <TableRow key={row.capabilityId}>
                          <TableCell>
                            <Badge
                              variant="outline"
                              className={cn(
                                "mr-2 uppercase",
                                KIND_BADGE_COLOR[row.kind as CapabilityKind],
                              )}
                            >
                              {row.kind}
                            </Badge>
                            {row.name}
                          </TableCell>
                          <TableCell className="text-muted-foreground">{row.sourceLabel}</TableCell>
                          <TableCell className="font-mono text-xs text-muted-foreground">
                            {row.relativePath}
                          </TableCell>
                        </TableRow>
                      ))}
                    </TableBody>
                  </Table>
                )}
              </CardContent>
            </Card>
          </TabsContent>
        </Tabs>
      )}
    </div>
  );
}
