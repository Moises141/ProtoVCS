# test-status.ps1
# Automated testing for proto status command - updated to match Git-style output

$ErrorActionPreference = "Stop"

$CARGO_BIN   = "cargo"
$CARGO_OPT   = @("run", "--quiet", "--")
$TEST_ROOT   = "test-status-temp"
$TEST_REPO   = Join-Path $TEST_ROOT "repo"

function Log {
    param([string]$msg, [string]$color = "White")
    Write-Host "[$((Get-Date).ToString('HH:mm:ss'))] $msg" -ForegroundColor $color
}

function Invoke-ProtoStatus {
    Set-Location $TEST_REPO
    $output = & $CARGO_BIN $CARGO_OPT status 2>&1
    Set-Location ../..
    return $output -join "`n"
}

function Assert-StatusContains {
    param(
        [string]$output,
        [string]$expected,
        [string]$testName
    )
    if ($output -match [regex]::Escape($expected)) {
        Log "PASS: $testName" "Green"
    } else {
        Log "FAIL: $testName" "Red"
        Log "Expected: '$expected'" "Yellow"
        Log "Actual output (first few lines):" "Yellow"
        Write-Host ($output -split "`n" | Select-Object -First 12) -ForegroundColor DarkYellow
        Write-Host "... (truncated)" -ForegroundColor DarkYellow
        # exit 1   # Uncomment to stop on first failure
    }
}

# ────────────────────────────────────────────────
# Setup
# ────────────────────────────────────────────────

Log "Starting ProtoVCS status tests..." "Magenta"

if (Test-Path $TEST_ROOT) {
    Remove-Item -Path $TEST_ROOT -Recurse -Force
}
New-Item -ItemType Directory -Path $TEST_REPO | Out-Null

Set-Location $TEST_REPO
& $CARGO_BIN $CARGO_OPT init | Out-Null
Set-Location ../..

Log "Initialized fresh repo → $TEST_REPO" "Cyan"

# ────────────────────────────────────────────────
# Test 1: Fresh repo - should be clean
# ────────────────────────────────────────────────

Log "Test 1: Fresh repo (clean)" "Yellow"
$status = Invoke-ProtoStatus
Assert-StatusContains $status "nothing to commit" "Fresh repo shows clean message (or equivalent)"

# ────────────────────────────────────────────────
# Test 2: Untracked files
# ────────────────────────────────────────────────

Log "Test 2: Untracked files" "Yellow"
"Initial content" | Out-File -FilePath (Join-Path $TEST_REPO "README.md") -Encoding utf8
"Some code"       | Out-File -FilePath (Join-Path $TEST_REPO "main.rs") -Encoding utf8

$status = Invoke-ProtoStatus
Assert-StatusContains $status "Untracked files:" "Shows 'Untracked files:' header"
Assert-StatusContains $status "README.md" "Detects README.md as untracked"
Assert-StatusContains $status "main.rs"   "Detects main.rs as untracked"

# ────────────────────────────────────────────────
# Test 3: Stage files → New files staged
# ────────────────────────────────────────────────

Log "Test 3: Staged new files" "Yellow"
Set-Location $TEST_REPO
& $CARGO_BIN $CARGO_OPT add README.md main.rs | Out-Null
Set-Location ../..

$status = Invoke-ProtoStatus
Assert-StatusContains $status "Changes to be committed" "Shows staged section"
Assert-StatusContains $status "new file:" "Shows 'new file:' label"
Assert-StatusContains $status "README.md" "Staged README.md appears"
Assert-StatusContains $status "main.rs"   "Staged main.rs appears"

# ────────────────────────────────────────────────
# Test 4: Commit → should be clean
# ────────────────────────────────────────────────

Log "Test 4: After commit → clean" "Yellow"
Set-Location $TEST_REPO
& $CARGO_BIN $CARGO_OPT commit -m "Initial commit" | Out-Null
Set-Location ../..

$status = Invoke-ProtoStatus
Assert-StatusContains $status "nothing to commit" "Repo is clean after commit"
# If this fails again, check if index was really deleted and HEAD updated correctly

# ────────────────────────────────────────────────
# Test 5: Modify tracked file → unstaged Modified
# ────────────────────────────────────────────────

Log "Test 5: Modified tracked file (unstaged)" "Yellow"
"More content" | Out-File -FilePath (Join-Path $TEST_REPO "README.md") -Append -Encoding utf8

$status = Invoke-ProtoStatus
Assert-StatusContains $status "Changes not staged for commit" "Shows unstaged section"
Assert-StatusContains $status "modified:" "Shows 'modified:' label"
Assert-StatusContains $status "README.md" "Detects modified README.md"

# ────────────────────────────────────────────────
# Test 6: Stage the modification
# ────────────────────────────────────────────────

Log "Test 6: Stage modification" "Yellow"
Set-Location $TEST_REPO
& $CARGO_BIN $CARGO_OPT add README.md | Out-Null
Set-Location ../..

$status = Invoke-ProtoStatus
Assert-StatusContains $status "Changes to be committed" "Modified file now staged"
Assert-StatusContains $status "modified:" "Shows staged 'modified:' label"
Assert-StatusContains $status "README.md" "Shows staged modified README.md"

# ────────────────────────────────────────────────
# Test 7: Delete tracked file
# ────────────────────────────────────────────────

Log "Test 7: Delete tracked file" "Yellow"
Remove-Item (Join-Path $TEST_REPO "main.rs") -Force

$status = Invoke-ProtoStatus
Assert-StatusContains $status "deleted:" "Detects deleted file"
Assert-StatusContains $status "main.rs" "Shows main.rs as deleted"

# ────────────────────────────────────────────────
# Cleanup
# ────────────────────────────────────────────────

Log "All tests finished. Cleaning up..." "Cyan"
Remove-Item -Path $TEST_ROOT -Recurse -Force

Log "Status tests completed!" "Magenta"
Write-Host "If any FAILs remain, especially after commit, run this manually:" -ForegroundColor White
Write-Host "cd test-status-temp\repo ; cargo run -- status ; cd ..\.." -ForegroundColor Cyan