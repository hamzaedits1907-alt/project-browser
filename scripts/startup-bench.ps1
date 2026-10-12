<#
  Startup timing: cold (brand-new data folder) and warm (the same folder again) launches of the real executable.

  Each launch opens https://example.com and reads two numbers the browser logs about itself:
    * "startup: shell UI ready N ms after process start"   (the window's interface is ready)
    * "startup: first page loaded N ms after process start" (the first web page finished loading, network included)
  The process is then closed. Reports the median of each over -Runs launches, plus every run, in startup-summary.json.

  usage: scripts/startup-bench.ps1 -Exe path\to\project-browser.exe [-Runs 3] [-Out bench-results]
#>
param(
  [Parameter(Mandatory = $true)][string]$Exe,
  [int]$Runs = 3,
  [string]$Out = "bench-results"
)
$ErrorActionPreference = "Stop"
if (-not (Test-Path $Exe)) { throw "executable not found: $Exe" }
New-Item -ItemType Directory -Force -Path $Out | Out-Null
$env:PROJECT_BROWSER_SKIP_ONBOARDING = "1"

function Launch([string]$dataDir) {
  $env:PROJECT_BROWSER_DATA_DIR = $dataDir
  $log = Join-Path $dataDir "logs\project-browser.log"
  if (Test-Path $log) { Remove-Item $log -Force }
  $p = Start-Process -FilePath $Exe -ArgumentList "https://example.com" -PassThru
  $ui = $null; $page = $null
  for ($i = 0; $i -lt 120 -and ($null -eq $ui -or $null -eq $page); $i++) {
    Start-Sleep -Milliseconds 250
    if (Test-Path $log) {
      $text = Get-Content $log -Raw
      if ($text -match "shell UI ready (\d+) ms") { $ui = [int]$Matches[1] }
      if ($text -match "first page loaded (\d+) ms") { $page = [int]$Matches[1] }
    }
  }
  Start-Sleep -Seconds 2
  Get-CimInstance Win32_Process | Where-Object { $_.ParentProcessId -eq $p.Id } | ForEach-Object { Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue }
  Stop-Process -Id $p.Id -Force -ErrorAction SilentlyContinue
  Start-Sleep -Seconds 3
  return [pscustomobject]@{ uiReadyMs = $ui; firstPageMs = $page }
}

function Median($values) {
  $a = @($values | Where-Object { $null -ne $_ } | Sort-Object)
  if ($a.Count -eq 0) { return $null }
  return $a[[int][math]::Floor(($a.Count - 1) / 2)]
}

$cold = @(); $warm = @()
for ($r = 1; $r -le $Runs; $r++) {
  $dir = Join-Path ([IO.Path]::GetTempPath()) ("pb-startup-" + [guid]::NewGuid().ToString("N"))
  New-Item -ItemType Directory -Force -Path $dir | Out-Null
  $c = Launch $dir
  $w = Launch $dir
  Write-Host ("run {0}: cold UI {1} ms, page {2} ms | warm UI {3} ms, page {4} ms" -f $r, $c.uiReadyMs, $c.firstPageMs, $w.uiReadyMs, $w.firstPageMs)
  $cold += $c; $warm += $w
  Remove-Item -Recurse -Force $dir -ErrorAction SilentlyContinue
}
Remove-Item Env:PROJECT_BROWSER_DATA_DIR -ErrorAction SilentlyContinue

$summary = [pscustomobject]@{
  when = (Get-Date).ToString("s")
  version = (Get-Item $Exe).VersionInfo.ProductVersion
  runs = $Runs
  cold = [pscustomobject]@{ uiReadyMs_median = (Median ($cold | ForEach-Object { $_.uiReadyMs })); firstPageMs_median = (Median ($cold | ForEach-Object { $_.firstPageMs })); all = $cold }
  warm = [pscustomobject]@{ uiReadyMs_median = (Median ($warm | ForEach-Object { $_.uiReadyMs })); firstPageMs_median = (Median ($warm | ForEach-Object { $_.firstPageMs })); all = $warm }
}
$summary | ConvertTo-Json -Depth 5 | Set-Content (Join-Path $Out "startup-summary.json") -Encoding utf8
Write-Host ("cold median: UI {0} ms, first page {1} ms | warm median: UI {2} ms, first page {3} ms" -f $summary.cold.uiReadyMs_median, $summary.cold.firstPageMs_median, $summary.warm.uiReadyMs_median, $summary.warm.firstPageMs_median)
