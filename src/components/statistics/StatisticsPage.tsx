import { useEffect } from "react";
import { BarChart3, RefreshCw } from "lucide-react";
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
import { TOOL_LABELS } from "@/shared";
import { cn } from "@/lib/utils";
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

function StatTile(props: { label: string; value: string | number; hint?: string }) {
  return (
    <div className="rounded-lg border bg-secondary/40 px-4 py-3">
      <p className={sectionTitle}>{props.label}</p>
      <p className="mt-1 text-2xl font-semibold tabular-nums">{props.value}</p>
      {props.hint && <p className={cn("mt-1", hint)}>{props.hint}</p>}
    </div>
  );
}

export function StatisticsPage() {
  const dashboard = useStatisticsStore((s) => s.dashboard);
  const range = useStatisticsStore((s) => s.range);
  const tracingStatus = useStatisticsStore((s) => s.tracingStatus);
  const loading = useStatisticsStore((s) => s.loading);
  const setRange = useStatisticsStore((s) => s.setRange);
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
              Local skill, command, and agent usage from your traced events.
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
        <>
          <Card className="p-4">
            <CardHeader className="p-0 pb-3">
              <CardTitle className={sectionTitle}>Overview</CardTitle>
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

          <Card className="p-4">
            <CardHeader className="p-0 pb-3">
              <CardTitle className={sectionTitle}>Most used</CardTitle>
            </CardHeader>
            <CardContent className="p-0">
              {dashboard.topCapabilities.length === 0 ? (
                <p className="text-sm text-muted-foreground">No attributed usage yet.</p>
              ) : (
                <Table>
                  <TableHeader>
                    <TableRow>
                      <TableHead>Capability</TableHead>
                      <TableHead>Source</TableHead>
                      <TableHead>Path</TableHead>
                      <TableHead className="text-right">Uses</TableHead>
                      <TableHead>Last used</TableHead>
                      <TableHead>By tool</TableHead>
                    </TableRow>
                  </TableHeader>
                  <TableBody>
                    {dashboard.topCapabilities.map((row) => (
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
                        <TableCell className="text-right tabular-nums">
                          {row.executionCount}
                        </TableCell>
                        <TableCell className="text-xs text-muted-foreground">
                          {row.lastUsedAt ?? "—"}
                        </TableCell>
                        <TableCell className="text-xs text-muted-foreground">
                          {formatBuckets(row)}
                        </TableCell>
                      </TableRow>
                    ))}
                  </TableBody>
                </Table>
              )}
            </CardContent>
          </Card>

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
        </>
      )}
    </div>
  );
}
