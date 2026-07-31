import { useEffect, useState } from "react";
import { toast } from "sonner";
import { saveSettings } from "@/ipc";
import { messageOf } from "@/shared";
import type { Settings } from "@/types";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";

const DEFAULT_SHORTCUTS = {
  hub: "Cmd+Alt+A",
  allResources: "Cmd+Alt+Ctrl+A",
  skills: "Cmd+Alt+Ctrl+S",
  commands: "Cmd+Alt+Ctrl+C",
} as const;

interface ShortcutDraft {
  hub: string;
  allResources: string;
  skills: string;
  commands: string;
}

interface ShortcutPanelProps {
  settings: Settings;
  onChanged: () => void;
}

const fields: ReadonlyArray<{ key: keyof ShortcutDraft; label: string }> = [
  { key: "hub", label: "Open or close palette" },
  { key: "allResources", label: "Search all resources" },
  { key: "skills", label: "Search skills" },
  { key: "commands", label: "Search commands" },
];

function draftFrom(settings: Settings): ShortcutDraft {
  return {
    hub: settings.paletteShortcut,
    ...settings.paletteQuickSearchShortcuts,
  };
}

export function ShortcutPanel({ settings, onChanged }: ShortcutPanelProps) {
  const [draft, setDraft] = useState<ShortcutDraft>(() => draftFrom(settings));
  const [busy, setBusy] = useState(false);

  useEffect(() => setDraft(draftFrom(settings)), [settings]);

  const persist = async (next: ShortcutDraft) => {
    setBusy(true);
    try {
      await saveSettings({
        ...settings,
        paletteShortcut: next.hub,
        paletteQuickSearchShortcuts: {
          allResources: next.allResources,
          skills: next.skills,
          commands: next.commands,
        },
      });
      onChanged();
    } catch (error) {
      toast.error(messageOf(error));
    } finally {
      setBusy(false);
    }
  };

  const hasEmptyValue = Object.values(draft).some((value) => value.trim() === "");
  const defaultsSelected = fields.every(({ key }) => draft[key] === DEFAULT_SHORTCUTS[key]);

  return (
    <Card className="p-4">
      <CardHeader className="p-0">
        <CardTitle className="text-xs font-semibold uppercase tracking-[0.08em] text-muted-foreground">
          Command palette
        </CardTitle>
      </CardHeader>
      <CardContent className="flex flex-col gap-3 p-0">
        <p className="text-xs text-muted-foreground">
          Use <code className="font-mono">+</code> between modifiers and a key. Shortcuts must be
          valid and unique; changes take effect together.
        </p>
        <div className="grid gap-2 sm:grid-cols-[minmax(0,12rem)_minmax(0,16rem)] sm:items-center">
          {fields.map(({ key, label }) => (
            <div key={key} className="contents">
              <Label htmlFor={`palette-shortcut-${key}`}>{label}</Label>
              <Input
                id={`palette-shortcut-${key}`}
                className="font-mono"
                value={draft[key]}
                placeholder={DEFAULT_SHORTCUTS[key]}
                onChange={(event) =>
                  setDraft((current) => ({ ...current, [key]: event.target.value }))
                }
              />
            </div>
          ))}
        </div>
        <div className="flex flex-wrap items-center gap-2.5">
          <Button
            onClick={() =>
              void persist({
                hub: draft.hub.trim(),
                allResources: draft.allResources.trim(),
                skills: draft.skills.trim(),
                commands: draft.commands.trim(),
              })
            }
            disabled={busy || hasEmptyValue}
          >
            Save shortcuts
          </Button>
          <Button
            variant="ghost"
            onClick={() => {
              setDraft(DEFAULT_SHORTCUTS);
              void persist(DEFAULT_SHORTCUTS);
            }}
            disabled={busy || defaultsSelected}
          >
            Reset default set
          </Button>
        </div>
      </CardContent>
    </Card>
  );
}
