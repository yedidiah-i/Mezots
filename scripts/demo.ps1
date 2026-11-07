# Run the whole thing locally.
#
# Starts two nodes on this machine, the web interface, and prints the
# addresses you need. The two nodes find each other over mDNS, so the
# interface shows a live peer, messages travel between them, and the chain
# grows on both sides.
#
#   .\scripts\demo.ps1
#
# On a school network, run the same script on one machine and open the
# printed LAN address from a phone or another laptop. The traffic never
# leaves the network.

param(
    # The port the node protocol listens on.
    [int]$ProtocolPort = 7700,
    # The port the browser talks to the node on.
    [int]$WebSocketPort = 7701,
    # The port the second node uses, so the two do not collide.
    [int]$SecondProtocolPort = 7720,
    [int]$SecondWebSocketPort = 7721,
    # The port the web interface listens on.
    [int]$WebPort = 3000,
    # Skip starting the second node.
    [switch]$SingleNode,
    # Do not start the web interface.
    [switch]$NoWeb
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$binary = Join-Path $root 'target\debug\mezots.exe'
$webDir = Join-Path $root 'frontend-next'

function Write-Step([string]$Message) {
    Write-Host ""
    Write-Host "==> $Message" -ForegroundColor Cyan
}

function Get-LanAddress {
    try {
        $address = Get-NetIPAddress -AddressFamily IPv4 -ErrorAction Stop |
            Where-Object {
                $_.IPAddress -notlike '127.*' -and
                $_.IPAddress -notlike '169.254.*' -and
                $_.InterfaceAlias -notmatch 'Loopback|vEthernet|WSL|VMware|VirtualBox'
            } |
            Select-Object -First 1
        if ($address) { return $address.IPAddress }
    } catch {
        # Fall through to the loopback address.
    }
    return '127.0.0.1'
}

# --- Build -----------------------------------------------------------------

Write-Step 'Building the node'
if (-not (Test-Path $binary)) {
    Push-Location $root
    cargo build
    if ($LASTEXITCODE -ne 0) { Pop-Location; exit 1 }
    Pop-Location
}
Write-Host '  the node is ready' -ForegroundColor DarkGray

# --- Work out the addresses ------------------------------------------------

$lan = Get-LanAddress
$logDir = Join-Path $env:TEMP "mezots-demo-$([guid]::NewGuid().ToString('N').Substring(0,8))"
New-Item -ItemType Directory -Path $logDir | Out-Null

$processes = @()

function Start-Node {
    param([string]$Id, [string]$Name, [int]$Port, [int]$WsPort)

    $out = Join-Path $logDir "$Id.out.txt"
    $err = Join-Path $logDir "$Id.err.txt"

    $arguments = @(
        '--node-id', $Id,
        '--name', $Name,
        '--port', $Port,
        '--ws-port', $WsPort
    )

    $process = Start-Process -FilePath $binary -ArgumentList $arguments `
        -RedirectStandardOutput $out -RedirectStandardError $err `
        -NoNewWindow -PassThru

    return [pscustomobject]@{ Id = $Id; Process = $process; Out = $out; Err = $err }
}

try {
    # --- Nodes -------------------------------------------------------------

    Write-Step 'Starting the nodes'
    $nodeA = Start-Node -Id 'node-a' -Name 'Alpha' -Port $ProtocolPort -WsPort $WebSocketPort
    $processes += $nodeA.Process
    Write-Host "  node-a  (Alpha)  ws://127.0.0.1:$WebSocketPort/ws" -ForegroundColor DarkGray

    if (-not $SingleNode) {
        Start-Sleep -Milliseconds 800
        $nodeB = Start-Node -Id 'node-b' -Name 'Bravo' -Port $SecondProtocolPort -WsPort $SecondWebSocketPort
        $processes += $nodeB.Process
        Write-Host "  node-b  (Bravo)  ws://127.0.0.1:$SecondWebSocketPort/ws" -ForegroundColor DarkGray
    }

    # --- The web interface -------------------------------------------------

    if (-not $NoWeb) {
        Write-Step 'Starting the web interface'
        $webOut = Join-Path $logDir 'web.out.txt'
        $webErr = Join-Path $logDir 'web.err.txt'

        if (-not (Test-Path (Join-Path $webDir 'node_modules'))) {
            Write-Host '  installing the interface dependencies (first run only)' -ForegroundColor Yellow
            Push-Location $webDir
            npm install
            if ($LASTEXITCODE -ne 0) { Pop-Location; exit 1 }
            Pop-Location
        }

        $web = Start-Process -FilePath 'npm.cmd' -ArgumentList @('run', 'dev', '--', '--port', $WebPort) `
            -WorkingDirectory $webDir `
            -RedirectStandardOutput $webOut -RedirectStandardError $webErr `
            -NoNewWindow -PassThru
        $processes += $web.Process
    }

    # --- Say what to do ----------------------------------------------------

    Write-Step 'Ready'
    Write-Host "  interface on this machine : http://localhost:$WebPort" -ForegroundColor Green
    Write-Host "  interface on the network   : http://${lan}:$WebPort" -ForegroundColor Green
    Write-Host "  node endpoint              : ws://${lan}:$WebSocketPort/ws" -ForegroundColor Green
    Write-Host ""
    Write-Host '  Open the interface, then press Ctrl+K and search for a channel.' -ForegroundColor Yellow
    Write-Host '  The two nodes find each other on their own - watch the peer list.' -ForegroundColor Yellow
    Write-Host '  Send a message in #general: it lands in the chain on both nodes.' -ForegroundColor Yellow
    Write-Host ''
    Write-Host "  logs: $logDir" -ForegroundColor DarkGray
    Write-Host ''
    Write-Host '  Press Ctrl+C to stop everything.' -ForegroundColor DarkGray

    # --- Stay up ------------------------------------------------------------

    while ($true) {
        Start-Sleep -Seconds 2
        $alive = $processes | Where-Object { -not $_.HasExited }
        if ($alive.Count -eq 0) { break }
    }
}
finally {
    Write-Host ''
    Write-Host 'Stopping everything...' -ForegroundColor Cyan
    foreach ($process in $processes) {
        if ($process -and -not $process.HasExited) {
            try { $process.Kill() } catch { }
        }
    }
    Write-Host "Logs kept in $logDir" -ForegroundColor DarkGray
}
