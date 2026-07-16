import { create } from "zustand";
import { loadSettings, onColorSchemeChanged, setColorScheme } from "@/ipc";
import type { ColorScheme } from "@/types";

export type ResolvedColorScheme = Exclude<ColorScheme, "system">;

interface AppearanceState {
  preference: ColorScheme;
  resolved: ResolvedColorScheme;
  initialized: boolean;
  initialize: () => Promise<void>;
  applyPreference: (preference: ColorScheme) => void;
  savePreference: (preference: ColorScheme) => Promise<void>;
}

const SYSTEM_DARK_QUERY = "(prefers-color-scheme: dark)";

let systemQuery: MediaQueryList | null = null;
let systemSubscriptionStarted = false;
let initializePromise: Promise<void> | null = null;
let eventSubscriptionStarted = false;

function getSystemQuery(): MediaQueryList | null {
  if (typeof window === "undefined" || typeof window.matchMedia !== "function") {
    return null;
  }
  systemQuery ??= window.matchMedia(SYSTEM_DARK_QUERY);
  return systemQuery;
}

function currentSystemScheme(): ResolvedColorScheme {
  return getSystemQuery()?.matches ? "dark" : "light";
}

function resolvePreference(preference: ColorScheme): ResolvedColorScheme {
  return preference === "system" ? currentSystemScheme() : preference;
}

function applyDocumentTheme(resolved: ResolvedColorScheme): void {
  if (typeof document === "undefined") return;
  document.documentElement.classList.toggle("dark", resolved === "dark");
}

function ensureSubscriptions(): void {
  const query = getSystemQuery();
  if (query && !systemSubscriptionStarted) {
    systemSubscriptionStarted = true;
    query.addEventListener("change", () => {
      const state = useAppearanceStore.getState();
      if (state.preference === "system") state.applyPreference("system");
    });
  }

  if (!eventSubscriptionStarted) {
    eventSubscriptionStarted = true;
    void onColorSchemeChanged((preference) =>
      useAppearanceStore.getState().applyPreference(preference),
    ).catch(() => {
      eventSubscriptionStarted = false;
    });
  }
}

export const useAppearanceStore = create<AppearanceState>((set, get) => ({
  preference: "system",
  resolved: currentSystemScheme(),
  initialized: false,

  applyPreference: (preference) => {
    const resolved = resolvePreference(preference);
    applyDocumentTheme(resolved);
    set({ preference, resolved });
  },

  initialize: () => {
    if (get().initialized) return Promise.resolve();
    if (initializePromise) return initializePromise;

    ensureSubscriptions();
    initializePromise = loadSettings()
      .then((settings) => get().applyPreference(settings.colorScheme))
      .catch(() => get().applyPreference("system"))
      .finally(() => {
        set({ initialized: true });
        initializePromise = null;
      });
    return initializePromise;
  },

  savePreference: async (preference) => {
    await setColorScheme(preference);
    get().applyPreference(preference);
  },
}));
