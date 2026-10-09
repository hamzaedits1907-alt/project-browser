<#
  Repeatable memory/CPU benchmark for a browser, measured from OUTSIDE the browser.

  What it does, for each tab count (default 1, 5 and 10):
    1. starts the browser with a brand-new, empty profile and the first N pages of a fixed list,
    2. waits for the pages to load, lets it settle, then samples for an idle window,
    3. every few seconds sums, over the browser's whole process tree (host + every child process, each counted once):
         * private working set  (the "Memory" column of Task Manager: what the browser itself holds, shared pages excluded)
         * CPU time as a percentage of the whole machine
    4. closes the browser and writes a CSV of all samples plus a JSON summary.

  It is deliberately separate from the in-app Performance page: nothing here polls inside the browser.

  Fair-comparison rules (also in docs/BENCHMARK.md): same machine, nothing else running, same page list, same timings,
  fresh profile (no extensions, no sign-in), power plan fixed, run each scenario at least 3 times and compare medians.
  Results are only valid for this page list and these conditions. GPU is not measured (no reliable per-process source).

  examples
    scripts/benchmark.ps1 -Browser project-browser -Exe "$env:LOCALAPPDATA\Project Browser\project-browser.exe"
    scripts/benchmark.ps1 -Browser project-browser -Exe ...\project-browser.exe -Gaming     # Gaming Mode on: background tabs sleep
    scripts/benchmark.ps1 -Browser chrome -Exe "C:\Program Files\Google\Chrome\Application\chrome.exe"
#>
param(
  [Parameter(Mandatory = $true)][string]$Browser,
  [Parameter(Mandatory = $true)][string]$Exe,
  [int[]]$Tabs = @(1, 5, 10),
  [int]$LoadWait = 30,
  [int]$Settle = 30,
  [int]$Idle = 60,
  [int]$SampleEvery = 5,
  [switch]$Gaming,
  [string]$Out = "bench-results"
)
$ErrorActionPreference = "Stop"
if (-not (Test-Path $Exe)) { throw "executable not found: $Exe" }

# Light, static, stable pages: the test measures the browser, not a site's ads or video. Real browsing is heavier.
$Pages = @(
  "https://example.com/",
  "https://en.wikipedia.org/wiki/Rust_(programming_language)",
  "https://www.w3.org/",
  "https://www.gnu.org/",
  "https://httpbin.org/html",
  "https://www.mozilla.org/en-US/",
  "https://developer.mozilla.org/en-US/docs/Web/HTML",
  "https://news.ycombinator.com/",
  "https://www.iana.org/",
  "https://www.kernel.org/"
)

$cpus = [Environment]::ProcessorCount
New-Item -ItemType Directory -Force -Path $Out | Out-Null
$stamp = Get-Date -Format "yyyyMMdd-HHmmss"
$version = (Get-Item $Exe).VersionInfo.ProductVersion
$isPb = ($Browser -eq "project-browser")

function Get-Tree([int]$rootPid) {
  $all = Get-CimInstance Win32_Process | Select-Object ProcessId, ParentProcessId
  $ids = New-Object System.Collections.Generic.HashSet[int]
  [void]$ids.Add($rootPid)
  do {
    $added = $false
    foreach ($p in $all) { if ($ids.Contains([int]$p.ParentProcessId) -and $ids.Add([int]$p.ProcessId)) { $added = $true } }
  } while ($added)
  return $ids
}

# One reading: summed over the process tree, each process counted once (distinct PIDs).
function Sample-Tree([int]$rootPid) {
  $tree = Get-Tree $rootPid
  $c = Get-Counter -Counter "\Process(*)\ID Process", "\Process(*)\Working Set - Private", "\Process(*)\% Processor Time" -SampleInterval 1 -MaxSamples 2
  $byInstance = @{}
  foreach ($s in $c[1].CounterSamples) {
    $inst = $s.InstanceName
    if ($inst -eq "_total" -or $inst -eq "idle") { continue }
    if (-not $byInstance.ContainsKey($inst)) { $byInstance[$inst] = @{} }
    $byInstance[$inst][($s.Path -split "\\")[-1]] = $s.CookedValue
  }
  $mem = 0.0; $cpu = 0.0; $count = 0
  foreach ($kv in $byInstance.GetEnumerator()) {
    $procId = [int]$kv.Value["id process"]
    if ($tree.Contains($procId)) {
      $mem += $kv.Value["working set - private"]
      $cpu += $kv.Value["% processor time"]
      $count++
    }
  }
  [pscustomobject]@{ Memory = $mem; Cpu = $cpu / $cpus; Processes = $count }
}

