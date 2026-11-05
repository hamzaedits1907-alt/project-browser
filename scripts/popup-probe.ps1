<#
  Popup probe: drives the real executable with real mouse and keyboard input and checks the popups drawn over pages.

    1. types into the address bar: the suggestion list is shown by the menu window and the page keeps running;
    2. opens the main menu (three dots): shown by the menu window below its button, the page keeps running, and a
       second click on the button closes it;
    3. opens the site information popover: visible over the page (a hole is cut into the page), page running beside it;
    4. opens a page's full screen on a maximized window: the page must reach the bottom of the screen (no strip).
  Captures land in -ReportDir; results are printed and also written as GitHub annotations.

  usage: scripts/popup-probe.ps1 -Exe <path to project-browser.exe> [-ReportDir popup-probe]
  exit code: 0 = all checks passed, 1 = a check failed, 2 = could not run
#>
param(
  [Parameter(Mandatory = $true)][string]$Exe,
  [string]$ReportDir = "popup-probe",
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
# Clicks the toolbar's "more" button (right end) and checks a popup is drawn below it and is still there 1.5 s later.
function ToolbarMenu([string]$label) {
  $base = Snap $main
  # The "more" button is the last thing on the toolbar: aim from the window's right edge (physical pixels), since the
  # shell's CSS pixels need not match the window's (a click aimed at 1253 landed in the address bar, per the shell trace).
  $cr = New-Object Probe+RECT; [void][Probe]::GetClientRect($main, [ref]$cr)
  $mb = New-Object Probe+POINT; $mb.X = $cr.Right - 28; $mb.Y = 63
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


function MainWindow() {
  for ($i = 0; $i -lt 150; $i++) { $w = [Probe]::Windows([uint32]$script:p.Id); if ($w.Count -gt 0) { return $w[0] }; Start-Sleep -Milliseconds 200 }
  return [IntPtr]::Zero
}
# Visible windows owned by $owner (the menu window while it shows a menu or the suggestion list), with their rects.
function OwnedPopups([IntPtr]$owner) {
  $list = @()
  foreach ($h in [Probe]::All([uint32]$script:p.Id)) {
    if (-not [Probe]::IsWindowVisible($h) -or [Probe]::GetWindow($h, 4) -ne $owner) { continue }
    $r = New-Object Probe+RECT; [void][Probe]::GetWindowRect($h, [ref]$r)
    if (($r.Right - $r.Left) -gt 20 -and ($r.Bottom - $r.Top) -gt 20) { $list += , $r }
  }
  return , $list
}
Add-Type -Namespace PP -Name U -MemberDefinition '[DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int c);'

try {
  # ---- 1. typing in the address bar, the suggestion list, the main menu and a popover, over a running page ----
  $main = MainWindow
  if ($main -eq [IntPtr]::Zero) { Write-Host "no browser window appeared"; exit 2 }
  # Fits any runner screen (GitHub's is 1024x768): the menu button must be on screen to be clicked.
  [void][Probe]::MoveWindow($main, 0, 0, 1000, 700, $true)
  [void][Probe]::SetForegroundWindow($main)
  Start-Sleep -Seconds 6
  $cr = New-Object Probe+RECT; [void][Probe]::GetClientRect($main, [ref]$cr)
  $scale = [Probe]::GetDpiForWindow($main) / 96.0
  $cssW = [int]($cr.Right / $scale)

  $pt = ClientPoint $main ([int]($cssW / 2)) 63
  Click $pt.X $pt.Y $false
  Start-Sleep -Milliseconds 500
  foreach ($l in [byte[]](0x48, 0x45, 0x4C, 0x4C, 0x4F)) { Key @($l); Start-Sleep -Milliseconds 220 }
  Start-Sleep -Milliseconds 900
  $a = Capture $main "1-typed"; Start-Sleep -Milliseconds 500; $b = Snap $main
  $notes["typing: page change while the suggestion list is open"] = PixelDiff $a $b $main 70 140 100 280
  $steps["typing: the page keeps running while typing"] = ($notes["typing: page change while the suggestion list is open"] -gt 3)
  # The address bar must still hold what was typed: it is dark text on the field; the field must differ from the
  # unfocused capture of the address (taken after Escape below) - judged from the log instead: the field reset makes
  # the shell log "press in the toolbar" only once, so the reliable signal is the suggestion list being shown.
  $popups = OwnedPopups $main
  $steps["typing: suggestion list shown by the menu window"] = ($popups.Count -ge 1)
  $a.Dispose(); $b.Dispose()
  Key @(0x1B); Start-Sleep -Milliseconds 300; Key @(0x1B); Start-Sleep -Milliseconds 800

  $mb = New-Object Probe+POINT; $mb.X = $cr.Right - [int](28 * $scale); $mb.Y = [int](63 * $scale); [void][Probe]::ClientToScreen($main, [ref]$mb)
  Click $mb.X $mb.Y $false
  Start-Sleep -Milliseconds 1000
  $popups = OwnedPopups $main
  $steps["main menu: shown by the menu window"] = ($popups.Count -ge 1)
  if ($popups.Count -ge 1) {
    $wr = New-Object Probe+RECT; [void][Probe]::GetWindowRect($main, [ref]$wr)
    $notes["main menu: top edge below the button (px from window top)"] = $popups[0].Top - $wr.Top
    $steps["main menu: opens below its button"] = (($popups[0].Top - $wr.Top) -gt (60 * $scale))
  }
  $a = Capture $main "2-menu"; Start-Sleep -Milliseconds 500; $b = Snap $main
  $notes["main menu: page change while open"] = PixelDiff $a $b $main 70 140 100 280
  $steps["main menu: the page keeps running"] = ($notes["main menu: page change while open"] -gt 3)
  $a.Dispose(); $b.Dispose()
  Click $mb.X $mb.Y $false
  Start-Sleep -Milliseconds 900
  $steps["main menu: a second click on the button closes it"] = ((OwnedPopups $main).Count -eq 0)

  # Site information popover (the icon at the left end of the address bar): drawn by the shell through a hole.
  $before = Snap $main
  $sp = ClientPoint $main 196 63
  Click $sp.X $sp.Y $false
  Start-Sleep -Milliseconds 1200
  $a = Capture $main "3-popover"; Start-Sleep -Milliseconds 500; $b = Snap $main
  $notes["popover: brightness where it is drawn"] = Brightness $a $main 200 110 260 120
  $notes["popover: brightness there before"] = Brightness $before $main 200 110 260 120
  $steps["popover: visible over the page"] = ($notes["popover: brightness where it is drawn"] -lt 90)
  $notes["popover: page change beside it"] = PixelDiff $a $b $main 70 140 100 280
  $steps["popover: the page keeps running beside it"] = ($notes["popover: page change beside it"] -gt 3)
  $a.Dispose(); $b.Dispose(); $before.Dispose()
  Key @(0x1B); Start-Sleep -Milliseconds 800

  # ---- 2. a page's full screen on a maximized window must cover the whole screen ----
  $pt = ClientPoint $main ([int]($cssW / 2)) 63
  Click $pt.X $pt.Y $false
  Start-Sleep -Milliseconds 300
  # Types the fixture address into the address bar (letters, digits, ':', '.', '/').
  $vk = @{ ':' = @(0x10, 0xBA); '.' = @(0xBE); '/' = @(0x6F) }
  foreach ($ch in "127.0.0.1:8766/fullscreen.html".ToCharArray()) {
    $s = [string]$ch
    if ($vk.ContainsKey($s)) { Key ([byte[]]$vk[$s]) }
    elseif ($s -match '[0-9]') { Key @([byte](0x30 + [int]$s)) }
    else { Key @([byte][char]$s.ToUpper()) }
    Start-Sleep -Milliseconds 40
  }
  Key @(0x0D)
  Start-Sleep -Seconds 3
  [void][PP.U]::ShowWindow($main, 3); Start-Sleep -Seconds 2
  [void][Probe]::GetClientRect($main, [ref]$cr)
  $notes["full screen: maximized client"] = "$($cr.Right)x$($cr.Bottom)"
  $pt = ClientPoint $main ([int]($cssW / 2)) 400
  Click $pt.X $pt.Y $false
  Start-Sleep -Seconds 3
  $bmp = Capture $main "4-fullscreen"
  $c = $bmp.GetPixel([int]($bmp.Width / 2), $bmp.Height - 3)
  $notes["full screen: window"] = "$($bmp.Width)x$($bmp.Height)"
  $notes["full screen: bottom pixel"] = "$($c.R),$($c.G),$($c.B)"
  $steps["full screen: the page reaches the bottom of the screen"] = ($c.G -gt 120 -and $c.R -lt 60)
  $bmp.Dispose()
  Key @(0x1B); Start-Sleep -Seconds 2
  [void][Probe]::GetClientRect($main, [ref]$cr)
  $notes["full screen: client after Escape"] = "$($cr.Right)x$($cr.Bottom)"
} finally {
  Copy-Item $logPath (Join-Path $ReportDir "log.txt") -ErrorAction SilentlyContinue
  Stop-Process -Id $p.Id -Force -ErrorAction SilentlyContinue
  Stop-Process -Id $server.Id -Force -ErrorAction SilentlyContinue
}

[ordered]@{ checks = $steps; measurements = $notes } | ConvertTo-Json -Depth 4 | Set-Content (Join-Path $ReportDir "probe-summary.json") -Encoding utf8
$failed = @($steps.Keys | Where-Object { -not $steps[$_] })
foreach ($k in $steps.Keys) { Write-Host ("{0,-6} {1}" -f $(if ($steps[$k]) { "PASS" } else { "FAIL" }), $k) }
foreach ($k in $notes.Keys) { Write-Host ("       {0}: {1}" -f $k, $notes[$k]) }
# One annotation line, readable without signing in to GitHub (check-run annotations are public).
$line = ($steps.Keys | ForEach-Object { "{0}={1}" -f $_, $(if ($steps[$_]) { "pass" } else { "FAIL" }) }) -join "; "
Write-Host "::notice title=popup-probe::$line"
$m = ($notes.Keys | ForEach-Object { "{0}={1}" -f $_, $notes[$_] }) -join "; "
Write-Host "::notice title=popup-probe measurements::$m"
if ($failed.Count -gt 0) { exit 1 } else { exit 0 }
