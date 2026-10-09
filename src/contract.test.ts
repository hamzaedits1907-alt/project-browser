// Contract tests: the TypeScript side must stay in lock-step with the Rust core.
// They parse the Rust sources, so a renamed command, a new setting or a changed
// payload fails `npm test` here instead of breaking at runtime inside the app.

import { readdirSync, readFileSync, statSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";

import { COMMANDS } from "./lib/ipc";
import { DEFAULT_SETTINGS } from "./lib/settings";
import { INTERNAL_PAGES } from "./lib/urls";

const root = process.cwd();
const read = (rel: string) => readFileSync(join(root, rel), "utf8");
const camel = (s: string) => s.replace(/_([a-z0-9])/g, (_, c: string) => c.toUpperCase());
const sorted = <T,>(xs: T[]) => [...xs].sort();

const commandsRs = read("src-tauri/src/commands.rs");
const typesTs = read("src/lib/types.ts");
const ipcTs = read("src/lib/ipc.ts");

function rustMacroCommands(): string[] {
  const body = commandsRs.slice(commandsRs.indexOf("generate_handler!["));
  return [...body.matchAll(/\$crate::commands::(\w+),/g)].map((m) => m[1] as string);
}

describe("command registry", () => {
  const rust = rustMacroCommands();

  it("registers every #[tauri::command] in all_commands!", () => {
    const declared = [...commandsRs.matchAll(/#\[tauri::command\]\s*pub async fn (\w+)/g)].map((m) => m[1] as string);
    expect(sorted(declared)).toEqual(sorted(rust));
  });

  it("matches the frontend command list", () => {
    expect(sorted([...COMMANDS])).toEqual(sorted(rust));
  });

  it("has a typed wrapper for every command", () => {
    const used = new Set([...ipcTs.matchAll(/call(?:<[^>]*>)?\("(\w+)"/g)].map((m) => m[1] as string));
    expect(sorted([...used])).toEqual(sorted(rust));
  });

  it("is declared in build.rs so Tauri enforces access control", () => {
    const build = read("src-tauri/build.rs");
    const listed = [...build.slice(build.indexOf("const COMMANDS")).matchAll(/"(\w+)",/g)].map((m) => m[1] as string);
    expect(sorted(listed)).toEqual(sorted(rust));
    expect(build).toContain("app_manifest");
  });

  it("is granted to the shell (and only the shell) in capabilities/shell.json", () => {
    const cap = JSON.parse(read("src-tauri/capabilities/shell.json")) as { webviews: string[]; permissions: string[]; local: boolean; remote?: unknown; windows?: unknown };
    const allowed = cap.permissions.filter((p) => p.startsWith("allow-")).map((p) => p.slice("allow-".length).replace(/-/g, "_"));
    expect(sorted(allowed)).toEqual(sorted(rust));
    expect(cap.webviews).toEqual(["main"]);
    expect(cap.local).toBe(true);
    expect(cap.remote).toBeUndefined();
    expect(cap.windows).toBeUndefined();
    for (const p of cap.permissions) expect(p.startsWith("core:") || p.startsWith("allow-")).toBe(true);
    expect(new Set(cap.permissions).size).toBe(cap.permissions.length);
  });
});

describe("security configuration", () => {
  const conf = JSON.parse(read("src-tauri/tauri.conf.json")) as {
    app: { security: { csp: Record<string, string>; capabilities: string[] }; windows: unknown[] };
    bundle: { targets: string[] };
  };
  it("keeps a strict content security policy for the shell", () => {
    const csp = conf.app.security.csp;
    expect(csp["script-src"]).toBe("'self'");
    for (const v of Object.values(csp)) {
      expect(v).not.toContain("unsafe-eval");
      expect(v).not.toContain("*");
    }
    expect(csp["connect-src"]).toBe("ipc: http://ipc.localhost");
  });
  it("only loads the shell capability and builds a normal Windows installer", () => {
    expect(conf.app.security.capabilities).toEqual(["shell"]);
    expect(conf.bundle.targets).toContain("nsis");
  });
  it("never injects raw HTML or evaluates strings in the shell", () => {
    const files: string[] = [];
    const walk = (dir: string) => {
      for (const name of readdirSync(dir)) {
        const p = join(dir, name);
        if (statSync(p).isDirectory()) walk(p);
        else if (/\.(ts|tsx)$/.test(name) && !name.endsWith(".test.ts")) files.push(p);
      }
    };
    walk(join(root, "src"));
    for (const f of files) {
      const text = readFileSync(f, "utf8");
      expect(text, f).not.toMatch(/dangerouslySetInnerHTML/);
      expect(text, f).not.toMatch(/\.innerHTML\s*=/);
      expect(text, f).not.toMatch(/\beval\s*\(/);
      expect(text, f).not.toMatch(/new Function\(/);
    }
  });
});

// ---- struct field parity ---------------------------------------------------------------

function rustStructFields(source: string, name: string): string[] {
  const start = source.search(new RegExp(`pub struct ${name}\\b[^{]*\\{`));
  if (start < 0) throw new Error(`struct ${name} not found`);
  const open = source.indexOf("{", start);
  const end = source.indexOf("\n}", open);
  const fields: string[] = [];
  let skipNext = false;
  for (const line of source.slice(open + 1, end).split("\n")) {
    const t = line.trim();
    if (t.startsWith("#[serde(skip")) skipNext = true;
    const m = /^pub (\w+):/.exec(t);
    if (m) {
      if (!skipNext) fields.push(camel(m[1] as string));
      skipNext = false;
    }
  }
  return fields;
}

function tsInterfaceFields(name: string): string[] {
  const start = typesTs.search(new RegExp(`export interface ${name}\\b`));
  if (start < 0) throw new Error(`interface ${name} not found`);
  const open = typesTs.indexOf("{", start);
  const end = typesTs.indexOf("\n}", open);
  return typesTs
    .slice(open + 1, end)
    .split("\n")
    .map((l) => /^\s{2}(\w+)\??:/.exec(l)?.[1])
    .filter((x): x is string => !!x);
}

const STRUCTS: [file: string, rust: string, ts: string][] = [
  ["tabs.rs", "Tab", "Tab"],
  ["tabs.rs", "NavError", "NavError"],
  ["controller/mod.rs", "BrowserState", "BrowserState"],
  ["controller/mod.rs", "Bootstrap", "Bootstrap"],
  ["controller/mod.rs", "PermissionPrompt", "PermissionPrompt"],
  ["controller/mod.rs", "FindState", "FindState"],
  ["controller/mod.rs", "StartupMetrics", "StartupMetrics"],
  ["controller/mod.rs", "ContentRect", "ContentRect"],
  ["controller/mod.rs", "Viewport", "Viewport"],
  ["controller/perf.rs", "PerfSample", "PerfSample"],
  ["controller/perf.rs", "ProcessStat", "ProcessStat"],
  ["controller/perf.rs", "TabStat", "TabStat"],
  ["db/downloads.rs", "Download", "Download"],
  ["db/bookmarks.rs", "Bookmark", "Bookmark"],
  ["db/history.rs", "HistoryEntry", "HistoryEntry"],
  ["db/history.rs", "UrlRow", "UrlRow"],
  ["db/permissions.rs", "SitePermission", "SitePermission"],
  ["profile.rs", "ProfileMeta", "ProfileMeta"],
  ["import.rs", "DetectedProfile", "DetectedProfile"],
  ["import.rs", "ImportItem", "ImportItem"],
  ["import.rs", "ImportReport", "ImportReport"],
  ["commands.rs", "Suggestion", "Suggestion"],
  ["commands.rs", "ClosedTabInfo", "ClosedTabInfo"],
  ["commands.rs", "NtpData", "NtpData"],
  ["commands.rs", "ClearOptions", "ClearOptions"],
  ["commands.rs", "ProfilesInfo", "ProfilesInfo"],
  ["commands.rs", "AppInfo", "AppInfo"],
  ["ai.rs", "AiConfigView", "AiConfigView"],
  ["settings.rs", "Settings", "Settings"],
];

describe("payload shapes match the Rust structs", () => {
  for (const [file, rust, ts] of STRUCTS) {
    it(`${rust}`, () => {
      const fields = rustStructFields(read(`src-tauri/src/${file}`), rust);
      expect(sorted(tsInterfaceFields(ts)), `${rust} fields`).toEqual(sorted(fields));
    });
  }
});

describe("secret handling", () => {
  const aiRs = read("src-tauri/src/ai.rs");
  const secretsRs = read("src-tauri/src/secrets.rs");

  it("AiConfig (the stored struct) has no key field that can be serialised", () => {
    const start = aiRs.indexOf("pub struct AiConfig {");
    const body = aiRs.slice(start, aiRs.indexOf("\n}", start));
    expect(body).not.toMatch(/pub api_key/);
    expect(body).toMatch(/skip_serializing\)\]\s*legacy_api_key/);
  });

  it("the view sent to the shell exposes only whether a key exists", () => {
    expect(tsInterfaceFields("AiConfigView")).toEqual(["provider", "baseUrl", "model", "hasKey", "keyMigrationPending"]);
  });

  it("keeps the secret type opaque and the store Windows-backed with no plaintext fallback", () => {
    expect(secretsRs).not.toMatch(/derive\([^)]*(Serialize|Debug)[^)]*\)\]\s*pub struct Secret/);
    expect(secretsRs).toContain("CredWriteW");
    expect(secretsRs).toContain("CredReadW");
    expect(secretsRs).not.toMatch(/kv_set/);
    expect(read("src-tauri/src/lib.rs")).toContain("secrets::os_store()");
  });

  it("never logs or emits a key", () => {
    for (const f of ["ai.rs", "secrets.rs", "commands.rs"]) {
      const src = read(`src-tauri/src/${f}`);
      for (const line of src.split("\n")) {
        if (/(log::|println!|eprintln!|dbg!)/.test(line)) {
          // Wording like "the AI key" is fine; interpolating a secret-ish variable is not.
          expect(line, `${f}: ${line}`).not.toMatch(/\{[^}]*(key|secret|legacy|token)[^}]*\}/i);
          expect(line, `${f}: ${line}`).not.toMatch(/",\s*[^)]*(key|secret|legacy|token)/i);
        }
      }
    }
  });

  it("the shell never keeps the key in state or storage", () => {
    const settings = read("src/pages/settings/AiSettings.tsx");
    expect(settings).not.toMatch(/useState[^\n]*[aA]piKey/);
    expect(settings).not.toMatch(/localStorage|sessionStorage/);
    expect(typesTs).not.toMatch(/apiKey\??:\s*string/);
  });
});

