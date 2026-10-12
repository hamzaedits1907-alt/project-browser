import "@fontsource-variable/inter";
import "@fontsource-variable/space-grotesk";
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";

import "./styles/tokens.css";
import "./styles/base.css";
import "./styles/ui.css";
import "./styles/shell.css";
import "./styles/pages.css";
import "./styles/features.css";
import "./styles/tools.css";

import { App } from "./App";
import { applyCachedLook } from "./lib/theme";
import { inTauri } from "./lib/window";

async function start(): Promise<void> {
  // Outside Tauri (plain `npm run dev` in a browser) there is no Rust core: run against a mock so the UI can be developed and screenshotted.
  if (!inTauri && import.meta.env.DEV) {
    const { installMockBackend } = await import("./dev/mockBackend");
    installMockBackend();
  }
  // Paint in the colours this viewer last used before the core has answered (no flash of the default theme).
  applyCachedLook();
  const root = document.getElementById("root");
  if (!root) throw new Error("missing #root");
  createRoot(root).render(
    <StrictMode>
      <App />
    </StrictMode>,
  );
}

void start();
