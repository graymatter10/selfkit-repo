# End-to-end test of the Selfkit Windows installers on a clean Windows machine (CI runner).
# usage: powershell -File installer\windows\test-installers.ps1 -Dist <folder with the .exe files>
# Checks, per app: silent install -> exe, Start-menu shortcut, Apps & features key; the app
# starts and creates its database; silent uninstall keeps the data; reinstall + uninstall
# with /DELETEDATA deletes only that app's data. Then the same for the agent (autostart,
# native host, running hidden, IPC status) and for the combined Selfkit Setup.
# Docs: docs/platforms/windows.md.
param([Parameter(Mandatory = $true)][string]$Dist, [string[]]$Apps = @('agenda', 'chrono', 'dusk', 'hisab', 'reflect'))
$ErrorActionPreference = 'Stop'
$script:failures = 0
$Data = Join-Path $env:APPDATA 'selfkit'
$Uninst = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall'
$StartMenu = Join-Path $env:APPDATA 'Microsoft\Windows\Start Menu\Programs\Selfkit'

function Check([bool]$cond, [string]$msg) {
  if ($cond) { Write-Host "  ok   $msg" } else { Write-Host "::error::FAIL $msg"; $script:failures++ }
}
function Title([string]$s) { $s.Substring(0, 1).ToUpper() + $s.Substring(1) }
function Run([string]$exe, [string[]]$argv) {
  $p = Start-Process -FilePath $exe -ArgumentList $argv -Wait -PassThru
  return $p.ExitCode
}
function Find-Setup([string]$pattern) {
  $f = Get-ChildItem -Path $Dist -Filter $pattern | Select-Object -First 1
  if (-not $f) { throw "no installer matching $pattern in $Dist" }
  return $f.FullName
}
# NSIS uninstallers copy themselves to %TEMP% and return at once; wait for the real work.
function Wait-Until([scriptblock]$cond, [int]$secs = 90) {
  for ($i = 0; $i -lt $secs; $i++) { if (& $cond) { return $true }; Start-Sleep 1 }
  return $false
}
function Seed-Markers([string]$app) {
  # Files that look like other apps' and shared data; they must survive.
  New-Item -ItemType Directory -Force -Path $Data, "$Data\files\$app", "$Data\backups\$app", "$Data\sync", "$Data\$app-photos" | Out-Null
  Set-Content "$Data\files\$app\a.bin" 'x'
  Set-Content "$Data\backups\$app\b.skb" 'x'
  Set-Content "$Data\sync\state-$app.json" '{}'
  Set-Content "$Data\$app-photos\p.webp" 'x'
  Set-Content "$Data\canary-keep.txt" 'keep'
  foreach ($o in @('agenda', 'chrono', 'dusk', 'hisab', 'reflect') | Where-Object { $_ -ne $app }) {
    New-Item -ItemType Directory -Force -Path "$Data\files\$o" | Out-Null
    Set-Content "$Data\files\$o\keep.bin" 'x'
    Set-Content "$Data\sync\state-$o.json" '{}'
  }
}
function Check-Others-Kept([string]$app) {
  Check (Test-Path "$Data\canary-keep.txt") "shared data kept"
  foreach ($o in @('agenda', 'chrono', 'dusk', 'hisab', 'reflect') | Where-Object { $_ -ne $app }) {
    Check (Test-Path "$Data\files\$o\keep.bin") "$o data kept"
  }
}

