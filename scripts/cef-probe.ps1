<#
  Runs the CEF prototype (cefsimple built from the bundled-Chromium distribution) against a list of pages and records
  what it rendered: the window title the page set, a PrintWindow capture, how many processes it ran, and whether it
  stayed alive. The engine test page reports its capabilities through its title (see prototype/cef/engine-test.html).

  usage: scripts/cef-probe.ps1 -Exe path\to\cefsimple.exe -TestUrl http://127.0.0.1:8765/engine-test.html [-ReportDir cef-report]
#>
param(
  [Parameter(Mandatory = $true)][string]$Exe,
  [Parameter(Mandatory = $true)][string]$TestUrl,
  [string]$ReportDir = "cef-report"
)
$ErrorActionPreference = "Stop"
New-Item -ItemType Directory -Force -Path $ReportDir | Out-Null
Add-Type -AssemblyName System.Drawing
Add-Type @"
using System;
using System.Text;
using System.Collections.Generic;
using System.Runtime.InteropServices;
public static class Cef {
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left, Top, Right, Bottom; }
  public delegate bool EnumProc(IntPtr h, IntPtr p);
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc f, IntPtr p);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr h, IntPtr dc, uint flags);
  [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern int GetWindowText(IntPtr h, StringBuilder s, int n);
  public static string Text(IntPtr h) { var s = new StringBuilder(4096); GetWindowText(h, s, 4096); return s.ToString(); }
  public static IntPtr Main(uint pid) {
    IntPtr found = IntPtr.Zero;
    EnumWindows((h, p) => { uint wp; GetWindowThreadProcessId(h, out wp); RECT r;
      if (wp == pid && IsWindowVisible(h) && GetWindowRect(h, out r) && (r.Right - r.Left) > 300) { found = h; return false; } return true; }, IntPtr.Zero);
    return found;
  }
}
"@

function Capture([IntPtr]$h, [string]$name) {
  $r = New-Object Cef+RECT
  [void][Cef]::GetWindowRect($h, [ref]$r)
  $bmp = New-Object System.Drawing.Bitmap ([Math]::Max(1, $r.Right - $r.Left)), ([Math]::Max(1, $r.Bottom - $r.Top))
  $g = [System.Drawing.Graphics]::FromImage($bmp); $dc = $g.GetHdc()
  [void][Cef]::PrintWindow($h, $dc, 2)
  $g.ReleaseHdc($dc); $g.Dispose()
  # Mean brightness of the centre: a black or empty capture means nothing was painted.
  $sum = 0.0; $n = 0
  for ($y = [int]($bmp.Height * 0.2); $y -lt [int]($bmp.Height * 0.8); $y += 11) { for ($x = [int]($bmp.Width * 0.2); $x -lt [int]($bmp.Width * 0.8); $x += 11) { $c = $bmp.GetPixel($x, $y); $sum += ($c.R + $c.G + $c.B) / 3; $n++ } }
  $bmp.Save((Join-Path $ReportDir "$name.png"), [System.Drawing.Imaging.ImageFormat]::Png); $bmp.Dispose()
  return [Math]::Round($sum / [Math]::Max(1, $n), 1)
}

function Run([string]$name, [string]$url, [string]$waitFor, [int]$seconds) {
  $cache = Join-Path ([IO.Path]::GetTempPath()) ("cef-" + [guid]::NewGuid().ToString("N"))
  $p = Start-Process -FilePath $Exe -ArgumentList "--url=$url", "--root-cache-path=$cache", "--no-first-run" -PassThru
  $title = ""; $h = [IntPtr]::Zero
  $deadline = (Get-Date).AddSeconds($seconds)
  while ((Get-Date) -lt $deadline) {
    Start-Sleep -Milliseconds 500
    if ($p.HasExited) { break }
    $h = [Cef]::Main([uint32]$p.Id)
    if ($h -ne [IntPtr]::Zero) { $title = [Cef]::Text($h); if ($title -match $waitFor) { break } }
  }
  Start-Sleep -Seconds 2
  $bright = if ($h -ne [IntPtr]::Zero) { Capture $h $name } else { 0 }
  $procs = @(Get-CimInstance Win32_Process | Where-Object { $_.ParentProcessId -eq $p.Id -or $_.ProcessId -eq $p.Id })
  $kinds = $procs | ForEach-Object { if ($_.CommandLine -match "--type=([\w-]+)") { $Matches[1] } else { "browser" } } | Group-Object | ForEach-Object { "$($_.Name)=$($_.Count)" }
  $mem = [Math]::Round((($procs | ForEach-Object { (Get-Process -Id $_.ProcessId -ErrorAction SilentlyContinue).PrivateMemorySize64 } | Measure-Object -Sum).Sum) / 1MB, 1)
  $alive = -not $p.HasExited
  foreach ($q in $procs) { Stop-Process -Id $q.ProcessId -Force -ErrorAction SilentlyContinue }
  Start-Sleep -Seconds 2
  Remove-Item -Recurse -Force $cache -ErrorAction SilentlyContinue
  return [ordered]@{ name = $name; url = $url; title = $title; titleMatched = ($title -match $waitFor); painted = ($bright -gt 25); brightness = $bright; alive = $alive; processes = ($kinds -join ", "); privateMemoryMB = $mem }
}

$results = @(
  (Run "engine-test" $TestUrl "^PBTEST \{" 40),
  (Run "example" "https://example.com/" "Example Domain" 30),
  (Run "wikipedia" "https://en.wikipedia.org/wiki/Chromium_(web_browser)" "Wikipedia" 40),
  (Run "youtube" "https://www.youtube.com/" "YouTube" 45)
)
$engine = $null
$t = $results[0].title
if ($t -match "^PBTEST (\{.*\})$") { $engine = $Matches[1] | ConvertFrom-Json }
[ordered]@{ when = (Get-Date).ToString("s"); runs = $results; engineCapabilities = $engine } | ConvertTo-Json -Depth 6 | Set-Content (Join-Path $ReportDir "cef-summary.json") -Encoding utf8
foreach ($r in $results) { Write-Host ("[{0}] title matched={1} painted={2} alive={3} procs: {4} mem {5} MB" -f $r.name, $r.titleMatched, $r.painted, $r.alive, $r.processes, $r.privateMemoryMB) }
if ($engine) { $engine.PSObject.Properties | ForEach-Object { Write-Host ("  {0}: {1}" -f $_.Name, $_.Value) } }
$failed = @($results | Where-Object { -not $_.titleMatched -or -not $_.painted })
if ($failed.Count -gt 0) { exit 1 } else { exit 0 }
