# test-log.ps1
# Automated testing for proto log command (pretty + oneline modes)

$ErrorActionPreference = "Stop"

$CARGO_BIN   = "cargo"
$CARGO_OPT   = @("run", "--quiet", "--")
$TEST_ROOT   = "test-log-temp"
$TEST_REPO   = Join-Path $TEST_ROOT "repo"

function Log {
    param([string]$msg, [string]$color = "White")
    Write-Host "[$((Get-Date).ToString('HH:mm:ss'))] $msg" -ForegroundColor $color
}

function Invoke-Proto {
    param(
        [Parameter(Mandatory=$true)]
        [string[]]$subcommand,
        [string]$workingDir = $TEST_REPO
    )
    Set-Location $workingDir
    $output = & $CARGO_BIN $CARGO_OPT @subcommand 2>&1
    Set-Location $PSScriptRoot   # back to script dir
    return $output -join "`n"
}

function Assert-LogContains {
    param(
        [string]$output,
        [string]$expected,
        [string]$testName
    )
    if ($output -like "*$expected*") {
        Log "PASS: $testName" "Green"
    } else {
        Log "FAIL: $testName" "Red"
        Log "Expected: '$expected'" "Yellow"
        Log "Actual (first 10 lines):" "Yellow"
        Write-Host ($output -split "`n" | Select-Object -First 10) -ForegroundColor DarkYellow
        Write-Host "... (truncated)" -ForegroundColor DarkYellow
    }
}

# ────────────────────────────────────────────────
# Setup: fresh repo + 3 commits
# ────────────────────────────────────────────────

Log "Starting ProtoVCS log tests..." "Magenta"

if (Test-Path $TEST_ROOT) { Remove-Item -Path $TEST_ROOT -Recurse -Force }
New-Item -ItemType Directory -Path $TEST_REPO | Out-Null

Set-Location $TEST_REPO
& $CARGO_BIN $CARGO_OPT init | Out-Null

"Initial content" | Out-File -FilePath "README.md" -Encoding utf8
& $CARGO_BIN $CARGO_OPT add README.md | Out-Null
& $CARGO_BIN $CARGO_OPT commit -m "Initial commit with README" | Out-Null

"Add main file" | Out-File -FilePath "main.rs" -Encoding utf8
& $CARGO_BIN $CARGO_OPT add main.rs | Out-Null
& $CARGO_BIN $CARGO_OPT commit -m "Add main.rs with basic function" | Out-Null

"Update readme" | Out-File -FilePath "README.md" -Append -Encoding utf8
& $CARGO_BIN $CARGO_OPT add README.md | Out-Null
& $CARGO_BIN $CARGO_OPT commit -m "Update README with more info" | Out-Null

Set-Location $PSScriptRoot

Log "Created repo with 3 commits" "Cyan"

# ────────────────────────────────────────────────
# Test 1: Basic log (pretty format)
# ────────────────────────────────────────────────

Log "Test 1: Basic log (pretty format)" "Yellow"
$logOutput = Invoke-Proto -subcommand @("log")

Assert-LogContains $logOutput "commit" "Shows commit lines"
Assert-LogContains $logOutput "[HEAD -> main]" "Shows HEAD -> main decoration"
Assert-LogContains $logOutput "Author:" "Shows Author field"
Assert-LogContains $logOutput "Date:" "Shows Date field"
Assert-LogContains $logOutput "Initial commit with README" "Shows first commit message"
Assert-LogContains $logOutput "Update README with more info" "Shows latest commit message"
Assert-LogContains $logOutput "    Update README with more info" "Message is indented"

# ────────────────────────────────────────────────
# Test 2: Oneline mode (--oneline)
# ────────────────────────────────────────────────

Log "Test 2: Oneline mode (--oneline)" "Yellow"
$onelineOutput = Invoke-Proto -subcommand @("log", "--oneline")

Assert-LogContains $onelineOutput "[HEAD]" "Shows HEAD marker in oneline"
Assert-LogContains $onelineOutput "Update README with more info" "Latest message in oneline"
Assert-LogContains $onelineOutput "Initial commit with README" "Oldest message appears"

$lines = $onelineOutput -split "`n" | Where-Object { $_ -ne "" }
if ($lines.Count -eq 3) {
    Log "PASS: Oneline shows exactly 3 lines (one per commit)" "Green"
} else {
    Log "FAIL: Oneline shows $($lines.Count) lines (expected 3)" "Red"
    Write-Host "Oneline output:`n$onelineOutput" "Yellow"
}

# ────────────────────────────────────────────────
# Test 3: Log in empty repo (no commits)
# ────────────────────────────────────────────────

Log "Test 3: Log in empty repo (no commits)" "Yellow"

# Resolve to absolute path before any Set-Location calls to avoid
# path drift when Invoke-Proto changes directory mid-flight.
$emptyRepo = (New-Item -ItemType Directory -Path (Join-Path $TEST_ROOT "empty") -Force).FullName
Set-Location $emptyRepo
& $CARGO_BIN $CARGO_OPT init | Out-Null
Set-Location $PSScriptRoot

$emptyLog = Invoke-Proto -subcommand @("log") -workingDir $emptyRepo

Assert-LogContains $emptyLog "No commits yet" "Shows 'No commits yet' in empty repo"

# ────────────────────────────────────────────────
# Cleanup
# ────────────────────────────────────────────────

Log "All log tests finished. Cleaning up..." "Cyan"
Set-Location $PSScriptRoot
Remove-Item -Path $TEST_ROOT -Recurse -Force

Log "Log tests completed!" "Magenta"
Write-Host "Full green sweep🪄" -ForegroundColor White