# ---------------- per-app installers ----------------
function Test-App([string]$app) {
  $P = Title $app
  Write-Host "== $P"
  $setup = Find-Setup "${P}_*_x64-setup.exe"
  $code = Run $setup @('/S')
  Check ($code -eq 0) "silent install exit code 0 (got $code)"
  $key = "$Uninst\$P"
  Check (Test-Path $key) "Apps & features entry $key"
  if (-not (Test-Path $key)) { return }
  $props = Get-ItemProperty $key
  Check ($props.Publisher -eq 'Selfkit') "publisher is Selfkit (got $($props.Publisher))"
  $inst = $props.InstallLocation.Trim('"')
  $exe = Join-Path $inst $props.MainBinaryName
  Check (Test-Path $exe) "program installed: $exe"
  Check (Test-Path "$inst\uninstall.exe") "uninstaller present"
  Check (Test-Path "$StartMenu\$P.lnk") "Start-menu shortcut Selfkit\$P.lnk"

  # Launch briefly: the app must stay up and create its database.
  $proc = Start-Process -FilePath $exe -PassThru
  $alive = -not (Wait-Until { $proc.HasExited } 10)
  Check $alive "app keeps running (10 s)"
  Check (Wait-Until { Test-Path "$Data\$app.db" } 20) "app created $Data\$app.db"
  if (-not $proc.HasExited) { Stop-Process -Id $proc.Id -Force; Start-Sleep 2 }
  Get-Process | Where-Object { $_.Path -eq $exe } | Stop-Process -Force -ErrorAction SilentlyContinue
  Start-Sleep 1

  # Uninstall, keep data.
  Seed-Markers $app
  $null = Run "$inst\uninstall.exe" @('/S')
  Check (Wait-Until { -not (Test-Path $exe) -and -not (Test-Path $key) }) "uninstall removed program + entry"
  Check (-not (Test-Path "$StartMenu\$P.lnk")) "shortcut removed"
  Check (Test-Path "$Data\$app.db") "data kept after normal uninstall ($app.db)"
  Check (Test-Path "$Data\files\$app\a.bin") "attachments kept after normal uninstall"

  # Reinstall, uninstall with /DELETEDATA.
  $null = Run $setup @('/S')
  Check (Test-Path $exe) "reinstalled"
  $null = Run "$inst\uninstall.exe" @('/S', '/DELETEDATA')
  Check (Wait-Until { -not (Test-Path $exe) -and -not (Test-Path $key) }) "uninstall /DELETEDATA removed program + entry"
  Check (Wait-Until { -not (Test-Path "$Data\$app.db") } 20) "$app.db deleted"
  Check (-not (Test-Path "$Data\files\$app")) "files\$app deleted"
  Check (-not (Test-Path "$Data\backups\$app")) "backups\$app deleted"
  Check (-not (Test-Path "$Data\$app-photos")) "$app-photos deleted"
  Check (-not (Test-Path "$Data\sync\state-$app.json")) "sync state deleted"
  Check-Others-Kept $app
}

