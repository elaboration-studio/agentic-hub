// The skill × tool selection grid for the install window. Pure presentation:
// the parent owns the picks and per-skill status; this renders the table, the
// per-cell checkboxes, and the column "select all for this tool" headers.

import { useCallback } from "react";
import { Check, Loader2, X } from "lucide-react";
import { WORKSPACE_TOOLS } from "@/shared";
import type { SkillFavorite, ToolId } from "@/types";
import { Checkbox } from "@/components/ui/checkbox";

export type InstallStatus = "idle" | "running" | "ok" | "failed" | "cancelled";

interface InstallMatrixProps {
  favorites: SkillFavorite[];
  picks: Record<string, ToolId[]>;
  statuses: Record<string, InstallStatus>;
  disabled: boolean;
  onToggleCell: (favId: string, tool: ToolId, on: boolean) => void;
  onToggleColumn: (tool: ToolId, on: boolean) => void;
}

function StatusBadge({ status }: { status: InstallStatus }) {
  if (status === "running")
    return <Loader2 className="size-3.5 animate-spin text-muted-foreground" aria-label="Installing" />;
  if (status === "ok") return <Check className="size-3.5 text-success" aria-label="Installed" />;
  if (status === "failed") return <X className="size-3.5 text-destructive" aria-label="Failed" />;
  if (status === "cancelled")
    return <X className="size-3.5 text-muted-foreground" aria-label="Cancelled" />;
  return null;
}

export function InstallMatrix(props: InstallMatrixProps) {
  const { favorites, picks, statuses, disabled, onToggleCell, onToggleColumn } = props;

  const columnState = useCallback(
    (tool: ToolId): boolean | "indeterminate" => {
      if (favorites.length === 0) return false;
      const on = favorites.filter((f) => (picks[f.id] ?? []).includes(tool)).length;
      if (on === 0) return false;
      return on === favorites.length ? true : "indeterminate";
    },
    [favorites, picks],
  );

  return (
    <div className="overflow-auto rounded-lg border">
      <table className="w-full text-sm">
        <thead className="sticky top-0 z-10 bg-secondary/80 backdrop-blur">
          <tr className="border-b text-left">
            <th className="px-3 py-2 font-medium">Skill</th>
            {WORKSPACE_TOOLS.map((t) => (
              <th key={t.id} className="px-3 py-2 text-center font-medium">
                <label className="flex cursor-pointer flex-col items-center gap-1">
                  <span>{t.label}</span>
                  <Checkbox
                    checked={columnState(t.id)}
                    disabled={disabled}
                    onCheckedChange={(v) => onToggleColumn(t.id, v === true)}
                    aria-label={`Install all skills for ${t.label}`}
                  />
                </label>
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {favorites.map((f) => (
            <tr key={`${f.provider}:${f.id}`} className="border-b last:border-0">
              <td className="px-3 py-2">
                <div className="flex min-w-0 items-center gap-2">
                  <StatusBadge status={statuses[f.id] ?? "idle"} />
                  <div className="flex min-w-0 flex-col">
                    <span className="truncate font-semibold">{f.name}</span>
                    <code className="truncate font-mono text-[11px] text-muted-foreground">
                      {f.installRef}
                    </code>
                  </div>
                </div>
              </td>
              {WORKSPACE_TOOLS.map((t) => (
                <td key={t.id} className="px-3 py-2 text-center">
                  <Checkbox
                    checked={(picks[f.id] ?? []).includes(t.id)}
                    disabled={disabled}
                    onCheckedChange={(v) => onToggleCell(f.id, t.id, v === true)}
                    aria-label={`Install ${f.name} for ${t.label}`}
                  />
                </td>
              ))}
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
