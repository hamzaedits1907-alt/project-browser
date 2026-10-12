<#
  Windows default-browser registration test (runs on the CI runner, never on a user's PC).

  1. Installs the built NSIS installer silently (per-user, like a real install).
  2. Checks every registry value windows/hooks.nsh must write: the ProgIDs and their open commands, the
     StartMenuInternet client, its Capabilities (URL and file associations) and the RegisteredApplications entry.
  3. Opens links the way Windows does: reads the registered `open` command of ProjectBrowserURL / ProjectBrowserHTM,
     puts the target in place of %1 and runs it. Checks that the installed browser opened them, including a second
     link forwarded to the already-running instance and a local .html file.
  4. Uninstalls silently and checks that the registration is gone.

  It cannot open Windows Settings or select the default browser: that needs a person (Windows protects the choice).

  usage: scripts/verify-registration.ps1 -Installer path\to\setup.exe [-ReportDir registration-report]
  exit code: 0 = all checks passed, 1 = a check failed, 2 = could not run
#>
param(
  [Parameter(Mandatory = $true)][string]$Installer,
  [string]$ReportDir = "registration-report"
)
$ErrorActionPreference = "Stop"
if (-not (Test-Path $Installer)) { Write-Host "installer not found: $Installer"; exit 2 }
New-Item -ItemType Directory -Force -Path $ReportDir | Out-Null
$checks = [ordered]@{}
$client = "HKCU:\Software\Clients\StartMenuInternet\Project Browser"
$caps = "$client\Capabilities"

function Value($path, $name) {
  try { return (Get-ItemProperty -Path $path -Name $name -ErrorAction Stop).$name } catch { return $null }
}
function DefaultValue($path) {
  try { return (Get-Item -Path $path -ErrorAction Stop).GetValue("") } catch { return $null }
}

# 1. Install.
Write-Host "installing $Installer"
$p = Start-Process -FilePath $Installer -ArgumentList "/S" -PassThru -Wait
$checks["installer exited with 0"] = ($p.ExitCode -eq 0)

# 2. Registry.
$urlCommand = DefaultValue "HKCU:\Software\Classes\ProjectBrowserURL\shell\open\command"
$htmCommand = DefaultValue "HKCU:\Software\Classes\ProjectBrowserHTM\shell\open\command"
$exe = if ($urlCommand -match '^"([^"]+)"') { $Matches[1] } else { "" }
$checks["ProjectBrowserURL open command names an installed exe"] = ($exe -ne "" -and (Test-Path $exe))
$checks["open commands pass the target as one argument"] = ($urlCommand -like '*--single-argument %1' -and $htmCommand -like '*--single-argument %1')
$checks["ProjectBrowserURL is a URL protocol handler"] = ($null -ne (Value "HKCU:\Software\Classes\ProjectBrowserURL" "URL Protocol"))
$checks["ProgIDs have an application name and icon"] = ((Value "HKCU:\Software\Classes\ProjectBrowserURL\Application" "ApplicationName") -eq "Project Browser" -and
  (DefaultValue "HKCU:\Software\Classes\ProjectBrowserHTM\DefaultIcon") -like "*.exe,0")
$checks["StartMenuInternet client registered"] = ((DefaultValue $client) -eq "Project Browser" -and (DefaultValue "$client\shell\open\command") -like '"*.exe"')
$checks["Capabilities: name, description, icon"] = ((Value $caps "ApplicationName") -eq "Project Browser" -and (Value $caps "ApplicationDescription") -and (Value $caps "ApplicationIcon") -like "*.exe,0")
$checks["URLAssociations: http and https"] = ((Value "$caps\URLAssociations" "http") -eq "ProjectBrowserURL" -and (Value "$caps\URLAssociations" "https") -eq "ProjectBrowserURL")
$checks["FileAssociations: .htm and .html"] = ((Value "$caps\FileAssociations" ".htm") -eq "ProjectBrowserHTM" -and (Value "$caps\FileAssociations" ".html") -eq "ProjectBrowserHTM")
$checks[".html lists ProjectBrowserHTM in OpenWithProgids"] = ($null -ne (Value "HKCU:\Software\Classes\.html\OpenWithProgids" "ProjectBrowserHTM"))
$checks["RegisteredApplications points at the Capabilities"] = ((Value "HKCU:\Software\RegisteredApplications" "Project Browser") -eq "Software\Clients\StartMenuInternet\Project Browser\Capabilities")
reg export "HKCU\Software\Clients\StartMenuInternet\Project Browser" (Join-Path $ReportDir "client.reg") /y | Out-Null
reg export "HKCU\Software\Classes\ProjectBrowserURL" (Join-Path $ReportDir "progid-url.reg") /y | Out-Null

# 3. Open links through the registered commands, against a throwaway profile.
$data = Join-Path ([IO.Path]::GetTempPath()) ("pb-reg-" + [guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Force -Path $data | Out-Null
$env:PROJECT_BROWSER_DATA_DIR = $data
$logPath = Join-Path $data "logs\project-browser.log"
function Log { if (Test-Path $logPath) { Get-Content $logPath -Raw } else { "" } }
function Launch($command, $target) {
  $argLine = ($command -replace '^"[^"]+"\s*', '') -replace '%1', $target
  Write-Host "running: `"$exe`" $argLine"
  Start-Process -FilePath $exe -ArgumentList $argLine -PassThru
}
function WaitFor($pattern, $seconds) {
  for ($i = 0; $i -lt ($seconds * 4); $i++) { if ((Log) -match $pattern) { return $true }; Start-Sleep -Milliseconds 250 }
  return $false
}

if ($exe) {
  $first = Launch $urlCommand "https://example.com/?pb=registration&q=caf%C3%A9&x=1"
  $checks["a link from Windows starts the browser and opens it"] = (WaitFor "webview created \(https://example\.com\)" 40)
  $second = Launch $urlCommand "https://example.org/forwarded?a=1&b=2"
  $checks["a second link goes to the running browser"] = (WaitFor "webview created \(https://example\.org\)" 25)
  Start-Sleep -Seconds 2
  $checks["the second launch did not stay running as another browser"] = $second.HasExited
  $html = Join-Path $data "local page.html"
  Set-Content -Path $html -Value "<!doctype html><title>local</title><p>local file" -Encoding utf8
  [void](Launch $htmCommand "`"$html`"")
  $checks["a local .html file opens"] = (WaitFor "webview created \(file://\)" 25)
  $checks["log: no panic"] = ((Log) -notmatch "panic:")
  Copy-Item $logPath (Join-Path $ReportDir "project-browser.log") -Force -ErrorAction SilentlyContinue
  Get-Process | Where-Object { $_.Path -eq $exe } | Stop-Process -Force -ErrorAction SilentlyContinue
  Start-Sleep -Seconds 2
}

# 4. Uninstall.
$uninstaller = Join-Path (Split-Path $exe) "uninstall.exe"
if ($exe -and (Test-Path $uninstaller)) {
  $u = Start-Process -FilePath $uninstaller -ArgumentList "/S" -PassThru -Wait
  for ($i = 0; $i -lt 40 -and (Test-Path $client); $i++) { Start-Sleep -Milliseconds 250 }
  $checks["uninstaller exited with 0"] = ($u.ExitCode -eq 0)
  $checks["uninstall removes the registration"] = (-not (Test-Path $client) -and -not (Test-Path "HKCU:\Software\Classes\ProjectBrowserURL") -and
    $null -eq (Value "HKCU:\Software\RegisteredApplications" "Project Browser"))
} else {
  $checks["uninstaller found"] = $false
}

$checks | ConvertTo-Json | Set-Content (Join-Path $ReportDir "registration-summary.json")
$failed = @($checks.GetEnumerator() | Where-Object { -not $_.Value })
foreach ($c in $checks.GetEnumerator()) { Write-Host ("{0}  {1}" -f ($(if ($c.Value) { "PASS" } else { "FAIL" })), $c.Key) }
Write-Host "Not tested here: choosing Project Browser in Windows Settings > Default apps (needs a person)."
if ($failed.Count -gt 0) { exit 1 } else { exit 0 }
