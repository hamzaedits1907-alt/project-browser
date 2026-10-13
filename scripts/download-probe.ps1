<#
  Download probe: real downloads through the real executable (WebView2 download pathway), checked on disk.

  A local fixture server (scripts/fixture-server.py) serves files with known bytes. The probe starts the browser with a
  throwaway data folder, then hands it one address after another the way Windows does when a link is opened from
  another program (a second process forwards the address to the running browser, which opens it in a new tab). Every
  download must produce a file at the configured destination whose size and SHA-256 match the fixture. Afterwards the
  browser's own download history is read from its database, the browser is restarted and the history is read again.

  Scenarios: plain file, Content-Disposition name with spaces, Unicode name (RFC 5987), large file with progress,
  redirect, ZIP, a 3.4 MB .exe (SmartScreen path), two files with the same name, a blob download made by a page
  script, a file that needs a session cookie, a 404 (no file may appear) and a transfer cut off half way (must end as
  failed, never as completed).

  -Config default : default settings, downloads into a chosen folder.
  -Config gaming  : Gaming Mode on with background tabs sleeping and the low memory target (the configuration of a real
                    user report), downloads into the Windows Downloads folder.
  -Config noshield: default settings with Project Shield off (isolates the Shield's request hook).

  usage: scripts/download-probe.ps1 -Exe path\to\project-browser.exe -Config default [-ReportDir download-probe]
  exit code: 0 = all checks passed, 1 = a check failed, 2 = could not run
#>
param(
  [Parameter(Mandatory = $true)][string]$Exe,
  [ValidateSet("default", "gaming", "noshield")][string]$Config = "default",
  [string]$ReportDir = "download-probe",
  [int]$Port = 8765
)
$ErrorActionPreference = "Stop"
if (-not (Test-Path $Exe)) { Write-Host "executable not found: $Exe"; exit 2 }
$Exe = (Resolve-Path $Exe).Path
$report = Join-Path $ReportDir $Config
New-Item -ItemType Directory -Force -Path $report | Out-Null
$dataDir = Join-Path ([IO.Path]::GetTempPath()) ("pb-dl-" + [guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Force -Path $dataDir | Out-Null

if ($Config -eq "gaming") {
  $dest = Join-Path $env:USERPROFILE "Downloads"
  $settings = '{"gamingMode":true,"gamingBackgroundAction":"sleep","gamingPauseDownloads":false,"gamingLowMemoryTarget":true}'
} else {
  $dest = Join-Path ([IO.Path]::GetTempPath()) ("pb-dl-dest-" + [guid]::NewGuid().ToString("N"))
  $settings = (@{ downloadDir = $dest } | ConvertTo-Json -Compress)
}
New-Item -ItemType Directory -Force -Path $dest | Out-Null
$env:PROJECT_BROWSER_DATA_DIR = $dataDir
$env:PROJECT_BROWSER_SKIP_ONBOARDING = "1"
$env:PROJECT_BROWSER_TEST_SETTINGS = $settings
if ($Config -eq "noshield") { $env:PROJECT_BROWSER_TEST_SHIELD_OFF = "1" } else { Remove-Item Env:PROJECT_BROWSER_TEST_SHIELD_OFF -ErrorAction SilentlyContinue }
$logPath = Join-Path $dataDir "logs\project-browser.log"
$base = "http://127.0.0.1:$Port"
$checks = [ordered]@{}
$notes = [ordered]@{}
$python = (Get-Command python -ErrorAction SilentlyContinue).Source
if (-not $python) { Write-Host "python not found"; exit 2 }

function LogText { if (Test-Path $logPath) { Get-Content $logPath -Raw } else { "" } }
function WaitFor([scriptblock]$cond, [int]$seconds) {
  $deadline = (Get-Date).AddSeconds($seconds)
  while ((Get-Date) -lt $deadline) {
    if (& $cond) { return $true }
    Start-Sleep -Milliseconds 200
  }
  return [bool](& $cond)
}
function Sha([string]$path) { (Get-FileHash -Algorithm SHA256 -LiteralPath $path).Hash.ToLowerInvariant() }
function Open([string]$url) {
  # A second process forwards the address to the running browser (single instance), as Windows does for links.
  $q = Start-Process -FilePath $Exe -ArgumentList $url -PassThru
  [void]$q.WaitForExit(15000)
}
# A finished file with exactly these bytes, by SHA-256 (the name may get a " (1)" suffix).
function WaitForFile([string]$name, [long]$size, [string]$sha, [int]$seconds) {
  $stem = [IO.Path]::GetFileNameWithoutExtension($name); $ext = [IO.Path]::GetExtension($name)
  $script:found = $null
  $ok = WaitFor {
    foreach ($f in Get-ChildItem -LiteralPath $dest -File -ErrorAction SilentlyContinue) {
      if (($f.Name -eq $name -or ($f.Name.StartsWith("$stem (") -and $f.Name.EndsWith($ext))) -and $f.Length -eq $size) {
        try { if ((Sha $f.FullName) -eq $sha) { $script:found = $f.Name; return $true } } catch { }
      }
    }
    return $false
  } $seconds
  if ($ok) { return $script:found } else { return $null }
}
function DbRows {
  $copy = Join-Path $report "db"
  New-Item -ItemType Directory -Force -Path $copy | Out-Null
  $src = Join-Path $dataDir "profiles\default"
  foreach ($f in "browser.db", "browser.db-wal", "browser.db-shm") { if (Test-Path (Join-Path $src $f)) { Copy-Item (Join-Path $src $f) $copy -Force } }
  $q = "import sqlite3,sys,json;c=sqlite3.connect(sys.argv[1]);print(json.dumps([dict(zip(['filename','state','error','received','total'],r)) for r in c.execute('select filename,state,error,received_bytes,total_bytes from downloads order by started_at')]))"
  $json = & $python -I -c $q (Join-Path $copy "browser.db")
  return ($json | ConvertFrom-Json)
}
function StartBrowser([string]$url) {
  $argv = @(); if ($url) { $argv = @($url) }
  $p = if ($argv.Count) { Start-Process -FilePath $Exe -ArgumentList $argv -PassThru } else { Start-Process -FilePath $Exe -PassThru }
  $ready = WaitFor { (LogText) -match "native hooks installed" } 60
  return @{ Process = $p; Ready = $ready }
}

$server = Start-Process -FilePath $python -ArgumentList @("-I", (Join-Path $PSScriptRoot "fixture-server.py"), "$Port") -PassThru -WindowStyle Hidden -RedirectStandardError (Join-Path $report "fixture-server.log")
$browser = $null
try {
  if (-not (WaitFor { try { (Invoke-WebRequest "$base/manifest.json" -UseBasicParsing -TimeoutSec 2).StatusCode -eq 200 } catch { $false } } 20)) { Write-Host "fixture server did not start"; exit 2 }
  $manifest = (Invoke-WebRequest "$base/manifest.json" -UseBasicParsing).Content | ConvertFrom-Json
  $m = @{}; foreach ($i in $manifest.items) { $m[$i.path] = $i }

  $browser = StartBrowser "$base/links.html"
  if (-not $browser.Ready) { Write-Host "the browser did not start"; exit 2 }
  $checks["browser started"] = $true

  # A 404 first: by the end of the run (long after it finished) no file of that name may exist.
  Open "$base/files/missing.bin"

  $cases = @(
    @{ key = "plain file"; url = "$base/files/plain.bin"; path = "/files/plain.bin"; wait = 40 },
    @{ key = "Content-Disposition name with spaces"; url = "$base/files/cd"; path = "/files/cd"; wait = 30 },
    @{ key = "Unicode file name"; url = "$base/files/unicode"; path = "/files/unicode"; wait = 30 },
    @{ key = "redirect to a file"; url = "$base/redirect"; path = "/files/target.zip"; wait = 30 },
    @{ key = "3.4 MB .exe"; url = "$base/files/setup.exe"; path = "/files/setup.exe"; wait = 60 },
    @{ key = "large file (24 MB, streamed)"; url = "$base/files/large.bin"; path = "/files/large.bin"; wait = 90 },
    @{ key = "same name, first"; url = "$base/files/a/same.txt"; path = "/files/a/same.txt"; wait = 30 },
    @{ key = "same name, second (kept apart)"; url = "$base/files/b/same.txt"; path = "/files/b/same.txt"; wait = 30 },
    @{ key = "blob made by a page script"; url = "$base/blob.html"; path = "/blob.html"; wait = 30 },
    @{ key = "needs a session cookie"; url = "$base/login"; path = "/files/protected.bin"; wait = 30 }
  )
  $names = @{}
  foreach ($c in $cases) {
    $want = $m[$c.path]
    $t0 = Get-Date
    Open $c.url
    $got = WaitForFile $want.name ([long]$want.size) $want.sha256 $c.wait
    $checks["download: $($c.key)"] = [bool]$got
    $notes["$($c.key)"] = if ($got) { "$got in $([int]((Get-Date) - $t0).TotalMilliseconds) ms" } else { "no file with the expected bytes after $($c.wait) s" }
    if ($got) { $names[$c.path] = $got }
  }
  $checks["same name: two different files kept"] = ($names["/files/a/same.txt"] -and $names["/files/b/same.txt"] -and $names["/files/a/same.txt"] -ne $names["/files/b/same.txt"])

  # A transfer that is cut off must end as failed; nothing named broken.bin may be presented as complete.
  Open "$base/files/broken.bin"
  $checks["cut-off transfer ends as failed"] = (WaitFor { (LogText) -match "download \d+: state 1 \(reason (1[2-9]|2[0-5])\)" } 60)
  $checks["cut-off transfer leaves no finished file"] = -not (Test-Path (Join-Path $dest "broken.bin"))
  $checks["404 leaves no file"] = -not (Test-Path (Join-Path $dest "missing.bin"))

  $rows = DbRows
  $rows | ConvertTo-Json -Depth 3 | Set-Content (Join-Path $report "history-before-restart.json") -Encoding utf8
  $completed = @($rows | Where-Object { $_.state -eq "completed" }).Count
  $notes["history rows"] = "$(@($rows).Count) rows, $completed completed"
  $checks["history: every finished download recorded as completed"] = ($completed -ge @($names.Keys).Count)
  $checks["history: the cut-off transfer is not completed"] = (@($rows | Where-Object { $_.filename -like "broken*" -and $_.state -eq "completed" }).Count -eq 0)

  # Restart: the history must come back unchanged.
  Stop-Process -Id $browser.Process.Id -Force
  [void](WaitFor { $browser.Process.HasExited } 10)
  $browser = StartBrowser ""
  $after = DbRows
  $after | ConvertTo-Json -Depth 3 | Set-Content (Join-Path $report "history-after-restart.json") -Encoding utf8
  $checks["history survives a restart"] = (@($after | Where-Object { $_.state -eq "completed" }).Count -eq $completed)
  $checks["log: no panic"] = ((LogText) -notmatch "panic:")
}
finally {
  if ($browser -and -not $browser.Process.HasExited) { Stop-Process -Id $browser.Process.Id -Force }
  Get-Process project-browser -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
  if ($server -and -not $server.HasExited) { Stop-Process -Id $server.Id -Force }
  if (Test-Path $logPath) { Copy-Item $logPath (Join-Path $report "project-browser.log") -Force }
  Get-ChildItem -LiteralPath $dest -ErrorAction SilentlyContinue | Select-Object Name, Length | Format-Table -AutoSize | Out-String | Set-Content (Join-Path $report "destination.txt") -Encoding utf8
}

[ordered]@{ config = $Config; destination = $dest; checks = $checks; notes = $notes } | ConvertTo-Json -Depth 4 | Set-Content (Join-Path $report "summary.json") -Encoding utf8
Write-Host "Download probe ($Config):"
foreach ($c in $checks.GetEnumerator()) { Write-Host ("  [{0}] {1}" -f ($(if ($c.Value) { "pass" } else { "FAIL" })), $c.Key) }
foreach ($c in $notes.GetEnumerator()) { Write-Host ("  ({0}: {1})" -f $c.Key, $c.Value) }
if (@($checks.GetEnumerator() | Where-Object { -not $_.Value }).Count -gt 0) { exit 1 } else { exit 0 }
