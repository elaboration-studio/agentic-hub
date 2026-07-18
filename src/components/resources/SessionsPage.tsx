// Session Explorer: read-only browse/search of local Codex, Claude Code, and
// Cursor session history. Left pane lists every session (title, tool,
// workspace, last activity); selecting one reads its full transcript live
// from the source — nothing is copied into an Agentic Hub database. See
// docs/features/session-explorer.md.

import { useEffect, useMemo, useState } from "react";
import { Bot, Check, Copy, MessageSquare, MousePointer2, RefreshCw, Search, Terminal } from "lucide-react";
import { toast } from "sonner";
import { useSessionsStore } from "@/state/sessions";
import type { SessionMessage, SessionSummary, UsageDateRange } from "@/types";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Badge } from "@/components/ui/badge";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { cn } from "@/lib/utils";
import { sessionRoleLabel, sessionToMarkdown } from "@/lib/sessionMarkdown";
import { copyText } from "@/ipc";
import { messageOf } from "@/shared";

const sectionTitle =
  "text-xs font-semibold uppercase tracking-[0.08em] text-muted-foreground";

const RANGE_OPTIONS: { value: UsageDateRange; label: string }[] = [
  { value: "today", label: "Today" },
  { value: "last7Days", label: "Last 7 days" },
  { value: "last30Days", label: "Last 30 days" },
  { value: "last90Days", label: "Last 90 days" },
  { value: "allTime", label: "All time" },
];

type ToolFilter = "all" | SessionSummary["tool"];

const TOOL_LABEL: Record<SessionSummary["tool"], string> = {
  codex: "Codex",
  claude: "Claude",
  cursor: "Cursor",
  openclaw: "OpenClaw",
  openstandard: "Open Standard",
  kiro: "Kiro",
  copilot: "Copilot",
  antigravity: "Antigravity",
};

function ToolIcon(props: { tool: SessionSummary["tool"]; className?: string }) {
  if (props.tool === "codex") return <Terminal className={props.className} />;
  if (props.tool === "cursor") return <MousePointer2 className={props.className} />;
  return <Bot className={props.className} />;
}

function formatRelative(iso: string | null): string {
  if (!iso) return "—";
  const then = new Date(iso).getTime();
  if (Number.isNaN(then)) return "—";
  const diffMs = Date.now() - then;
  const minute = 60_000;
  const hour = 60 * minute;
  const day = 24 * hour;
  if (diffMs < minute) return "just now";
  if (diffMs < hour) return `${Math.floor(diffMs / minute)}m ago`;
  if (diffMs < day) return `${Math.floor(diffMs / hour)}h ago`;
  if (diffMs < 7 * day) return `${Math.floor(diffMs / day)}d ago`;
  return new Date(iso).toLocaleDateString();
}

function matchesQuery(session: SessionSummary, query: string): boolean {
  if (query.trim().length < 2) return true;
  const haystack = `${session.title} ${session.workspace ?? ""} ${session.gitBranch ?? ""}`.toLowerCase();
  return haystack.includes(query.trim().toLowerCase());
}

