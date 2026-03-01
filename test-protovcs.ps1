# test-protovcs.ps1
# Quick & dirty test script for ProtoVCS
# Run this from the project root directory (where Cargo.toml is)

$ErrorActionPreference = "Stop"

# ================= CONFIG =================
$CARGO_BIN = "cargo"
$CARGO_OPT = @("run", "--quiet", "--")
$BASE_PORT = 3333
$HOST_PORT = $BASE_PORT
$NUM_FOLLOWERS = 3
$TEST_ROOT = "test-repos"
# ================= CONFIG =================

function Log {
    param([string]$msg, [string]$color = "White")
    Write-Host "[$((Get-Date).ToString('HH:mm:ss'))] $msg" -ForegroundColor $color
}

function New-TestRepo {
    param([string]$name)

    $path = Join-Path $TEST_ROOT $name
    if (Test-Path $path) {
        Remove-Item -Path $path -Recurse -Force
    }
    New-Item -ItemType Directory -Path $path | Out-Null
    Set-Location $path

    Log "Creating & initializing repo: $name" "Cyan"
    & $CARGO_BIN $CARGO_OPT init | Out-Null

    # Create some dummy files
    "Hello from $name" | Out-File -FilePath "readme-$name.md" -Encoding utf8
    "Some content"     | Out-File -FilePath "data.txt" -Encoding utf8

    & $CARGO_BIN $CARGO_OPT add readme-$name.md data.txt | Out-Null

    Set-Location ../..
    return (Convert-Path $path)
}

# ────────────────────────────────────────────────

Clear-Host
Log "Starting ProtoVCS multi-node test..." "Magenta"
Write-Host ""

if (-not (Test-Path ".\Cargo.toml")) {
    Write-Error "Please run this script from the project root (where Cargo.toml is located)"
    exit 1
}

# Prepare test area
if (Test-Path $TEST_ROOT) {
    Remove-Item -Path $TEST_ROOT -Recurse -Force
}
New-Item -ItemType Directory -Path $TEST_ROOT | Out-Null

# ─── Create repositories ───────────────────────────────────────
$repos = @()
$repos += New-TestRepo "host"
for ($i = 1; $i -le $NUM_FOLLOWERS; $i++) {
    $repos += New-TestRepo "node$i"
}

# ─── Get public keys ───────────────────────────────────────────
$pubkeys = @{}

Log "Collecting public keys..." "Yellow"

for ($i = 0; $i -lt $repos.Count; $i++) {
    $repoPath = $repos[$i]


    # We start each repo once just to generate the key if missing
    Set-Location $repoPath
    & $CARGO_BIN $CARGO_OPT init 2>$null | Out-Null
    
    # Use the new 'whoami' command to get the public key reliably
    $pubHex = & $CARGO_BIN $CARGO_OPT whoami
    $pubHex = $pubHex.Trim()
    
    Set-Location ../..

    if ([string]::IsNullOrWhiteSpace($pubHex)) {
        Write-Error "Failed to get identity for $repoPath"
        exit 1
    }

    $pubkeys["node$i"] = $pubHex
    Log "node$i pubkey = $pubHex" "Gray"
}

# ─── Start Host ────────────────────────────────────────────────
Log "Starting HOST node on port $HOST_PORT ..." "Green"
$hostRepo = $repos[0]
$hostJob = Start-Job -ScriptBlock {
    Set-Location $using:hostRepo
    cargo run -- serve $using:HOST_PORT
} -Name "ProtoVCS-Host"

Start-Sleep -Seconds 3

# Wait until port is listening (simple way)
$ready = $false
for ($i = 0; $i -lt 15; $i++) {
    if (Test-NetConnection -ComputerName localhost -Port $HOST_PORT -InformationLevel Quiet -WarningAction SilentlyContinue) {
        $ready = $true
        break
    }
    Start-Sleep -Seconds 1
}

if (-not $ready) {
    Write-Error "Host did not start listening on port $HOST_PORT"
    Stop-Job $hostJob
    exit 1
}

Log "Host appears to be running ✓" "Green"

Log "Joining host to itself to claim Host role..." "Yellow"
Set-Location $repos[0]
& $CARGO_BIN $CARGO_OPT join "http://127.0.0.1:$HOST_PORT"
Set-Location ../..

# ─── Let followers join ────────────────────────────────────────
$followerPorts = @()
for ($i = 1; $i -le $NUM_FOLLOWERS; $i++) {
    $port = $BASE_PORT + $i
    $followerPorts += $port

    Log "Starting follower node$i on port $port ..." "Cyan"

    $repoPath = $repos[$i]
    $null = Start-Job -ScriptBlock {
        Set-Location $using:repoPath
        cargo run -- serve $using:port
    } -Name "ProtoVCS-Node$i"

    Start-Sleep -Seconds 1

    Log "Letting node$i join the host..." "Yellow"
    Set-Location $repos[$i]
    & $CARGO_BIN $CARGO_OPT join "http://127.0.0.1:$HOST_PORT"
    Set-Location ../..
}

Start-Sleep -Seconds 4

# ─── Try some votes ────────────────────────────────────────────
Log "Casting some votes for node2 to become Gate..." "Yellow"

# node0 (host) votes for node2
Set-Location $repos[0]
& $CARGO_BIN $CARGO_OPT vote $pubkeys["node2"] "http://127.0.0.1:$HOST_PORT"

# node1 votes for node2
Set-Location $repos[1]
& $CARGO_BIN $CARGO_OPT vote $pubkeys["node2"] "http://127.0.0.1:$HOST_PORT"

Set-Location ../..

Start-Sleep -Seconds 2

# ─── Try remote shutdown (only host should succeed) ────────────
Log "Trying remote shutdown (should succeed from host)..." "Magenta"

Set-Location $repos[0]
& $CARGO_BIN $CARGO_OPT shutdown "http://127.0.0.1:$HOST_PORT"

Start-Sleep -Seconds 6

Log "Test sequence finished." "Green"
Write-Host ""
Write-Host "You can now check the console output of the jobs or kill them manually."
Write-Host "To clean up:"
Write-Host "    Remove-Item -Path '$TEST_ROOT' -Recurse -Force"
Write-Host ""

# Optional: don't kill automatically so you can read output
# Stop-Job -Name "ProtoVCS-*"
# Remove-Job -Name "ProtoVCS-*"