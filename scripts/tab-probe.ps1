<#
  Tab probe: real mouse input on the real browser's tab strip; the result is read from the browser's own saved session
  (tab order by address, which tab is active), never from how the strip looks.

    1. three tabs A, B, C (distinct pages) in one window;
    2. drag the background tab A past C: order B C A, and C must still be the active tab (a drag never activates);
    3. drag B, press Escape before releasing: nothing changes;
    4. click B: B becomes active;
    5. drag the active tab B one place right: order C B A, B still active;
    6. middle-click A after the reordering: exactly A closes (order C B);
    7. 10 fast drags back and forth: the order is what the drags say, no tab is lost or duplicated.

  usage: scripts/tab-probe.ps1 -Exe path\to\project-browser.exe [-ReportDir tab-probe]
  exit code: 0 = all checks passed, 1 = a check failed, 2 = could not run
#>
param(
  [Parameter(Mandatory = $true)][string]$Exe,
  [string]$ReportDir = "tab-probe",
  [int]$Port = 8767
)
$ErrorActionPreference = "Stop"
if (-not (Test-Path $Exe)) { Write-Host "executable not found: $Exe"; exit 2 }
$Exe = (Resolve-Path $Exe).Path
New-Item -ItemType Directory -Force -Path $ReportDir | Out-Null
$dataDir = Join-Path ([IO.Path]::GetTempPath()) ("pb-tabs-" + [guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Force -Path $dataDir | Out-Null
$python = (Get-Command python -ErrorAction SilentlyContinue).Source
if (-not $python) { Write-Host "python not found"; exit 2 }

Add-Type @"
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
public static class TP {
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left, Top, Right, Bottom; }
  [StructLayout(LayoutKind.Sequential)] public struct POINT { public int X, Y; }
  public delegate bool EnumProc(IntPtr h, IntPtr p);
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc f, IntPtr p);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
  [DllImport("user32.dll")] public static extern IntPtr GetWindow(IntPtr h, uint cmd);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern bool ClientToScreen(IntPtr h, ref POINT p);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern bool MoveWindow(IntPtr h, int x, int y, int w, int hgt, bool repaint);
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll")] public static extern void mouse_event(uint f, int dx, int dy, uint d, IntPtr e);
  [DllImport("user32.dll")] public static extern void keybd_event(byte vk, byte scan, uint f, IntPtr e);
  [DllImport("user32.dll")] public static extern uint GetDpiForWindow(IntPtr h);
  public static List<IntPtr> Windows(uint pid) {
    var list = new List<IntPtr>();
    EnumWindows((h, p) => { uint wp; GetWindowThreadProcessId(h, out wp); RECT r;
      if (wp == pid && IsWindowVisible(h) && GetWindow(h, 4) == IntPtr.Zero && GetWindowRect(h, out r) && (r.Right - r.Left) > 300) list.Add(h); return true; }, IntPtr.Zero);
    return list;
  }
}
"@

$env:PROJECT_BROWSER_DATA_DIR = $dataDir
$env:PROJECT_BROWSER_SKIP_ONBOARDING = "1"
Remove-Item Env:PROJECT_BROWSER_TEST_SETTINGS -ErrorAction SilentlyContinue
$logPath = Join-Path $dataDir "logs\project-browser.log"
$base = "http://127.0.0.1:$Port"
$checks = [ordered]@{}
$notes = [ordered]@{}
function LogText { if (Test-Path $logPath) { Get-Content $logPath -Raw } else { "" } }
function WaitFor([scriptblock]$cond, [int]$seconds) {
  $deadline = (Get-Date).AddSeconds($seconds)
  while ((Get-Date) -lt $deadline) { if (& $cond) { return $true }; Start-Sleep -Milliseconds 200 }
  return [bool](& $cond)
}
# The saved session of the first window: tab names in order (A, B, C from /t/<name>) and the active one.
function Session {
  $copy = Join-Path $ReportDir "db"; New-Item -ItemType Directory -Force -Path $copy | Out-Null
  $src = Join-Path $dataDir "profiles\default"
  foreach ($f in "browser.db", "browser.db-wal", "browser.db-shm") { if (Test-Path (Join-Path $src $f)) { Copy-Item (Join-Path $src $f) $copy -Force } }
  $q = @'
import sqlite3, sys, json, re
c = sqlite3.connect(sys.argv[1])
row = c.execute("select value from kv where key='session'").fetchone()
s = json.loads(row[0]) if row else {}
def find(o):
    if isinstance(o, dict):
        if isinstance(o.get("tabs"), list): return o
        for v in o.values():
            r = find(v)
            if r: return r
    if isinstance(o, list):
        for v in o:
            r = find(v)
            if r: return r
    return None
w = find(s) or {"tabs": []}
names = []
for t in w["tabs"]:
    m = re.search(r"/t/(\w+)", t.get("url", ""))
    names.append(m.group(1) if m else "?")
active = w.get("active", w.get("activeIndex"))
print(json.dumps({"order": names, "active": names[active] if isinstance(active, int) and 0 <= active < len(names) else None}))
'@
  $out = & $python -I -c $q (Join-Path $copy "browser.db")
  return ($out | ConvertFrom-Json)
}
function Expect([string]$name, [string]$order, [string]$active, [int]$seconds = 6) {
  $last = $null
  $ok = WaitFor { $script:last = Session; (($script:last.order -join " ") -eq $order) -and (-not $active -or $script:last.active -eq $active) } $seconds
  $checks[$name] = $ok
  $notes[$name] = "order: $($script:last.order -join ' '), active: $($script:last.active)"
}
function Pt([int]$x, [int]$y) {
  $s = [TP]::GetDpiForWindow($script:main) / 96.0
  $p = New-Object TP+POINT; $p.X = [int]($x * $s); $p.Y = [int]($y * $s)
  [void][TP]::ClientToScreen($script:main, [ref]$p); return $p
}
function Drag([int]$fromX, [int]$toX, [switch]$Escape) {
  $a = Pt $fromX 20; $b = Pt $toX 20
  [void][TP]::SetCursorPos($a.X, $a.Y); Start-Sleep -Milliseconds 80
  [TP]::mouse_event(0x0002, 0, 0, 0, [IntPtr]::Zero)
  for ($i = 1; $i -le 16; $i++) { [void][TP]::SetCursorPos($a.X + [int](($b.X - $a.X) * $i / 16), $a.Y); Start-Sleep -Milliseconds 15 }
  Start-Sleep -Milliseconds 120
  if ($Escape) { [TP]::keybd_event(0x1B, 0, 0, [IntPtr]::Zero); [TP]::keybd_event(0x1B, 0, 2, [IntPtr]::Zero); Start-Sleep -Milliseconds 120 }
  [TP]::mouse_event(0x0004, 0, 0, 0, [IntPtr]::Zero)
}
function Click([int]$x, [uint32]$down, [uint32]$up) {
  $p = Pt $x 20; [void][TP]::SetCursorPos($p.X, $p.Y); Start-Sleep -Milliseconds 80
  [TP]::mouse_event($down, 0, 0, 0, [IntPtr]::Zero); Start-Sleep -Milliseconds 50; [TP]::mouse_event($up, 0, 0, 0, [IntPtr]::Zero)
}

