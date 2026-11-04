<#
  DevTools probe: drives the real executable with real keyboard input and checks that DevTools opens INSIDE the
  browser window, not as a separate window.

    1. starts the browser on a local animated page and waits until the page area is painted;
    2. presses F12: within a few seconds a window titled "DevTools..." must be a child window of the browser window,
       visible, below the page (docked at the bottom by default), and painted (not black); no top-level DevTools
       window may be left on screen;
    3. presses F12 again: the docked DevTools must be gone.
  Captures are taken with PrintWindow on the browser's own window only (never the screen).

  usage: scripts/devtools-probe.ps1 -Exe path	o\project-browser.exe [-ReportDir devtools-probe]
  exit code: 0 = all checks passed, 1 = a check failed, 2 = could not run
#>
param(
  [Parameter(Mandatory = $true)][string]$Exe,
  [string]$ReportDir = "devtools-probe",
  [string]$DataDir = (Join-Path ([IO.Path]::GetTempPath()) ("pb-probe-" + [guid]::NewGuid().ToString("N")))
)
$ErrorActionPreference = "Stop"
if (-not (Test-Path $Exe)) { Write-Host "executable not found: $Exe"; exit 2 }
New-Item -ItemType Directory -Force -Path $DataDir, $ReportDir | Out-Null

