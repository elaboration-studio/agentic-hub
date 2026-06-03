import React from "react";
import ReactDOM from "react-dom/client";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { App } from "./App";
import { CommandPalette } from "./components/palette/CommandPalette";
import "./index.css";

const rootEl = document.getElementById("root");
if (!rootEl) {
  throw new Error("missing #root element");
}

// One bundle, two windows: the dedicated `palette` window renders the floating
// command panel; every other window renders the main app shell.
const isPalette = getCurrentWindow().label === "palette";
if (isPalette) document.documentElement.classList.add("palette-window");

ReactDOM.createRoot(rootEl).render(
  <React.StrictMode>{isPalette ? <CommandPalette /> : <App />}</React.StrictMode>,
);
