<#
  Native UI probe: drives the real executable with real mouse and keyboard input and captures its own windows.

  What it does, against a throwaway data folder:
    1. starts the browser on https://example.com and waits until the page area is actually painted;
    2. right-clicks the page (the browser's own menu must appear, not WebView2's) and checks the pixels changed;
    3. presses Escape: the menu must be gone from the pixels (not just from the log);
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
$p = Start-Process -FilePath $Exe -ArgumentList "https://example.com" -PassThru
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
  [void][Probe]::MoveWindow($main, 40, 40, 1280, 800, $true)
  [void][Probe]::SetForegroundWindow($main)
  $b = WaitPainted $main 20
  $notes["page brightness at start"] = $b
  $before = Capture $main "01-page"
  $steps["page area painted"] = ($b -gt 120)

  # 2. Right-click the middle of the page area: our menu must appear (pixels around the pointer change).
  $pt = ClientPoint $main 700 450
  Click $pt.X $pt.Y $true
  Start-Sleep -Milliseconds 1500
  $menu = Capture $main "02-right-click"
  $d = PixelDiff $before $menu $main 700 450 240 260
  $notes["pixel change under the menu"] = $d
  $steps["right-click shows a menu"] = ($d -gt 8)
  $steps["log: our menu, not WebView2's"] = ((& $log) -match "context menu \d+: page target")

  # 3. Escape closes it: the pixels return to the page.
  Key @(0x1B)
  Start-Sleep -Milliseconds 900
  $after = Capture $main "03-escape"
  $d2 = PixelDiff $before $after $main 700 450 240 260
  $notes["pixel change after Escape"] = $d2
  $steps["Escape removes the menu"] = ($d2 -lt 4)

  # 4. Ctrl+T opens a tab.
  Key @(0x11, 0x54)
  Start-Sleep -Milliseconds 1500
  $newTab = Capture $main "04-new-tab"
  $d4 = PixelDiff $after $newTab $main 200 250 700 350
  $notes["pixel change after Ctrl+T"] = $d4
  $steps["Ctrl+T shows the new-tab page"] = ($d4 -gt 20)

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
  Start-Sleep -Milliseconds 500
  if (Test-Path $logPath) { Copy-Item $logPath (Join-Path $ReportDir "project-browser.log") -Force }
}

$failed = @($steps.GetEnumerator() | Where-Object { -not $_.Value })
[ordered]@{ checks = $steps; measurements = $notes } | ConvertTo-Json -Depth 4 | Set-Content (Join-Path $ReportDir "probe-summary.json") -Encoding utf8
foreach ($c in $steps.GetEnumerator()) { Write-Host ("  [{0}] {1}" -f ($(if ($c.Value) { "pass" } else { "FAIL" })), $c.Key) }
foreach ($c in $notes.GetEnumerator()) { Write-Host ("  ({0}: {1})" -f $c.Key, $c.Value) }
if ($failed.Count -gt 0) { exit 1 } else { exit 0 }