Add-Type -AssemblyName System.Drawing
Add-Type @"
using System;
using System.Text;
using System.Collections.Generic;
using System.Runtime.InteropServices;
public static class Probe {
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left, Top, Right, Bottom; }
  [StructLayout(LayoutKind.Sequential)] public struct POINT { public int X, Y; }
  public delegate bool EnumProc(IntPtr h, IntPtr p);
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc f, IntPtr p);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
  [DllImport("user32.dll")] public static extern bool IsIconic(IntPtr h);
  [DllImport("user32.dll")] public static extern IntPtr GetWindow(IntPtr h, uint cmd);
  [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern int GetWindowText(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern int GetClassName(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern bool ClientToScreen(IntPtr h, ref POINT p);
  [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr h, IntPtr dc, uint flags);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern bool MoveWindow(IntPtr h, int x, int y, int w, int hgt, bool repaint);
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll")] public static extern void mouse_event(uint f, int dx, int dy, uint d, IntPtr e);
  [DllImport("user32.dll")] public static extern void keybd_event(byte vk, byte scan, uint f, IntPtr e);
  [DllImport("user32.dll")] public static extern uint GetDpiForWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern IntPtr WindowFromPoint(POINT p);
  [DllImport("user32.dll")] public static extern bool GetClientRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern IntPtr GetParent(IntPtr h);
  [DllImport("user32.dll")] public static extern bool EnumChildWindows(IntPtr parent, EnumProc f, IntPtr p);
  [DllImport("user32.dll")] public static extern bool ScreenToClient(IntPtr h, ref POINT p);
  // Child windows (at any depth) of $parent whose title starts with "DevTools".
  public static List<IntPtr> DevtoolsChildren(IntPtr parent) {
    var list = new List<IntPtr>();
    EnumChildWindows(parent, (h, p) => { if (Text(h).StartsWith("DevTools")) list.Add(h); return true; }, IntPtr.Zero);
    return list;
  }
  // Top-level DevTools windows of any process.
  public static List<IntPtr> DevtoolsTopLevel() {
    var list = new List<IntPtr>();
    EnumWindows((h, p) => { if (IsWindowVisible(h) && Text(h).StartsWith("DevTools")) list.Add(h); return true; }, IntPtr.Zero);
    return list;
  }
  public static string Text(IntPtr h) { var s = new StringBuilder(256); GetWindowText(h, s, 256); return s.ToString(); }
  public static string Class(IntPtr h) { var s = new StringBuilder(256); GetClassName(h, s, 256); return s.ToString(); }
  // Every top-level window of the process (owned or not, visible or not): for diagnostics.
  public static List<IntPtr> All(uint pid) {
    var list = new List<IntPtr>();
    EnumWindows((h, p) => { uint wp; GetWindowThreadProcessId(h, out wp); if (wp == pid) list.Add(h); return true; }, IntPtr.Zero);
    return list;
  }
  // Browser windows: visible, not minimized, unowned, of a real size.
  public static List<IntPtr> Windows(uint pid) {
    var list = new List<IntPtr>();
    foreach (var h in All(pid)) {
      RECT r;
      if (IsWindowVisible(h) && !IsIconic(h) && GetWindow(h, 4) == IntPtr.Zero && GetWindowRect(h, out r) && (r.Right - r.Left) > 300 && (r.Bottom - r.Top) > 200) list.Add(h);
    }
    return list;
  }
}
"@

$env:PROJECT_BROWSER_DATA_DIR = $DataDir
$env:PROJECT_BROWSER_SKIP_ONBOARDING = "1"
$logPath = Join-Path $DataDir "logs\project-browser.log"
$windowsLog = Join-Path $ReportDir "windows.txt"
# A local animated page (scripts/fixture-server.py): the probe needs a page that visibly runs.
$python = (Get-Command python -ErrorAction SilentlyContinue).Source
$server = Start-Process -FilePath $python -ArgumentList @("-I", ('"' + (Join-Path $PSScriptRoot "fixture-server.py") + '"'), "8766") -PassThru -WindowStyle Hidden
for ($i = 0; $i -lt 50; $i++) { try { if ((Invoke-WebRequest "http://127.0.0.1:8766/anim.html" -UseBasicParsing -TimeoutSec 2).StatusCode -eq 200) { break } } catch { Start-Sleep -Milliseconds 200 } }
$p = Start-Process -FilePath $Exe -ArgumentList "http://127.0.0.1:8766/anim.html" -PassThru
$log = { if (Test-Path $logPath) { Get-Content $logPath -Raw } else { "" } }
$steps = [ordered]@{}
$notes = [ordered]@{}

function Snap([IntPtr]$h) {
  $r = New-Object Probe+RECT
  [void][Probe]::GetWindowRect($h, [ref]$r)
  $w = [Math]::Max(1, $r.Right - $r.Left); $hh = [Math]::Max(1, $r.Bottom - $r.Top)
  $bmp = New-Object System.Drawing.Bitmap $w, $hh
  $g = [System.Drawing.Graphics]::FromImage($bmp)
  $dc = $g.GetHdc()
  [void][Probe]::PrintWindow($h, $dc, 2) # PW_RENDERFULLCONTENT: includes the WebView2 surfaces
  $g.ReleaseHdc($dc); $g.Dispose()
  return $bmp
}
function Capture([IntPtr]$h, [string]$name) {
  $bmp = Snap $h
  $bmp.Save((Join-Path $ReportDir "$name.png"), [System.Drawing.Imaging.ImageFormat]::Png)
  return $bmp
}
# Mean brightness (0-255) of a region given in logical pixels, sampled on a grid.
function Brightness($bmp, [IntPtr]$h, [int]$x, [int]$y, [int]$w, [int]$hh) {
  $s = [Probe]::GetDpiForWindow($h) / 96.0
  $sum = 0.0; $n = 0
  for ($yy = [int]($y * $s); $yy -lt [Math]::Min($bmp.Height, [int](($y + $hh) * $s)); $yy += 9) {
    for ($xx = [int]($x * $s); $xx -lt [Math]::Min($bmp.Width, [int](($x + $w) * $s)); $xx += 9) {
      $c = $bmp.GetPixel($xx, $yy); $sum += ($c.R + $c.G + $c.B) / 3.0; $n++
    }
  }
  if ($n -eq 0) { return 0 } else { return [Math]::Round($sum / $n, 1) }
}
# Mean absolute per-channel difference (0-255) between two captures over a region (logical pixels).
function PixelDiff($a, $b, [IntPtr]$h, [int]$x, [int]$y, [int]$w, [int]$hh) {
  $s = [Probe]::GetDpiForWindow($h) / 96.0
  $sum = 0.0; $n = 0
  $maxX = [Math]::Min($a.Width, $b.Width); $maxY = [Math]::Min($a.Height, $b.Height)
  for ($yy = [int]($y * $s); $yy -lt [Math]::Min($maxY, [int](($y + $hh) * $s)); $yy += 7) {
    for ($xx = [int]($x * $s); $xx -lt [Math]::Min($maxX, [int](($x + $w) * $s)); $xx += 7) {
      $c = $a.GetPixel($xx, $yy); $d = $b.GetPixel($xx, $yy)
      $sum += ([Math]::Abs($c.R - $d.R) + [Math]::Abs($c.G - $d.G) + [Math]::Abs($c.B - $d.B)) / 3.0; $n++
    }
  }
  if ($n -eq 0) { return 0 } else { return [Math]::Round($sum / $n, 1) }
}
function ClientPoint([IntPtr]$h, [int]$x, [int]$y) {
  $scale = [Probe]::GetDpiForWindow($h) / 96.0
  $pt = New-Object Probe+POINT
  $pt.X = [int]($x * $scale); $pt.Y = [int]($y * $scale)
  [void][Probe]::ClientToScreen($h, [ref]$pt)
  return $pt
}
function Click([int]$x, [int]$y, [bool]$right) {
  [void][Probe]::SetCursorPos($x, $y); Start-Sleep -Milliseconds 120
  if ($right) { [Probe]::mouse_event(0x0008, 0, 0, 0, [IntPtr]::Zero); Start-Sleep -Milliseconds 60; [Probe]::mouse_event(0x0010, 0, 0, 0, [IntPtr]::Zero) }
  else { [Probe]::mouse_event(0x0002, 0, 0, 0, [IntPtr]::Zero); Start-Sleep -Milliseconds 60; [Probe]::mouse_event(0x0004, 0, 0, 0, [IntPtr]::Zero) }
}
function Key([byte[]]$keys) {
  foreach ($k in $keys) { [Probe]::keybd_event($k, 0, 0, [IntPtr]::Zero) }
  Start-Sleep -Milliseconds 60
  [array]::Reverse($keys)
  foreach ($k in $keys) { [Probe]::keybd_event($k, 0, 2, [IntPtr]::Zero) }
}
function DumpWindows([string]$label) {
  Add-Content $windowsLog "== $label"
  foreach ($h in [Probe]::All([uint32]$p.Id)) {
    $r = New-Object Probe+RECT; [void][Probe]::GetWindowRect($h, [ref]$r)
    Add-Content $windowsLog ("  {0} class={1} visible={2} iconic={3} owner={4} rect={5},{6} {7}x{8} title='{9}'" -f `
      $h, [Probe]::Class($h), [Probe]::IsWindowVisible($h), [Probe]::IsIconic($h), [Probe]::GetWindow($h, 4), `
      $r.Left, $r.Top, ($r.Right - $r.Left), ($r.Bottom - $r.Top), [Probe]::Text($h))
  }
}
function WaitForLog([string]$pattern, [int]$seconds) {
  $deadline = (Get-Date).AddSeconds($seconds)
  while ((Get-Date) -lt $deadline) { if ((& $log) -match $pattern) { return $true }; Start-Sleep -Milliseconds 100 }
  return $false
}
# Waits until the page area (below the toolbar) of window $h is painted, i.e. no longer the dark shell background.
function WaitPainted([IntPtr]$h, [int]$seconds) {
  $deadline = (Get-Date).AddSeconds($seconds)
  $b = 0
  while ((Get-Date) -lt $deadline) {
    $bmp = Snap $h; $b = Brightness $bmp $h 200 250 700 350; $bmp.Dispose()
    if ($b -gt 120) { return $b }
    Start-Sleep -Milliseconds 400
  }
  return $b
}

try {
  $main = [IntPtr]::Zero
  for ($i = 0; $i -lt 160; $i++) {
    Start-Sleep -Milliseconds 250
    $ws = [Probe]::Windows([uint32]$p.Id)
    if ($ws.Count -gt 0 -and (& $log) -match "native hooks installed") { $main = $ws[0]; break }
  }
  DumpWindows "after start"
  if ($main -eq [IntPtr]::Zero) { Write-Host "no browser window or page appeared"; exit 2 }
  [void][Probe]::MoveWindow($main, 0, 0, 1000, 740, $true)
  [void][Probe]::SetForegroundWindow($main)
  $b = WaitPainted $main 25
  $notes["page brightness at start"] = $b
  [void](Capture $main "01-page")
  $steps["page area painted"] = ($b -gt 120)

  # Focus the page, then F12.
  $c = ClientPoint $main 500 400
  Click $c.X $c.Y $false
  Start-Sleep -Milliseconds 400
  $sw = [Diagnostics.Stopwatch]::StartNew()
  Key @(0x7B)
  $docked = [IntPtr]::Zero
  while ($sw.ElapsedMilliseconds -lt 9000) {
    $kids = [Probe]::DevtoolsChildren($main)
    foreach ($k in $kids) { if ([Probe]::IsWindowVisible($k)) { $docked = $k } }
    if ($docked -ne [IntPtr]::Zero) { break }
    Start-Sleep -Milliseconds 50
  }
  $notes["F12 to docked DevTools (ms, -1 = never)"] = $(if ($docked -ne [IntPtr]::Zero) { $sw.ElapsedMilliseconds } else { -1 })
  DumpWindows "after F12"
  $steps["F12: DevTools is a child of the browser window"] = ($docked -ne [IntPtr]::Zero)
  $tops = @([Probe]::DevtoolsTopLevel())
  $notes["top-level DevTools windows on screen"] = $tops.Count
  $steps["F12: no separate DevTools window on screen"] = ($tops.Count -eq 0)
  if ($docked -ne [IntPtr]::Zero) {
    Start-Sleep -Milliseconds 2500
    $r = New-Object Probe+RECT; [void][Probe]::GetWindowRect($docked, [ref]$r)
    $tl = New-Object Probe+POINT; $tl.X = $r.Left; $tl.Y = $r.Top
    [void][Probe]::ScreenToClient($main, [ref]$tl)
    $cr = New-Object Probe+RECT; [void][Probe]::GetClientRect($main, [ref]$cr)
    $w = $r.Right - $r.Left; $hh = $r.Bottom - $r.Top
    $notes["docked DevTools rect in the window (physical)"] = "{0},{1} {2}x{3} of {4}x{5}" -f $tl.X, $tl.Y, $w, $hh, $cr.Right, $cr.Bottom
    $steps["docked DevTools sits in the lower part of the window"] = ($tl.Y -gt ($cr.Bottom * 0.3) -and ($tl.Y + $hh) -le ($cr.Bottom + 2) -and $w -gt ($cr.Right * 0.8))
    $shot = Capture $main "02-docked"
    $s = [Probe]::GetDpiForWindow($main) / 96.0
    $db = Brightness $shot $main ([int](($tl.X + 10) / $s)) ([int](($tl.Y + 10) / $s)) ([int](($w - 20) / $s)) ([int](($hh - 20) / $s))
    $notes["docked DevTools brightness"] = $db
    $steps["docked DevTools is painted (not black)"] = ($db -gt 25)
    $tb = Capture $docked "03-devtools-only"
    $tb.Dispose()
    $steps["log: DevTools docked"] = ((& $log) -match "devtools: docked")

    # F12 again closes it (focus is in the page or in DevTools: both reach the browser's shortcut).
    [void][Probe]::SetForegroundWindow($main)
    $c = ClientPoint $main 500 200
    Click $c.X $c.Y $false
    Start-Sleep -Milliseconds 300
    Key @(0x7B)
    $gone = $false
    $sw.Restart()
    while ($sw.ElapsedMilliseconds -lt 5000) {
      if (-not [Probe]::IsWindowVisible($docked)) { $gone = $true; break }
      Start-Sleep -Milliseconds 50
    }
    DumpWindows "after second F12"
    [void](Capture $main "04-closed")
    $steps["second F12 closes docked DevTools"] = $gone
  }
  $steps["log: no panic"] = ((& $log) -notmatch "panic:")
}
finally {
  if (-not $p.HasExited) { Stop-Process -Id $p.Id -Force }
  if ($server -and -not $server.HasExited) { Stop-Process -Id $server.Id -Force }
  Start-Sleep -Milliseconds 500
  if (Test-Path $logPath) { Copy-Item $logPath (Join-Path $ReportDir "project-browser.log") -Force }
}

$failed = @($steps.GetEnumerator() | Where-Object { -not $_.Value })
[ordered]@{ checks = $steps; measurements = $notes } | ConvertTo-Json -Depth 4 | Set-Content (Join-Path $ReportDir "probe-summary.json") -Encoding utf8
foreach ($c in $steps.GetEnumerator()) { Write-Host ("  [{0}] {1}" -f ($(if ($c.Value) { "pass" } else { "FAIL" })), $c.Key) }
foreach ($c in $notes.GetEnumerator()) { Write-Host ("  ({0}: {1})" -f $c.Key, $c.Value) }
if ($failed.Count -gt 0) { exit 1 } else { exit 0 }
