# test-push-tamper.ps1
# Verifies post-signature content tampering is rejected

$ErrorActionPreference = "Stop"

$CARGO_BIN = "cargo"
$CARGO_OPT = @("run", "--quiet", "--")
$TEST_ROOT = "test-push-tamper"
$MEMBER_PORT = 5004

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

# Cleanup + setup
if (Test-Path $TEST_ROOT) { Remove-Item -Path $TEST_ROOT -Recurse -Force }
New-Item -ItemType Directory -Path $TEST_ROOT | Out-Null

Log "Creating test repos..." "Cyan"
$memberPath = New-TestRepo "member"
$pusherPath = New-TestRepo "pusher"

# Start member node
Log "Starting MEMBER node on port $MEMBER_PORT..." "Green"
$memberJob = Start-Job -ScriptBlock {
    param($path, $port)
    Set-Location $path
    cargo run --quiet -- serve $port
} -ArgumentList $memberPath, $MEMBER_PORT

Start-Sleep -Seconds 4

# Pusher joins (so it has a pubkey the receiver knows)
Log "Letting pusher join receiver network..." "Yellow"
Set-Location $pusherPath
& $CARGO_BIN $CARGO_OPT join "http://127.0.0.1:$MEMBER_PORT"
Set-Location ../..

# Enable remote push on the receiver
Log "Enabling remote push on receiver..." "Yellow"
Set-Location $memberPath
& $CARGO_BIN $CARGO_OPT permissions --allow-push true --address "http://127.0.0.1:$MEMBER_PORT"
Set-Location ../..

# Create the test file
$payloadFile = Join-Path $pusherPath "tamper-test.txt"
"ORIGINAL CONTENT - should be accepted" | Out-File -FilePath $payloadFile -Encoding utf8

# === GOOD PUSH (should succeed) ===
Log "Good push (valid signature + hash) - expect success..." "Yellow"
Set-Location $pusherPath
$out = & $CARGO_BIN $CARGO_OPT push $payloadFile "http://127.0.0.1:$MEMBER_PORT" 2>&1
Set-Location ../..

if ($out -match "File pushed successfully") {
    Log "Good: Valid push succeeded as expected" "Green"
} else {
    Log "FAIL: Good push failed unexpectedly" "Red"
    Write-Host $out
    exit 1
}

# Quick check the file arrived correctly
$receivedFile = Join-Path $memberPath "received" "tamper-test.txt"
if (-not (Test-Path $receivedFile) -or -not ((Get-Content $receivedFile -Raw) -match "ORIGINAL CONTENT")) {
    Log "FAIL: Good file not saved correctly" "Red"
    exit 1
}

# === TAMPERED PUSH (should fail) ===
Log "Tampered push (post-sign tamper via --tamper flag) - expect rejection..." "Yellow"
Set-Location $pusherPath
$out = & $CARGO_BIN $CARGO_OPT push --tamper $payloadFile "http://127.0.0.1:$MEMBER_PORT" 2>&1
Set-Location ../..

if ($out -match "Content hash mismatch" -and $out -match "possible tampering") {
    Log "PASS: Tampered push correctly rejected!" "Green"
} else {
    Log "FAIL: Tampered push was accepted (bad!)" "Red"
    Write-Host $out
    exit 1
}

# Final safety check: make sure the original file wasn't overwritten
if ((Get-Content $receivedFile -Raw) -match "TAMPERED") {
    Log "FAIL: Tampered content was saved anyway!" "Red"
    exit 1
}

# Cleanup
Stop-Job $memberJob | Out-Null
Remove-Job $memberJob -Force | Out-Null
Remove-Item -Path $TEST_ROOT -Recurse -Force | Out-Null

Log "Tamper protection test completed successfully!" "Magenta"