import { useEffect, useMemo } from "react";
import { ChevronDown, X } from "lucide-react";
import { useCliToolsStore } from "@/state/cliTools";
import { useSuitesStore } from "@/state/suites";
import { AGENT_LIMITS, agentDraftError, utf8Bytes } from "@/state/agentDraft";
import { cn } from "@/lib/utils";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuCheckboxItem,
  DropdownMenuContent,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Textarea } from "@/components/ui/textarea";

// The suite's optional agent block: face, instructions, and the CLIs a run
// needs. Saved with the rest of the suite draft.
export function AgentSection({ titleClassName }: { titleClassName: string }) {
  const agent = useSuitesStore((state) => state.draft?.agent);
  const isCreating = useSuitesStore((state) => state.isCreating);
  const version = useSuitesStore((state) => state.agentVersion);
  const setAgent = useSuitesStore((state) => state.setAgent);
  const toggleRequiredCli = useSuitesStore((state) => state.toggleRequiredCli);
  const catalog = useCliToolsStore((state) => state.catalog);
  const catalogLoaded = useCliToolsStore((state) => state.loaded);
  const loadCatalog = useCliToolsStore((state) => state.loadCatalog);

  useEffect(() => {
    if (!catalogLoaded) void loadCatalog();
  }, [catalogLoaded, loadCatalog]);

  const instructionBytes = useMemo(() => utf8Bytes(agent?.instructions ?? ""), [agent?.instructions]);
  const labelById = useMemo(() => new Map(catalog.map((tool) => [tool.id, tool.name])), [catalog]);

  if (!agent) return null;
  const error = agentDraftError(agent);
  const overLimit = instructionBytes > AGENT_LIMITS.instructionsBytes;
  const cliFull = agent.requiredClis.length >= AGENT_LIMITS.requiredClis;

  return (
    <div className="flex flex-col gap-3 rounded-xl border bg-card p-4">
      <div className="flex items-baseline justify-between gap-3">
        <span className={titleClassName}>Agent</span>
        {!isCreating && version && (
          <span className="text-xs text-muted-foreground" title="Content version of the saved suite, as reported by `ehub agents list`">
            Version <code className="font-mono text-foreground">{version}</code>
          </span>
        )}
      </div>
      <div className="flex flex-wrap items-end gap-3">
        <div className="grid w-24 gap-1">
          <Label htmlFor="agent-emoji" className="text-xs text-muted-foreground">Emoji</Label>
          <Input
            id="agent-emoji"
            value={agent.emoji}
            placeholder="🤖"
            aria-invalid={Array.from(agent.emoji.trim()).length > AGENT_LIMITS.emojiChars}
            onChange={(event) => setAgent({ emoji: event.target.value })}
          />
        </div>
        <div className="grid min-w-[260px] flex-1 gap-1">
          <span className="text-xs text-muted-foreground">Required CLIs</span>
          <div className="flex min-h-9 flex-wrap items-center gap-1.5">
            {agent.requiredClis.map((id) => (
              <Badge key={id} variant="secondary" className="gap-1 pr-1">
                {labelById.get(id) ?? id}
                <button
                  type="button"
                  className="rounded-sm text-muted-foreground hover:text-foreground"
                  aria-label={`Remove ${labelById.get(id) ?? id}`}
                  onClick={() => toggleRequiredCli(id)}
                >
                  <X className="size-3" />
                </button>
              </Badge>
            ))}
            <DropdownMenu>
              <DropdownMenuTrigger asChild>
                <Button variant="outline" size="sm" disabled={catalog.length === 0}>
                  Add CLI <ChevronDown />
                </Button>
              </DropdownMenuTrigger>
              <DropdownMenuContent align="start" className="max-h-72 overflow-y-auto">
                {catalog.map((tool) => {
                  const checked = agent.requiredClis.includes(tool.id);
                  return (
                    <DropdownMenuCheckboxItem
                      key={tool.id}
                      checked={checked}
                      disabled={!checked && cliFull}
                      onSelect={(event) => event.preventDefault()}
                      onCheckedChange={() => toggleRequiredCli(tool.id)}
                    >
                      {tool.name}
                    </DropdownMenuCheckboxItem>
                  );
                })}
              </DropdownMenuContent>
            </DropdownMenu>
          </div>
        </div>
      </div>
      <div className="grid gap-1">
        <Label htmlFor="agent-instructions" className="text-xs text-muted-foreground">Instructions (markdown)</Label>
        <Textarea
          id="agent-instructions"
          value={agent.instructions}
          rows={6}
          className="max-h-96 font-mono text-xs"
          placeholder="Who this agent is and how it works. Prepended to the suite's rules in every run bundle."
          aria-invalid={overLimit}
          onChange={(event) => setAgent({ instructions: event.target.value })}
        />
        <div className="flex items-center justify-between gap-3 text-xs">
          <span className="text-destructive">{error}</span>
          <span className={cn("font-mono tabular-nums text-muted-foreground", overLimit && "text-destructive")}>
            {instructionBytes.toLocaleString()} / {AGENT_LIMITS.instructionsBytes.toLocaleString()} bytes
          </span>
        </div>
      </div>
    </div>
  );
}
