# ULVO Section 7: Multi-Language & Multi-Project Regression Test Suite
$ErrorActionPreference = "Stop"

$ulvo = "E:\Workspace\Active\ULVO\target\release\ulvo.exe"
$nexusProj = "E:\Workspace\Active\NexusNode\server"
$crossLabProj = "E:\Workspace\Active\CrossModalLab"
$dlProj = "E:\Workspace\Academic\sem5\DL\exp1"

Write-Host "============================================================" -ForegroundColor Cyan
Write-Host "     ULVO Section 7: Multi-Language Regression Test Suite   " -ForegroundColor Cyan
Write-Host "============================================================" -ForegroundColor Cyan
Write-Host ""

$testsPassed = 0
$totalTests = 6

# --------------------------------------------------------------------------
# Test 1: Python Standard Library (Math & Core)
# --------------------------------------------------------------------------
Write-Host ">>> TEST 1: Python Standard Library Execution <<<" -ForegroundColor Yellow
$pyMath = (& $ulvo run $nexusProj -- python -c "import math; print('CALC_OK', math.isclose(math.pi, 3.141592653589793))" 2>&1 | Out-String)
Write-Host "  Output: $($pyMath.Trim())" -ForegroundColor DarkGray
if ($pyMath -match "CALC_OK True") {
    Write-Host "  TEST 1 PASSED: Core Python execution verified." -ForegroundColor Green
    $testsPassed++
} else {
    throw "TEST 1 FAILED: Math script failed!"
}

# --------------------------------------------------------------------------
# Test 2: NexusNode / Server (Python 3.11 Project)
# --------------------------------------------------------------------------
Write-Host "`n>>> TEST 2: NexusNode Server Execution <<<" -ForegroundColor Yellow
$pyNexus = (& $ulvo run $nexusProj -- python -c "import sys; print('NEXUS_PY_VER:', sys.version_info[0], sys.version_info[1])" 2>&1 | Out-String)
Write-Host "  Output: $($pyNexus.Trim())" -ForegroundColor DarkGray
if ($pyNexus -match "NEXUS_PY_VER: 3 11") {
    Write-Host "  TEST 2 PASSED: NexusNode Python 3.11 project verified." -ForegroundColor Green
    $testsPassed++
} else {
    throw "TEST 2 FAILED: NexusNode execution failed!"
}

# --------------------------------------------------------------------------
# Test 3: CrossModalLab (PIL Imaging Framework)
# --------------------------------------------------------------------------
Write-Host "`n>>> TEST 3: CrossModalLab Framework Execution <<<" -ForegroundColor Yellow
$pyPIL = (& $ulvo run $crossLabProj -- python -c "import PIL; print('PIL_VER:', PIL.__version__)" 2>&1 | Out-String)
Write-Host "  Output: $($pyPIL.Trim())" -ForegroundColor DarkGray
if ($pyPIL -match "PIL_VER: 12.3.0") {
    Write-Host "  TEST 3 PASSED: CrossModalLab PIL framework verified." -ForegroundColor Green
    $testsPassed++
} else {
    throw "TEST 3 FAILED: CrossModalLab execution failed!"
}

# --------------------------------------------------------------------------
# Test 4: Deep Learning PyTorch + NVIDIA CUDA Telemetry
# --------------------------------------------------------------------------
Write-Host "`n>>> TEST 4: Deep Learning PyTorch + CUDA GPU Execution <<<" -ForegroundColor Yellow
$pyDL = (& $ulvo run $dlProj -- python -c "import torch; print('TORCH_VER:', torch.__version__, 'CUDA_AVAIL:', torch.cuda.is_available(), 'GPU_DEVICE:', torch.cuda.get_device_name(0) if torch.cuda.is_available() else 'NONE')" 2>&1 | Out-String)
Write-Host "  Output: $($pyDL.Trim())" -ForegroundColor DarkGray
if ($pyDL -match "CUDA_AVAIL: True" -and $pyDL -match "NVIDIA GeForce RTX 2050") {
    Write-Host "  TEST 4 PASSED: PyTorch CUDA acceleration verified." -ForegroundColor Green
    $testsPassed++
} else {
    throw "TEST 4 FAILED: PyTorch CUDA execution failed!"
}

# --------------------------------------------------------------------------
# Test 5: Universal Gateway Multi-Language Execution (Go, Ruby, GCC, G++)
# --------------------------------------------------------------------------
Write-Host "`n>>> TEST 5: Gateway Multi-Language Execution <<<" -ForegroundColor Yellow
$goVer = (& $ulvo exec --tool go -- go version 2>&1 | Out-String)
$rubyVer = (& $ulvo exec --tool ruby -- ruby -v 2>&1 | Out-String)
$gccVer = (& $ulvo exec --tool gcc -- gcc --version 2>&1 | Out-String)

Write-Host "  Go:   $($goVer.Trim().Split("`n")[0])" -ForegroundColor DarkGray
Write-Host "  Ruby: $($rubyVer.Trim().Split("`n")[0])" -ForegroundColor DarkGray
Write-Host "  GCC:  $($gccVer.Trim().Split("`n")[0])" -ForegroundColor DarkGray

if ($goVer -match "go version" -and $rubyVer -match "ruby" -and $gccVer -match "gcc") {
    Write-Host "  TEST 5 PASSED: Multi-language toolchains verified through execution gateway." -ForegroundColor Green
    $testsPassed++
} else {
    throw "TEST 5 FAILED: Multi-language toolchain verification failed!"
}

# --------------------------------------------------------------------------
# Test 6: Zero Project .venv & Ephemeral Quiescence
# --------------------------------------------------------------------------
Write-Host "`n>>> TEST 6: Zero Project .venv & Cleanup Invariant <<<" -ForegroundColor Yellow
$nexusVenv = Join-Path $nexusProj ".venv"
$crossVenv = Join-Path $crossLabProj ".venv"
$dlVenv = Join-Path $dlProj ".venv"

if ((Test-Path $nexusVenv) -or (Test-Path $crossVenv) -or (Test-Path $dlVenv)) {
    throw "TEST 6 FAILED: Project directory polluted with .venv!"
}

# Prune ephemeral environments
& $ulvo env prune | Out-Null
$audit = (& $ulvo env audit --json 2>&1 | Out-String | ConvertFrom-Json)
Write-Host "  Active Environments: $($audit.active_environments)" -ForegroundColor DarkGray
Write-Host "  Reclaimable Environments: $($audit.reclaimable_environments)" -ForegroundColor DarkGray

if ($audit.active_environments -eq 0) {
    Write-Host "  TEST 6 PASSED: Zero project .venv and zero lingering active environments." -ForegroundColor Green
    $testsPassed++
} else {
    throw "TEST 6 FAILED: Lingering active environments found after prune!"
}

Write-Host "`n============================================================" -ForegroundColor Cyan
Write-Host "   MULTI-LANGUAGE REGRESSION: $testsPassed / $totalTests PASSED           " -ForegroundColor Cyan
Write-Host "============================================================" -ForegroundColor Cyan
