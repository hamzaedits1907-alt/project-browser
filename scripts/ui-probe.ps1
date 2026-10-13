<#
  Native UI probe: drives the real executable with real mouse and keyboard input and captures its own windows.

  What it does, against a throwaway data folder:
    1. starts the browser on a local animated page and waits until the page area is actually painted;
    2. right-clicks the page: the browser's own menu window must appear (latency measured) while the page under it
       keeps animating (two captures of the page differ);
    3. presses Escape: the menu window must be hidden again;
    4. opens a new tab (Ctrl+T);
    5. drags the first tab out of the strip: a NEW top-level window (a handle that did not exist before) must appear,
       its page area must be painted (not black), and the page must still take input (Tab + Enter follows the
       page's link, and the window's pixels change to the new page).
  Every step is captured with PrintWindow on the browser's own windows only (never the screen). Outcomes are judged
  from window handles and pixels; the log is only used for diagnostics and the panic check.

  It needs an interactive desktop (GitHub's Windows runners have one). Evidence lands in -ReportDir: PNG captures,
  the log, windows.txt (every top-level window of the process at each step) and probe-summary.json.

  usage: scripts/ui-probe.ps1 -Exe path\to\project-browser.exe [-ReportDir ui-probe]
  exit code: 0 = all checks passed, 1 = a check failed, 2 = could not run
#>
param(
  [Parameter(Mandatory = $true)][string]$Exe,
  [string]$ReportDir = "ui-probe",
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
$server = Start-Process -FilePath $python -ArgumentList @("-I", (Join-Path $PSScriptRoot "fixture-server.py"), "8766") -PassThru -WindowStyle Hidden
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
# Clicks the toolbar's "more" button (right end) and checks a popup is drawn below it and is still there 1.5 s later.
function ToolbarMenu([string]$label) {
  $base = Snap $main
  # The "more" button is the last thing on the toolbar: aim from the window's right edge (physical pixels), since the
  # shell's CSS pixels need not match the window's (a click aimed at 1253 landed in the address bar, per the shell trace).
  $cr = New-Object Probe+RECT; [void][Probe]::GetClientRect($main, [ref]$cr)
  $mb = New-Object Probe+POINT; $mb.X = $cr.Right - 45; $mb.Y = 81
  [void][Probe]::ClientToScreen($main, [ref]$mb)
  $under = [Probe]::WindowFromPoint($mb)
  $script:notes["toolbar menu ($label): window under the click"] = "{0} class={1} (main={2})" -f $under, [Probe]::Class($under), $main
  $sw = [Diagnostics.Stopwatch]::StartNew()
  Click $mb.X $mb.Y $false
  $shownAt = -1
  while ($sw.ElapsedMilliseconds -lt 3000) {
    $c = Snap $main; $dd = PixelDiff $base $c $main 660 90 330 300; $c.Dispose()
    if ($dd -gt 8) { $shownAt = $sw.ElapsedMilliseconds; break }
    Start-Sleep -Milliseconds 30
  }
  $script:notes["toolbar menu ($label): click to drawn (ms, -1 = never)"] = $shownAt
  $script:steps["toolbar menu opens ($label)"] = ($shownAt -ge 0)
  Start-Sleep -Milliseconds 1500
  $still = Snap $main
  $still.Save((Join-Path $ReportDir ("toolbar-" + ($label -replace '[^a-z]', '-') + ".png")), [System.Drawing.Imaging.ImageFormat]::Png)
  $dd2 = PixelDiff $base $still $main 660 90 330 300
  $script:steps["toolbar menu stays open ($label)"] = ($dd2 -gt 8)
  Key @(0x1B)
  Start-Sleep -Milliseconds 700
  $base.Dispose(); $still.Dispose()
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
  # 1. Wait for the window and a painted page (example.com has a light background; the shell is dark).
  $main = [IntPtr]::Zero
  for ($i = 0; $i -lt 120; $i++) {
    Start-Sleep -Milliseconds 250
    $ws = [Probe]::Windows([uint32]$p.Id)
    if ($ws.Count -gt 0 -and (& $log) -match "native hooks installed") { $main = $ws[0]; break }
  }
  DumpWindows "after start"
  if ($main -eq [IntPtr]::Zero) { Write-Host "no browser window or page appeared"; exit 2 }
  [void][Probe]::MoveWindow($main, 0, 0, 1000, 740, $true)  # the runner screen is 1024x768: everything must be on it
  [void][Probe]::SetForegroundWindow($main)
  $b = WaitPainted $main 20
  $notes["page brightness at start"] = $b
  $before = Capture $main "01-page"
  $steps["page area painted"] = ($b -gt 120)

  ToolbarMenu "web page, clean state"
  $before = Capture $main "01b-page-again"

  # 2. Right-click the middle of the page area. The menu is a window of its own (menu_surface.rs), owned by the browser
  #    window, drawn over the LIVE page: it must appear quickly, and the page under it must keep animating (the old
  #    menu replaced the page with a still capture while open).
  $menuReady = [bool](WaitForLog "menu window ready" 20)
  $steps["menu window loaded"] = $menuReady
  $pt = ClientPoint $main 700 450
  $sw = [Diagnostics.Stopwatch]::StartNew()
  Click $pt.X $pt.Y $true
  $menuWin = [IntPtr]::Zero
  while ($sw.ElapsedMilliseconds -lt 3000 -and $menuWin -eq [IntPtr]::Zero) {
    foreach ($h in [Probe]::All([uint32]$p.Id)) { if ([Probe]::IsWindowVisible($h) -and [Probe]::GetWindow($h, 4) -eq $main) { $menuWin = $h } }
    if ($menuWin -eq [IntPtr]::Zero) { Start-Sleep -Milliseconds 5 }
  }
  $latency = $sw.ElapsedMilliseconds
  DumpWindows "menu open"
  $notes["right-click to visible menu (ms, includes 180 ms of simulated mouse input)"] = $latency
  $steps["right-click shows our menu window"] = ($menuWin -ne [IntPtr]::Zero)
  $steps["menu visible within 600 ms"] = ($menuWin -ne [IntPtr]::Zero -and $latency -lt 600)
  $steps["log: our menu, not WebView2's"] = ((& $log) -match "context menu \d+: page target")
  if ($menuWin -ne [IntPtr]::Zero) { [void](Capture $menuWin "02-menu-window") }
  $a1 = Capture $main "02-page-under-menu-a"
  Start-Sleep -Milliseconds 400
  $a2 = Capture $main "02-page-under-menu-b"
  $live = PixelDiff $a1 $a2 $main 60 150 300 180
  $notes["page change under an open menu (animated page)"] = $live
  $steps["page keeps running while the menu is open"] = ($live -gt 6)

  # 3. Escape closes it: the menu window is hidden again.
  Key @(0x1B)
  $gone = $false
  for ($i = 0; $i -lt 100 -and -not $gone; $i++) { Start-Sleep -Milliseconds 20; $gone = ($menuWin -eq [IntPtr]::Zero) -or -not [Probe]::IsWindowVisible($menuWin) }
  $after = Capture $main "03-escape"
  $steps["Escape removes the menu"] = $gone
  $steps["log: menu closed"] = ((& $log) -match "context menu \d+: closed")

  ToolbarMenu "web page, after a right-click menu"

  # 4. Ctrl+T opens a tab.
  Key @(0x11, 0x54)
  Start-Sleep -Milliseconds 1500
  $newTab = Capture $main "04-new-tab"
  $d4 = PixelDiff $after $newTab $main 200 250 700 350
  $notes["pixel change after Ctrl+T"] = $d4
  $steps["Ctrl+T shows the new-tab page"] = ($d4 -gt 20)

  ToolbarMenu "New Tab page (control, no page to cover)"

  # 5. Drag the first tab down out of the strip: it must become a new window that paints and takes input.
  $known = @([Probe]::Windows([uint32]$p.Id))
  $from = ClientPoint $main 150 20
  [void][Probe]::SetCursorPos($from.X, $from.Y); Start-Sleep -Milliseconds 150
  [Probe]::mouse_event(0x0002, 0, 0, 0, [IntPtr]::Zero)
  for ($s = 1; $s -le 20; $s++) { [void][Probe]::SetCursorPos($from.X + $s * 6, $from.Y + $s * 16); Start-Sleep -Milliseconds 40 }
  Start-Sleep -Milliseconds 300
  [Probe]::mouse_event(0x0004, 0, 0, 0, [IntPtr]::Zero)
  $torn = [IntPtr]::Zero
  for ($i = 0; $i -lt 60 -and $torn -eq [IntPtr]::Zero; $i++) {
    Start-Sleep -Milliseconds 250
    foreach ($w in [Probe]::Windows([uint32]$p.Id)) { if ($known -notcontains $w) { $torn = $w } }
  }
  DumpWindows "after drag"
  $steps["drag out: a new window appeared"] = ($torn -ne [IntPtr]::Zero)
  $steps["drag out: the original window is still open"] = ([Probe]::Windows([uint32]$p.Id) -contains $main)
  $notes["log: tab moved"] = ((& $log) -match "moved from main to win-")
  if ($torn -ne [IntPtr]::Zero) {
    [void][Probe]::SetForegroundWindow($torn)
    $tb = WaitPainted $torn 12
    $notes["torn-off page brightness"] = $tb
    $tornBefore = Capture $torn "05-torn-off"
    $steps["torn-off window: page painted (not black)"] = ($tb -gt 120)
    [void](Capture $main "06-original")
    # Interaction: click into the page, Tab to its link, Enter. The page must change.
    $c = ClientPoint $torn 640 520
    Click $c.X $c.Y $false
    Start-Sleep -Milliseconds 300
    Key @(0x09); Start-Sleep -Milliseconds 200; Key @(0x0D)
    Start-Sleep -Seconds 5
    $tornAfter = Capture $torn "07-torn-off-after-link"
    $d3 = PixelDiff $tornBefore $tornAfter $torn 100 140 1000 560
    $notes["torn-off page change after following the link"] = $d3
    $steps["torn-off window: page takes input and navigates"] = ($d3 -gt 6)
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
