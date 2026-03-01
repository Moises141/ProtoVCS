# test-permissions.ps1
# Test script for ProtoVCS Permissions & Push

$ErrorActionPreference = "Stop"

# Config
$CARGO_BIN = "cargo"
$CARGO_OPT = @("run", "--quiet", "--")
$TEST_ROOT = "test-perms"
$HOST_PORT = 4000
$MEMBER_PORT = 4001

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

# --- Cleanup & Setup ---
if (Test-Path $TEST_ROOT) { Remove-Item -Path $TEST_ROOT -Recurse -Force }
New-Item -ItemType Directory -Path $TEST_ROOT | Out-Null

Log "Creating repos..." "Cyan"
$hostPath = New-TestRepo "host"
$memberPath = New-TestRepo "member"
$pusherPath = New-TestRepo "pusher"

# --- Start Host ---
Log "Starting HOST on $HOST_PORT..." "Green"
$hostJob = Start-Job -ScriptBlock {
    param($path, $port)
    Set-Location $path
    cargo run --quiet -- serve $port
} -ArgumentList $hostPath, $HOST_PORT

Start-Sleep -Seconds 3

# Host self-join
Set-Location $hostPath
Log "Host self-join..." "Gray"
& $CARGO_BIN $CARGO_OPT join "http://127.0.0.1:$HOST_PORT" | Out-Null
Set-Location ../..

# --- Start Member ---
Log "Starting MEMBER on $MEMBER_PORT..." "Green"
$memberJob = Start-Job -ScriptBlock {
    param($path, $port)
    Set-Location $path
    cargo run --quiet -- serve $port
} -ArgumentList $memberPath, $MEMBER_PORT

Start-Sleep -Seconds 3

# Member join
Set-Location $memberPath
Log "Member joining host..." "Gray"
& $CARGO_BIN $CARGO_OPT join "http://127.0.0.1:$HOST_PORT" | Out-Null
Set-Location ../..

# --- Create payload ---
$payloadFile = Join-Path $pusherPath "payload.txt"
"SUPER SECRET DATA" | Out-File -FilePath $payloadFile -Encoding utf8

# --- Test 1: Push should FAIL by default ---
Log "Test 1: Attempting push (should fail)..." "Yellow"
Set-Location $pusherPath
# Note: Pusher doesn't technically need to be a node to push, just needs the CLI tool. 
# But our current Push command is just a client wrapper.
$out = & $CARGO_BIN $CARGO_OPT push $payloadFile "http://127.0.0.1:$MEMBER_PORT" 2>&1
Set-Location ../..

if ($out -match "Remote push not allowed") {
    Log "Test 1 PASSED: Push rejected as expected." "Green"
}
else {
    Log "Test 1 FAILED: Unexpected output: $out" "Red"
}

# --- Test 2: Enable Permissions ---
Log "Test 2: Enabling remote push on Member..." "Yellow"
Set-Location $memberPath
# This command updates the member's own permissions
& $CARGO_BIN $CARGO_OPT permissions --allow-push true --address "http://127.0.0.1:$MEMBER_PORT"
Set-Location ../..

# --- Test 3: Push should SUCCEED ---
Log "Test 3: Attempting push again (should succeed)..." "Yellow"
Set-Location $pusherPath
$out = & $CARGO_BIN $CARGO_OPT push $payloadFile "http://127.0.0.1:$MEMBER_PORT" 2>&1
Set-Location ../..

if ($out -match "File received successfully") {
    Log "Test 3 PASSED: Push accepted." "Green"
}
else {
    Log "Test 3 FAILED: Unexpected output: $out" "Red"
}

# --- Verify File ---
$receivedFile = Join-Path $memberPath "received" "payload.txt"
if (Test-Path $receivedFile) {
    $content = Get-Content $receivedFile
    if ($content -match "SUPER SECRET DATA") {
        Log "Verification PASSED: File exists and content matches." "Green"
    }
    else {
        Log "Verification FAILED: Content mismatch." "Red"
    }
}
else {
    Log "Verification FAILED: File '$receivedFile' not found." "Red"
}

# --- Cleanup ---
Stop-Job $hostJob
Stop-Job $memberJob
Remove-Job $hostJob
Remove-Job $memberJob
Stop-Process -Name "protovcs" -Force -ErrorAction SilentlyContinue

Log "Done."
