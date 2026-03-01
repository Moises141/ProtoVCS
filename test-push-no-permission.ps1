# test-push-unauthorized.ps1
# Verifies that push is rejected when allow_remote_push = false or sender not registered

$ErrorActionPreference = "Stop"

$CARGO_BIN = "cargo"
$CARGO_OPT = @("run", "--quiet", "--")
$TEST_ROOT = "test-push-unauth"
$MEMBER_PORT = 5001

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

Log "Creating minimal test repos..." "Cyan"
$memberPath = New-TestRepo "member"
$pusherPath = New-TestRepo "pusher"

# Start member node (no permissions, no join needed for this test)
Log "Starting MEMBER node on port $MEMBER_PORT (no permissions set)..." "Green"
$memberJob = Start-Job -ScriptBlock {
    param($path, $port)
    Set-Location $path
    cargo run --quiet -- serve $port
} -ArgumentList $memberPath, $MEMBER_PORT

Start-Sleep -Seconds 4

# Create payload
$payloadFile = Join-Path $pusherPath "unauth-test.txt"
"SHOULD NOT BE ACCEPTED - unauthorized push test" | Out-File -FilePath $payloadFile -Encoding utf8

# Attempt push from unauthorized pusher
Log "Attempting push from unauthorized pusher (expect rejection)..." "Yellow"
Set-Location $pusherPath
$out = & $CARGO_BIN $CARGO_OPT push $payloadFile "http://127.0.0.1:$MEMBER_PORT" 2>&1
Set-Location ../..

# Determine expected received path (use original filename)
$receivedDir = Join-Path $memberPath "received"
$receivedFile = Join-Path $receivedDir "unauth-test.txt"

Log "DEBUG: Checking for file at: $receivedFile" "Magenta"

# Check if file was created anyway (should NOT happen)
if ([string]::IsNullOrWhiteSpace($receivedFile)) {
    Log "WARNING: Received file path is empty/null - assuming not created" "Yellow"
} elseif (Test-Path $receivedFile) {
    Log "FAIL: File was received even though push should be unauthorized ($receivedFile)" "Red"
    exit 1
} else {
    Log "Good: No unauthorized file created" "Green"
}

# Check client output for success (should NOT appear)
if ($out -match "pushed successfully" -or $out -match "received and verified" -or $out -match "File received") {
    Log "FAIL: Push appears to have succeeded when it should have been rejected! Output:" "Red"
    Write-Host $out
    exit 1
}

# Check for expected rejection signals
if ($out -match "403" -or $out -match "Forbidden" -or $out -match "not registered" -or $out -match "not allowed" -or $out -match "failed") {
    Log "PASS: Push correctly rejected (unauthorized) - got expected error" "Green"
} else {
    Log "FAIL: Push failed but not for the expected reason. Output:" "Red"
    Write-Host $out
    exit 1
}

# Cleanup
Stop-Job $memberJob
Remove-Job $memberJob -Force
Remove-Item -Path $TEST_ROOT -Recurse -Force

Log "Test completed successfully." "Magenta"