export function SessionsPage() {
  const sessions = useSessionsStore((s) => s.sessions);
  const range = useSessionsStore((s) => s.range);
  const setRange = useSessionsStore((s) => s.setRange);
  const loading = useSessionsStore((s) => s.loading);
  const loaded = useSessionsStore((s) => s.loaded);
  const load = useSessionsStore((s) => s.load);
  const selectedKey = useSessionsStore((s) => s.selectedKey);
  const messages = useSessionsStore((s) => s.messages);
  const messagesLoading = useSessionsStore((s) => s.messagesLoading);
  const messagesError = useSessionsStore((s) => s.messagesError);
  const select = useSessionsStore((s) => s.select);

  const [query, setQuery] = useState("");
  const [toolFilter, setToolFilter] = useState<ToolFilter>("all");

  useEffect(() => {
    if (!loaded) void load();
  }, [loaded, load]);

  const visible = useMemo(
    () =>
      sessions
        .filter((s) => toolFilter === "all" || s.tool === toolFilter)
        .filter((s) => matchesQuery(s, query)),
    [sessions, toolFilter, query],
  );

  const selected = sessions.find((s) => s.sessionKey === selectedKey) ?? null;
  const availableTools = useMemo(
    () => Array.from(new Set(sessions.map((s) => s.tool))),
    [sessions],
  );

  return (
    <div className="flex h-full flex-col gap-[18px]">
      <div className="flex items-center justify-between">
        <h2 className={sectionTitle}>Sessions</h2>
        <div className="flex items-center gap-2">
          <Select value={range} onValueChange={(v) => setRange(v as UsageDateRange)}>
            <SelectTrigger className="w-[150px]" aria-label="Sessions date range">
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
            className="gap-2"
            onClick={() => void load()}
            disabled={loading}
          >
            <RefreshCw className={cn("size-4", loading && "animate-spin")} />
            Refresh
          </Button>
        </div>
      </div>

      <p className="text-sm text-muted-foreground">
        Browse and search your local Codex, Claude Code, and Cursor session history.
        Nothing here is uploaded — content is read live from each tool's own
        files on this machine, narrowed to the selected date range for speed.
      </p>

      <div className="flex flex-1 gap-4 overflow-hidden">
        <div className="flex w-80 shrink-0 flex-col gap-2.5">
          <div className="relative">
            <Search className="pointer-events-none absolute left-2.5 top-1/2 size-3.5 -translate-y-1/2 text-muted-foreground" />
            <Input
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              placeholder="Search title, workspace, branch…"
              className="pl-8"
            />
          </div>

          {availableTools.length > 1 && (
            <div className="flex flex-wrap gap-1.5">
              <FilterChip
                active={toolFilter === "all"}
                onClick={() => setToolFilter("all")}
                label="All"
              />
              {availableTools.map((tool) => (
                <FilterChip
                  key={tool}
                  active={toolFilter === tool}
                  onClick={() => setToolFilter(tool)}
                  label={TOOL_LABEL[tool]}
                />
              ))}
            </div>
          )}

          <div className="flex-1 overflow-y-auto rounded-lg border bg-card">
            {loading && sessions.length === 0 ? (
              <p className="p-4 text-sm text-muted-foreground">Loading sessions…</p>
            ) : visible.length === 0 ? (
              <p className="p-4 text-sm text-muted-foreground">
                {sessions.length === 0
                  ? range === "allTime"
                    ? "No local sessions found for the enabled tools."
                    : "No local sessions in this date range. Try a wider range above."
                  : "No sessions match this filter."}
              </p>
            ) : (
              <ul className="divide-y">
                {visible.map((session) => (
                  <li key={session.sessionKey}>
                    <button
                      type="button"
                      onClick={() => void select(session.sessionKey)}
                      aria-current={session.sessionKey === selectedKey ? "true" : undefined}
                      className={cn(
                        "flex w-full flex-col gap-1 px-3 py-2.5 text-left hover:bg-accent",
                        session.sessionKey === selectedKey && "bg-primary/10",
                      )}
                    >
                      <span className="flex items-center gap-1.5">
                        <ToolIcon
                          tool={session.tool}
                          className="size-3.5 shrink-0 text-muted-foreground"
                        />
                        <span className="truncate text-sm font-medium">
                          {session.title}
                        </span>
                      </span>
                      <span className="flex items-center gap-2 text-[11px] text-muted-foreground">
                        <span className="truncate">
                          {session.workspace ?? "unknown workspace"}
                        </span>
                        <span className="shrink-0">·</span>
                        <span className="shrink-0">{formatRelative(session.updatedAt)}</span>
                        <span className="shrink-0">·</span>
                        <span className="flex shrink-0 items-center gap-0.5">
                          <MessageSquare className="size-3" />
                          {session.messageCount}
                        </span>
                      </span>
                    </button>
                  </li>
                ))}
              </ul>
            )}
          </div>
        </div>

        <div className="flex min-w-0 flex-1 flex-col rounded-lg border bg-card">
          {!selected ? (
            <div className="flex flex-1 items-center justify-center p-6 text-sm text-muted-foreground">
              Select a session to read its transcript.
            </div>
          ) : (
            <SessionDetail
              key={selected.sessionKey}
              session={selected}
              messages={messages}
              loading={messagesLoading}
              error={messagesError}
            />
          )}
        </div>
      </div>
    </div>
  );
}

