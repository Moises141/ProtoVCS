# test-push-non-registered.ps1
# PURPOSE: Verify push is rejected when sender pubkey is NOT in receiver's registry
#          (even when allow_remote_push = true)

$ErrorActionPreference = "Stop"

$CARGO_BIN = "cargo"
$CARGO_OPT = @("run", "--quiet", "--")
$TEST_ROOT = "test-nonreg"
$RECEIVER_PORT = 5010

function Log {
    param([string]$msg, [string]$color = "White")
    Write-Host "[$((Get-Date).ToString('HH:mm:ss'))] $msg" -ForegroundColor $color
}

function New-TestRepo {
    param([string]$name)
    $path = Join-Path $TEST_ROOT $name
    if (Test-Path $path) { Remove-Item -Path $path -Recurse -Force }
    New-Item -ItemType Directory -Path $path | Out-Null
    Set-Location $path
    & $CARGO_BIN $CARGO_OPT init | Out-Null
    Set-Location ../..
    return (Convert-Path $path)
}

# Cleanup previous run
if (Test-Path $TEST_ROOT) { Remove-Item -Path $TEST_ROOT -Recurse -Force }
New-Item -ItemType Directory -Path $TEST_ROOT | Out-Null

Log "Creating repos (receiver + unregistered pusher)..." "Cyan"
$receiverPath = New-TestRepo "receiver"
$pusherPath   = New-TestRepo "unregistered-pusher"

# Start receiver (no joins → registry only contains itself)
Log "Starting RECEIVER on port $RECEIVER_PORT..." "Green"
$receiverJob = Start-Job -ScriptBlock {
    param($path, $port)
    Set-Location $path
    cargo run --quiet -- serve $port
} -ArgumentList $receiverPath, $RECEIVER_PORT

Start-Sleep -Seconds 4

# Explicitly enable remote push so failure is ONLY due to unregistered sender
Log "Enabling remote push on receiver (permission ON)..." "Yellow"
Set-Location $receiverPath
& $CARGO_BIN $CARGO_OPT permissions --allow-push true --address "http://127.0.0.1:$RECEIVER_PORT"
Set-Location ../..

Start-Sleep -Seconds 2

# Create payload from unregistered pusher
$payloadFile = Join-Path $pusherPath "secret.txt"
"THIS SHOULD NEVER ARRIVE - unregistered sender test" | Out-File -FilePath $payloadFile -Encoding utf8

# Attempt push
Log "Attempting push from unregistered sender (expect rejection)..." "Yellow"
Set-Location $pusherPath
$out = & $CARGO_BIN $CARGO_OPT push $payloadFile "http://127.0.0.1:$RECEIVER_PORT" 2>&1
Set-Location ../..

$receivedDir = Join-Path $receiverPath "received"
$receivedFile = Join-Path $receivedDir "secret.txt"

if (Test-Path $receivedFile) {
    Log "FAIL: File was received despite unregistered sender! ($receivedFile)" "Red"
    exit 1
} else {
    Log "Good: No file was saved (as expected)" "Green"
}

# Check for unregistered-specific rejection signals
if ($out -match "not registered" -or $out -match "unknown sender" -or $out -match "unauthorized pubkey" -or $out -match "403" -or $out -match "forbidden") {
    Log "PASS: Push correctly rejected for unregistered sender" "Green"
} else {
    Log "FAIL: Rejection not for expected reason (wanted 'not registered' or 403). Output was:" "Red"
    Write-Host $out
    exit 1
}

# Cleanup
Stop-Job $receiverJob | Out-Null
Remove-Job $receiverJob -Force | Out-Null
Remove-Item -Path $TEST_ROOT -Recurse -Force | Out-Null

Log "Non-registered sender push test completed successfully! 🎉" "Magenta"