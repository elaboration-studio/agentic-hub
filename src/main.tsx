import React from "react";
import ReactDOM from "react-dom/client";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { App } from "./App";
import { CommandPalette } from "./components/palette/CommandPalette";
import { InstallWindow } from "./components/install/InstallWindow";
import "./index.css";

const rootEl = document.getElementById("root");
if (!rootEl) {
  throw new Error("missing #root element");
}

// One bundle, several windows, keyed by label: the floating `palette` panel, the
// dedicated `install` window, and the main app shell for everything else.
const label = getCurrentWindow().label;
const isPalette = label === "palette";
const isInstall = label === "install";
if (isPalette) document.documentElement.classList.add("palette-window");

ReactDOM.createRoot(rootEl).render(
  <React.StrictMode>
    {isPalette ? <CommandPalette /> : isInstall ? <InstallWindow /> : <App />}
  </React.StrictMode>,
);