describe("settings defaults", () => {
  it("match Settings::default() in Rust", () => {
    const src = read("src-tauri/src/settings.rs");
    const block = src.slice(src.indexOf("impl Default for Settings"), src.indexOf("impl Settings {"));
    const enumValue: Record<string, string> = { Google: "google", RestoreSession: "restoreSession", Sleep: "sleep" };
    const parsed: Record<string, unknown> = {};
    for (const m of block.matchAll(/^\s+(\w+): (.+),$/gm)) {
      const key = camel(m[1] as string);
      const raw = (m[2] as string).trim();
      let value: unknown;
      if (raw === "true" || raw === "false") value = raw === "true";
      else if (/^\d+$/.test(raw)) value = Number(raw);
      else if (raw === "None") value = null;
      else if (raw === "Vec::new()") value = [];
      else if (/^"[^"]*"\.into\(\)$/.test(raw)) value = raw.slice(1, raw.indexOf('".into'));
      else if (/^\w+::\w+$/.test(raw)) value = enumValue[raw.split("::")[1] as string];
      else continue;
      parsed[key] = value;
    }
    expect(parsed).toEqual(DEFAULT_SETTINGS);
  });
});

describe("events, commands and pages", () => {
  it("every event the core emits is declared on the frontend", () => {
    const sources = ["controller/mod.rs", "controller/perf.rs", "ai.rs", "updates.rs"].map((f) => read(`src-tauri/src/${f}`)).join("\n");
    const emitted = new Set([...sources.matchAll(/"(pb:[a-z]+)"/g)].map((m) => m[1] as string));
    const declared = new Set([...ipcTs.slice(ipcTs.indexOf("interface EventMap")).matchAll(/"(pb:[a-z]+)":/g)].map((m) => m[1] as string));
    expect(sorted([...declared])).toEqual(sorted([...emitted]));
  });

  it("BrowserCommand covers every shortcuts::Command", () => {
    const rs = read("src-tauri/src/shortcuts.rs");
    const body = rs.slice(rs.indexOf("pub enum Command {"), rs.indexOf("\n}", rs.indexOf("pub enum Command {")));
    const variants = [...body.matchAll(/^\s{4}([A-Z]\w*)/gm)].map((m) => (m[1] as string).charAt(0).toLowerCase() + (m[1] as string).slice(1));
    const ts = typesTs.slice(typesTs.indexOf("export type BrowserCommand"));
    const tsVariants = [...ts.matchAll(/type: "(\w+)"/g)].map((m) => m[1] as string);
    expect(sorted(tsVariants)).toEqual(sorted(variants));
  });

  it("internal pages match navigation::INTERNAL_PAGES", () => {
    const nav = read("src-tauri/src/navigation.rs");
    const block = nav.slice(nav.indexOf("pub const INTERNAL_PAGES"), nav.indexOf("];", nav.indexOf("pub const INTERNAL_PAGES")));
    const pages = [...block.matchAll(/"(\w+)"/g)].map((m) => m[1] as string);
    expect(sorted([...INTERNAL_PAGES])).toEqual(sorted(pages));
  });
});

