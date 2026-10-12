// Contract tests: the TypeScript side must stay in lock-step with the Rust core.
// They parse the Rust sources, so a renamed command, a new setting or a changed
// payload fails `npm test` here instead of breaking at runtime inside the app.

import { execSync } from "node:child_process";
import { existsSync, readdirSync, readFileSync, statSync } from "node:fs";
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
  ["ai.rs", "Capabilities", "AiCapabilities"],
  ["ai.rs", "ModelInfo", "ModelInfo"],
  ["ai.rs", "ModelsResponse", "ModelsResponse"],
  ["ai.rs", "TestResult", "AiTestResult"],
  ["ai.rs", "ProbeResult", "AiProbeResult"],
  ["vault.rs", "ExportPreview", "ExportPreview"],
  ["vault.rs", "PasswordImportReport", "PasswordImportReport"],
  ["vault.rs", "ProfileSummary", "ProfileSummary"],
  ["vault.rs", "FillResult", "FillResult"],
  ["db/vault.rs", "LoginSummary", "LoginSummary"],
  ["commands.rs", "FillOptions", "FillOptions"],
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
  const vaultRs = read("src-tauri/src/vault.rs");
  const dbVaultRs = read("src-tauri/src/db/vault.rs");

  it("AiConfig (the stored struct) holds the chosen model and nothing secret", () => {
    const start = aiRs.indexOf("pub struct AiConfig {");
    const body = aiRs.slice(start, aiRs.indexOf("\n}", start));
    expect(body).not.toMatch(/key|secret|token/i);
    expect(rustStructFields(aiRs, "AiConfig")).toEqual(["provider", "model"]);
  });

  it("the view sent to the shell exposes only whether a key exists", () => {
    expect(tsInterfaceFields("AiConfigView")).toEqual(["provider", "model", "hasKey", "staleKey", "verified"]);
  });

  it("NVIDIA is the only provider and its key is only ever sent to NVIDIA's own address", () => {
    expect(typesTs).toMatch(/export type AiProvider = "nvidia";/);
    expect(aiRs).toContain('pub const NVIDIA_BASE: &str = "https://integrate.api.nvidia.com/v1";');
    expect(aiRs).toMatch(/redirect\(reqwest::redirect::Policy::none\(\)\)/);
    expect(aiRs.slice(0, aiRs.indexOf("#[cfg(test)]"))).not.toMatch(/api\.openai\.com|api\.anthropic\.com|localhost:11434/);
    expect(read("src/features/ai/aiStore.ts")).not.toMatch(/localStorage|sessionStorage/);
  });

  it("keeps the secret type opaque and the store Windows-backed with no plaintext fallback", () => {
    expect(secretsRs).not.toMatch(/derive\([^)]*(Serialize|Debug)[^)]*\)\]\s*pub struct Secret/);
    expect(secretsRs).toContain("CredWriteW");
    expect(secretsRs).toContain("CredReadW");
    expect(secretsRs).not.toMatch(/kv_set/);
    expect(read("src-tauri/src/lib.rs")).toContain("secrets::os_store()");
  });

  it("never logs or emits a key or a password", () => {
    for (const f of ["ai.rs", "secrets.rs", "commands.rs", "vault.rs", "import.rs", "db/vault.rs"]) {
      const src = read(`src-tauri/src/${f}`);
      for (const line of src.split("\n")) {
        if (/(log::|println!|eprintln!|dbg!)/.test(line)) {
          // Wording like "the AI key" is fine; interpolating a secret-ish variable is not.
          expect(line, `${f}: ${line}`).not.toMatch(/\{[^}]*(key|secret|legacy|token|password)[^}]*\}/i);
          expect(line, `${f}: ${line}`).not.toMatch(/",\s*[^)]*(key|secret|legacy|token|password)/i);
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

  it("saved passwords are sealed with DPAPI, bound to their site, and stored only as opaque blobs", () => {
    expect(vaultRs).toContain("CryptProtectData");
    expect(vaultRs).toContain("CryptUnprotectData");
    expect(vaultRs).toMatch(/fn login_context/);
    const migration = read("src-tauri/src/db/mod.rs");
    const table = migration.slice(migration.indexOf("CREATE TABLE vault_logins"), migration.indexOf("CREATE TABLE vault_profiles"));
    expect(table).toMatch(/sealed\s+BLOB NOT NULL/);
    expect(table).not.toMatch(/password\s+TEXT/i);
    expect(migration.slice(migration.indexOf("CREATE TABLE vault_profiles"))).toMatch(/sealed\s+BLOB NOT NULL/);
  });

  it("no type that holds a password can be serialised, and nothing sent to the shell has a password field", () => {
    expect(vaultRs).toMatch(/pub struct ParsedLogin/);
    const before = vaultRs.slice(0, vaultRs.indexOf("pub struct ParsedLogin")).trimEnd().split("\n").pop() ?? "";
    expect(before).not.toMatch(/Serialize|derive/);
    expect(dbVaultRs.slice(dbVaultRs.indexOf("pub struct LoginSummary"), dbVaultRs.indexOf("}", dbVaultRs.indexOf("pub struct LoginSummary")))).not.toMatch(/sealed|password/);
    expect(typesTs).not.toMatch(/\bpassword\??:\s*string/i);
    expect(read("src/lib/ipc.ts")).not.toMatch(/password\??:\s*string/i);
  });

  it("the shell cannot name a file to read: the picked path stays in the core", () => {
    const sig = commandsRs.slice(commandsRs.indexOf("pub async fn passwords_import("), commandsRs.indexOf("-> Result<PasswordImportReport>"));
    expect(sig).not.toMatch(/path|file:\s*String/i);
    expect(read("src/lib/ipc.ts")).toMatch(/passwordsPickFile: \(\) =>/);
    expect(read("src/lib/ipc.ts")).toMatch(/passwordsImport: \(deleteFile: boolean\)/);
  });

  it("filling is a user action on one tab and one login id, never a password in a request", () => {
    const sig = commandsRs.slice(commandsRs.indexOf("pub async fn fill_login("), commandsRs.indexOf("-> Result<FillResult>", commandsRs.indexOf("pub async fn fill_login(")));
    expect(sig).toContain("login_id: i64");
    expect(sig).not.toMatch(/password/i);
    const js = read("src-tauri/src/native/fill_login.js") + read("src-tauri/src/native/fill_profile.js");
    expect(js).not.toMatch(/\.submit\(|requestSubmit|\.click\(/);
    expect(js).toContain("location.origin !== args.origin");
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
    for (const arg of ["browser", "profile_dir", "bookmarks", "history", "search_engine", "addresses", "session"]) expect(sig).toContain(`${arg}:`);
    expect(ipcTs).toMatch(/call<ImportReport>\("import_run", \{ browser, profileDir, \.\.\.choices \}\)/);
    expect(tsInterfaceFields("ImportChoices").sort()).toEqual(["addresses", "bookmarks", "history", "searchEngine", "session"]);
  });

  it("the file logger is installed at startup", () => {
    expect(read("src-tauri/src/lib.rs")).toContain("logging::init(");
  });
});

// ---- release and update pipeline ----------------------------------------------------------

describe("release pipeline", () => {
  const conf = JSON.parse(read("src-tauri/tauri.conf.json")) as {
    version: string;
    bundle: { createUpdaterArtifacts: boolean; targets: string[] };
    plugins: { updater: { endpoints: string[]; pubkey: string; windows?: { installMode?: string } } };
  };
  const release = read(".github/workflows/release.yml");

  it("uses one version everywhere", () => {
    const cargo = /^version = "(.+)"$/m.exec(read("src-tauri/Cargo.toml"))?.[1];
    const lock = /name = "project-browser"\r?\nversion = "(.+)"/.exec(read("src-tauri/Cargo.lock"))?.[1];
    const pkg = (JSON.parse(read("package.json")) as { version: string }).version;
    expect([pkg, cargo, lock]).toEqual([conf.version, conf.version, conf.version]);
    expect(existsSync(join(root, `release-notes/v${conf.version}.md`)), "release notes for the current version").toBe(true);
  });

  it("points the updater at the GitHub release feed with signed artifacts", () => {
    expect(conf.plugins.updater.endpoints).toEqual(["https://github.com/hamzaedits1907-alt/project-browser/releases/latest/download/latest.json"]);
    expect(Buffer.from(conf.plugins.updater.pubkey, "base64").toString("utf8")).toMatch(/^untrusted comment: minisign public key: [0-9A-F]{16}\n/);
    expect(conf.bundle.createUpdaterArtifacts).toBe(true);
    expect(conf.bundle.targets).toContain("nsis");
    expect(conf.plugins.updater.windows?.installMode).toBe("passive");
  });

  it("never keeps an updater private key in the repository", () => {
    const tracked = execSync("git ls-files", { cwd: root, encoding: "utf8" }).split("\n").filter(Boolean);
    expect(tracked.filter((f) => /\.key$/i.test(f))).toEqual([]);
    for (const f of tracked) {
      if (/\.(png|ico|woff2|exe)$/i.test(f) || f === "package-lock.json") continue;
      const text = readFileSync(join(root, f), "utf8");
      // Built from pieces so this very file does not contain the marker it is looking for.
      expect(text, f).not.toMatch(new RegExp("rsign " + "encrypted secret key|minisign " + "secret key"));
    }
    expect(read(".gitignore")).toMatch(/^\*\.key$/m);
  });

  it("is started only deliberately, and uses the signing secret without ever printing it", () => {
    expect(release).toMatch(/on:\s*\n\s*push:\s*\n\s*tags:\s*\["v\*"\]\s*\n\s*workflow_dispatch:/);
    expect(release).not.toMatch(/pull_request/);
    expect(release).toContain("secrets.TAURI_SIGNING_PRIVATE_KEY");
    expect(release).toMatch(/TAURI_SIGNING_PRIVATE_KEY is not set/);
    expect(release).not.toMatch(/(echo|Write-Host|Write-Output)[^\n]*\$env:TAURI_SIGNING_PRIVATE_KEY\b(?!_PASSWORD)(?![^\n]*IsNullOrWhiteSpace)/);
    expect(release).toMatch(/tauri-apps\/tauri-action@v\d+\.\d+\.\d+/);
    expect(release).toContain("scripts/verify-release.py");
    expect(release).toMatch(/cargo clippy[^\n]*-D warnings/);
    expect(release).toContain("cargo test --locked");
  });

  it("does not expose the updater plugin's own commands to any webview", () => {
    const cap = read("src-tauri/capabilities/shell.json");
    expect(cap).not.toMatch(/updater:/);
    expect(cap).not.toMatch(/process:/);
  });

  it("builds test installers without needing the private key", () => {
    expect(read(".github/workflows/windows-build.yml")).toContain("tauri.ci.conf.json");
    expect(JSON.parse(read("src-tauri/tauri.ci.conf.json"))).toEqual({ bundle: { createUpdaterArtifacts: false } });
  });
});
