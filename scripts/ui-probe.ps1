<#
  Native UI probe: drives the real executable with real mouse and keyboard input and captures its own windows.

  What it does, against a throwaway data folder:
    1. starts the browser on https://example.com and waits for the page;
    2. right-clicks the page (the browser's own menu must appear, not WebView2's);
    3. presses Escape, opens a new tab (Ctrl+T);
    4. drags the first tab down out of the strip (it must become a second window).
  Every step is captured with PrintWindow on the browser's own windows only (never the screen), and the log is checked.

  It needs an interactive desktop (GitHub's Windows runners have one). It reports what it saw; it does not fake input
  into the page. Evidence lands in -ReportDir: PNG captures, the log and probe-summary.json.

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
using System.Collections.Generic;
using System.Runtime.InteropServices;
public static class Probe {
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left, Top, Right, Bottom; }
  [StructLayout(LayoutKind.Sequential)] public struct POINT { public int X, Y; }
  public delegate bool EnumProc(IntPtr h, IntPtr p);
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc f, IntPtr p);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern bool GetClientRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern bool ClientToScreen(IntPtr h, ref POINT p);
  [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr h, IntPtr dc, uint flags);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern bool MoveWindow(IntPtr h, int x, int y, int w, int hgt, bool repaint);
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll")] public static extern void mouse_event(uint f, int dx, int dy, uint d, IntPtr e);
  [DllImport("user32.dll")] public static extern void keybd_event(byte vk, byte scan, uint f, IntPtr e);
  [DllImport("user32.dll")] public static extern uint GetDpiForWindow(IntPtr h);
  public static List<IntPtr> Windows(uint pid) {
    var list = new List<IntPtr>();
    EnumWindows((h, p) => { uint wp; GetWindowThreadProcessId(h, out wp); RECT r;
      if (wp == pid && IsWindowVisible(h) && GetWindowRect(h, out r) && (r.Right - r.Left) > 300) list.Add(h); return true; }, IntPtr.Zero);
    return list;
  }
}
"@

$env:PROJECT_BROWSER_DATA_DIR = $DataDir
$env:PROJECT_BROWSER_SKIP_ONBOARDING = "1"
$logPath = Join-Path $DataDir "logs\project-browser.log"
$p = Start-Process -FilePath $Exe -ArgumentList "https://example.com" -PassThru
$log = { if (Test-Path $logPath) { Get-Content $logPath -Raw } else { "" } }
$steps = [ordered]@{}

function Capture([IntPtr]$h, [string]$name) {
  $r = New-Object Probe+RECT
  [void][Probe]::GetWindowRect($h, [ref]$r)
  $w = $r.Right - $r.Left; $hh = $r.Bottom - $r.Top
  $bmp = New-Object System.Drawing.Bitmap $w, $hh
  $g = [System.Drawing.Graphics]::FromImage($bmp)
  $dc = $g.GetHdc()
  $ok = [Probe]::PrintWindow($h, $dc, 2) # PW_RENDERFULLCONTENT: includes the WebView2 surfaces
  $g.ReleaseHdc($dc); $g.Dispose()
  $bmp.Save((Join-Path $ReportDir "$name.png"), [System.Drawing.Imaging.ImageFormat]::Png)
  $bmp.Dispose()
  return $ok
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

try {
  # 1. Wait for the window and the page.
  $main = [IntPtr]::Zero
  for ($i = 0; $i -lt 120; $i++) {
    Start-Sleep -Milliseconds 250
    $ws = [Probe]::Windows([uint32]$p.Id)
    if ($ws.Count -gt 0 -and (& $log) -match "native hooks installed") { $main = $ws[0]; break }
  }
  if ($main -eq [IntPtr]::Zero) { Write-Host "no browser window or page appeared"; exit 2 }
  [void][Probe]::MoveWindow($main, 40, 40, 1280, 800, $true)
  [void][Probe]::SetForegroundWindow($main)
  Start-Sleep -Seconds 6
  $steps["page rendered (capture)"] = Capture $main "01-page"

  # 2. Right-click the middle of the page area.
  $pt = ClientPoint $main 700 450
  Click $pt.X $pt.Y $true
  Start-Sleep -Milliseconds 1800
  [void](Capture $main "02-right-click")
  $steps["log: our context menu was requested"] = ((& $log) -match "context menu \d+: page target")

  # 3. Escape closes it; Ctrl+T opens a tab.
  Key @(0x1B)
  Start-Sleep -Milliseconds 900
  $steps["log: menu closed"] = ((& $log) -match "context menu \d+: closed")
  Key @(0x11, 0x54)
  Start-Sleep -Milliseconds 1500
  [void](Capture $main "03-new-tab")

  # 4. Drag the first tab down out of the strip: it should become a window of its own.
  $from = ClientPoint $main 150 20
  [void][Probe]::SetCursorPos($from.X, $from.Y); Start-Sleep -Milliseconds 150
  [Probe]::mouse_event(0x0002, 0, 0, 0, [IntPtr]::Zero)
  for ($s = 1; $s -le 20; $s++) { [void][Probe]::SetCursorPos($from.X + $s * 6, $from.Y + $s * 16); Start-Sleep -Milliseconds 40 }
  Start-Sleep -Milliseconds 300
  [Probe]::mouse_event(0x0004, 0, 0, 0, [IntPtr]::Zero)
  Start-Sleep -Seconds 3
  $ws = [Probe]::Windows([uint32]$p.Id)
  $steps["drag out: a second window exists"] = ($ws.Count -ge 2)
  $steps["log: tab moved to a new window"] = ((& $log) -match "moved from main to win-")
  $n = 4
  foreach ($w in $ws) { [void](Capture $w ("0{0}-window" -f $n)); $n++ }
  $steps["log: no panic"] = ((& $log) -notmatch "panic:")
}
finally {
  if (-not $p.HasExited) { Stop-Process -Id $p.Id -Force }
  Start-Sleep -Milliseconds 500
  if (Test-Path $logPath) { Copy-Item $logPath (Join-Path $ReportDir "project-browser.log") -Force }
}

$failed = @($steps.GetEnumerator() | Where-Object { -not $_.Value })
$steps | ConvertTo-Json | Set-Content (Join-Path $ReportDir "probe-summary.json") -Encoding utf8
foreach ($c in $steps.GetEnumerator()) { Write-Host ("  [{0}] {1}" -f ($(if ($c.Value) { "pass" } else { "FAIL" })), $c.Key) }
if ($failed.Count -gt 0) { exit 1 } else { exit 0 }
