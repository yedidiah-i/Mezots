# Live two-node demo.
#
# Starts two nodes on this machine, lets them find each other over mDNS (with
# the multicast beacon as the fallback), exchanges a message in both
# directions and prints what happened.
#
#   .\scripts\demo-two-nodes.ps1

param(
    [int]$ProtocolPortA = 7710,
    [int]$ProtocolPortB = 7720,
    [int]$WebSocketPortA = 7711,
    [int]$WebSocketPortB = 7721,
    [int]$BeaconPort = 7712,
    [int]$RunSeconds = 20
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$binary = Join-Path $root 'target\debug\mezots.exe'

if (-not (Test-Path $binary)) {
    Write-Host "Building the node..." -ForegroundColor Cyan
    Push-Location $root
    cargo build
    if ($LASTEXITCODE -ne 0) { Pop-Location; exit 1 }
    Pop-Location
}

$logDir = Join-Path $env:TEMP "mezots-demo-$([guid]::NewGuid().ToString('N').Substring(0,8))"
New-Item -ItemType Directory -Path $logDir | Out-Null

function Start-Node {
    param([string]$Id, [string]$Name, [int]$Port, [int]$WsPort, [string]$Say)

    $out = Join-Path $logDir "$Id.out.txt"
    $err = Join-Path $logDir "$Id.err.txt"

    $arguments = @(
        '--node-id', $Id,
        '--name', $Name,
        '--port', $Port,
        '--ws-port', $WsPort,
        '--beacon-port', $BeaconPort
    )
    if ($Say) { $arguments += @('--say', "`"$Say`"", '--say-after', '10') }

    $process = Start-Process -FilePath $binary -ArgumentList $arguments `
        -RedirectStandardOutput $out -RedirectStandardError $err `
        -NoNewWindow -PassThru

    return [pscustomobject]@{ Id = $Id; Process = $process; Out = $out; Err = $err }
}

function Show-Node {
    param($Node)
    Write-Host ""
    Write-Host "=== $($Node.Id) ===" -ForegroundColor Yellow
    if (Test-Path $Node.Out) {
        $lines = @(Get-Content $Node.Out -ErrorAction SilentlyContinue)
        if ($lines.Count -eq 0) { Write-Host "  (no output)" -ForegroundColor DarkGray }
        foreach ($line in $lines) {
            if ($line -match '^\s*\[(discovery|connect|message|disconnect|sent|error|warn)') {
                Write-Host "  $line" -ForegroundColor Green
            } else {
                Write-Host "  $line"
            }
        }
    }
    $errLines = @()
    if (Test-Path $Node.Err) { $errLines = @(Get-Content $Node.Err -ErrorAction SilentlyContinue) }
    $interesting = @($errLines | Where-Object { $_ -match 'WARN|ERROR|error' })
    if ($interesting.Count -gt 0) {
        Write-Host "  --- warnings ---" -ForegroundColor DarkYellow
        foreach ($line in $interesting) { Write-Host "  $line" -ForegroundColor DarkYellow }
    }
}

Write-Host "Starting two nodes on this machine (mDNS + multicast beacon)..." -ForegroundColor Cyan
$nodeA = Start-Node -Id 'node-a' -Name 'Alpha' -Port $ProtocolPortA -WsPort $WebSocketPortA -Say 'hello from A'
$nodeB = Start-Node -Id 'node-b' -Name 'Bravo' -Port $ProtocolPortB -WsPort $WebSocketPortB -Say 'hi from B'

Write-Host "Running for $RunSeconds seconds..." -ForegroundColor Cyan
Start-Sleep -Seconds $RunSeconds

Show-Node $nodeA
Show-Node $nodeB

Write-Host ""
Write-Host "Stopping the nodes..." -ForegroundColor Cyan
foreach ($node in @($nodeA, $nodeB)) {
    if (-not $node.Process.HasExited) { $node.Process.Kill() }
}
Write-Host "Logs kept in $logDir" -ForegroundColor DarkGray
Write-Host "Done." -ForegroundColor Green