function Median($values) {
  $a = @($values | Sort-Object)
  if ($a.Count -eq 0) { return $null }
  return $a[[int][math]::Floor(($a.Count - 1) / 2)]
}

$summaries = @()
foreach ($n in $Tabs) {
  $profile = Join-Path ([IO.Path]::GetTempPath()) ("bench-$Browser-$n-" + [guid]::NewGuid().ToString("N"))
  New-Item -ItemType Directory -Force -Path $profile | Out-Null
  $urls = @($Pages[0..($n - 1)])
  $launch = @()
  if ($isPb) {
    $env:PROJECT_BROWSER_DATA_DIR = $profile
    $launch += "--load-all"
    if ($Gaming) { $launch += "--gaming" }
    $launch += $urls
  } else {
    $launch += "--user-data-dir=`"$profile`"", "--no-first-run", "--no-default-browser-check", "--disable-extensions", "--new-window"
    $launch += $urls
  }
  Write-Host "== $Browser $version, $n tab(s)$(if ($Gaming) { ', Gaming Mode' })"
  $p = Start-Process -FilePath $Exe -ArgumentList $launch -PassThru
  Start-Sleep -Seconds $LoadWait
  Start-Sleep -Seconds $Settle
  # Chromium browsers re-exec themselves: the real root is the process that owns our --user-data-dir.
  $root = $p.Id
  if (-not $isPb) {
    $owner = Get-CimInstance Win32_Process | Where-Object { $_.CommandLine -like "*$profile*" -and $_.CommandLine -notlike "*--type=*" } | Select-Object -First 1
    if ($owner) { $root = [int]$owner.ProcessId }
  }
  $rows = @()
  $t0 = Get-Date
  while (((Get-Date) - $t0).TotalSeconds -lt $Idle) {
    $s = Sample-Tree $root
    $rows += [pscustomobject]@{ Seconds = [int](((Get-Date) - $t0).TotalSeconds); MemoryMB = [math]::Round($s.Memory / 1MB, 1); CpuPercent = [math]::Round($s.Cpu, 2); Processes = $s.Processes }
    Write-Host ("  t={0,3}s  {1,8} MB  {2,6} % CPU  {3} processes" -f $rows[-1].Seconds, $rows[-1].MemoryMB, $rows[-1].CpuPercent, $rows[-1].Processes)
    Start-Sleep -Seconds ([math]::Max(0, $SampleEvery - 2))
  }
  $rows | Export-Csv -NoTypeInformation -Path (Join-Path $Out "$stamp-$Browser-$n-tabs.csv")
  foreach ($procId in (Get-Tree $root)) { Stop-Process -Id $procId -Force -ErrorAction SilentlyContinue }
  Start-Sleep -Seconds 3
  Remove-Item Env:PROJECT_BROWSER_DATA_DIR -ErrorAction SilentlyContinue
  Remove-Item -Recurse -Force $profile -ErrorAction SilentlyContinue

  $summaries += [pscustomobject]@{
    browser = $Browser; version = $version; tabs = $n; gaming = [bool]$Gaming; samples = $rows.Count
    memoryMB_median = (Median ($rows | ForEach-Object { $_.MemoryMB })); memoryMB_max = ($rows | Measure-Object MemoryMB -Maximum).Maximum
    cpuPercent_median = (Median ($rows | ForEach-Object { $_.CpuPercent })); cpuPercent_mean = [math]::Round(($rows | Measure-Object CpuPercent -Average).Average, 2)
    processes_median = (Median ($rows | ForEach-Object { $_.Processes }))
  }
}
$os = Get-CimInstance Win32_OperatingSystem
$result = [pscustomobject]@{
  when = (Get-Date).ToString("s")
  machine = [pscustomobject]@{ logicalCpus = $cpus; ramGB = [math]::Round((Get-CimInstance Win32_ComputerSystem).TotalPhysicalMemory / 1GB, 1); os = "$($os.Caption) $($os.BuildNumber)" }
  conditions = [pscustomobject]@{ loadWaitSec = $LoadWait; settleSec = $Settle; idleSec = $Idle; sampleEverySec = $SampleEvery; pages = @($Pages[0..(($Tabs | Measure-Object -Maximum).Maximum - 1)]) }
  results = $summaries
}
$jsonPath = Join-Path $Out "$stamp-$Browser$(if ($Gaming) { '-gaming' })-summary.json"
$result | ConvertTo-Json -Depth 6 | Set-Content $jsonPath -Encoding utf8
Write-Host "`n---- summary ($jsonPath)"
$summaries | Format-Table -AutoSize