function FilterChip(props: { active: boolean; onClick: () => void; label: string }) {
  return (
    <button
      type="button"
      onClick={props.onClick}
      aria-pressed={props.active}
      className={cn(
        "rounded-full border px-2.5 py-1 text-[11px] font-medium text-muted-foreground",
        props.active && "border-primary bg-primary/10 text-foreground",
      )}
    >
      {props.label}
    </button>
  );
}

function SessionDetail(props: {
  session: SessionSummary;
  messages: SessionMessage[];
  loading: boolean;
  error: string | null;
}) {
  const { session, messages, loading, error } = props;
  const [copied, setCopied] = useState(false);

  const copyAsMarkdown = async () => {
    try {
      await copyText(sessionToMarkdown(session, messages));
      setCopied(true);
      toast.success("Copied session as Markdown");
      setTimeout(() => setCopied(false), 1500);
    } catch (e) {
      toast.error(messageOf(e));
    }
  };

  return (
    <div className="flex h-full flex-col">
      <div className="flex flex-col gap-1.5 border-b p-3.5">
        <div className="flex items-center justify-between gap-2">
          <div className="flex min-w-0 items-center gap-2">
            <ToolIcon tool={session.tool} className="size-4 shrink-0 text-muted-foreground" />
            <h3 className="truncate font-semibold">{session.title}</h3>
          </div>
          <Button
            variant="outline"
            size="sm"
            className="shrink-0 gap-2"
            onClick={() => void copyAsMarkdown()}
            disabled={loading || messages.length === 0}
          >
            {copied ? <Check className="size-3.5" /> : <Copy className="size-3.5" />}
            Copy as Markdown
          </Button>
        </div>
        <div className="flex flex-wrap items-center gap-1.5 text-[11px] text-muted-foreground">
          <Badge variant="outline">{TOOL_LABEL[session.tool]}</Badge>
          {session.workspace && <span>{session.workspace}</span>}
          {session.gitBranch && <span>· {session.gitBranch}</span>}
          {session.model && <span>· {session.model}</span>}
        </div>
      </div>

      <div className="flex-1 overflow-y-auto p-3.5">
        {loading ? (
          <p className="text-sm text-muted-foreground">Reading transcript…</p>
        ) : error ? (
          <p className="text-sm text-destructive">{error}</p>
        ) : messages.length === 0 ? (
          <p className="text-sm text-muted-foreground">
            This session has no readable messages.
          </p>
        ) : (
          <div className="flex flex-col gap-3">
            {messages.map((message, index) => (
              <MessageBubble key={index} message={message} />
            ))}
          </div>
        )}
      </div>
    </div>
  );
}

function MessageBubble(props: { message: SessionMessage }) {
  const { message } = props;
  const roleLabel = sessionRoleLabel(message);

  return (
    <div className="flex flex-col gap-1">
      <div className="flex items-center gap-2 text-[11px] text-muted-foreground">
        <span className="font-semibold uppercase tracking-wide">{roleLabel}</span>
        {message.timestamp && <span>{new Date(message.timestamp).toLocaleString()}</span>}
      </div>
      <div
        className={cn(
          "whitespace-pre-wrap rounded-md border px-3 py-2 text-sm",
          message.role === "user" && "bg-secondary",
          message.role === "tool" && "font-mono text-[12px]",
        )}
      >
        {message.text}
      </div>
    </div>
  );
}
