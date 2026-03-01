# test-push-authorized.ps1
# Verifies that push SUCCEEDS after enabling allow_remote_push

$ErrorActionPreference = "Stop"

$CARGO_BIN = "cargo"
$CARGO_OPT = @("run", "--quiet", "--")
$TEST_ROOT = "test-push-auth"
$RECEIVER_PORT = 5002
$PUSHER_PORT   = 5003   # pusher also runs a server (so it can join)

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

# Cleanup
if (Test-Path $TEST_ROOT) { Remove-Item -Path $TEST_ROOT -Recurse -Force }
New-Item -ItemType Directory -Path $TEST_ROOT | Out-Null

Log "Creating test repos..." "Cyan"
$receiverPath = New-TestRepo "receiver"
$pusherPath   = New-TestRepo "pusher"

# Start receiver node
Log "Starting RECEIVER node on port $RECEIVER_PORT..." "Green"
$receiverJob = Start-Job -ScriptBlock {
    param($path, $port)
    Set-Location $path
    cargo run --quiet -- serve $port
} -ArgumentList $receiverPath, $RECEIVER_PORT

Start-Sleep -Seconds 4

# Start pusher node
Log "Starting PUSHER node on port $PUSHER_PORT..." "Green"
$pusherJob = Start-Job -ScriptBlock {
    param($path, $port)
    Set-Location $path
    cargo run --quiet -- serve $port
} -ArgumentList $pusherPath, $PUSHER_PORT

Start-Sleep -Seconds 2

# Let pusher join receiver (so receiver knows pusher's pubkey)
Log "Letting pusher join receiver network..." "Yellow"
Set-Location $pusherPath
& $CARGO_BIN $CARGO_OPT join "http://127.0.0.1:$RECEIVER_PORT"
Set-Location ../..

Start-Sleep -Seconds 2

# Create payload
$payloadFile = Join-Path $pusherPath "authorized-test.txt"
"THIS SHOULD BE ACCEPTED AFTER PERMISSION ENABLED" | Out-File -FilePath $payloadFile -Encoding utf8

# Test 1: Push BEFORE enabling permission (expect failure)
Log "Test 1: Push attempt BEFORE permission (expect rejection)..." "Yellow"
Set-Location $pusherPath
$outBefore = & $CARGO_BIN $CARGO_OPT push $payloadFile "http://127.0.0.1:$RECEIVER_PORT" 2>&1
Set-Location ../..

if ($outBefore -match "pushed successfully" -or $outBefore -match "received and verified") {
    Log "FAIL: Push succeeded before permission was enabled!" "Red"
    exit 1
}
Log "Good: Push rejected before permission (as expected)" "Green"

# Enable permission on receiver
Log "Enabling remote push on receiver..." "Yellow"
Set-Location $receiverPath
& $CARGO_BIN $CARGO_OPT permissions --allow-push true --address "http://127.0.0.1:$RECEIVER_PORT"
Set-Location ../..

Start-Sleep -Seconds 2

# Test 2: Push AFTER enabling permission (expect success)
Log "Test 2: Push attempt AFTER permission (expect success)..." "Yellow"
Set-Location $pusherPath
$outAfter = & $CARGO_BIN $CARGO_OPT push $payloadFile "http://127.0.0.1:$RECEIVER_PORT" 2>&1
Set-Location ../..

if ($outAfter -match "pushed successfully" -or $outAfter -match "received and verified") {
    Log "PASS: Authorized push succeeded!" "Green"
} else {
    Log "FAIL: Authorized push did not succeed. Output:" "Red"
    Write-Host $outAfter
    exit 1
}

# Verify received file
$receivedDir  = Join-Path $receiverPath "received"
$receivedFile = Join-Path $receivedDir "authorized-test.txt"

if (Test-Path $receivedFile) {
    $content = Get-Content $receivedFile -Raw
    if ($content -match "THIS SHOULD BE ACCEPTED") {
        Log "Verification PASSED: File content matches" "Green"
    } else {
        Log "Verification FAILED: Content mismatch" "Red"
        exit 1
    }
} else {
    Log "Verification FAILED: File not found in received/" "Red"
    exit 1
}

# Cleanup
Stop-Job $receiverJob, $pusherJob
Remove-Job $receiverJob, $pusherJob -Force
Remove-Item -Path $TEST_ROOT -Recurse -Force

Log "Authorized push test completed successfully." "Magenta"