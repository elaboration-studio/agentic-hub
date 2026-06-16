// Left rail for the Resources panel: switches the content pane between the
// Tools preflight (always available) and the skills.sh browser (only when that
// source is enabled in Config). Mirrors the Manager's ScopeRail look.

import { Sparkles, Wrench } from "lucide-react";
import { cn } from "@/lib/utils";

export type ResourcePane = "tools" | "skills";

interface ResourcesRailProps {
  pane: ResourcePane;
  onSelect: (pane: ResourcePane) => void;
  skillsEnabled: boolean;
}

export function ResourcesRail(props: ResourcesRailProps) {
  return (
    <aside className="sticky top-0 flex max-h-[calc(100vh-7.5rem)] w-64 shrink-0 flex-col gap-2.5 self-start">
      <span className="text-xs font-semibold uppercase tracking-[0.08em] text-muted-foreground">
        Resources
      </span>
      <ul className="flex flex-col gap-1.5">
        <RailItem
          active={props.pane === "tools"}
          onClick={() => props.onSelect("tools")}
          icon={<Wrench className="size-4 shrink-0 text-muted-foreground" />}
          title="Tools"
          subtitle="CLI preflight"
        />
        {props.skillsEnabled && (
          <RailItem
            active={props.pane === "skills"}
            onClick={() => props.onSelect("skills")}
            icon={<Sparkles className="size-4 shrink-0 text-muted-foreground" />}
            title="Skills"
            subtitle="skills.sh"
          />
        )}
      </ul>
    </aside>
  );
}

interface RailItemProps {
  active: boolean;
  onClick: () => void;
  icon: React.ReactNode;
  title: string;
  subtitle: string;
}

function RailItem(props: RailItemProps) {
  return (
    <li
      className={cn(
        "flex items-center gap-2 rounded-lg border bg-secondary px-2.5 py-2",
        props.active && "border-primary bg-primary/10",
      )}
    >
      <button
        type="button"
        className="flex min-w-0 flex-1 cursor-pointer items-center gap-2.5 text-left"
        onClick={props.onClick}
        aria-pressed={props.active}
      >
        {props.icon}
        <span className="flex min-w-0 flex-col">
          <span className="truncate font-semibold">{props.title}</span>
          <span className="truncate text-[11px] text-muted-foreground">
            {props.subtitle}
          </span>
        </span>
      </button>
    </li>
  );
}
