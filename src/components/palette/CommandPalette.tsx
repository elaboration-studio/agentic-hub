// The floating command palette UI. Renders into the dedicated `palette` window
// (see main.tsx). Thin view over usePaletteStore: search input, keyboard nav,
// and a result list. The root is a sectioned hub of first-class commands;
// drill-in views (search modes, suite-tools) show a breadcrumb and step back
// on Backspace-with-empty-query. Esc or losing focus dismisses the window.

import { useEffect, useRef } from "react";
import { getCurrentWindow, LogicalSize } from "@tauri-apps/api/window";
import { Check, ChevronLeft, Circle, Lock, Search } from "lucide-react";
import { usePaletteStore, type PaletteView } from "@/state/palette";
import { useApplyStore } from "@/state/apply";
import { ApplySuiteConfirmDialog } from "@/components/suites/ApplySuiteConfirmDialog";
import { MODE_DEFS, searchModeFromShortcut, shouldDismissPaletteAfterRun, type PaletteItem } from "./commands";
import { Badge } from "@/components/ui/badge";
import { cn } from "@/lib/utils";
import { PALETTE_WIDTH, paletteWindowHeight } from "./layout";

/// Breadcrumb label for a drill-in view; null at the root (no breadcrumb).
function breadcrumbLabel(view: PaletteView): string | null {
  if (view.kind === "search") return MODE_DEFS[view.mode].title;
  if (view.kind === "suite-tools") return view.suiteName;
  if (view.kind === "capability-tools") return view.itemName;
  return null;
}

function placeholderFor(view: PaletteView): string {
  if (view.kind === "search") return MODE_DEFS[view.mode].placeholder;
  if (view.kind === "suite-tools") return `Apply “${view.suiteName}” to a tool…`;
  if (view.kind === "capability-tools") return `Toggle “${view.itemName}” for a tool…`;
  return "Search commands or pick an action…";
}

/// Trailing toggle-state accessory for a capability-tools row.
function StateIcon({ state }: { state: NonNullable<PaletteItem["state"]> }) {
  if (state === "locked") return <Lock className="size-3.5 shrink-0 text-muted-foreground" aria-label="Locked by a suite" />;
  if (state === "on") return <Check className="size-4 shrink-0 text-success" aria-label="Enabled" />;
  return <Circle className="size-3.5 shrink-0 text-muted-foreground/50" aria-label="Disabled" />;
}

/// Empty-list message per view. Inside a drill-in mode an empty query is a
/// prompt, not a failure; at the root every row matches the empty query, so an
/// empty list always means the filter excluded everything.
function emptyMessage(view: PaletteView, query: string): string {
  if (view.kind === "root") return "No matching commands.";
  if (!query.trim()) return "Type to search.";
  return "No matches.";
}

