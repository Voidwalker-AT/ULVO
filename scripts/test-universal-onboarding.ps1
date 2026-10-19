# ULVO Stage 3.3: Universal Multi-Ecosystem Project Onboarding Verification Suite

$ErrorActionPreference = "Stop"

$ulvoRoot = "E:\Workspace\Active\ULVO"
$ulvoExe = "$ulvoRoot\bin\ulvo.exe"
$evidenceDir = "$ulvoRoot\docs\evidence\stage3-3"
if (-not (Test-Path $evidenceDir)) {
    New-Item -ItemType Directory -Path $evidenceDir -Force | Out-Null
}
$evidenceJson = "$evidenceDir\universal-onboarding-verification.json"

Write-Host "============================================================" -ForegroundColor Cyan
Write-Host " ULVO Stage 3.3: Universal Onboarding Engine Verification   " -ForegroundColor Cyan
Write-Host "============================================================" -ForegroundColor Cyan

$testRoot = [System.IO.Path]::Combine([System.IO.Path]::GetTempPath(), "ulvo_onboard_suite_" + [System.Guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Path $testRoot -Force | Out-Null
Write-Host "Scratch test sandbox: $testRoot" -ForegroundColor DarkGray

$testResults = @()

function Run-Onboard-Test {
    param(
        [string]$TestName,
        [scriptblock]$SetupBlock,
        [string[]]$ArgsList,
        [scriptblock]$VerifyBlock
    )
    Write-Host "`n>>> Running: $TestName ..." -ForegroundColor Yellow
    $projDir = Join-Path $testRoot ($TestName -replace '\s+', '_')
    New-Item -ItemType Directory -Path $projDir -Force | Out-Null

    & $SetupBlock $projDir

    $pinfo = New-Object System.Diagnostics.ProcessStartInfo
    $pinfo.FileName = $ulvoExe
    $pinfo.Arguments = ($ArgsList -join " ")
    $pinfo.WorkingDirectory = $projDir
    $pinfo.RedirectStandardOutput = $true
    $pinfo.RedirectStandardError = $true
    $pinfo.UseShellExecute = $false
    $pinfo.CreateNoWindow = $true

    $proc = [System.Diagnostics.Process]::Start($pinfo)
    $stdout = $proc.StandardOutput.ReadToEnd()
    $stderr = $proc.StandardError.ReadToEnd()
    $proc.WaitForExit(10000)
    $exitCode = $proc.ExitCode

    $passed = $false
    $details = ""
    try {
        & $VerifyBlock $projDir $exitCode $stdout $stderr
        $passed = $true
        Write-Host "  [PASS] $TestName" -ForegroundColor Green
    } catch {
        $details = $_.Exception.Message
        Write-Host "  [FAIL] $TestName : $details" -ForegroundColor Red
    }

    $global:testResults += [PSCustomObject]@{
        TestName = $TestName
        Passed = $passed
        ExitCode = $exitCode
        Details = $details
        StdoutPreview = if ($stdout.Length -gt 300) { $stdout.Substring(0, 300) + "..." } else { $stdout }
    }
}

# 1. Single Ecosystem: Python
Run-Onboard-Test -TestName "Python Single Ecosystem" -SetupBlock {
    param($dir)
    Set-Content -Path (Join-Path $dir "requirements.txt") -Value "flask>=2.0`nrequests>=2.28"
} -ArgsList @("project", "onboard", "--policy", "prompt", "-y", "--json") -VerifyBlock {
    param($dir, $code, $stdout, $stderr)
    if ($code -ne 0) { throw "Exit code $code != 0: $stderr" }
    $res = $stdout | ConvertFrom-Json
    if ($res.status -ne $null -and $res.status -ne "INITIALIZED") {
        # Check if direct OnboardResult was output
    }
    $manifest = Join-Path $dir ".ulvo\manifest.toml"
    if (-not (Test-Path $manifest)) { throw "Manifest not created" }
    $content = Get-Content $manifest -Raw
    if ($content -notmatch 'python = ') { throw "Missing python runtime in manifest" }
}

# 2. Single Ecosystem: Rust
Run-Onboard-Test -TestName "Rust Single Ecosystem" -SetupBlock {
    param($dir)
    Set-Content -Path (Join-Path $dir "Cargo.toml") -Value "[package]`nname = `"demo-rs`"`nversion = `"0.1.0`""
} -ArgsList @("project", "onboard", "--policy", "prompt", "-y", "--json") -VerifyBlock {
    param($dir, $code, $stdout, $stderr)
    if ($code -ne 0) { throw "Exit code $code != 0: $stderr" }
    $manifest = Join-Path $dir ".ulvo\manifest.toml"
    $content = Get-Content $manifest -Raw
    if ($content -notmatch 'rust = "1.96.0"') { throw "Missing rust runtime 1.96.0" }
}

# 3. Single Ecosystem: Node.js
Run-Onboard-Test -TestName "Node Single Ecosystem" -SetupBlock {
    param($dir)
    Set-Content -Path (Join-Path $dir "package.json") -Value '{"name": "demo-node", "dependencies": {}}'
    Set-Content -Path (Join-Path $dir "pnpm-lock.yaml") -Value 'lockfileVersion: 5.4'
} -ArgsList @("project", "onboard", "--policy", "prompt", "-y", "--json") -VerifyBlock {
    param($dir, $code, $stdout, $stderr)
    if ($code -ne 0) { throw "Exit code $code != 0: $stderr" }
    $manifest = Join-Path $dir ".ulvo\manifest.toml"
    $content = Get-Content $manifest -Raw
    if ($content -notmatch 'node = "24.15"') { throw "Missing node runtime 24.15" }
    if ($content -notmatch 'pnpm-lock.yaml') { throw "Lockfile not captured in manifest" }
}

# 4. Single Ecosystem: Go
Run-Onboard-Test -TestName "Go Single Ecosystem" -SetupBlock {
    param($dir)
    Set-Content -Path (Join-Path $dir "go.mod") -Value "module example.com/demo"
} -ArgsList @("project", "onboard", "--policy", "prompt", "-y", "--json") -VerifyBlock {
    param($dir, $code, $stdout, $stderr)
    if ($code -ne 0) { throw "Exit code $code != 0: $stderr" }
    $manifest = Join-Path $dir ".ulvo\manifest.toml"
    $content = Get-Content $manifest -Raw
    if ($content -notmatch 'go = "1.26.5"') { throw "Missing go runtime 1.26.5" }
}

# 5. Single Ecosystem: Ruby
Run-Onboard-Test -TestName "Ruby Single Ecosystem" -SetupBlock {
    param($dir)
    Set-Content -Path (Join-Path $dir "Gemfile") -Value "source 'https://rubygems.org'"
} -ArgsList @("project", "onboard", "--policy", "prompt", "-y", "--json") -VerifyBlock {
    param($dir, $code, $stdout, $stderr)
    if ($code -ne 0) { throw "Exit code $code != 0: $stderr" }
    $manifest = Join-Path $dir ".ulvo\manifest.toml"
    $content = Get-Content $manifest -Raw
    if ($content -notmatch 'ruby = "4.0.1"') { throw "Missing ruby runtime 4.0.1" }
}

# 6. Single Ecosystem: C/C++
Run-Onboard-Test -TestName "Cpp Single Ecosystem" -SetupBlock {
    param($dir)
    Set-Content -Path (Join-Path $dir "CMakeLists.txt") -Value "cmake_minimum_required(VERSION 3.20)`nproject(demo)"
} -ArgsList @("project", "onboard", "--policy", "prompt", "-y", "--json") -VerifyBlock {
    param($dir, $code, $stdout, $stderr)
    if ($code -ne 0) { throw "Exit code $code != 0: $stderr" }
    $manifest = Join-Path $dir ".ulvo\manifest.toml"
    $content = Get-Content $manifest -Raw
    if ($content -notmatch 'cpp = "gcc-14.2.0"') { throw "Missing gcc-14.2.0 runtime" }
    if ($content -notmatch 'build_system = "cmake"') { throw "Missing build_system cmake" }
}

# 7. Single Ecosystem: Java
Run-Onboard-Test -TestName "Java Single Ecosystem" -SetupBlock {
    param($dir)
    Set-Content -Path (Join-Path $dir "pom.xml") -Value "<project><modelVersion>4.0.0</modelVersion><groupId>com.demo</groupId><artifactId>demo</artifactId><version>1.0</version></project>"
} -ArgsList @("project", "onboard", "--policy", "prompt", "-y", "--json") -VerifyBlock {
    param($dir, $code, $stdout, $stderr)
    if ($code -ne 0) { throw "Exit code $code != 0: $stderr" }
    $manifest = Join-Path $dir ".ulvo\manifest.toml"
    $content = Get-Content $manifest -Raw
    if ($content -notmatch 'java = "21"') { throw "Missing java runtime 21" }
}

# 8. Polyglot Multi-Ecosystem (Python + Node.js + C++)
Run-Onboard-Test -TestName "Polyglot Multi-Ecosystem" -SetupBlock {
    param($dir)
    Set-Content -Path (Join-Path $dir "pyproject.toml") -Value "[project]`nname = `"ai-app`""
    Set-Content -Path (Join-Path $dir "package.json") -Value '{"name": "ai-frontend"}'
    Set-Content -Path (Join-Path $dir "CMakeLists.txt") -Value "project(native_accel)"
} -ArgsList @("project", "onboard", "--policy", "prompt", "-y", "--json") -VerifyBlock {
    param($dir, $code, $stdout, $stderr)
    if ($code -ne 0) { throw "Exit code $code != 0: $stderr" }
    $manifest = Join-Path $dir ".ulvo\manifest.toml"
    $content = Get-Content $manifest -Raw
    if ($content -notmatch 'python = ') { throw "Missing python in multi-ecosystem" }
    if ($content -notmatch 'node = ') { throw "Missing node in multi-ecosystem" }
    if ($content -notmatch 'cpp = ') { throw "Missing cpp in multi-ecosystem" }
    if ($content -notmatch '\[python\]') { throw "Missing [python] section" }
    if ($content -notmatch '\[node\]') { throw "Missing [node] section" }
    if ($content -notmatch '\[cpp\]') { throw "Missing [cpp] section" }
}

# 9. Monorepo Structure Detection
Run-Onboard-Test -TestName "Monorepo Detection" -SetupBlock {
    param($dir)
    $srv = Join-Path $dir "apps\server"
    $web = Join-Path $dir "apps\web"
    New-Item -ItemType Directory -Path $srv -Force | Out-Null
    New-Item -ItemType Directory -Path $web -Force | Out-Null
    Set-Content -Path (Join-Path $srv "Cargo.toml") -Value "[package]`nname = `"srv`""
    Set-Content -Path (Join-Path $web "package.json") -Value '{"name": "web"}'
} -ArgsList @("project", "onboard", "--policy", "prompt", "-y", "--json") -VerifyBlock {
    param($dir, $code, $stdout, $stderr)
    if ($code -ne 0) { throw "Exit code $code != 0: $stderr" }
    $manifest = Join-Path $dir ".ulvo\manifest.toml"
    $content = Get-Content $manifest -Raw
    if ($content -notmatch 'Monorepo with subprojects:') { throw "Monorepo metadata comment missing" }
    if ($content -notmatch 'rust = ') { throw "Missing rust in monorepo" }
    if ($content -notmatch 'node = ') { throw "Missing node in monorepo" }
}

# 10. Policy MANUAL: Blocks onboarding without -y
Run-Onboard-Test -TestName "Policy Manual Blocking" -SetupBlock {
    param($dir)
    Set-Content -Path (Join-Path $dir "requirements.txt") -Value "pytest"
} -ArgsList @("project", "onboard", "--policy", "manual", "--json") -VerifyBlock {
    param($dir, $code, $stdout, $stderr)
    if ($code -ne 0) { throw "Exit code $code != 0: $stderr" }
    $res = $stdout | ConvertFrom-Json
    if ($res.status -ne "MANUAL_REQUIRED") { throw "Expected MANUAL_REQUIRED, got $($res.status)" }
    $manifest = Join-Path $dir ".ulvo\manifest.toml"
    if (Test-Path $manifest) { throw "Manifest should NOT be created under MANUAL without confirmation" }
}

# 11. Thin Connector Compliance (< 100 KiB Budget)
Run-Onboard-Test -TestName "Thin Connector Budget Check" -SetupBlock {
    param($dir)
    Set-Content -Path (Join-Path $dir "requirements.txt") -Value "numpy"
} -ArgsList @("project", "onboard", "--policy", "prompt", "-y") -VerifyBlock {
    param($dir, $code, $stdout, $stderr)
    if ($code -ne 0) { throw "Exit code $code != 0: $stderr" }
    $ulvoDir = Join-Path $dir ".ulvo"
    $files = Get-ChildItem -Recurse $ulvoDir
    $totalBytes = ($files | Measure-Object -Property Length -Sum).Sum
    if ($totalBytes -gt 102400) { throw "Connector size $totalBytes exceeds 100 KiB budget" }
}

# 12. Idempotent & Force Overwrite
Run-Onboard-Test -TestName "Idempotent and Force Overwrite" -SetupBlock {
    param($dir)
    Set-Content -Path (Join-Path $dir "requirements.txt") -Value "pandas"
} -ArgsList @("project", "onboard", "--policy", "prompt", "-y", "--json") -VerifyBlock {
    param($dir, $code, $stdout, $stderr)
    if ($code -ne 0) { throw "Initial onboard failed" }

    # Second run without force should return ALREADY_INITIALIZED
    $proc2 = [System.Diagnostics.Process]::Start((New-Object System.Diagnostics.ProcessStartInfo -Property @{
        FileName = $ulvoExe; Arguments = "project onboard --policy prompt --json"; WorkingDirectory = $dir;
        RedirectStandardOutput = $true; RedirectStandardError = $true; UseShellExecute = $false; CreateNoWindow = $true
    }))
    $out2 = $proc2.StandardOutput.ReadToEnd()
    $proc2.WaitForExit(5000)
    $res2 = $out2 | ConvertFrom-Json
    if ($res2.status -ne "ALREADY_INITIALIZED") { throw "Expected ALREADY_INITIALIZED, got $($res2.status)" }

    # Third run with --force should succeed
    $proc3 = [System.Diagnostics.Process]::Start((New-Object System.Diagnostics.ProcessStartInfo -Property @{
        FileName = $ulvoExe; Arguments = "project onboard --policy prompt --force -y --json"; WorkingDirectory = $dir;
        RedirectStandardOutput = $true; RedirectStandardError = $true; UseShellExecute = $false; CreateNoWindow = $true
    }))
    $out3 = $proc3.StandardOutput.ReadToEnd()
    $proc3.WaitForExit(5000)
    if ($proc3.ExitCode -ne 0) { throw "Force onboard failed with exit code $($proc3.ExitCode)" }
}

# Cleanup scratch sandbox
try {
    Remove-Item -Recurse -Force $testRoot -ErrorAction SilentlyContinue
} catch { }

Write-Host "`n============================================================" -ForegroundColor Cyan
Write-Host " UNIVERSAL ONBOARDING ENGINE VERIFICATION SUMMARY           " -ForegroundColor Cyan
Write-Host "============================================================" -ForegroundColor Cyan

$testResults | Format-Table -Property TestName, Passed, ExitCode, Details -AutoSize | Out-String | Write-Host

$allPassed = ($testResults | Where-Object { -not $_.Passed }).Count -eq 0
Write-Host "Overall Result: " -NoNewline
if ($allPassed) {
    Write-Host "ALL 12 TESTS PASSED CLEANLY" -ForegroundColor Green
} else {
    Write-Host "TEST FAILURES DETECTED" -ForegroundColor Red
}

$evidencePayload = @{
    schema_version = 1
    timestamp_utc = [DateTimeOffset]::UtcNow.ToString("o")
    total_tests = $testResults.Count
    passed_tests = ($testResults | Where-Object { $_.Passed }).Count
    failed_tests = ($testResults | Where-Object { -not $_.Passed }).Count
    all_passed = $allPassed
    tests = $testResults
}
$evidencePayload | ConvertTo-Json -Depth 5 | Set-Content -Path $evidenceJson -Encoding utf8
Write-Host "Saved machine-readable evidence to $evidenceJson" -ForegroundColor Green
