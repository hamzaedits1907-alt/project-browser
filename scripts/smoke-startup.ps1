<#
  Startup smoke test: launches the real executable against a throwaway data folder, waits, and checks that it
  is still alive, that it logged the startup stages, and that Windows recorded no crash for it.

  This proves the process starts and survives the first seconds, including the first process-memory sample (the
  step that crashed 0.1.0). It does NOT prove that web pages render or look right.

  usage: scripts/smoke-startup.ps1 -Exe path\to\project-browser.exe [-Seconds 25] [-ReportDir smoke-report]
  exit code: 0 = passed, 1 = a check failed, 2 = could not run
#>
param(
  [Parameter(Mandatory = $true)][string]$Exe,
  [int]$Seconds = 25,
  [string]$ReportDir = "smoke-report",
  [string]$DataDir = (Join-Path ([IO.Path]::GetTempPath()) ("pb-smoke-" + [guid]::NewGuid().ToString("N")))
)
$ErrorActionPreference = "Stop"
if (-not (Test-Path $Exe)) { Write-Host "executable not found: $Exe"; exit 2 }
New-Item -ItemType Directory -Force -Path $DataDir, $ReportDir | Out-Null
$env:PROJECT_BROWSER_DATA_DIR = $DataDir
$started = Get-Date
Write-Host "launching $Exe with data folder $DataDir"
$p = Start-Process -FilePath $Exe -PassThru
$exited = $false
for ($i = 0; $i -lt ($Seconds * 4); $i++) {
  Start-Sleep -Milliseconds 250
  if ($p.HasExited) { $exited = $true; break }
}
$exitCode = if ($exited) { '0x{0:X}' -f $p.ExitCode } else { "still running" }
if (-not $exited) {
  # Give a late crash one more chance to show before we stop it ourselves.
  Start-Sleep -Seconds 2
  if ($p.HasExited) { $exited = $true; $exitCode = '0x{0:X}' -f $p.ExitCode } else { Stop-Process -Id $p.Id -Force }
}
Start-Sleep -Milliseconds 500

$logPath = Join-Path $DataDir "logs\project-browser.log"
$log = if (Test-Path $logPath) { Get-Content $logPath -Raw } else { "" }
if ($log) { Copy-Item $logPath (Join-Path $ReportDir "project-browser.log") -Force }

$crashes = @(Get-WinEvent -FilterHashtable @{ LogName = 'Application'; Id = 1000; StartTime = $started } -ErrorAction SilentlyContinue |
  Where-Object { $_.Message -match 'project-browser' } | ForEach-Object { ($_.Message -split "`r?`n" | Select-Object -First 8) -join ' | ' })

$checks = [ordered]@{
  "process stayed alive for $Seconds s"          = (-not $exited)
  "no Windows crash event"                       = ($crashes.Count -eq 0)
  "log: shell window created"                    = ($log -match 'startup: shell window and WebView2 controller created')
  "log: shell UI ready"                          = ($log -match 'startup: shell UI ready')
  "log: first process sample completed"          = ($log -match 'perf: first process sample complete')
  "log: no panic"                                = ($log -notmatch 'panic:')
}
$failed = @($checks.GetEnumerator() | Where-Object { -not $_.Value })
$summary = [ordered]@{
  exe = $Exe; exited = $exited; exitCode = $exitCode; seconds = $Seconds
  checks = $checks; crashEvents = $crashes
  lastLogLines = @(($log -split "`r?`n" | Where-Object { $_ } | Select-Object -Last 12))
}
$summary | ConvertTo-Json -Depth 5 | Set-Content (Join-Path $ReportDir "smoke-summary.json") -Encoding utf8
Write-Host "---- result (exit: $exitCode)"
foreach ($c in $checks.GetEnumerator()) { Write-Host ("  [{0}] {1}" -f ($(if ($c.Value) { "pass" } else { "FAIL" })), $c.Key) }
Write-Host "---- last log lines"
$summary.lastLogLines | ForEach-Object { Write-Host "  $_" }
if ($crashes.Count -gt 0) { Write-Host "---- crash events"; $crashes | ForEach-Object { Write-Host "  $_" } }
Remove-Item Env:PROJECT_BROWSER_DATA_DIR -ErrorAction SilentlyContinue
if ($failed.Count -gt 0) { exit 1 }
exit 0