export function CommandPalette() {
  const status = usePaletteStore((s) => s.status);
  const query = usePaletteStore((s) => s.query);
  const view = usePaletteStore((s) => s.view);
  const results = usePaletteStore((s) => s.results);
  const selectedIndex = usePaletteStore((s) => s.selectedIndex);
  const load = usePaletteStore((s) => s.load);
  const setQuery = usePaletteStore((s) => s.setQuery);
  const move = usePaletteStore((s) => s.move);
  const setSelected = usePaletteStore((s) => s.setSelected);
  const runSelected = usePaletteStore((s) => s.runSelected);
  const runSelectedAlt = usePaletteStore((s) => s.runSelectedAlt);
  const back = usePaletteStore((s) => s.back);
  const enterMode = usePaletteStore((s) => s.enterMode);
  const reset = usePaletteStore((s) => s.reset);

  const inputRef = useRef<HTMLInputElement>(null);
  const panelRef = useRef<HTMLDivElement>(null);
  const selectedRef = useRef<HTMLButtonElement>(null);
  const breadcrumb = breadcrumbLabel(view);

  useEffect(() => {
    selectedRef.current?.scrollIntoView({ block: "nearest" });
  }, [selectedIndex]);

  // Fit the transparent window to the panel so no dead space below the card
  // reveals the main window behind it (the "stacked layers" look). The panel's
  // height is content-driven, so a ResizeObserver re-syncs on every list change.
  useEffect(() => {
    const panel = panelRef.current;
    if (!panel) return;
    const win = getCurrentWindow();
    const sync = () =>
      void win.setSize(new LogicalSize(PALETTE_WIDTH, paletteWindowHeight(panel.offsetHeight)));
    sync();
    const observer = new ResizeObserver(sync);
    observer.observe(panel);
    return () => observer.disconnect();
  }, []);

  // Load on mount, and re-load + reset on every re-summon (window regains
  // focus) so resource edits are picked up and each summon starts clean.
  useEffect(() => {
    void load();
    inputRef.current?.focus();
    const unlisten = getCurrentWindow().listen("tauri://focus", () => {
      reset();
      void load();
      inputRef.current?.focus();
      inputRef.current?.select();
    });
    return () => void unlisten.then((fn) => fn());
  }, [load, reset]);

  const hide = () => void getCurrentWindow().hide();

  // Run the selected item; dismiss only when it is terminal. Drill-in rows
  // (a suite) set `dismissOnRun: false` so the palette stays open on the
  // suite-tools view. Suite apply also stays open when extras need confirm.
  const runAndHide = (alt = false) => {
    const item = results[selectedIndex];
    const action = alt ? runSelectedAlt() : runSelected();
    void action.then(() => {
      if (
        shouldDismissPaletteAfterRun(item, useApplyStore.getState().pending !== null)
      ) {
        hide();
      }
    }, hide);
  };

  const onKeyDown = (e: React.KeyboardEvent) => {
    // Ctrl+1…7 jump into the matching search mode from any palette view.
    if (e.ctrlKey && !e.metaKey && !e.altKey && !e.shiftKey) {
      const digit = Number(e.key);
      if (digit >= 1 && digit <= 7) {
        const mode = searchModeFromShortcut(digit);
        if (mode) {
          e.preventDefault();
          enterMode(mode);
          return;
        }
      }
    }

    switch (e.key) {
      case "ArrowDown":
        e.preventDefault();
        move(1);
        break;
      case "ArrowUp":
        e.preventDefault();
        move(-1);
        break;
      case "Enter":
        e.preventDefault();
        runAndHide(e.altKey);
        break;
      case "Backspace":
        // Empty query in a drill-in view steps back one level instead of
        // deleting nothing.
        if (breadcrumb !== null && query === "") {
          e.preventDefault();
          back();
        }
        break;
      case "Escape":
        e.preventDefault();
        hide();
        break;
    }
  };

  const hasResults = status !== "error" && results.length > 0;

  return (
    <>
    <div className="flex h-full w-full items-start justify-center p-3" onKeyDown={onKeyDown}>
      <div
        ref={panelRef}
        className="flex w-full flex-col overflow-hidden rounded-xl border border-border bg-popover/95 text-popover-foreground shadow-2xl backdrop-blur-xl"
      >
        <div className="flex items-center gap-3 px-4 py-3.5">
          {breadcrumb !== null ? (
            <button
              type="button"
              onClick={() => back()}
              className="flex shrink-0 items-center gap-1 rounded-md bg-secondary px-2 py-1 text-xs font-medium text-secondary-foreground hover:bg-accent"
            >
              <ChevronLeft className="size-3.5" aria-hidden />
              {breadcrumb}
            </button>
          ) : (
            <Search className="size-4 shrink-0 text-muted-foreground" aria-hidden />
          )}
          <input
            ref={inputRef}
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder={placeholderFor(view)}
            className="w-full bg-transparent text-[15px] leading-none outline-none placeholder:text-muted-foreground"
            spellCheck={false}
            autoFocus
          />
        </div>
        {hasResults ? (
          <ul className="max-h-[360px] overflow-auto border-t border-border p-1.5">
            {results.map((item, i) => (
              <li key={item.id}>
                {item.section && item.section !== results[i - 1]?.section && (
                  <p className="px-3 pb-1 pt-2.5 text-[10px] font-medium uppercase tracking-wider text-muted-foreground">
                    {item.section}
                  </p>
                )}
                <button
                  ref={i === selectedIndex ? selectedRef : undefined}
                  type="button"
                  className={cn(
                    "flex w-full items-center gap-3 rounded-lg px-3 py-2 text-left transition-colors",
                    i === selectedIndex ? "bg-accent text-accent-foreground" : "hover:bg-accent/60",
                  )}
                  onMouseMove={() => setSelected(i)}
                  onClick={(e) => {
                    setSelected(i);
                    runAndHide(e.altKey);
                  }}
                >
                  {item.state && <StateIcon state={item.state} />}
                  <span className="min-w-0 flex-1">
                    <span className="block truncate font-medium">{item.title}</span>
                    {item.subtitle && (
                      <span className="block truncate font-mono text-[11px] text-muted-foreground">
                        {item.subtitle}
                      </span>
                    )}
                  </span>
                  {item.shortcut && (
                    <kbd className="shrink-0 rounded border border-border bg-muted/40 px-1.5 py-0.5 font-mono text-[10px] leading-none text-muted-foreground">
                      {item.shortcut}
                    </kbd>
                  )}
                  <Badge variant="outline" className="shrink-0 text-[10px] tracking-wide uppercase">
                    {item.group}
                  </Badge>
                </button>
              </li>
            ))}
          </ul>
        ) : (
          <p className="border-t border-border px-4 py-6 text-center text-muted-foreground">
            {status === "error" ? "Failed to load resources." : emptyMessage(view, query)}
          </p>
        )}
      </div>
    </div>
    <ApplySuiteConfirmDialog />
  </>
  );
}
