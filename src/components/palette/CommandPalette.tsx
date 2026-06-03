// The floating command palette UI. Renders into the dedicated `palette` window
// (see main.tsx). Thin view over usePaletteStore: search input, keyboard nav,
// and a result list. Esc or losing focus dismisses the window.

import { useEffect, useRef } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { Search } from "lucide-react";
import { usePaletteStore } from "@/state/palette";
import { Badge } from "@/components/ui/badge";
import { cn } from "@/lib/utils";

export function CommandPalette() {
  const status = usePaletteStore((s) => s.status);
  const query = usePaletteStore((s) => s.query);
  const results = usePaletteStore((s) => s.results);
  const selectedIndex = usePaletteStore((s) => s.selectedIndex);
  const load = usePaletteStore((s) => s.load);
  const setQuery = usePaletteStore((s) => s.setQuery);
  const move = usePaletteStore((s) => s.move);
  const setSelected = usePaletteStore((s) => s.setSelected);
  const runSelected = usePaletteStore((s) => s.runSelected);
  const reset = usePaletteStore((s) => s.reset);

  const inputRef = useRef<HTMLInputElement>(null);

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
  const runAndHide = () => void runSelected().then(hide, hide);

  const onKeyDown = (e: React.KeyboardEvent) => {
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
        runAndHide();
        break;
      case "Escape":
        e.preventDefault();
        hide();
        break;
    }
  };

  const hasResults = status !== "error" && results.length > 0;

  return (
    <div className="flex h-full w-full items-start justify-center p-3" onKeyDown={onKeyDown}>
      <div className="flex max-h-full w-full flex-col overflow-hidden rounded-xl border border-border bg-popover/95 text-popover-foreground shadow-2xl backdrop-blur-xl">
        <div className="flex items-center gap-3 px-4 py-3.5">
          <Search className="size-4 shrink-0 text-muted-foreground" aria-hidden />
          <input
            ref={inputRef}
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder="Search resources or type a command…"
            className="w-full bg-transparent text-[15px] leading-none outline-none placeholder:text-muted-foreground"
            spellCheck={false}
            autoFocus
          />
        </div>
        {hasResults ? (
          <ul className="max-h-[360px] overflow-auto border-t border-border p-1.5">
            {results.map((item, i) => (
              <li key={item.id}>
                <button
                  type="button"
                  className={cn(
                    "flex w-full items-center gap-3 rounded-lg px-3 py-2 text-left transition-colors",
                    i === selectedIndex ? "bg-accent text-accent-foreground" : "hover:bg-accent/60",
                  )}
                  onMouseMove={() => setSelected(i)}
                  onClick={() => {
                    setSelected(i);
                    runAndHide();
                  }}
                >
                  <span className="min-w-0 flex-1">
                    <span className="block truncate font-medium">{item.title}</span>
                    {item.subtitle && (
                      <span className="block truncate font-mono text-[11px] text-muted-foreground">
                        {item.subtitle}
                      </span>
                    )}
                  </span>
                  <Badge variant="outline" className="shrink-0 text-[10px] tracking-wide uppercase">
                    {item.group}
                  </Badge>
                </button>
              </li>
            ))}
          </ul>
        ) : (
          <p className="border-t border-border px-4 py-6 text-center text-muted-foreground">
            {status === "error"
              ? "Failed to load resources."
              : query.trim()
                ? "No matches."
                : "Type to search resources."}
          </p>
        )}
      </div>
    </div>
  );
}