# ---------------- agent installer ----------------
function Agent-Running { [bool](Get-Process selfkit-agent -ErrorAction SilentlyContinue) }
function Test-Agent {
  Write-Host "== Selfkit agent"
  $setup = Find-Setup 'SelfkitAgent_*_x64-setup.exe'
  $code = Run $setup @('/S')
  Check ($code -eq 0) "silent install exit code 0 (got $code)"
  $key = "$Uninst\Selfkit Agent"
  Check (Test-Path $key) "Apps & features entry"
  $inst = Join-Path $env:LOCALAPPDATA 'Programs\Selfkit Agent'
  $exe = "$inst\selfkit-agent.exe"
  Check (Test-Path $exe) "agent installed"
  $run = (Get-ItemProperty 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run' -ErrorAction SilentlyContinue).'Selfkit agent'
  Check ($run -like '*selfkit-agent-start.exe*') "autostart Run value ($run)"
  Check (Test-Path 'HKCU:\Software\Google\Chrome\NativeMessagingHosts\com.selfkit.agent') "Chrome native host registered"
  Check (Test-Path 'HKCU:\Software\Chromium\NativeMessagingHosts\com.selfkit.agent') "Chromium native host registered"
  Check (Test-Path 'HKCU:\Software\Microsoft\Edge\NativeMessagingHosts\com.selfkit.agent') "Edge native host registered"
  Check (Test-Path 'HKCU:\Software\Mozilla\NativeMessagingHosts\com.selfkit.agent') "Firefox native host registered"
  $mf = "$env:LOCALAPPDATA\Selfkit\native-host\com.selfkit.agent.chrome.json"
  Check (Test-Path $mf) "native host manifest"
  if (Test-Path $mf) { Check ((Get-Content $mf -Raw | ConvertFrom-Json).path -eq $exe) "manifest points at the installed exe" }
  Check (Test-Path "$inst\extension\manifest.json") "browser extension shipped (L3-3)"
  # One real native-messaging round trip, as a browser would talk to the host (L3-4):
  # 4-byte little-endian length + JSON in, the same framing out.
  try {
    $psi = New-Object System.Diagnostics.ProcessStartInfo $exe, 'native-host chrome-extension://ijiimbgfojojhjlchgbkkhhodifbnami/'
    $psi.UseShellExecute = $false; $psi.RedirectStandardInput = $true; $psi.RedirectStandardOutput = $true; $psi.RedirectStandardError = $true; $psi.CreateNoWindow = $true
    $hp = [System.Diagnostics.Process]::Start($psi)
    $msg = [System.Text.Encoding]::UTF8.GetBytes('{"type":"status"}')
    $hp.StandardInput.BaseStream.Write([BitConverter]::GetBytes([int]$msg.Length), 0, 4)
    $hp.StandardInput.BaseStream.Write($msg, 0, $msg.Length)
    $hp.StandardInput.BaseStream.Flush()
    $len = New-Object byte[] 4
    $task = $hp.StandardOutput.BaseStream.ReadAsync($len, 0, 4)
    $replyOk = $false
    if ($task.Wait(15000) -and $task.Result -eq 4) {
      $buf = New-Object byte[] ([BitConverter]::ToInt32($len, 0))
      $got = 0
      while ($got -lt $buf.Length) { $n = $hp.StandardOutput.BaseStream.Read($buf, $got, $buf.Length - $got); if ($n -le 0) { break }; $got += $n }
      $reply = [System.Text.Encoding]::UTF8.GetString($buf, 0, $got)
      $replyOk = $reply -match '"ok":\s*true'
    }
    $hp.StandardInput.Close()
    if (-not $hp.WaitForExit(5000)) { $hp.Kill() }
    Check $replyOk "native host answers a status message ($reply)"
  } catch { Check $false "native host round trip: $_" }
  Check (Wait-Until { Agent-Running } 20) "agent started"
  $p = Get-Process selfkit-agent -ErrorAction SilentlyContinue | Select-Object -First 1
  if ($p) { Check ($p.MainWindowHandle -eq 0) "agent runs without a window" }
  $ok = Wait-Until { $o = & $exe status 2>$null; $LASTEXITCODE -eq 0 -and "$o" -match '"ok"' } 30
  Check $ok "agent answers 'selfkit-agent status' over the named pipe"
  if ($ok) { & $exe status }

  New-Item -ItemType Directory -Force -Path "$Data\logs" | Out-Null
  Set-Content "$Data\agent.json" '{}'
  Set-Content "$Data\logs\agent-errors.log" ''
  Set-Content "$Data\canary-keep.txt" 'keep'
  $null = Run "$inst\uninstall.exe" @('/S')
  Check (Wait-Until { -not (Test-Path $exe) -and -not (Test-Path $key) }) "uninstall removed agent + entry"
  Check (-not (Agent-Running)) "agent stopped"
  $run = (Get-ItemProperty 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run' -ErrorAction SilentlyContinue).'Selfkit agent'
  Check (-not $run) "autostart removed"
  foreach ($b in 'Google\Chrome', 'Chromium', 'Microsoft\Edge', 'Mozilla') {
    Check (-not (Test-Path "HKCU:\Software\$b\NativeMessagingHosts\com.selfkit.agent")) "$b native host unregistered"
  }
  Check (-not (Test-Path "$env:LOCALAPPDATA\Selfkit\native-host")) "native host manifests removed"
  Check (Test-Path "$Data\agent.json") "agent data kept after normal uninstall"

  $null = Run $setup @('/S', '/NOSTART')
  Check (Test-Path $exe) "reinstalled (/NOSTART)"
  Check (-not (Agent-Running)) "/NOSTART did not start the agent"
  $null = Run "$inst\uninstall.exe" @('/S', '/DELETEDATA')
  Check (Wait-Until { -not (Test-Path $exe) -and -not (Test-Path $key) }) "uninstall /DELETEDATA removed agent"
  Check (-not (Test-Path "$Data\agent.json")) "agent.json deleted"
  Check (-not (Test-Path "$Data\logs")) "agent logs deleted"
  Check (Test-Path "$Data\canary-keep.txt") "shared data kept"
}

# ---------------- combined Selfkit Setup ----------------
function Test-Setup {
  Write-Host "== Selfkit Setup"
  $setup = Find-Setup 'SelfkitSetup_*_x64.exe'
  $code = Run $setup @('/S')
  Check ($code -eq 0) "silent install exit code 0 (got $code)"
  foreach ($a in $Apps) { Check (Test-Path "$Uninst\$(Title $a)") "$(Title $a) installed" }
  Check (Test-Path "$Uninst\Selfkit Agent") "agent installed"
  Check (Test-Path "$Uninst\Selfkit") "Selfkit (all apps) entry"
  Check (Wait-Until { Agent-Running } 20) "agent running"
  $u = Join-Path $env:LOCALAPPDATA 'Programs\Selfkit\Uninstall Selfkit.exe'
  Check (Test-Path $u) "combined uninstaller present"
  New-Item -ItemType Directory -Force -Path $Data | Out-Null
  Set-Content "$Data\canary-keep.txt" 'keep'
  Set-Content "$Data\hisab.db" 'x'
  $null = Run $u @('/S')
  Check (Wait-Until { -not (Test-Path "$Uninst\Selfkit") }) "combined uninstall finished"
  foreach ($a in $Apps) { Check (-not (Test-Path "$Uninst\$(Title $a)")) "$(Title $a) removed" }
  Check (-not (Test-Path "$Uninst\Selfkit Agent")) "agent removed"
  Check (-not (Agent-Running)) "agent stopped"
  Check (-not (Test-Path "$StartMenu\Agenda.lnk")) "shortcuts removed"
  Check ((Test-Path "$Data\canary-keep.txt") -and (Test-Path "$Data\hisab.db")) "data kept after combined uninstall"

  # Pick components, then remove everything including data.
  $code = Run $setup @('/S', '/APPS=hisab,agent')
  Check ($code -eq 0) "silent install /APPS=hisab,agent"
  Check (Test-Path "$Uninst\Hisab") "Hisab installed"
  Check (Test-Path "$Uninst\Selfkit Agent") "agent installed"
  Check (-not (Test-Path "$Uninst\Agenda")) "Agenda not installed"
  $null = Run $u @('/S', '/DELETEDATA')
  Check (Wait-Until { -not (Test-Path "$Uninst\Selfkit") }) "combined uninstall /DELETEDATA finished"
  Check (-not (Test-Path "$Uninst\Hisab")) "Hisab removed"
  Check (Wait-Until { -not (Test-Path $Data) } 20) "all Selfkit data deleted ($Data)"
  Check (-not (Test-Path "$env:LOCALAPPDATA\Selfkit")) "LOCALAPPDATA\Selfkit deleted"
}

foreach ($a in $Apps) { Test-App $a }
Test-Agent
Test-Setup
if ($script:failures -gt 0) { Write-Host "$($script:failures) check(s) failed"; exit 1 }
Write-Host "All installer checks passed."
