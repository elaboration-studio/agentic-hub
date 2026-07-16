import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ColorScheme, Settings } from "@/types";

const { loadSettings, onColorSchemeChanged, setColorScheme } = vi.hoisted(() => ({
  loadSettings: vi.fn(),
  onColorSchemeChanged: vi.fn(),
  setColorScheme: vi.fn(),
}));

vi.mock("@/ipc", () => ({ loadSettings, onColorSchemeChanged, setColorScheme }));

let colorSchemeListener: ((preference: ColorScheme) => void) | undefined;

interface DomHarness {
  classToggle: ReturnType<typeof vi.fn>;
  setSystemDark: (matches: boolean) => void;
}

function installDom(initialSystemDark = false): DomHarness {
  let matches = initialSystemDark;
  const listeners: Array<() => void> = [];
  const classToggle = vi.fn();

  Object.defineProperty(globalThis, "document", {
    configurable: true,
    value: { documentElement: { classList: { toggle: classToggle } } },
  });
  Object.defineProperty(globalThis, "window", {
    configurable: true,
    value: {
      matchMedia: vi.fn(() => ({
        get matches() {
          return matches;
        },
        addEventListener: (_event: string, listener: () => void) => listeners.push(listener),
      })),
    },
  });

  return {
    classToggle,
    setSystemDark: (next) => {
      matches = next;
      listeners.forEach((listener) => listener());
    },
  };
}

async function loadStore() {
  return (await import("./appearance")).useAppearanceStore;
}

function settings(colorScheme: ColorScheme): Settings {
  return { colorScheme } as Settings;
}

beforeEach(() => {
  vi.clearAllMocks();
  vi.resetModules();
  colorSchemeListener = undefined;
  onColorSchemeChanged.mockImplementation((listener: (preference: ColorScheme) => void) => {
    colorSchemeListener = listener;
    return Promise.resolve(() => {});
  });
});

afterEach(() => {
  Reflect.deleteProperty(globalThis, "window");
  Reflect.deleteProperty(globalThis, "document");
});

describe("appearance store", () => {
  it("applies an explicit persisted preference before marking initialization complete", async () => {
    const dom = installDom();
    loadSettings.mockResolvedValue(settings("dark"));
    const store = await loadStore();

    await store.getState().initialize();

    expect(store.getState()).toMatchObject({ preference: "dark", resolved: "dark", initialized: true });
    expect(dom.classToggle).toHaveBeenLastCalledWith("dark", true);
  });

  it("updates a system preference when the operating-system scheme changes", async () => {
    const dom = installDom(false);
    loadSettings.mockResolvedValue(settings("system"));
    const store = await loadStore();

    await store.getState().initialize();
    dom.setSystemDark(true);

    expect(store.getState()).toMatchObject({ preference: "system", resolved: "dark" });
    expect(dom.classToggle).toHaveBeenLastCalledWith("dark", true);
  });

  it("applies an appearance change broadcast by another window", async () => {
    installDom();
    loadSettings.mockResolvedValue(settings("light"));
    const store = await loadStore();

    await store.getState().initialize();
    colorSchemeListener?.("dark");

    expect(store.getState()).toMatchObject({ preference: "dark", resolved: "dark" });
  });

  it("keeps the current appearance when persistence rejects a change", async () => {
    installDom();
    loadSettings.mockResolvedValue(settings("light"));
    setColorScheme.mockRejectedValue(new Error("write failed"));
    const store = await loadStore();
    await store.getState().initialize();

    await expect(store.getState().savePreference("dark")).rejects.toThrow("write failed");

    expect(store.getState()).toMatchObject({ preference: "light", resolved: "light" });
  });

  it("falls back to the current system appearance when settings cannot load", async () => {
    installDom(true);
    loadSettings.mockRejectedValue(new Error("settings unavailable"));
    const store = await loadStore();

    await store.getState().initialize();

    expect(store.getState()).toMatchObject({ preference: "system", resolved: "dark", initialized: true });
  });
});