// ---- honesty of the browser-less preview ------------------------------------------------

function sourceFiles(dir: string, out: string[] = []): string[] {
  for (const name of readdirSync(dir)) {
    const p = join(dir, name);
    if (statSync(p).isDirectory()) sourceFiles(p, out);
    else if (/\.(ts|tsx)$/.test(name) && !name.endsWith(".test.ts")) out.push(p);
  }
  return out;
}

describe("development preview", () => {
  const files = sourceFiles(join(root, "src"));

  it("never fakes a browser engine with an iframe, webview tag or screenshot", () => {
    for (const f of files) {
      const text = readFileSync(f, "utf8");
      expect(text, f).not.toMatch(/<iframe/i);
      expect(text, f).not.toMatch(/createElement\(\s*["'](iframe|webview)["']/);
    }
  });

  it("installs the mock core only in development builds outside Tauri, and nothing else imports it", () => {
    const main = read("src/main.tsx");
    expect(main).toMatch(/!inTauri\s*&&\s*import\.meta\.env\.DEV/);
    expect(main).toMatch(/await import\("\.\/dev\/mockBackend"\)/);
    for (const f of files) {
      if (f.endsWith("main.tsx") || f.includes(`${join("src", "dev")}`)) continue;
      expect(readFileSync(f, "utf8"), f).not.toMatch(/dev\/mockBackend/);
    }
  });

  it("shows the preview notice in the page area exactly when the native runtime is absent", () => {
    const host = read("src/components/chrome/ContentHost.tsx");
    expect(host).toMatch(/!inTauri\s*&&\s*tab\s*&&\s*<DevPreviewNotice/);
    expect(read("src/components/chrome/DevPreviewNotice.tsx")).toMatch(/no web engine/i);
  });
});

// ---- native wiring that must not drift --------------------------------------------------

describe("native wiring", () => {
  const windowRs = read("src-tauri/src/window.rs");

  it("the shell and every tab webview use the profile's own WebView2 folder and identical browser arguments", () => {
    const uses = [...windowRs.matchAll(/\.data_directory\(([\w.]+(?:\(\))?(?:\.clone\(\))?)\)/g)].map((m) => m[1] as string);
    expect(uses.length).toBeGreaterThanOrEqual(3); // shell, tab, popup
    for (const u of uses) expect(u).toMatch(/^(profile\.webview_dir\(\)|data_dir(\.clone\(\))?)$/);
    const args = [...windowRs.matchAll(/\.additional_browser_args\(([^)]*)\)/g)].map((m) => m[1] as string);
    expect(new Set(args)).toEqual(new Set(["BROWSER_ARGS"]));
  });

  it("private tabs use WebView2 private mode", () => {
    expect(windowRs).toMatch(/\.incognito\(private\)/);
  });

  it("import options reach the core under the same names", () => {
    const sig = commandsRs.slice(commandsRs.indexOf("pub async fn import_run("), commandsRs.indexOf("-> Result<ImportReport>"));
    for (const arg of ["browser", "profile_dir", "bookmarks", "history", "search_engine"]) expect(sig).toContain(`${arg}:`);
    expect(ipcTs).toMatch(/call<ImportReport>\("import_run", \{ browser, profileDir, bookmarks, history, searchEngine \}\)/);
  });

  it("the file logger is installed at startup", () => {
    expect(read("src-tauri/src/lib.rs")).toContain("logging::init(");
  });
});
