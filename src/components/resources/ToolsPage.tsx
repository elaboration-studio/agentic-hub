// Tools preflight: a table of high-frequency CLIs the user wants installed (and
// authenticated, where it matters) before building agentic systems. Each row's
// status comes from a Rust-run probe; the first open auto-checks every tool, and
// a per-row or "Refresh all" action re-checks on demand. The Install action
// opens the tool's website. This is the pre-flight before the real work.

import { useEffect } from "react";
import { Download, Loader2, RefreshCw } from "lucide-react";
import { toast } from "sonner";
import { openUrl } from "@/ipc";
import { useCliToolsStore } from "@/state/cliTools";
import type { CliTool, CliToolStatus } from "@/types";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";

const sectionTitle =
  "text-xs font-semibold uppercase tracking-[0.08em] text-muted-foreground";

// Anchor navigation is a no-op inside the WebView; open external links via Rust.
function openExternal(url: string): void {
  void openUrl(url).catch((e) =>
    toast.error(e instanceof Error ? e.message : String(e)),
  );
}

export function ToolsPage() {
  const catalog = useCliToolsStore((s) => s.catalog);
  const statusById = useCliToolsStore((s) => s.statusById);
  const checking = useCliToolsStore((s) => s.checking);
  const loading = useCliToolsStore((s) => s.loading);
  const loaded = useCliToolsStore((s) => s.loaded);
  const loadCatalog = useCliToolsStore((s) => s.loadCatalog);
  const checkOne = useCliToolsStore((s) => s.checkOne);
  const checkAll = useCliToolsStore((s) => s.checkAll);

  // First open: load the catalog, then auto-probe every tool once — but only
  // probe if the load succeeded, so a failed load doesn't fan out checks over an
  // empty catalog. Re-checks are explicit (per-row Refresh or Refresh all).
  useEffect(() => {
    if (loaded) return;
    void (async () => {
      if (await loadCatalog()) await checkAll();
    })();
  }, [loaded, loadCatalog, checkAll]);

  const busy = loading || checking.size > 0;

  // "Refresh all" doubles as the retry path: if the catalog never loaded (load
  // failed), load it first, then probe; otherwise just re-probe.
  const refreshAll = () => {
    void (async () => {
      if (catalog.length === 0) {
        if (await loadCatalog()) await checkAll();
      } else {
        await checkAll();
      }
    })();
  };

  return (
    <div className="flex h-full flex-col gap-[18px]">
      <div className="flex items-center justify-between">
        <h2 className={sectionTitle}>Tools</h2>
        <Button
          variant="outline"
          size="sm"
          className="gap-2"
          onClick={refreshAll}
          disabled={busy}
        >
          <RefreshCw className="size-4" />
          Refresh all
        </Button>
      </div>

      <p className="text-sm text-muted-foreground">
        Pre-flight check for the command-line tools your agents rely on. Install
        and sign in to each before building.
      </p>

      <div className="rounded-lg border bg-card">
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead>Tool</TableHead>
              <TableHead>Status</TableHead>
              <TableHead className="w-px text-right">Action</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {catalog.map((tool) => (
              <ToolRow
                key={tool.id}
                tool={tool}
                status={statusById[tool.id]}
                checking={checking.has(tool.id)}
                onRefresh={() => void checkOne(tool.id)}
              />
            ))}
          </TableBody>
        </Table>
      </div>
    </div>
  );
}

interface ToolRowProps {
  tool: CliTool;
  status: CliToolStatus | undefined;
  checking: boolean;
  onRefresh: () => void;
}

function ToolRow(props: ToolRowProps) {
  const { tool, status, checking, onRefresh } = props;
  const installed = status?.installed ?? false;

  return (
    <TableRow>
      <TableCell>
        <div className="flex min-w-0 flex-col">
          <span className="font-semibold">{tool.name}</span>
          <code className="font-mono text-[11px] text-muted-foreground">
            {tool.check.program}
          </code>
        </div>
      </TableCell>
      <TableCell>
        <StatusCell status={status} checking={checking} />
      </TableCell>
      <TableCell className="text-right">
        <div className="flex items-center justify-end gap-1.5">
          {status && !installed && (
            <Button
              variant="outline"
              size="sm"
              className="gap-1.5"
              onClick={() => openExternal(tool.installUrl)}
            >
              <Download className="size-3.5" />
              Install
            </Button>
          )}
          <Button
            variant="ghost"
            size="icon-sm"
            onClick={onRefresh}
            disabled={checking}
            title="Re-check"
            aria-label={`Re-check ${tool.name}`}
          >
            <RefreshCw className="size-4" />
          </Button>
        </div>
      </TableCell>
    </TableRow>
  );
}

function StatusCell(props: { status: CliToolStatus | undefined; checking: boolean }) {
  const { status, checking } = props;

  if (checking) {
    return (
      <span className="flex items-center gap-1.5 text-sm text-muted-foreground">
        <Loader2 className="size-3.5 animate-spin" />
        Checking…
      </span>
    );
  }
  if (!status) {
    return <span className="text-sm text-muted-foreground">—</span>;
  }
  if (!status.installed) {
    return (
      <div className="flex flex-wrap items-center gap-1.5">
        <Badge variant="outline" className="text-muted-foreground">
          Not installed
        </Badge>
        {status.message && (
          <span className="text-[11px] text-muted-foreground">{status.message}</span>
        )}
      </div>
    );
  }

  return (
    <div className="flex flex-wrap items-center gap-1.5">
      <Badge className="border-success/40 bg-success/15 text-success">
        Installed
      </Badge>
      {status.version && (
        <code className="font-mono text-[11px] text-muted-foreground">
          {status.version}
        </code>
      )}
      <AuthBadge auth={status.auth} />
    </div>
  );
}

function AuthBadge(props: { auth: CliToolStatus["auth"] }) {
  switch (props.auth) {
    case "authed":
      return (
        <Badge className="border-success/40 bg-success/15 text-success">Authed</Badge>
      );
    case "notAuthed":
      return (
        <Badge className="border-warning/40 bg-warning/15 text-warning">
          Needs auth
        </Badge>
      );
    case "unknown":
      return (
        <Badge variant="outline" className="text-muted-foreground">
          Auth unknown
        </Badge>
      );
    case "notApplicable":
      return null;
    default: {
      // Exhaustiveness guard: a newly added AuthState variant fails to compile
      // here until it gets an explicit case above.
      const _exhaustive: never = props.auth;
      void _exhaustive;
      return null;
    }
  }
}