$server = Start-Process -FilePath $python -ArgumentList @("-I", (Join-Path $PSScriptRoot "fixture-server.py"), "$Port") -PassThru -WindowStyle Hidden
$p = $null
try {
  if (-not (WaitFor { try { (Invoke-WebRequest "$base/t/A" -UseBasicParsing -TimeoutSec 2).StatusCode -eq 200 } catch { $false } } 20)) { Write-Host "fixture server did not start"; exit 2 }
  $p = Start-Process -FilePath $Exe -ArgumentList "$base/t/A" -PassThru
  if (-not (WaitFor { (LogText) -match "native hooks installed" } 60)) { Write-Host "the browser did not start"; exit 2 }
  foreach ($n in "B", "C") { $q = Start-Process -FilePath $Exe -ArgumentList "$base/t/$n" -PassThru; [void]$q.WaitForExit(15000) }
  $script:main = [IntPtr]::Zero
  [void](WaitFor { $w = [TP]::Windows([uint32]$p.Id); if ($w.Count -gt 0) { $script:main = $w[0] }; $script:main -ne [IntPtr]::Zero } 20)
  [void][TP]::MoveWindow($script:main, 40, 40, 1280, 800, $true)
  [void][TP]::SetForegroundWindow($script:main)
  Expect "three tabs, C active" "A B C" "C" 20

  # Tab centres in a 1280-wide window with three full-size tabs (about 227 px apart, the first centred near 170).
  $c0 = 170; $c1 = 397; $c2 = 624
  Drag $c0 ($c2 + 90)
  Expect "background tab dragged to the end, active tab unchanged" "B C A" "C"
  Drag $c0 ($c2 + 60) -Escape
  Expect "Escape during a drag changes nothing" "B C A" "C"
  Click $c0 0x0002 0x0004
  Expect "click activates the tab" "B C A" "B"
  Drag $c0 ($c1 + 60)
  Expect "active tab dragged one place right, still active" "C B A" "B"
  Click $c2 0x0020 0x0040
  Expect "middle-click after reordering closes exactly that tab" "C B" "B"
  # Each drag swaps the two tabs; wait until the saved order shows the swap before the next one.
  $want = @("B C", "C B"); $flips = 0
  for ($i = 0; $i -lt 10; $i++) {
    Drag $c0 ($c1 + 70)
    if (WaitFor { ((Session).order -join " ") -eq $want[$i % 2] } 6) { $flips++ }
  }
  $notes["fast drags confirmed"] = "$flips of 10"
  Expect "10 drags in a row: every swap applied, nothing lost or duplicated" "C B" $null 8
  $checks["10 drags in a row: each one confirmed"] = ($flips -eq 10)
  $checks["log: no panic"] = ((LogText) -notmatch "panic:")
}
finally {
  if ($p -and -not $p.HasExited) { Stop-Process -Id $p.Id -Force }
  if ($server -and -not $server.HasExited) { Stop-Process -Id $server.Id -Force }
  if (Test-Path $logPath) { Copy-Item $logPath (Join-Path $ReportDir "project-browser.log") -Force }
}
[ordered]@{ checks = $checks; notes = $notes } | ConvertTo-Json -Depth 4 | Set-Content (Join-Path $ReportDir "summary.json") -Encoding utf8
Write-Host "Tab probe:"
foreach ($c in $checks.GetEnumerator()) { Write-Host ("  [{0}] {1}" -f ($(if ($c.Value) { "pass" } else { "FAIL" })), $c.Key) }
foreach ($c in $notes.GetEnumerator()) { Write-Host ("  ({0}: {1})" -f $c.Key, $c.Value) }
if (@($checks.GetEnumerator() | Where-Object { -not $_.Value }).Count -gt 0) { exit 1 } else { exit 0 }
