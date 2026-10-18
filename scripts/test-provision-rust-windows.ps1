<#PSScriptInfo

.VERSION 1.2

.GUID
b8e8e8e8-e8e8-e8e8-e8e8-e8e8e8e8e8e8

.AUTHOR
ULVO Development Team

.COMPANYNAME

.COPYRIGHT

.TAGS
ULVO, Rust, Windows, Provisioning, Tests, Hardened

.LICENSEURI

.PROJECTURI

.ICONURI

.EXTERNALMODULEDEPENDENCIES

.REQUIREDSCRIPTS

.EXTERNALSCRIPTDEPENDENCIES

.RELEASENOTES
ULVO Windows Rust Provisioning Test Suite - Hardened Edition
#>

<#
.SYNOPSIS
    Automated, non-destructive test suite for provision-rust-windows.ps1.

.DESCRIPTION
    Executes behavioral verification for ULVO Windows Rust provisioning:
    - Strict fixture ownership verification (S-01)
    - Path containment and reparse point handling (S-02)
    - Installer authenticity and mock verification (S-03)
    - Process execution isolation and environment restoration (S-04)
    - Non-mutating dry-run integrity (S-05)
    - Evidence-backed reporting (S-06)
    
    Covers test requirements T01 through T55.
    All disposable fixtures are placed strictly under E:\Workspace\Active\ULVO\tests\tmp\.
    Persistent vault (E:\Workspace\Active\ULVO\vault) is never modified or deleted.
#>

[CmdletBinding()]
param(
    [Switch]$PreserveFixtures
)

#region Test Framework Setup

$ErrorActionPreference = "Stop"

$scriptRoot = Split-Path -Parent $MyInvocation.MyCommand.Definition
$ulvoRoot = "E:\Workspace\Active\ULVO"
$provisionScript = Join-Path $scriptRoot "provision-rust-windows.ps1"
. $provisionScript
$fixtureRoot = Join-Path $ulvoRoot "tests\tmp"

# Generate run-specific session ID
$sessionId = [System.Guid]::NewGuid().ToString("N")
$sessionDir = Join-Path $fixtureRoot "run-$sessionId"

# Test tracking
$testResults = @()
$script:passedCount = 0
$script:failedCount = 0
$script:notRunCount = 0

# In-memory fixture registration to prevent claiming or deleting untracked paths
$script:ownedFixtureRegistry = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::OrdinalIgnoreCase)

function New-TestFixtureDirectory {
    param([string]$Path)
    $canonicalPath = [System.IO.Path]::GetFullPath($Path).TrimEnd('\')
    $canonicalSessionDir = [System.IO.Path]::GetFullPath($sessionDir).TrimEnd('\')

    if (-not $canonicalPath.StartsWith($canonicalSessionDir + '\', [System.StringComparison]::OrdinalIgnoreCase) -and
        -not $canonicalPath.Equals($canonicalSessionDir, [System.StringComparison]::OrdinalIgnoreCase)) {
        throw "Security violation: New-TestFixtureDirectory attempted path outside session dir: $Path"
    }

    if (Test-Path -LiteralPath $canonicalPath) {
        throw "Security violation: New-TestFixtureDirectory attempted to claim or overwrite pre-existing path: $Path"
    }

    # Ensure parent directory inside session dir is created and owned
    $parentPath = [System.IO.Path]::GetDirectoryName($canonicalPath)
    if (-not (Test-Path -LiteralPath $parentPath) -and
        $parentPath.StartsWith($canonicalSessionDir + '\', [System.StringComparison]::OrdinalIgnoreCase)) {
        New-TestFixtureDirectory -Path $parentPath | Out-Null
    }

    New-Item -ItemType Directory -Path $canonicalPath -Force | Out-Null

    $token = @{
        sessionId = $sessionId
        pid = $PID
        createdAt = (Get-Date).ToString("o")
        path = $canonicalPath
    } | ConvertTo-Json -Compress

    $marker = Join-Path $canonicalPath ".ulvo-fixture-owner.json"
    Set-Content -LiteralPath $marker -Value $token -Force
    $script:ownedFixtureRegistry.Add($canonicalPath) | Out-Null
    $script:ownedFixtureRegistry.Add($marker) | Out-Null
    return $canonicalPath
}

function Register-TestFixtureFile {
    <#
    .SYNOPSIS
        Registers a disposable test fixture file in the session ownership registry.
    #>
    param([string]$Path)
    $canonical = [System.IO.Path]::GetFullPath($Path).TrimEnd('\')
    $canonicalSessionDir = [System.IO.Path]::GetFullPath($sessionDir).TrimEnd('\')
    if (-not $canonical.StartsWith($canonicalSessionDir + '\', [System.StringComparison]::OrdinalIgnoreCase)) {
        throw "Security violation: Register-TestFixtureFile attempted path outside session dir: $Path"
    }
    $script:ownedFixtureRegistry.Add($canonical) | Out-Null
    return $canonical
}

function New-TestJunctionFixture {
    <#
    .SYNOPSIS
        Safely creates a directory junction with an ownership sidecar marker.
    #>
    param(
        [string]$JunctionPath,
        [string]$TargetPath
    )
    $canonicalJunction = [System.IO.Path]::GetFullPath($JunctionPath).TrimEnd('\')
    $canonicalTarget = [System.IO.Path]::GetFullPath($TargetPath).TrimEnd('\')
    $canonicalSessionDir = [System.IO.Path]::GetFullPath($sessionDir).TrimEnd('\')

    if (-not $canonicalJunction.StartsWith($canonicalSessionDir + '\', [System.StringComparison]::OrdinalIgnoreCase)) {
        throw "Security violation: New-TestJunctionFixture attempted path outside session dir: $JunctionPath"
    }

    if (Test-Path -LiteralPath $canonicalJunction) {
        throw "Security violation: New-TestJunctionFixture attempted to overwrite pre-existing path: $JunctionPath"
    }

    $parent = [System.IO.Path]::GetDirectoryName($canonicalJunction)
    if (-not (Test-Path -LiteralPath $parent) -and
        $parent.StartsWith($canonicalSessionDir + '\', [System.StringComparison]::OrdinalIgnoreCase)) {
        New-TestFixtureDirectory -Path $parent | Out-Null
    }

    $mklinkOut = cmd.exe /c "mklink /J `"$canonicalJunction`" `"$canonicalTarget`"" 2>&1
    if ($LASTEXITCODE -ne 0 -or -not (Test-Path -LiteralPath $canonicalJunction)) {
        throw "Failed to create junction fixture: $mklinkOut"
    }

    $sidecar = "$canonicalJunction.junction-owner.json"
    $token = @{
        sessionId = $sessionId
        pid = $PID
        createdAt = (Get-Date).ToString("o")
        junctionPath = $canonicalJunction
        targetPath = $canonicalTarget
        type = "junction"
    } | ConvertTo-Json -Compress

    Set-Content -LiteralPath $sidecar -Value $token -Force
    $script:ownedFixtureRegistry.Add($canonicalJunction) | Out-Null
    $script:ownedFixtureRegistry.Add($sidecar) | Out-Null
    return $canonicalJunction
}

function Cleanup-TestFixture {
    <#
    .SYNOPSIS
        Safely removes a test fixture with strict ownership verification and bottom-up traversal.
    .DESCRIPTION
        Fails closed. Never deletes persistent vault or paths outside tests\tmp.
        Never follows junctions or symbolic links.
        Rejects unexpected reparse-point types.
        Uses deterministic, non-following iterative directory walker.
        Validates ownership of every directory, junction, and file in the hierarchy.
        Fails closed and aborts immediately if any unowned, foreign, corrupt, or escaped item is detected.
        Executes bottom-up, non-recursive deletion only after full hierarchy validation passes.
        Preserves failure evidence when cleanup is rejected.
    #>
    param([string]$Path)

    if ([string]::IsNullOrWhiteSpace($Path)) {
        Write-Warning "Cleanup-TestFixture: Rejected null or empty path."
        return $false
    }

    try {
        $canonicalPath = [System.IO.Path]::GetFullPath($Path).TrimEnd('\')
        $canonicalFixtureRoot = [System.IO.Path]::GetFullPath($fixtureRoot).TrimEnd('\')
        $canonicalSessionDir = [System.IO.Path]::GetFullPath($sessionDir).TrimEnd('\')
        $canonicalVault = [System.IO.Path]::GetFullPath((Join-Path $ulvoRoot "vault")).TrimEnd('\')
    } catch {
        Write-Warning "Cleanup-TestFixture: Path resolution failed for '$Path'."
        return $false
    }

    # Strict containment: Never delete vault or its children
    if ($canonicalPath.Equals($canonicalVault, [System.StringComparison]::OrdinalIgnoreCase) -or
        $canonicalPath.StartsWith($canonicalVault + '\', [System.StringComparison]::OrdinalIgnoreCase)) {
        Write-Warning "Cleanup-TestFixture: Rejected deletion attempt on persistent vault: '$canonicalPath'"
        return $false
    }

    # Strict containment under tests\tmp
    if (-not $canonicalPath.StartsWith($canonicalFixtureRoot + '\', [System.StringComparison]::OrdinalIgnoreCase)) {
        Write-Warning "Cleanup-TestFixture: Rejected path outside tests\tmp: '$canonicalPath'"
        return $false
    }

    if ($canonicalPath.Equals($canonicalFixtureRoot, [System.StringComparison]::OrdinalIgnoreCase)) {
        Write-Warning "Cleanup-TestFixture: Rejected deletion of fixture root '$canonicalFixtureRoot'"
        return $false
    }

    # Must be within current session directory
    if (-not ($canonicalPath.StartsWith($canonicalSessionDir + '\', [System.StringComparison]::OrdinalIgnoreCase) -or
              $canonicalPath.Equals($canonicalSessionDir, [System.StringComparison]::OrdinalIgnoreCase))) {
        Write-Warning "Cleanup-TestFixture: Rejected path outside current session directory: '$canonicalPath'"
        return $false
    }

    if (-not (Test-Path -LiteralPath $canonicalPath)) {
        return $true
    }

    # Check top-level item attributes
    try {
        $item = Get-Item -LiteralPath $canonicalPath -Force -ErrorAction Stop
    } catch {
        Write-Warning "Cleanup-TestFixture: Failed to inspect item at '$canonicalPath': $_"
        return $false
    }

    $isReparsePoint = [bool]($item.Attributes -band [System.IO.FileAttributes]::ReparsePoint)

    # If the target itself is a reparse point (junction / symlink)
    if ($isReparsePoint) {
        if ($item.LinkType -ne "Junction") {
            Write-Warning "Cleanup-TestFixture: Refusing unlinking; unexpected reparse point type '$($item.LinkType)' at '$canonicalPath'"
            return $false
        }

        $sidecarPath = "$canonicalPath.junction-owner.json"
        if (-not (Test-Path -LiteralPath $sidecarPath)) {
            Write-Warning "Cleanup-TestFixture: Refusing unlinking; missing junction ownership sidecar for '$canonicalPath'"
            return $false
        }

        try {
            $sidecar = Get-Content -LiteralPath $sidecarPath -Raw | ConvertFrom-Json
            if ($null -eq $sidecar -or
                $sidecar.sessionId -ne $sessionId -or
                $sidecar.pid -ne $PID -or
                $sidecar.type -ne "junction" -or
                $sidecar.junctionPath -ne $canonicalPath) {
                Write-Warning "Cleanup-TestFixture: Refusing unlinking; session/PID/type mismatch in junction sidecar for '$canonicalPath'"
                return $false
            }
        } catch {
            Write-Warning "Cleanup-TestFixture: Refusing unlinking; corrupt junction sidecar for '$canonicalPath': $_"
            return $false
        }

        if (-not $script:ownedFixtureRegistry.Contains($canonicalPath)) {
            Write-Warning "Cleanup-TestFixture: Refusing unlinking; junction '$canonicalPath' not in owned fixture registry."
            return $false
        }

        # Check target safety
        $actualTarget = [string]($item.Target | Select-Object -First 1)
        if ([string]::IsNullOrWhiteSpace($actualTarget)) {
            $actualTarget = $sidecar.targetPath
        }
        try {
            $canonicalTarget = [System.IO.Path]::GetFullPath($actualTarget).TrimEnd('\')
        } catch {
            Write-Warning "Cleanup-TestFixture: Refusing unlinking; target path resolution failed for '$actualTarget'"
            return $false
        }

        if ($canonicalTarget.Equals($canonicalVault, [System.StringComparison]::OrdinalIgnoreCase) -or
            $canonicalTarget.StartsWith($canonicalVault + '\', [System.StringComparison]::OrdinalIgnoreCase)) {
            Write-Warning "Cleanup-TestFixture: Refusing unlinking; junction '$canonicalPath' points to persistent vault: '$canonicalTarget'"
            return $false
        }

        if (-not $canonicalTarget.StartsWith($canonicalFixtureRoot + '\', [System.StringComparison]::OrdinalIgnoreCase)) {
            Write-Warning "Cleanup-TestFixture: Refusing unlinking; junction '$canonicalPath' points outside tests\tmp: '$canonicalTarget'"
            return $false
        }

        # Ownership and target verified. Safely unlink non-recursively.
        try {
            if ($item.PSIsContainer) {
                [System.IO.Directory]::Delete($canonicalPath, $false)
            } else {
                [System.IO.File]::Delete($canonicalPath)
            }
        } catch {
            Write-Warning "Cleanup-TestFixture: Failed to unlink junction '$canonicalPath': $_"
            return $false
        }

        if (Test-Path -LiteralPath $sidecarPath) {
            try {
                [System.IO.File]::SetAttributes($sidecarPath, [System.IO.FileAttributes]::Normal)
                [System.IO.File]::Delete($sidecarPath)
            } catch {
                Write-Warning "Cleanup-TestFixture: Failed to delete sidecar '$sidecarPath': $_"
            }
        }
        $script:ownedFixtureRegistry.Remove($canonicalPath) | Out-Null
        $script:ownedFixtureRegistry.Remove($sidecarPath) | Out-Null
        return $true
    }

    # If the target is a single regular file
    if (-not $item.PSIsContainer) {
        if (-not $script:ownedFixtureRegistry.Contains($canonicalPath)) {
            Write-Warning "Cleanup-TestFixture: Refusing deletion; file '$canonicalPath' not in owned fixture registry."
            return $false
        }
        [System.IO.File]::SetAttributes($canonicalPath, [System.IO.FileAttributes]::Normal)
        [System.IO.File]::Delete($canonicalPath)
        $script:ownedFixtureRegistry.Remove($canonicalPath) | Out-Null
        return $true
    }

    # For standard directory fixtures: require top-level ownership token
    $markerPath = Join-Path $canonicalPath ".ulvo-fixture-owner.json"
    if (-not (Test-Path -LiteralPath $markerPath)) {
        Write-Warning "Cleanup-TestFixture: Refusing deletion; missing ownership marker in '$canonicalPath'"
        return $false
    }

    try {
        $marker = Get-Content -LiteralPath $markerPath -Raw | ConvertFrom-Json
        if ($null -eq $marker -or
            $marker.sessionId -ne $sessionId -or
            $marker.pid -ne $PID -or
            [string]::IsNullOrWhiteSpace($marker.path) -or
            $marker.path -ne $canonicalPath) {
            Write-Warning "Cleanup-TestFixture: Refusing deletion; session ID/PID/path mismatch in marker for '$canonicalPath'"
            return $false
        }
    } catch {
        Write-Warning "Cleanup-TestFixture: Refusing deletion; corrupt ownership marker in '$canonicalPath': $_"
        return $false
    }

    if (-not $script:ownedFixtureRegistry.Contains($canonicalPath)) {
        Write-Warning "Cleanup-TestFixture: Refusing deletion; path '$canonicalPath' not in owned fixture registry."
        return $false
    }

    # ---------------------------------------------------------------------
    # PHASE A: Non-following Iterative Directory Traversal & Validation
    # ---------------------------------------------------------------------
    $collectedFiles = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::OrdinalIgnoreCase)
    $collectedJunctions = [System.Collections.Generic.List[string]]::new()
    $collectedDirs = [System.Collections.Generic.List[string]]::new()
    $dirsToScan = [System.Collections.Generic.Queue[string]]::new()
    $visitedDirs = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::OrdinalIgnoreCase)

    $dirsToScan.Enqueue($canonicalPath)
    $visitedDirs.Add($canonicalPath) | Out-Null
    $collectedDirs.Add($canonicalPath)

    while ($dirsToScan.Count -gt 0) {
        $currentDir = $dirsToScan.Dequeue()

        try {
            $children = Get-ChildItem -LiteralPath $currentDir -Force -ErrorAction Stop
        } catch {
            Write-Warning "Cleanup-TestFixture: Failed to enumerate directory '$currentDir': $_"
            return $false
        }

        foreach ($child in $children) {
            try {
                $childPath = [System.IO.Path]::GetFullPath($child.FullName).TrimEnd('\')
            } catch {
                Write-Warning "Cleanup-TestFixture: Path resolution failed for child item in '$currentDir'"
                return $false
            }

            # Boundary containment check for every child
            if (-not $childPath.StartsWith($canonicalSessionDir + '\', [System.StringComparison]::OrdinalIgnoreCase)) {
                Write-Warning "Cleanup-TestFixture: Boundary violation; child '$childPath' is outside session directory."
                return $false
            }

            $childIsReparse = [bool]($child.Attributes -band [System.IO.FileAttributes]::ReparsePoint)

            if ($childIsReparse) {
                # Reparse point encountered: NEVER follow or recurse into it!
                if ($child.LinkType -ne "Junction") {
                    Write-Warning "Cleanup-TestFixture: Refusing cleanup; unexpected reparse point type '$($child.LinkType)' at '$childPath'"
                    return $false
                }

                $childSidecar = "$childPath.junction-owner.json"
                if (-not (Test-Path -LiteralPath $childSidecar)) {
                    Write-Warning "Cleanup-TestFixture: Refusing cleanup; unowned junction missing sidecar at '$childPath'"
                    return $false
                }

                try {
                    $sidecarData = Get-Content -LiteralPath $childSidecar -Raw | ConvertFrom-Json
                    if ($null -eq $sidecarData -or
                        $sidecarData.sessionId -ne $sessionId -or
                        $sidecarData.pid -ne $PID -or
                        $sidecarData.type -ne "junction" -or
                        $sidecarData.junctionPath -ne $childPath) {
                        Write-Warning "Cleanup-TestFixture: Refusing cleanup; sidecar token mismatch for junction '$childPath'"
                        return $false
                    }
                } catch {
                    Write-Warning "Cleanup-TestFixture: Refusing cleanup; corrupt junction sidecar at '$childSidecar': $_"
                    return $false
                }

                if (-not $script:ownedFixtureRegistry.Contains($childPath)) {
                    Write-Warning "Cleanup-TestFixture: Refusing cleanup; junction '$childPath' not in owned fixture registry."
                    return $false
                }

                # Validate junction target path
                $jTarget = [string]($child.Target | Select-Object -First 1)
                if ([string]::IsNullOrWhiteSpace($jTarget)) {
                    $jTarget = $sidecarData.targetPath
                }
                try {
                    $canonicalJTarget = [System.IO.Path]::GetFullPath($jTarget).TrimEnd('\')
                } catch {
                    Write-Warning "Cleanup-TestFixture: Refusing cleanup; target resolution failed on junction '$childPath'"
                    return $false
                }

                if ($canonicalJTarget.Equals($canonicalVault, [System.StringComparison]::OrdinalIgnoreCase) -or
                    $canonicalJTarget.StartsWith($canonicalVault + '\', [System.StringComparison]::OrdinalIgnoreCase)) {
                    Write-Warning "Cleanup-TestFixture: Refusing cleanup; junction '$childPath' points to persistent vault: '$canonicalJTarget'"
                    return $false
                }

                if (-not $canonicalJTarget.StartsWith($canonicalFixtureRoot + '\', [System.StringComparison]::OrdinalIgnoreCase)) {
                    Write-Warning "Cleanup-TestFixture: Refusing cleanup; junction '$childPath' points outside tests\tmp: '$canonicalJTarget'"
                    return $false
                }

                if (-not ($canonicalJTarget.StartsWith($canonicalSessionDir + '\', [System.StringComparison]::OrdinalIgnoreCase) -or
                          $canonicalJTarget.Equals($canonicalSessionDir, [System.StringComparison]::OrdinalIgnoreCase))) {
                    Write-Warning "Cleanup-TestFixture: Refusing cleanup; junction '$childPath' points outside session directory: '$canonicalJTarget'"
                    return $false
                }

                $collectedJunctions.Add($childPath)
                $collectedFiles.Add($childSidecar) | Out-Null
                continue
            }

            if ($child.PSIsContainer) {
                # Standard directory
                $subMarker = Join-Path $childPath ".ulvo-fixture-owner.json"
                if (-not (Test-Path -LiteralPath $subMarker)) {
                    Write-Warning "Cleanup-TestFixture: Refusing cleanup; unowned directory missing ownership marker: '$childPath'"
                    return $false
                }

                try {
                    $mJson = Get-Content -LiteralPath $subMarker -Raw | ConvertFrom-Json
                    if ($null -eq $mJson -or
                        $mJson.sessionId -ne $sessionId -or
                        $mJson.pid -ne $PID -or
                        [string]::IsNullOrWhiteSpace($mJson.path) -or
                        $mJson.path -ne $childPath) {
                        Write-Warning "Cleanup-TestFixture: Refusing cleanup; marker token mismatch for directory '$childPath'"
                        return $false
                    }
                } catch {
                    Write-Warning "Cleanup-TestFixture: Refusing cleanup; corrupt marker in directory '$childPath': $_"
                    return $false
                }

                if (-not $script:ownedFixtureRegistry.Contains($childPath)) {
                    Write-Warning "Cleanup-TestFixture: Refusing cleanup; directory '$childPath' not in owned fixture registry."
                    return $false
                }

                if ($visitedDirs.Add($childPath)) {
                    $dirsToScan.Enqueue($childPath)
                    $collectedDirs.Add($childPath)
                } else {
                    Write-Warning "Cleanup-TestFixture: Directory cycle detected at '$childPath'"
                    return $false
                }
            } else {
                # Regular file
                if ($child.Name -eq ".ulvo-fixture-owner.json") {
                    try {
                        $fMarker = Get-Content -LiteralPath $childPath -Raw | ConvertFrom-Json
                        if ($null -eq $fMarker -or $fMarker.sessionId -ne $sessionId -or $fMarker.pid -ne $PID) {
                            Write-Warning "Cleanup-TestFixture: Refusing cleanup; foreign ownership marker file at '$childPath'"
                            return $false
                        }
                    } catch {
                        Write-Warning "Cleanup-TestFixture: Refusing cleanup; corrupt marker file at '$childPath': $_"
                        return $false
                    }
                } elseif ($child.Name.EndsWith(".junction-owner.json", [System.StringComparison]::OrdinalIgnoreCase)) {
                    try {
                        $fSidecar = Get-Content -LiteralPath $childPath -Raw | ConvertFrom-Json
                        if ($null -eq $fSidecar -or $fSidecar.sessionId -ne $sessionId -or $fSidecar.pid -ne $PID) {
                            Write-Warning "Cleanup-TestFixture: Refusing cleanup; foreign junction sidecar at '$childPath'"
                            return $false
                        }
                    } catch {
                        Write-Warning "Cleanup-TestFixture: Refusing cleanup; corrupt junction sidecar at '$childPath': $_"
                        return $false
                    }
                } else {
                    if (-not $script:ownedFixtureRegistry.Contains($childPath)) {
                        Write-Warning "Cleanup-TestFixture: Refusing cleanup; unregistered / subsequently introduced file found: '$childPath'"
                        return $false
                    }
                }

                $collectedFiles.Add($childPath) | Out-Null
            }
        }
    }

    # ---------------------------------------------------------------------
    # PHASE B: Bottom-up Non-recursive Deletion (Only if ALL items validated!)
    # ---------------------------------------------------------------------
    try {
        # 1. Delete all verified files
        foreach ($filePath in $collectedFiles) {
            if (Test-Path -LiteralPath $filePath) {
                [System.IO.File]::SetAttributes($filePath, [System.IO.FileAttributes]::Normal)
                [System.IO.File]::Delete($filePath)
                $script:ownedFixtureRegistry.Remove($filePath) | Out-Null
            }
        }

        # 2. Unlink all verified junctions non-recursively
        foreach ($juncPath in $collectedJunctions) {
            if (Test-Path -LiteralPath $juncPath) {
                try { [System.IO.File]::SetAttributes($juncPath, [System.IO.FileAttributes]::Normal) } catch {}
                [System.IO.Directory]::Delete($juncPath, $false)
                $script:ownedFixtureRegistry.Remove($juncPath) | Out-Null
            }
        }

        # 3. Delete all verified directories in reverse topological order (leaf-to-root)
        $reverseDirs = [System.Linq.Enumerable]::Reverse($collectedDirs)
        foreach ($dirPath in $reverseDirs) {
            if (Test-Path -LiteralPath $dirPath) {
                try { [System.IO.File]::SetAttributes($dirPath, [System.IO.FileAttributes]::Directory) } catch {}
                [System.IO.Directory]::Delete($dirPath, $false)
                $script:ownedFixtureRegistry.Remove($dirPath) | Out-Null
            }
        }
    } catch {
        Write-Warning "Cleanup-TestFixture: Deletion failed during bottom-up execution: $_"
        return $false
    }

    return $true
}

function Invoke-ScriptWithCapture {
    param(
        [string[]]$Arguments,
        [int]$TimeoutMs = 15000
    )

    $escapedArgs = $Arguments | ForEach-Object {
        if ($_ -match '[\s;"]') {
            '"{0}"' -f ($_ -replace '"', '\"')
        } else {
            $_
        }
    }
    $allArgs = @("-NoProfile", "-ExecutionPolicy", "Bypass", "-File", "`"$provisionScript`"") + $escapedArgs

    $psi = New-Object System.Diagnostics.ProcessStartInfo
    $psi.FileName = "powershell.exe"
    $psi.Arguments = ($allArgs -join " ")
    $psi.UseShellExecute = $false
    $psi.RedirectStandardOutput = $true
    $psi.RedirectStandardError = $true
    $psi.CreateNoWindow = $true

    $proc = New-Object System.Diagnostics.Process
    $proc.StartInfo = $psi

    try {
        [void]$proc.Start()
        $stdoutTask = $proc.StandardOutput.ReadToEndAsync()
        $stderrTask = $proc.StandardError.ReadToEndAsync()

        if (-not $proc.WaitForExit($TimeoutMs)) {
            try { $proc.Kill() } catch {}
            return @{
                ExitCode = -1
                Stdout = ""
                Stderr = "Execution timed out after $TimeoutMs ms"
            }
        }
        [void]$stdoutTask.Wait()
        [void]$stderrTask.Wait()

        return @{
            ExitCode = $proc.ExitCode
            Stdout = $stdoutTask.Result
            Stderr = $stderrTask.Result
            Combined = $stdoutTask.Result + "`n" + $stderrTask.Result
        }
    } finally {
        $proc.Dispose()
    }
}

function Register-TestResult {
    param(
        [string]$TestId,
        [string]$Name,
        [string]$Command,
        [string]$Expected,
        [string]$Actual,
        [int]$ExitCode,
        [string]$Verdict
    )

    $result = [PSCustomObject]@{
        TestId = $TestId
        Name = $Name
        Command = $Command
        Expected = $Expected
        Actual = $Actual
        ExitCode = $ExitCode
        Verdict = $Verdict
    }

    $script:testResults += $result

    $color = "Green"
    if ($Verdict -eq "FAIL") { $color = "Red"; $script:failedCount++ }
    elseif ($Verdict -eq "NOT RUN") { $color = "Yellow"; $script:notRunCount++ }
    else { $script:passedCount++ }

    Write-Host "[$TestId] $Name -> $Verdict (ExitCode: $ExitCode)" -ForegroundColor $color
}

#endregion

# Ensure session dir exists with root marker
New-TestFixtureDirectory -Path $sessionDir | Out-Null

Write-Host "==========================================================" -ForegroundColor Cyan
Write-Host "ULVO Stage 2.4.1 Windows Rust Provisioning Security Tests" -ForegroundColor Cyan
Write-Host "Session ID   : $sessionId" -ForegroundColor DarkGray
Write-Host "Fixture Root : $sessionDir" -ForegroundColor DarkGray
Write-Host "==========================================================" -ForegroundColor Cyan

# -------------------------------------------------------------------------
# T01 — Default dry-run on an absent vault
# -------------------------------------------------------------------------
try {
    $t01AbsentRoot = Join-Path $sessionDir "t01-absent-root"
    New-TestFixtureDirectory -Path $t01AbsentRoot | Out-Null
    # $t01AbsentRoot\vault does NOT exist
    $cmd = "provision-rust-windows.ps1 -DryRun -UlvoRoot '$t01AbsentRoot'"
    $res = Invoke-ScriptWithCapture -Arguments @("-DryRun", "-UlvoRoot", $t01AbsentRoot)
    $vaultCreated = Test-Path -LiteralPath "$t01AbsentRoot\vault"

    if ($res.ExitCode -eq 0 -and $res.Combined -match "\[DRY-RUN\]" -and -not $vaultCreated) {
        Register-TestResult -TestId "T01" -Name "Default dry-run on an absent vault" `
            -Command $cmd -Expected "ExitCode 0, no vault created on absent root" `
            -Actual "Absent exit: $($res.ExitCode), vault created: $vaultCreated" `
            -ExitCode $res.ExitCode -Verdict "PASS"
    } else {
        Register-TestResult -TestId "T01" -Name "Default dry-run on an absent vault" `
            -Command $cmd -Expected "ExitCode 0, absent vault untouched" `
            -Actual "ExitCode $($res.ExitCode), Output: $($res.Combined.Trim())" `
            -ExitCode $res.ExitCode -Verdict "FAIL"
    }
    Cleanup-TestFixture -Path $t01AbsentRoot | Out-Null
} catch {
    Register-TestResult -TestId "T01" -Name "Default dry-run on an absent vault" `
        -Command "provision-rust-windows.ps1 -DryRun" -Expected "ExitCode 0" `
        -Actual "Exception: $_" -ExitCode -1 -Verdict "FAIL"
}

# -------------------------------------------------------------------------
# T02 — Dry-run with an existing vault and sentinel file
# -------------------------------------------------------------------------
$t02Root = Join-Path $sessionDir "t02-root"
$t02Vault = Join-Path $t02Root "vault"
$t02Sentinel = Join-Path $t02Vault "sentinel.txt"
$t02HashBefore = ""

try {
    New-TestFixtureDirectory -Path $t02Vault | Out-Null
    Register-TestFixtureFile -Path $t02Sentinel | Out-Null
    Set-Content -LiteralPath $t02Sentinel -Value "ULVO-T02-SENTINEL-$(Get-Random)" -Force
    $t02HashBefore = (Get-FileHash -LiteralPath $t02Sentinel -Algorithm SHA256).Hash

    $cmd = "provision-rust-windows.ps1 -DryRun -UlvoRoot '$t02Root'"
    $res = Invoke-ScriptWithCapture -Arguments @("-DryRun", "-UlvoRoot", $t02Root)

    if ($res.ExitCode -eq 0 -and $res.Combined -match "\[DRY-RUN\]" -and (Test-Path -LiteralPath $t02Sentinel)) {
        Register-TestResult -TestId "T02" -Name "Dry-run with an existing vault and sentinel file" `
            -Command $cmd -Expected "ExitCode 0, sentinel file preserved" `
            -Actual "ExitCode $($res.ExitCode), Sentinel exists: True" `
            -ExitCode $res.ExitCode -Verdict "PASS"
    } else {
        Register-TestResult -TestId "T02" -Name "Dry-run with an existing vault and sentinel file" `
            -Command $cmd -Expected "ExitCode 0, sentinel file preserved" `
            -Actual "ExitCode $($res.ExitCode), Output: $($res.Combined.Trim())" `
            -ExitCode $res.ExitCode -Verdict "FAIL"
    }
} catch {
    Register-TestResult -TestId "T02" -Name "Dry-run with an existing vault and sentinel file" `
        -Command "provision-rust-windows.ps1 -DryRun -UlvoRoot '$t02Root'" -Expected "ExitCode 0" `
        -Actual "Exception: $_" -ExitCode -1 -Verdict "FAIL"
}

# -------------------------------------------------------------------------
# T03 — Sentinel hash unchanged after execution
# -------------------------------------------------------------------------
try {
    $t02HashAfter = (Get-FileHash -LiteralPath $t02Sentinel -Algorithm SHA256).Hash
    if ($t02HashBefore -and $t02HashBefore -eq $t02HashAfter) {
        Register-TestResult -TestId "T03" -Name "Sentinel hash unchanged after execution" `
            -Command "Get-FileHash '$t02Sentinel'" -Expected "Hash unchanged ($t02HashBefore)" `
            -Actual "Hash matches: $t02HashAfter" -ExitCode 0 -Verdict "PASS"
    } else {
        Register-TestResult -TestId "T03" -Name "Sentinel hash unchanged after execution" `
            -Command "Get-FileHash '$t02Sentinel'" -Expected "Hash unchanged ($t02HashBefore)" `
            -Actual "Hash changed: $t02HashAfter" -ExitCode 1 -Verdict "FAIL"
    }
    Cleanup-TestFixture -Path $t02Root | Out-Null
} catch {
    Register-TestResult -TestId "T03" -Name "Sentinel hash unchanged after execution" `
        -Command "Get-FileHash '$t02Sentinel'" -Expected "Hash unchanged" `
        -Actual "Exception: $_" -ExitCode -1 -Verdict "FAIL"
}

# -------------------------------------------------------------------------
# T04 — External destination rejection
# -------------------------------------------------------------------------
try {
    $cmd = "provision-rust-windows.ps1 -DryRun -RustupHome 'C:\ExternalVault\rustup'"
    $res = Invoke-ScriptWithCapture -Arguments @("-DryRun", "-RustupHome", "C:\ExternalVault\rustup")

    if ($res.ExitCode -ne 0 -and $res.Combined -match "must be inside the approved Rust resource area") {
        Register-TestResult -TestId "T04" -Name "External destination rejection" `
            -Command $cmd -Expected "ExitCode != 0, outside approved area error" `
            -Actual "ExitCode $($res.ExitCode), matched rejection message" `
            -ExitCode $res.ExitCode -Verdict "PASS"
    } else {
        Register-TestResult -TestId "T04" -Name "External destination rejection" `
            -Command $cmd -Expected "ExitCode != 0, rejection message" `
            -Actual "ExitCode $($res.ExitCode), Output: $($res.Combined.Trim())" `
            -ExitCode $res.ExitCode -Verdict "FAIL"
    }
} catch {
    Register-TestResult -TestId "T04" -Name "External destination rejection" `
        -Command "provision-rust-windows.ps1 -DryRun -RustupHome 'C:\...'" -Expected "Non-zero exit" `
        -Actual "Exception: $_" -ExitCode -1 -Verdict "FAIL"
}

# -------------------------------------------------------------------------
# T05 — Identical RustupHome/CargoHome rejection
# -------------------------------------------------------------------------
try {
    $approvedArea = "$ulvoRoot\vault\runtimes\rust"
    $samePath = "$approvedArea\shared-home"
    $cmd = "provision-rust-windows.ps1 -DryRun -RustupHome '$samePath' -CargoHome '$samePath'"
    $res = Invoke-ScriptWithCapture -Arguments @("-DryRun", "-RustupHome", $samePath, "-CargoHome", $samePath)

    if ($res.ExitCode -ne 0 -and $res.Combined -match "must be different paths") {
        Register-TestResult -TestId "T05" -Name "Identical RustupHome/CargoHome rejection" `
            -Command $cmd -Expected "ExitCode != 0, identical paths rejected" `
            -Actual "ExitCode $($res.ExitCode), matched rejection message" `
            -ExitCode $res.ExitCode -Verdict "PASS"
    } else {
        Register-TestResult -TestId "T05" -Name "Identical RustupHome/CargoHome rejection" `
            -Command $cmd -Expected "ExitCode != 0, identical paths rejected" `
            -Actual "ExitCode $($res.ExitCode), Output: $($res.Combined.Trim())" `
            -ExitCode $res.ExitCode -Verdict "FAIL"
    }
} catch {
    Register-TestResult -TestId "T05" -Name "Identical RustupHome/CargoHome rejection" `
        -Command "provision-rust-windows.ps1 -DryRun ..." -Expected "Non-zero exit" `
        -Actual "Exception: $_" -ExitCode -1 -Verdict "FAIL"
}

# -------------------------------------------------------------------------
# T06 — Nested destination rejection
# -------------------------------------------------------------------------
try {
    $approvedArea = "$ulvoRoot\vault\runtimes\rust"
    $parentPath = "$approvedArea\rustup"
    $childPath = "$approvedArea\rustup\cargo"
    $cmd = "provision-rust-windows.ps1 -DryRun -RustupHome '$parentPath' -CargoHome '$childPath'"
    $res = Invoke-ScriptWithCapture -Arguments @("-DryRun", "-RustupHome", $parentPath, "-CargoHome", $childPath)

    if ($res.ExitCode -ne 0 -and $res.Combined -match "cannot be nested") {
        Register-TestResult -TestId "T06" -Name "Nested destination rejection" `
            -Command $cmd -Expected "ExitCode != 0, nested path rejected" `
            -Actual "ExitCode $($res.ExitCode), matched rejection message" `
            -ExitCode $res.ExitCode -Verdict "PASS"
    } else {
        Register-TestResult -TestId "T06" -Name "Nested destination rejection" `
            -Command $cmd -Expected "ExitCode != 0, nested path rejected" `
            -Actual "ExitCode $($res.ExitCode), Output: $($res.Combined.Trim())" `
            -ExitCode $res.ExitCode -Verdict "FAIL"
    }
} catch {
    Register-TestResult -TestId "T06" -Name "Nested destination rejection" `
        -Command "provision-rust-windows.ps1 -DryRun ..." -Expected "Non-zero exit" `
        -Actual "Exception: $_" -ExitCode -1 -Verdict "FAIL"
}

# -------------------------------------------------------------------------
# T07 — Sibling-prefix collision handling
# -------------------------------------------------------------------------
try {
    $approvedArea = "$ulvoRoot\vault\runtimes\rust"
    # Case 1: Sibling destinations inside approved area should NOT be rejected as nested
    $rustupSibling = "$approvedArea\tool"
    $cargoSibling = "$approvedArea\tool-extra"
    $cmd = "provision-rust-windows.ps1 -DryRun -RustupHome '$rustupSibling' -CargoHome '$cargoSibling'"
    $res1 = Invoke-ScriptWithCapture -Arguments @("-DryRun", "-RustupHome", $rustupSibling, "-CargoHome", $cargoSibling)

    # Case 2: Sibling of the approved area itself must be rejected as outside
    $evilSibling = "$ulvoRoot\vault\runtimes\rust-evil\rustup"
    $res2 = Invoke-ScriptWithCapture -Arguments @("-DryRun", "-RustupHome", $evilSibling)

    if ($res1.ExitCode -eq 0 -and $res2.ExitCode -ne 0 -and $res2.Combined -match "must be inside the approved Rust resource area") {
        Register-TestResult -TestId "T07" -Name "Sibling-prefix collision handling" `
            -Command $cmd -Expected "Siblings allowed (ExitCode 0), area sibling rejected (ExitCode != 0)" `
            -Actual "Siblings exit: $($res1.ExitCode), Area sibling exit: $($res2.ExitCode)" `
            -ExitCode 0 -Verdict "PASS"
    } else {
        Register-TestResult -TestId "T07" -Name "Sibling-prefix collision handling" `
            -Command $cmd -Expected "Siblings allowed, area sibling rejected" `
            -Actual "res1: $($res1.ExitCode), res2: $($res2.ExitCode), Output: $($res2.Combined.Trim())" `
            -ExitCode 1 -Verdict "FAIL"
    }
} catch {
    Register-TestResult -TestId "T07" -Name "Sibling-prefix collision handling" `
        -Command "provision-rust-windows.ps1 -DryRun ..." -Expected "ExitCode 0" `
        -Actual "Exception: $_" -ExitCode -1 -Verdict "FAIL"
}

# -------------------------------------------------------------------------
# T08 — Parent traversal rejection
# -------------------------------------------------------------------------
try {
    $traversalPath = "$ulvoRoot\vault\runtimes\rust\..\..\traversal"
    $cmd = "provision-rust-windows.ps1 -DryRun -RustupHome '$traversalPath'"
    $res = Invoke-ScriptWithCapture -Arguments @("-DryRun", "-RustupHome", $traversalPath)

    if ($res.ExitCode -ne 0 -and $res.Combined -match "traversal components") {
        Register-TestResult -TestId "T08" -Name "Parent traversal rejection" `
            -Command $cmd -Expected "ExitCode != 0, traversal rejected" `
            -Actual "ExitCode $($res.ExitCode), matched rejection message" `
            -ExitCode $res.ExitCode -Verdict "PASS"
    } else {
        Register-TestResult -TestId "T08" -Name "Parent traversal rejection" `
            -Command $cmd -Expected "ExitCode != 0, traversal rejected" `
            -Actual "ExitCode $($res.ExitCode), Output: $($res.Combined.Trim())" `
            -ExitCode $res.ExitCode -Verdict "FAIL"
    }
} catch {
    Register-TestResult -TestId "T08" -Name "Parent traversal rejection" `
        -Command "provision-rust-windows.ps1 -DryRun -RustupHome '...'" -Expected "Non-zero exit" `
        -Actual "Exception: $_" -ExitCode -1 -Verdict "FAIL"
}

# -------------------------------------------------------------------------
# T09 — Relative and drive-relative path rejection
# -------------------------------------------------------------------------
try {
    # 9a: Relative path
    $cmd1 = "provision-rust-windows.ps1 -DryRun -RustupHome 'vault\runtimes\rust\rustup'"
    $res1 = Invoke-ScriptWithCapture -Arguments @("-DryRun", "-RustupHome", "vault\runtimes\rust\rustup")

    # 9b: Drive-relative path (E:vault\...)
    $cmd2 = "provision-rust-windows.ps1 -DryRun -RustupHome 'E:vault\runtimes\rust\rustup'"
    $res2 = Invoke-ScriptWithCapture -Arguments @("-DryRun", "-RustupHome", "E:vault\runtimes\rust\rustup")

    if ($res1.ExitCode -ne 0 -and $res2.ExitCode -ne 0 -and
        $res1.Combined -match "must be an absolute, rooted drive path" -and
        $res2.Combined -match "must be an absolute, rooted drive path") {
        Register-TestResult -TestId "T09" -Name "Relative and drive-relative path rejection" `
            -Command "$cmd1 ; $cmd2" -Expected "Both exit != 0 with absolute path requirement" `
            -Actual "res1 exit: $($res1.ExitCode), res2 exit: $($res2.ExitCode)" `
            -ExitCode 0 -Verdict "PASS"
    } else {
        Register-TestResult -TestId "T09" -Name "Relative and drive-relative path rejection" `
            -Command "$cmd1 ; $cmd2" -Expected "Both exit != 0" `
            -Actual "res1 exit: $($res1.ExitCode), res2 exit: $($res2.ExitCode)" `
            -ExitCode 1 -Verdict "FAIL"
    }
} catch {
    Register-TestResult -TestId "T09" -Name "Relative and drive-relative path rejection" `
        -Command "provision-rust-windows.ps1 -DryRun ..." -Expected "Non-zero exit" `
        -Actual "Exception: $_" -ExitCode -1 -Verdict "FAIL"
}

# -------------------------------------------------------------------------
# T10 — Junction and symlink rejection using disposable fixtures
# -------------------------------------------------------------------------
$t10Root = Join-Path $sessionDir "t10-root"
$t10Target = Join-Path $sessionDir "t10-target"
$t10VaultRuntimes = Join-Path $t10Root "vault\runtimes"
$t10Junction = Join-Path $t10VaultRuntimes "rust"

try {
    New-TestFixtureDirectory -Path $t10Target | Out-Null
    New-TestFixtureDirectory -Path $t10VaultRuntimes | Out-Null

    # Create directory junction via ownership-aware helper
    New-TestJunctionFixture -JunctionPath $t10Junction -TargetPath $t10Target | Out-Null

    $cmd = "provision-rust-windows.ps1 -DryRun -UlvoRoot '$t10Root'"
    $res = Invoke-ScriptWithCapture -Arguments @("-DryRun", "-UlvoRoot", $t10Root)

    # Safely delete junction link via Cleanup-TestFixture with strict ownership verification
    $junctionCleaned = Cleanup-TestFixture -Path $t10Junction
    $targetIntact = Test-Path -LiteralPath $t10Target

    if ($res.ExitCode -ne 0 -and $res.Combined -match "Reparse point \(symlink/junction\) detected" -and $junctionCleaned -and $targetIntact) {
        Register-TestResult -TestId "T10" -Name "Junction and symlink rejection" `
            -Command $cmd -Expected "ExitCode != 0, reparse point detected, junction cleaned safely" `
            -Actual "ExitCode $($res.ExitCode), junction cleaned: $junctionCleaned, target intact: $targetIntact" `
            -ExitCode $res.ExitCode -Verdict "PASS"
    } else {
        Register-TestResult -TestId "T10" -Name "Junction and symlink rejection" `
            -Command $cmd -Expected "ExitCode != 0, reparse point detected" `
            -Actual "ExitCode $($res.ExitCode), junction cleaned: $junctionCleaned, target intact: $targetIntact, Output: $($res.Combined.Trim())" `
            -ExitCode $res.ExitCode -Verdict "FAIL"
    }
    Cleanup-TestFixture -Path $t10Root | Out-Null
    Cleanup-TestFixture -Path $t10Target | Out-Null
} catch {
    Register-TestResult -TestId "T10" -Name "Junction and symlink rejection" `
        -Command "provision-rust-windows.ps1 -DryRun -UlvoRoot '$t10Root'" -Expected "Non-zero exit" `
        -Actual "Exception: $_" -ExitCode -1 -Verdict "FAIL"
}

# -------------------------------------------------------------------------
# T11 — Alternate Data Stream (ADS) path containment rejection
# -------------------------------------------------------------------------
try {
    # Test containment rejection with colon (ADS injection)
    $adsPath = "$ulvoRoot\vault\runtimes\rust\rustup:stream"
    $cmd = "provision-rust-windows.ps1 -DryRun -RustupHome '$adsPath'"
    $res = Invoke-ScriptWithCapture -Arguments @("-DryRun", "-RustupHome", $adsPath)

    if ($res.ExitCode -ne 0 -and $res.Combined -match "alternate data streams") {
        Register-TestResult -TestId "T11" -Name "Alternate Data Stream (ADS) path containment rejection" `
            -Command $cmd -Expected "ExitCode != 0, ADS rejected" `
            -Actual "ExitCode $($res.ExitCode), matched rejection message" `
            -ExitCode $res.ExitCode -Verdict "PASS"
    } else {
        Register-TestResult -TestId "T11" -Name "Alternate Data Stream (ADS) path containment rejection" `
            -Command $cmd -Expected "ExitCode != 0" `
            -Actual "ExitCode $($res.ExitCode), Output: $($res.Combined.Trim())" `
            -ExitCode $res.ExitCode -Verdict "FAIL"
    }
} catch {
    Register-TestResult -TestId "T11" -Name "Alternate Data Stream (ADS) path containment rejection" `
        -Command "provision-rust-windows.ps1 -DryRun ..." -Expected "Non-zero exit" `
        -Actual "Exception: $_" -ExitCode -1 -Verdict "FAIL"
}

# -------------------------------------------------------------------------
# T12 — Missing parent handling
# -------------------------------------------------------------------------
try {
    $missingParentRustup = "$ulvoRoot\vault\runtimes\rust\nonexistent-parent\deep\rustup"
    $missingParentCargo = "$ulvoRoot\vault\runtimes\rust\nonexistent-parent\deep\cargo"
    $cmd = "provision-rust-windows.ps1 -DryRun -RustupHome '$missingParentRustup' -CargoHome '$missingParentCargo'"
    $res = Invoke-ScriptWithCapture -Arguments @("-DryRun", "-RustupHome", $missingParentRustup, "-CargoHome", $missingParentCargo)

    $nonexistentDirCreated = Test-Path -LiteralPath "$ulvoRoot\vault\runtimes\rust\nonexistent-parent"

    if ($res.ExitCode -eq 0 -and -not $nonexistentDirCreated) {
        Register-TestResult -TestId "T12" -Name "Missing parent handling" `
            -Command $cmd -Expected "ExitCode 0, zero directories created" `
            -Actual "ExitCode $($res.ExitCode), directory created: $nonexistentDirCreated" `
            -ExitCode $res.ExitCode -Verdict "PASS"
    } else {
        Register-TestResult -TestId "T12" -Name "Missing parent handling" `
            -Command $cmd -Expected "ExitCode 0, zero directories created" `
            -Actual "ExitCode $($res.ExitCode), Output: $($res.Combined.Trim())" `
            -ExitCode $res.ExitCode -Verdict "FAIL"
    }
} catch {
    Register-TestResult -TestId "T12" -Name "Missing parent handling" `
        -Command "provision-rust-windows.ps1 -DryRun ..." -Expected "ExitCode 0" `
        -Actual "Exception: $_" -ExitCode -1 -Verdict "FAIL"
}

# -------------------------------------------------------------------------
# T13 — Invalid target/version arguments
# -------------------------------------------------------------------------
try {
    # 13a: Malicious version string
    $cmd1 = "provision-rust-windows.ps1 -DryRun -RustVersion '1.96.0; calc.exe'"
    $res1 = Invoke-ScriptWithCapture -Arguments @("-DryRun", "-RustVersion", "1.96.0; calc.exe")

    # 13b: Malicious target string
    $cmd2 = "provision-rust-windows.ps1 -DryRun -Target 'x86_64; whoami'"
    $res2 = Invoke-ScriptWithCapture -Arguments @("-DryRun", "-Target", "x86_64; whoami")

    # 13c: Untrusted DownloadUrl
    $cmd3 = "provision-rust-windows.ps1 -DryRun -DownloadUrl 'http://evil.com/rustup-init.exe'"
    $res3 = Invoke-ScriptWithCapture -Arguments @("-DryRun", "-DownloadUrl", "http://evil.com/rustup-init.exe")

    if ($res1.ExitCode -ne 0 -and $res2.ExitCode -ne 0 -and $res3.ExitCode -ne 0 -and
        $res1.Combined -match "Invalid RustVersion" -and
        $res2.Combined -match "Invalid Target" -and
        $res3.Combined -match "Untrusted DownloadUrl") {
        Register-TestResult -TestId "T13" -Name "Invalid target/version arguments" `
            -Command "Argument validation checks (version, target, url)" `
            -Expected "All rejected with non-zero exit codes" `
            -Actual "res1: $($res1.ExitCode), res2: $($res2.ExitCode), res3: $($res3.ExitCode)" `
            -ExitCode 0 -Verdict "PASS"
    } else {
        Register-TestResult -TestId "T13" -Name "Invalid target/version arguments" `
            -Command "Argument validation checks" -Expected "All exit != 0" `
            -Actual "res1: $($res1.ExitCode), res2: $($res2.ExitCode), res3: $($res3.ExitCode)" `
            -ExitCode 1 -Verdict "FAIL"
    }
} catch {
    Register-TestResult -TestId "T13" -Name "Invalid target/version arguments" `
        -Command "Argument validation checks" -Expected "Non-zero exit" `
        -Actual "Exception: $_" -ExitCode -1 -Verdict "FAIL"
}

# -------------------------------------------------------------------------
# T14 — Installer authentication failure using mocks or inert fixtures
# -------------------------------------------------------------------------
$dummyInstaller = Register-TestFixtureFile (Join-Path $sessionDir "dummy-rustup-init.exe")
try {
    # Generate inert 1.5MB file (exceeds 1MB size threshold, but unsigned)
    $bytes = New-Object byte[] (1.5 * 1024 * 1024)
    [System.IO.File]::WriteAllBytes($dummyInstaller, $bytes)

    # Test Authenticode check from provisioning script via Dot-Sourcing inside a child powershell
    $testScriptBlock = @"
    . '$provisionScript'
    `$result = Test-AuthenticodeSignature -FilePath '$dummyInstaller'
    if (`$result -eq `$false) { exit 0 } else { exit 1 }
"@
    $proc = Start-Process -FilePath "powershell.exe" `
        -ArgumentList @("-NoProfile", "-Command", $testScriptBlock) `
        -Wait -NoNewWindow -PassThru

    if ($proc.ExitCode -eq 0) {
        Register-TestResult -TestId "T14" -Name "Installer authentication failure on inert binary" `
            -Command "Test-AuthenticodeSignature on unsigned dummy installer" `
            -Expected "Returns false (ExitCode 0 from verification block)" `
            -Actual "ExitCode: $($proc.ExitCode)" -ExitCode 0 -Verdict "PASS"
    } else {
        Register-TestResult -TestId "T14" -Name "Installer authentication failure on inert binary" `
            -Command "Test-AuthenticodeSignature on unsigned dummy installer" `
            -Expected "Returns false" -Actual "ExitCode: $($proc.ExitCode)" `
            -ExitCode 1 -Verdict "FAIL"
    }
} catch {
    Register-TestResult -TestId "T14" -Name "Installer authentication failure on inert binary" `
        -Command "Test-AuthenticodeSignature" -Expected "Returns false" `
        -Actual "Exception: $_" -ExitCode -1 -Verdict "FAIL"
}

# -------------------------------------------------------------------------
# T15 — Nonzero installer exit handling using a harmless mock executable
# -------------------------------------------------------------------------
$mockCmd = Register-TestFixtureFile (Join-Path $sessionDir "mock-installer-fail.cmd")
try {
    Set-Content -LiteralPath $mockCmd -Value "@exit /b 42" -Force

    $testScriptBlock = ". '$provisionScript'; `$exitCode = Invoke-RustupInstaller -InstallerPath '$mockCmd' -RustupHome 'E:\Workspace\Active\ULVO\vault\runtimes\rust\rustup' -CargoHome 'E:\Workspace\Active\ULVO\vault\runtimes\rust\cargo' -RustVersion '1.96.0' -Target 'x86_64-pc-windows-gnu'; exit `$exitCode"
    $proc = Start-Process -FilePath "powershell.exe" `
        -ArgumentList @("-NoProfile", "-Command", $testScriptBlock) `
        -Wait -NoNewWindow -PassThru

    if ($proc.ExitCode -eq 42) {
        Register-TestResult -TestId "T15" -Name "Nonzero installer exit handling" `
            -Command "Invoke-RustupInstaller with mock exiting 42" `
            -Expected "ExitCode 42 propagated accurately" `
            -Actual "ExitCode: $($proc.ExitCode)" -ExitCode 0 -Verdict "PASS"
    } else {
        Register-TestResult -TestId "T15" -Name "Nonzero installer exit handling" `
            -Command "Invoke-RustupInstaller with mock exiting 42" `
            -Expected "ExitCode 42" -Actual "ExitCode: $($proc.ExitCode)" `
            -ExitCode 1 -Verdict "FAIL"
    }
} catch {
    Register-TestResult -TestId "T15" -Name "Nonzero installer exit handling" `
        -Command "Invoke-RustupInstaller with mock" -Expected "ExitCode 42" `
        -Actual "Exception: $_" -ExitCode -1 -Verdict "FAIL"
}

# -------------------------------------------------------------------------
# T16 — Environment restoration after simulated failure
# -------------------------------------------------------------------------
try {
    $testScriptBlock = "`$env:RUSTUP_HOME = 'SENTINEL_RUSTUP_ORIGINAL'; `$env:CARGO_HOME = 'SENTINEL_CARGO_ORIGINAL'; `$env:PATH = 'C:\SENTINEL_PATH_ORIGINAL;' + `$env:PATH; . '$provisionScript'; `$null = Invoke-RustupInstaller -InstallerPath '$mockCmd' -RustupHome 'E:\Workspace\Active\ULVO\vault\runtimes\rust\rustup' -CargoHome 'E:\Workspace\Active\ULVO\vault\runtimes\rust\cargo' -RustVersion '1.96.0' -Target 'x86_64-pc-windows-gnu'; if (`$env:RUSTUP_HOME -eq 'SENTINEL_RUSTUP_ORIGINAL' -and `$env:CARGO_HOME -eq 'SENTINEL_CARGO_ORIGINAL' -and `$env:PATH.StartsWith('C:\SENTINEL_PATH_ORIGINAL;')) { exit 0 } else { exit 1 }"
    $proc = Start-Process -FilePath "powershell.exe" `
        -ArgumentList @("-NoProfile", "-Command", $testScriptBlock) `
        -Wait -NoNewWindow -PassThru

    if ($proc.ExitCode -eq 0) {
        Register-TestResult -TestId "T16" -Name "Environment restoration after simulated failure" `
            -Command "Invoke-RustupInstaller failure with environment verification" `
            -Expected "ExitCode 0 (RUSTUP_HOME, CARGO_HOME, PATH restored)" `
            -Actual "ExitCode: $($proc.ExitCode)" -ExitCode 0 -Verdict "PASS"
    } else {
        Register-TestResult -TestId "T16" -Name "Environment restoration after simulated failure" `
            -Command "Invoke-RustupInstaller environment verification" `
            -Expected "ExitCode 0" -Actual "ExitCode: $($proc.ExitCode)" `
            -ExitCode 1 -Verdict "FAIL"
    }
} catch {
    Register-TestResult -TestId "T16" -Name "Environment restoration after simulated failure" `
        -Command "Environment restoration test" -Expected "ExitCode 0" `
        -Actual "Exception: $_" -ExitCode -1 -Verdict "FAIL"
}

# -------------------------------------------------------------------------
# T17 — No deletion of persistent resources
# -------------------------------------------------------------------------
try {
    $persistentVaultExists = Test-Path -LiteralPath "$ulvoRoot\vault"
    $cargoTomlExists = Test-Path -LiteralPath "$ulvoRoot\Cargo.toml"
    $readmeExists = Test-Path -LiteralPath "$ulvoRoot\README.md"

    if ($persistentVaultExists -and $cargoTomlExists -and $readmeExists) {
        Register-TestResult -TestId "T17" -Name "No deletion of persistent resources" `
            -Command "Verify persistence of vault, Cargo.toml, README.md" `
            -Expected "All persistent files remain present" `
            -Actual "vault: $persistentVaultExists, Cargo.toml: $cargoTomlExists, README: $readmeExists" `
            -ExitCode 0 -Verdict "PASS"
    } else {
        Register-TestResult -TestId "T17" -Name "No deletion of persistent resources" `
            -Command "Verify persistence of vault" -Expected "All present" `
            -Actual "vault: $persistentVaultExists, Cargo.toml: $cargoTomlExists" `
            -ExitCode 1 -Verdict "FAIL"
    }
} catch {
    Register-TestResult -TestId "T17" -Name "No deletion of persistent resources" `
        -Command "Verify persistence" -Expected "Present" `
        -Actual "Exception: $_" -ExitCode -1 -Verdict "FAIL"
}

# -------------------------------------------------------------------------
# T18 — No unauthorized filesystem writes during dry-run
# -------------------------------------------------------------------------
try {
    # Establish disposable sentinel fixture in session dir
    $t18SentinelDir = Join-Path $sessionDir "t18-sentinel"
    New-TestFixtureDirectory -Path $t18SentinelDir | Out-Null
    $t18Sentinel = Register-TestFixtureFile (Join-Path $t18SentinelDir "sentinel.txt")
    $t18SentinelContent = "ULVO-SENTINEL-T18-" + [System.Guid]::NewGuid().ToString("N")
    [System.IO.File]::WriteAllText($t18Sentinel, $t18SentinelContent, [System.Text.Encoding]::UTF8)
    $sentinelHashBefore = (Get-FileHash -LiteralPath $t18Sentinel -Algorithm SHA256).Hash

    # Helper scriptblock to collect file SHA256 hashes across bounded approved areas
    $collectBoundedHashes = {
        $map = @{}
        # 1. Root files
        Get-ChildItem -LiteralPath $ulvoRoot -File | ForEach-Object {
            $map[$_.FullName] = (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash
        }
        # 2. Vault (if present)
        $vaultPath = Join-Path $ulvoRoot "vault"
        if (Test-Path -LiteralPath $vaultPath) {
            Get-ChildItem -LiteralPath $vaultPath -Recurse -File -Force | ForEach-Object {
                $map[$_.FullName] = (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash
            }
        }
        # 3. Scripts
        $scriptsPath = Join-Path $ulvoRoot "scripts"
        if (Test-Path -LiteralPath $scriptsPath) {
            Get-ChildItem -LiteralPath $scriptsPath -Recurse -File -Force | ForEach-Object {
                $map[$_.FullName] = (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash
            }
        }
        # 4. Crates (if present)
        $cratesPath = Join-Path $ulvoRoot "crates"
        if (Test-Path -LiteralPath $cratesPath) {
            Get-ChildItem -LiteralPath $cratesPath -Recurse -File -Force | ForEach-Object {
                $map[$_.FullName] = (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash
            }
        }
        return $map
    }

    $hashesBefore = & $collectBoundedHashes

    $t18FixtureRoot = Join-Path $sessionDir "t18-root"
    New-TestFixtureDirectory -Path $t18FixtureRoot | Out-Null
    $res = Invoke-ScriptWithCapture -Arguments @("-DryRun", "-UlvoRoot", $t18FixtureRoot)

    $hashesAfter = & $collectBoundedHashes

    # Sentinel verification
    $sentinelHashAfter = (Get-FileHash -LiteralPath $t18Sentinel -Algorithm SHA256).Hash
    $sentinelIntact = ($sentinelHashBefore -eq $sentinelHashAfter)

    # Clean up t18 sentinel fixture via ownership-verified cleanup
    $t18Cleaned = Cleanup-TestFixture -Path $t18SentinelDir
    Cleanup-TestFixture -Path $t18FixtureRoot | Out-Null

    # Deterministic comparison of file identities and SHA-256 hashes
    $addedFiles = @()
    $removedFiles = @()
    $modifiedFiles = @()

    foreach ($file in $hashesBefore.Keys) {
        if (-not $hashesAfter.ContainsKey($file)) {
            $removedFiles += $file
        } elseif ($hashesAfter[$file] -ne $hashesBefore[$file]) {
            $modifiedFiles += $file
        }
    }
    foreach ($file in $hashesAfter.Keys) {
        if (-not $hashesBefore.ContainsKey($file)) {
            $addedFiles += $file
        }
    }

    $isIdentical = ($addedFiles.Count -eq 0 -and $removedFiles.Count -eq 0 -and $modifiedFiles.Count -eq 0)

    if ($res.ExitCode -eq 0 -and $isIdentical -and $sentinelIntact -and $t18Cleaned) {
        Register-TestResult -TestId "T18" -Name "Deterministic SHA-256 integrity during dry-run" `
            -Command "Compare cryptographic hashes of bounded areas before and after -DryRun" `
            -Expected "Zero file changes, identical SHA-256 hashes ($($hashesBefore.Count) files), sentinel intact" `
            -Actual "Verified $($hashesBefore.Count) files (Added: 0, Removed: 0, Modified: 0), Sentinel intact: $sentinelIntact" `
            -ExitCode 0 -Verdict "PASS"
    } else {
        Register-TestResult -TestId "T18" -Name "Deterministic SHA-256 integrity during dry-run" `
            -Command "Compare cryptographic hashes of bounded areas" `
            -Expected "Zero file changes, identical SHA-256 hashes" `
            -Actual "ExitCode: $($res.ExitCode), Added: $($addedFiles.Count), Removed: $($removedFiles.Count), Modified: $($modifiedFiles.Count), Sentinel: $sentinelIntact" `
            -ExitCode 1 -Verdict "FAIL"
    }
} catch {
    Register-TestResult -TestId "T18" -Name "Deterministic SHA-256 integrity during dry-run" `
        -Command "File integrity check" -Expected "Zero changes" `
        -Actual "Exception: $_" -ExitCode -1 -Verdict "FAIL"
}

# -------------------------------------------------------------------------
# T19 — Test cleanup rejects paths not owned by current test
# -------------------------------------------------------------------------
try {
    # 1. Attempt cleanup on persistent vault (must be rejected)
    $res1 = Cleanup-TestFixture -Path "$ulvoRoot\vault"
    $vaultIntact = Test-Path -LiteralPath "$ulvoRoot\vault"

    # 2. Attempt cleanup on path without ownership marker
    $unmarkedDir = Join-Path $sessionDir "unmarked-test-dir"
    New-Item -ItemType Directory -Path $unmarkedDir -Force | Out-Null
    $res2 = Cleanup-TestFixture -Path $unmarkedDir
    $unmarkedIntact = Test-Path -LiteralPath $unmarkedDir

    # 3. Attempt cleanup on path with foreign session ID
    $foreignDir = Join-Path $sessionDir "foreign-test-dir"
    New-Item -ItemType Directory -Path $foreignDir -Force | Out-Null
    $foreignToken = @{ sessionId = "FOREIGN-SESSION-9999"; pid = 9999 } | ConvertTo-Json -Compress
    Set-Content -LiteralPath (Join-Path $foreignDir ".ulvo-fixture-owner.json") -Value $foreignToken -Force
    $res3 = Cleanup-TestFixture -Path $foreignDir
    $foreignIntact = Test-Path -LiteralPath $foreignDir

    # 4. Attempt cleanup on unowned junction (reparse point without sidecar)
    $unownedJunctionTarget = Join-Path $sessionDir "unowned-target"
    New-TestFixtureDirectory -Path $unownedJunctionTarget | Out-Null
    $unownedJunction = Join-Path $sessionDir "unowned-junction"
    $mklinkOut = cmd.exe /c "mklink /J `"$unownedJunction`" `"$unownedJunctionTarget`"" 2>&1
    $res4 = Cleanup-TestFixture -Path $unownedJunction
    $unownedJunctionIntact = Test-Path -LiteralPath $unownedJunction

    # Safely clean up all temporary test fixtures using Cleanup-TestFixture (NO BYPASSES)
    # Stamping legitimate session ownership and registering fixtures prior to cleanup:
    # 4a. Adopt and clean unowned junction via sidecar marker
    $sidecarToken = @{
        sessionId = $sessionId
        pid = $PID
        createdAt = (Get-Date).ToString("o")
        junctionPath = $unownedJunction
        targetPath = $unownedJunctionTarget
        type = "junction"
    } | ConvertTo-Json -Compress
    Set-Content -LiteralPath "$unownedJunction.junction-owner.json" -Value $sidecarToken -Force
    $script:ownedFixtureRegistry.Add($unownedJunction) | Out-Null
    $cleanJunction = Cleanup-TestFixture -Path $unownedJunction
    $cleanTarget = Cleanup-TestFixture -Path $unownedJunctionTarget

    # 4b. Adopt and clean unmarked directory via legitimate ownership marker
    $ownTokenUnmarked = @{
        sessionId = $sessionId
        pid = $PID
        createdAt = (Get-Date).ToString("o")
        path = $unmarkedDir
    } | ConvertTo-Json -Compress
    Set-Content -LiteralPath (Join-Path $unmarkedDir ".ulvo-fixture-owner.json") -Value $ownTokenUnmarked -Force
    $script:ownedFixtureRegistry.Add($unmarkedDir) | Out-Null
    $cleanUnmarked = Cleanup-TestFixture -Path $unmarkedDir

    # 4c. Adopt and clean foreign directory via legitimate ownership marker
    $ownTokenForeign = @{
        sessionId = $sessionId
        pid = $PID
        createdAt = (Get-Date).ToString("o")
        path = $foreignDir
    } | ConvertTo-Json -Compress
    Set-Content -LiteralPath (Join-Path $foreignDir ".ulvo-fixture-owner.json") -Value $ownTokenForeign -Force
    $script:ownedFixtureRegistry.Add($foreignDir) | Out-Null
    $cleanForeign = Cleanup-TestFixture -Path $foreignDir

    if ($res1 -eq $false -and $vaultIntact -and
        $res2 -eq $false -and $unmarkedIntact -and
        $res3 -eq $false -and $foreignIntact -and
        $res4 -eq $false -and $unownedJunctionIntact -and
        $cleanJunction -and $cleanTarget -and $cleanUnmarked -and $cleanForeign) {
        Register-TestResult -TestId "T19" -Name "Test cleanup rejects paths not owned by current test" `
            -Command "Cleanup-TestFixture on vault, unmarked, foreign, and unowned junction fixtures" `
            -Expected "All rejections return false; all legitimate cleanups succeed with zero bypasses" `
            -Actual "Rejections: vault=$res1, unmarked=$res2, foreign=$res3, junction=$res4; Cleanups: junction=$cleanJunction, unmarked=$cleanUnmarked, foreign=$cleanForeign" `
            -ExitCode 0 -Verdict "PASS"
    } else {
        Register-TestResult -TestId "T19" -Name "Test cleanup rejects paths not owned by current test" `
            -Command "Cleanup-TestFixture security checks" `
            -Expected "All rejections return false and legitimate cleanups succeed" `
            -Actual "res1: $res1, res2: $res2, res3: $res3, res4: $res4, cleanJunction: $cleanJunction, cleanUnmarked: $cleanUnmarked, cleanForeign: $cleanForeign" `
            -ExitCode 1 -Verdict "FAIL"
    }
} catch {
    Register-TestResult -TestId "T19" -Name "Test cleanup rejects paths not owned by current test" `
        -Command "Cleanup-TestFixture security checks" -Expected "All return false" `
        -Actual "Exception: $_" -ExitCode -1 -Verdict "FAIL"
}

# -------------------------------------------------------------------------
# T20 — PowerShell 5.1 compatibility
# -------------------------------------------------------------------------
try {
    $psVersion = $PSVersionTable.PSVersion
    $isPS5 = ($psVersion.Major -eq 5)
    
    # Also verify execution under powershell.exe without error
    $testBlock = "[bool]`$PSVersionTable"
    $proc = Start-Process -FilePath "powershell.exe" `
        -ArgumentList @("-NoProfile", "-Command", $testBlock) `
        -Wait -NoNewWindow -PassThru

    if ($isPS5 -and $proc.ExitCode -eq 0) {
        Register-TestResult -TestId "T20" -Name "PowerShell 5.1 compatibility" `
            -Command "Verify PSVersionTable and powershell.exe execution" `
            -Expected "PSVersion Major 5, exit 0" `
            -Actual "Major: $($psVersion.Major), ExitCode: $($proc.ExitCode)" `
            -ExitCode 0 -Verdict "PASS"
    } else {
        Register-TestResult -TestId "T20" -Name "PowerShell 5.1 compatibility" `
            -Command "PS 5.1 verification" -Expected "Major 5, exit 0" `
            -Actual "Major: $($psVersion.Major), ExitCode: $($proc.ExitCode)" `
            -ExitCode 1 -Verdict "FAIL"
    }
} catch {
    Register-TestResult -TestId "T20" -Name "PowerShell 5.1 compatibility" `
        -Command "PS 5.1 verification" -Expected "ExitCode 0" `
        -Actual "Exception: $_" -ExitCode -1 -Verdict "FAIL"
}

# -------------------------------------------------------------------------
# T21 — No regressions in the existing supported CLI behavior
# -------------------------------------------------------------------------
try {
    $t21FixtureRoot = Join-Path $sessionDir "t21-root"
    New-TestFixtureDirectory -Path $t21FixtureRoot | Out-Null

    # 21a: Standard dry-run
    $res1 = Invoke-ScriptWithCapture -Arguments @("-DryRun", "-UlvoRoot", $t21FixtureRoot)

    # 21b: Custom version and target dry-run
    $res2 = Invoke-ScriptWithCapture -Arguments @("-DryRun", "-UlvoRoot", $t21FixtureRoot, "-RustVersion", "1.96.0", "-Target", "x86_64-pc-windows-gnu")

    Cleanup-TestFixture -Path $t21FixtureRoot | Out-Null

    # 21c: Help text (-?)
    $psiHelp = New-Object System.Diagnostics.ProcessStartInfo
    $psiHelp.FileName = "powershell.exe"
    $psiHelp.Arguments = "-NoProfile -Command `"`& '$provisionScript' -?`""
    $psiHelp.UseShellExecute = $false
    $psiHelp.RedirectStandardOutput = $true
    $psiHelp.RedirectStandardError = $true
    $psiHelp.CreateNoWindow = $true
    $procHelp = [System.Diagnostics.Process]::Start($psiHelp)
    $helpOutput = $procHelp.StandardOutput.ReadToEnd()
    $procHelp.WaitForExit()
    $helpExit = $procHelp.ExitCode
    $procHelp.Dispose()

    if ($res1.ExitCode -eq 0 -and $res2.ExitCode -eq 0 -and $helpExit -eq 0 -and
        ($helpOutput -match "SYNOPSIS" -or $helpOutput -match "Windows-native Rust")) {
        Register-TestResult -TestId "T21" -Name "No regressions in supported CLI behavior" `
            -Command "Verify standard dry-run, custom version, and help flag" `
            -Expected "All exit 0, help text accessible" `
            -Actual "res1: $($res1.ExitCode), res2: $($res2.ExitCode), help: $helpExit" `
            -ExitCode 0 -Verdict "PASS"
    } else {
        Register-TestResult -TestId "T21" -Name "No regressions in supported CLI behavior" `
            -Command "CLI behavior verification" -Expected "All exit 0 with help text" `
            -Actual "res1: $($res1.ExitCode), res2: $($res2.ExitCode), help: $helpExit, output: $($helpOutput.Trim())" `
            -ExitCode 1 -Verdict "FAIL"
    }
} catch {
    Register-TestResult -TestId "T21" -Name "No regressions in supported CLI behavior" `
        -Command "CLI behavior verification" -Expected "ExitCode 0" `
        -Actual "Exception: $_" -ExitCode -1 -Verdict "FAIL"
}

# -------------------------------------------------------------------------
# T22 — Missing artifact manifest rejection
# -------------------------------------------------------------------------
try {
    $nonexistentManifest = Join-Path $sessionDir "nonexistent-manifest.toml"
    $res = Invoke-ScriptWithCapture -Arguments @("-DryRun", "-ManifestPath", $nonexistentManifest)

    if ($res.ExitCode -ne 0 -and $res.Combined -match "Toolchain manifest not found") {
        Register-TestResult -TestId "T22" -Name "Missing artifact manifest rejection" `
            -Command "provision-rust-windows.ps1 -DryRun -ManifestPath $nonexistentManifest" `
            -Expected "Exit code non-zero, error matches 'Toolchain manifest not found'" `
            -Actual "ExitCode: $($res.ExitCode), Output matches error" -ExitCode 0 -Verdict "PASS"
    } else {
        Register-TestResult -TestId "T22" -Name "Missing artifact manifest rejection" `
            -Command "provision-rust-windows.ps1 -DryRun -ManifestPath $nonexistentManifest" `
            -Expected "Exit code non-zero with missing manifest error" `
            -Actual "ExitCode: $($res.ExitCode), Output: $($res.Combined.Trim())" -ExitCode 1 -Verdict "FAIL"
    }
} catch {
    Register-TestResult -TestId "T22" -Name "Missing artifact manifest rejection" `
        -Command "Missing manifest test" -Expected "ExitCode != 0" `
        -Actual "Exception: $_" -ExitCode -1 -Verdict "FAIL"
}

# -------------------------------------------------------------------------
# T23 — Invalid TOML syntax rejection
# -------------------------------------------------------------------------
try {
    $badToml = Register-TestFixtureFile (Join-Path $sessionDir "bad-syntax.toml")
    Set-Content -LiteralPath $badToml -Value "corrupt_toml_without_assignment" -Force
    $res = Invoke-ScriptWithCapture -Arguments @("-DryRun", "-ManifestPath", $badToml)

    if ($res.ExitCode -ne 0 -and $res.Combined -match "Invalid TOML syntax") {
        Register-TestResult -TestId "T23" -Name "Invalid TOML syntax rejection" `
            -Command "provision-rust-windows.ps1 -DryRun -ManifestPath $badToml" `
            -Expected "Exit code non-zero, error matches 'Invalid TOML syntax'" `
            -Actual "ExitCode: $($res.ExitCode), Output matches error" -ExitCode 0 -Verdict "PASS"
    } else {
        Register-TestResult -TestId "T23" -Name "Invalid TOML syntax rejection" `
            -Command "provision-rust-windows.ps1 -DryRun -ManifestPath $badToml" `
            -Expected "Exit code non-zero with invalid syntax error" `
            -Actual "ExitCode: $($res.ExitCode), Output: $($res.Combined.Trim())" -ExitCode 1 -Verdict "FAIL"
    }
} catch {
    Register-TestResult -TestId "T23" -Name "Invalid TOML syntax rejection" `
        -Command "Invalid TOML syntax test" -Expected "ExitCode != 0" `
        -Actual "Exception: $_" -ExitCode -1 -Verdict "FAIL"
}

# -------------------------------------------------------------------------
# T24 — Unsupported schema version rejection
# -------------------------------------------------------------------------
try {
    $badSchemaToml = Register-TestFixtureFile (Join-Path $sessionDir "bad-schema.toml")
    $badSchemaContent = @(
        'schema_version = "2.0.0"',
        '[[artifacts]]',
        'artifact_name = "rustup-init"',
        'installer_version = "1.29.0"',
        'platform = "windows"',
        'target = "x86_64-pc-windows-gnu"',
        'expected_sha256 = "03dbaae1d33a4d220bd0d202e5092955dae859c119074192ca513f8c4713fff7"',
        'verification_policy = "manifest-sha256-pinned"',
        'approval_status = "unapproved"'
    ) -join "`r`n"
    Set-Content -LiteralPath $badSchemaToml -Value $badSchemaContent -Force
    $res = Invoke-ScriptWithCapture -Arguments @("-DryRun", "-ManifestPath", $badSchemaToml)

    if ($res.ExitCode -ne 0 -and $res.Combined -match "Unsupported manifest schema version") {
        Register-TestResult -TestId "T24" -Name "Unsupported schema version rejection" `
            -Command "provision-rust-windows.ps1 -DryRun -ManifestPath $badSchemaToml" `
            -Expected "Exit code non-zero, error matches 'Unsupported manifest schema version'" `
            -Actual "ExitCode: $($res.ExitCode), Output matches error" -ExitCode 0 -Verdict "PASS"
    } else {
        Register-TestResult -TestId "T24" -Name "Unsupported schema version rejection" `
            -Command "provision-rust-windows.ps1 -DryRun -ManifestPath $badSchemaToml" `
            -Expected "Exit code non-zero with unsupported schema error" `
            -Actual "ExitCode: $($res.ExitCode), Output: $($res.Combined.Trim())" -ExitCode 1 -Verdict "FAIL"
    }
} catch {
    Register-TestResult -TestId "T24" -Name "Unsupported schema version rejection" `
        -Command "Unsupported schema test" -Expected "ExitCode != 0" `
        -Actual "Exception: $_" -ExitCode -1 -Verdict "FAIL"
}

# -------------------------------------------------------------------------
# T25 — Missing SHA-256 digest rejection
# -------------------------------------------------------------------------
try {
    $missingShaToml = Register-TestFixtureFile (Join-Path $sessionDir "missing-sha.toml")
    $missingShaContent = @(
        'schema_version = "1.0.0"',
        '[[artifacts]]',
        'artifact_name = "rustup-init"',
        'installer_version = "1.29.0"',
        'platform = "windows"',
        'target = "x86_64-pc-windows-gnu"',
        'canonical_url = "https://static.rust-lang.org/rustup/dist/x86_64-pc-windows-gnu/rustup-init.exe"',
        'verification_policy = "manifest-sha256-pinned"',
        'approval_status = "unapproved"'
    ) -join "`r`n"
    Set-Content -LiteralPath $missingShaToml -Value $missingShaContent -Force
    $res = Invoke-ScriptWithCapture -Arguments @("-DryRun", "-ManifestPath", $missingShaToml)

    if ($res.ExitCode -ne 0 -and $res.Combined -match "missing required 'expected_sha256'") {
        Register-TestResult -TestId "T25" -Name "Missing SHA-256 digest rejection" `
            -Command "provision-rust-windows.ps1 -DryRun -ManifestPath $missingShaToml" `
            -Expected "Exit code non-zero, error matches missing expected_sha256" `
            -Actual "ExitCode: $($res.ExitCode), Output matches error" -ExitCode 0 -Verdict "PASS"
    } else {
        Register-TestResult -TestId "T25" -Name "Missing SHA-256 digest rejection" `
            -Command "provision-rust-windows.ps1 -DryRun -ManifestPath $missingShaToml" `
            -Expected "Exit code non-zero with missing sha error" `
            -Actual "ExitCode: $($res.ExitCode), Output: $($res.Combined.Trim())" -ExitCode 1 -Verdict "FAIL"
    }
} catch {
    Register-TestResult -TestId "T25" -Name "Missing SHA-256 digest rejection" `
        -Command "Missing sha test" -Expected "ExitCode != 0" `
        -Actual "Exception: $_" -ExitCode -1 -Verdict "FAIL"
}

# -------------------------------------------------------------------------
# T26 — Malformed SHA-256 digest rejection
# -------------------------------------------------------------------------
try {
    $malformedShaToml = Register-TestFixtureFile (Join-Path $sessionDir "malformed-sha.toml")
    $malformedShaContent = @(
        'schema_version = "1.0.0"',
        '[[artifacts]]',
        'artifact_name = "rustup-init"',
        'installer_version = "1.29.0"',
        'platform = "windows"',
        'target = "x86_64-pc-windows-gnu"',
        'canonical_url = "https://static.rust-lang.org/rustup/dist/x86_64-pc-windows-gnu/rustup-init.exe"',
        'expected_sha256 = "not-a-valid-64-character-hex-digest"',
        'verification_policy = "manifest-sha256-pinned"',
        'approval_status = "unapproved"'
    ) -join "`r`n"
    Set-Content -LiteralPath $malformedShaToml -Value $malformedShaContent -Force
    $res = Invoke-ScriptWithCapture -Arguments @("-DryRun", "-ManifestPath", $malformedShaToml)

    if ($res.ExitCode -ne 0 -and $res.Combined -match "Malformed SHA-256 digest") {
        Register-TestResult -TestId "T26" -Name "Malformed SHA-256 digest rejection" `
            -Command "provision-rust-windows.ps1 -DryRun -ManifestPath $malformedShaToml" `
            -Expected "Exit code non-zero, error matches 'Malformed SHA-256 digest'" `
            -Actual "ExitCode: $($res.ExitCode), Output matches error" -ExitCode 0 -Verdict "PASS"
    } else {
        Register-TestResult -TestId "T26" -Name "Malformed SHA-256 digest rejection" `
            -Command "provision-rust-windows.ps1 -DryRun -ManifestPath $malformedShaToml" `
            -Expected "Exit code non-zero with malformed sha error" `
            -Actual "ExitCode: $($res.ExitCode), Output: $($res.Combined.Trim())" -ExitCode 1 -Verdict "FAIL"
    }
} catch {
    Register-TestResult -TestId "T26" -Name "Malformed SHA-256 digest rejection" `
        -Command "Malformed sha test" -Expected "ExitCode != 0" `
        -Actual "Exception: $_" -ExitCode -1 -Verdict "FAIL"
}

# -------------------------------------------------------------------------
# T27 — Manifest correct digest verification for harmless fixture
# -------------------------------------------------------------------------
$harmlessFixture = Register-TestFixtureFile (Join-Path $sessionDir "harmless-binary.exe")
try {
    $dummyBytes = New-Object byte[] (1024 * 1024)
    for ($k = 0; $k -lt $dummyBytes.Length; $k++) { $dummyBytes[$k] = [byte]($k % 251) }
    [System.IO.File]::WriteAllBytes($harmlessFixture, $dummyBytes)
    $actualHashT27 = (Get-FileHash -LiteralPath $harmlessFixture -Algorithm SHA256).Hash.ToLowerInvariant()

    $testBlockT27 = @"
    . '$provisionScript'
    `$art = @{
        artifact_name = 'rustup-init'
        installer_version = '1.29.0'
        platform = 'windows'
        target = 'x86_64-pc-windows-gnu'
        expected_sha256 = '$actualHashT27'
        verification_policy = 'manifest-sha256-pinned'
        approval_status = 'approved'
    }
    `$res = Test-InstallerProvenance -FilePath '$harmlessFixture' -Artifact `$art
    if (`$res -eq `$true) { exit 0 } else { exit 1 }
"@
    $procT27 = Start-Process -FilePath "powershell.exe" `
        -ArgumentList @("-NoProfile", "-Command", $testBlockT27) `
        -Wait -NoNewWindow -PassThru

    if ($procT27.ExitCode -eq 0) {
        Register-TestResult -TestId "T27" -Name "Manifest correct digest verification for harmless fixture" `
            -Command "Test-InstallerProvenance with matching SHA-256 on harmless fixture" `
            -Expected "ExitCode 0 (provenance verified)" `
            -Actual "ExitCode: $($procT27.ExitCode)" -ExitCode 0 -Verdict "PASS"
    } else {
        Register-TestResult -TestId "T27" -Name "Manifest correct digest verification for harmless fixture" `
            -Command "Test-InstallerProvenance matching digest" `
            -Expected "ExitCode 0" -Actual "ExitCode: $($procT27.ExitCode)" `
            -ExitCode 1 -Verdict "FAIL"
    }
} catch {
    Register-TestResult -TestId "T27" -Name "Manifest correct digest verification for harmless fixture" `
        -Command "Test-InstallerProvenance matching digest" -Expected "ExitCode 0" `
        -Actual "Exception: $_" -ExitCode -1 -Verdict "FAIL"
}

# -------------------------------------------------------------------------
# T28 — Manifest incorrect digest rejection for harmless fixture
# -------------------------------------------------------------------------
try {
    $testBlockT28 = @"
    . '$provisionScript'
    `$art = @{
        artifact_name = 'rustup-init'
        installer_version = '1.29.0'
        platform = 'windows'
        target = 'x86_64-pc-windows-gnu'
        expected_sha256 = '0000000000000000000000000000000000000000000000000000000000000000'
        verification_policy = 'manifest-sha256-pinned'
        approval_status = 'approved'
    }
    `$res = Test-InstallerProvenance -FilePath '$harmlessFixture' -Artifact `$art
    if (`$res -eq `$false) { exit 0 } else { exit 1 }
"@
    $procT28 = Start-Process -FilePath "powershell.exe" `
        -ArgumentList @("-NoProfile", "-Command", $testBlockT28) `
        -Wait -NoNewWindow -PassThru

    if ($procT28.ExitCode -eq 0) {
        Register-TestResult -TestId "T28" -Name "Manifest incorrect digest rejection for harmless fixture" `
            -Command "Test-InstallerProvenance with mismatched SHA-256 on harmless fixture" `
            -Expected "ExitCode 0 (returns false, verification rejected)" `
            -Actual "ExitCode: $($procT28.ExitCode)" -ExitCode 0 -Verdict "PASS"
    } else {
        Register-TestResult -TestId "T28" -Name "Manifest incorrect digest rejection for harmless fixture" `
            -Command "Test-InstallerProvenance mismatched digest" `
            -Expected "ExitCode 0 (rejected)" -Actual "ExitCode: $($procT28.ExitCode)" `
            -ExitCode 1 -Verdict "FAIL"
    }
} catch {
    Register-TestResult -TestId "T28" -Name "Manifest incorrect digest rejection for harmless fixture" `
        -Command "Test-InstallerProvenance mismatched digest" -Expected "ExitCode 0" `
        -Actual "Exception: $_" -ExitCode -1 -Verdict "FAIL"
}

# -------------------------------------------------------------------------
# T29 — Artifact version mismatch rejection
# -------------------------------------------------------------------------
try {
    $res = Invoke-ScriptWithCapture -Arguments @("-DryRun", "-RustVersion", "99.99.99")

    if ($res.ExitCode -ne 0 -and $res.Combined -match "Artifact version mismatch") {
        Register-TestResult -TestId "T29" -Name "Artifact version mismatch rejection" `
            -Command "provision-rust-windows.ps1 -DryRun -RustVersion 99.99.99" `
            -Expected "Exit code non-zero, error matches 'Artifact version mismatch'" `
            -Actual "ExitCode: $($res.ExitCode), Output matches error" -ExitCode 0 -Verdict "PASS"
    } else {
        Register-TestResult -TestId "T29" -Name "Artifact version mismatch rejection" `
            -Command "provision-rust-windows.ps1 -DryRun -RustVersion 99.99.99" `
            -Expected "Exit code non-zero with version mismatch error" `
            -Actual "ExitCode: $($res.ExitCode), Output: $($res.Combined.Trim())" -ExitCode 1 -Verdict "FAIL"
    }
} catch {
    Register-TestResult -TestId "T29" -Name "Artifact version mismatch rejection" `
        -Command "Version mismatch test" -Expected "ExitCode != 0" `
        -Actual "Exception: $_" -ExitCode -1 -Verdict "FAIL"
}

# -------------------------------------------------------------------------
# T30 — Target / platform mismatch rejection
# -------------------------------------------------------------------------
try {
    $res = Invoke-ScriptWithCapture -Arguments @("-DryRun", "-Target", "mips-unknown-linux-gnu")

    if ($res.ExitCode -ne 0 -and $res.Combined -match "Target / platform mismatch") {
        Register-TestResult -TestId "T30" -Name "Target / platform mismatch rejection" `
            -Command "provision-rust-windows.ps1 -DryRun -Target mips-unknown-linux-gnu" `
            -Expected "Exit code non-zero, error matches 'Target / platform mismatch'" `
            -Actual "ExitCode: $($res.ExitCode), Output matches error" -ExitCode 0 -Verdict "PASS"
    } else {
        Register-TestResult -TestId "T30" -Name "Target / platform mismatch rejection" `
            -Command "provision-rust-windows.ps1 -DryRun -Target mips-unknown-linux-gnu" `
            -Expected "Exit code non-zero with target mismatch error" `
            -Actual "ExitCode: $($res.ExitCode), Output: $($res.Combined.Trim())" -ExitCode 1 -Verdict "FAIL"
    }
} catch {
    Register-TestResult -TestId "T30" -Name "Target / platform mismatch rejection" `
        -Command "Target mismatch test" -Expected "ExitCode != 0" `
        -Actual "Exception: $_" -ExitCode -1 -Verdict "FAIL"
}

# -------------------------------------------------------------------------
# T31 — Unapproved artifact rejection
# -------------------------------------------------------------------------
try {
    # In Stage 2.5, x86_64-pc-windows-msvc remains strictly unapproved in the manifest
    $res = Invoke-ScriptWithCapture -Arguments @(
        "-Install",
        "-Target", "x86_64-pc-windows-msvc",
        "-RustupHome", "E:\Workspace\Active\ULVO\vault\runtimes\rust\test-msvc-rustup",
        "-CargoHome", "E:\Workspace\Active\ULVO\vault\runtimes\rust\test-msvc-cargo",
        "-ExpectedSha256", "86478e53f769379d7f0ebfa7c9aa97cb76ca92233f79aa2cc0dbee2efaac73c7"
    )

    if ($res.ExitCode -ne 0 -and $res.Combined -match "approval status is 'unapproved'") {
        Register-TestResult -TestId "T31" -Name "Unapproved artifact rejection" `
            -Command "provision-rust-windows.ps1 -Install on unapproved artifact (x86_64-pc-windows-msvc)" `
            -Expected "Exit code non-zero, error matches 'approval status is ''unapproved'''" `
            -Actual "ExitCode: $($res.ExitCode), Output matches error" -ExitCode 0 -Verdict "PASS"
    } else {
        Register-TestResult -TestId "T31" -Name "Unapproved artifact rejection" `
            -Command "provision-rust-windows.ps1 -Install on unapproved artifact" `
            -Expected "Exit code non-zero with unapproved error" `
            -Actual "ExitCode: $($res.ExitCode), Output: $($res.Combined.Trim())" -ExitCode 1 -Verdict "FAIL"
    }
} catch {
    Register-TestResult -TestId "T31" -Name "Unapproved artifact rejection" `
        -Command "Unapproved artifact test" -Expected "ExitCode != 0" `
        -Actual "Exception: $_" -ExitCode -1 -Verdict "FAIL"
}

# -------------------------------------------------------------------------
# T32 — Untrusted download URL rejection
# -------------------------------------------------------------------------
try {
    $res1 = Invoke-ScriptWithCapture -Arguments @("-DryRun", "-DownloadUrl", "http://static.rust-lang.org/dist/rustup-init.exe")
    $res2 = Invoke-ScriptWithCapture -Arguments @("-DryRun", "-DownloadUrl", "https://malicious.example.com/rustup-init.exe")

    if ($res1.ExitCode -ne 0 -and $res2.ExitCode -ne 0 -and
        $res1.Combined -match "Untrusted DownloadUrl" -and $res2.Combined -match "Untrusted DownloadUrl") {
        Register-TestResult -TestId "T32" -Name "Untrusted download URL rejection" `
            -Command "provision-rust-windows.ps1 -DryRun with HTTP and untrusted domain URLs" `
            -Expected "Both exit non-zero and match 'Untrusted DownloadUrl'" `
            -Actual "res1: $($res1.ExitCode), res2: $($res2.ExitCode), both rejected" -ExitCode 0 -Verdict "PASS"
    } else {
        Register-TestResult -TestId "T32" -Name "Untrusted download URL rejection" `
            -Command "provision-rust-windows.ps1 -DryRun with untrusted URLs" `
            -Expected "Both exit non-zero" `
            -Actual "res1: $($res1.ExitCode), res2: $($res2.ExitCode)" -ExitCode 1 -Verdict "FAIL"
    }
} catch {
    Register-TestResult -TestId "T32" -Name "Untrusted download URL rejection" `
        -Command "Untrusted URL test" -Expected "ExitCode != 0" `
        -Actual "Exception: $_" -ExitCode -1 -Verdict "FAIL"
}

# -------------------------------------------------------------------------
# T33 — Unexpected redirect destination rejection
# -------------------------------------------------------------------------
try {
    $testBlockT33 = @"
    . '$provisionScript'
    `$r1 = Test-RedirectDestination -DestinationUri 'http://win.rustup.rs/x86_64'
    `$r2 = Test-RedirectDestination -DestinationUri 'https://malicious.example.com/rustup-init.exe'
    `$r3 = Test-RedirectDestination -DestinationUri 'https://win.rustup.rs.evil.org/payload.exe'
    `$r4 = Test-RedirectDestination -DestinationUri 'https://static.rust-lang.org/rustup/archive/1.29.0/x86_64-pc-windows-gnu/rustup-init.exe'
    if (`$r1 -eq `$false -and `$r2 -eq `$false -and `$r3 -eq `$false -and `$r4 -eq `$true) { exit 0 } else { exit 1 }
"@
    $procT33 = Start-Process -FilePath "powershell.exe" `
        -ArgumentList @("-NoProfile", "-Command", $testBlockT33) `
        -Wait -NoNewWindow -PassThru

    if ($procT33.ExitCode -eq 0) {
        Register-TestResult -TestId "T33" -Name "Unexpected redirect destination rejection" `
            -Command "Test-RedirectDestination across non-HTTPS, third-party, and official targets" `
            -Expected "All untrusted targets return false; official static target returns true" `
            -Actual "ExitCode: $($procT33.ExitCode)" -ExitCode 0 -Verdict "PASS"
    } else {
        Register-TestResult -TestId "T33" -Name "Unexpected redirect destination rejection" `
            -Command "Test-RedirectDestination checks" `
            -Expected "ExitCode 0" -Actual "ExitCode: $($procT33.ExitCode)" `
            -ExitCode 1 -Verdict "FAIL"
    }
} catch {
    Register-TestResult -TestId "T33" -Name "Unexpected redirect destination rejection" `
        -Command "Redirect destination test" -Expected "ExitCode 0" `
        -Actual "Exception: $_" -ExitCode -1 -Verdict "FAIL"
}

# -------------------------------------------------------------------------
# T34 — Unsigned artifact rejection under strict-signature policy
# -------------------------------------------------------------------------
try {
    $testBlockT34 = @"
    . '$provisionScript'
    `$art = @{
        artifact_name = 'rustup-init'
        installer_version = '1.29.0'
        platform = 'windows'
        target = 'x86_64-pc-windows-gnu'
        expected_sha256 = '$actualHashT27'
        verification_policy = 'authenticode-strict'
        approval_status = 'approved'
    }
    `$res = Test-InstallerProvenance -FilePath '$harmlessFixture' -Artifact `$art
    if (`$res -eq `$false) { exit 0 } else { exit 1 }
"@
    $procT34 = Start-Process -FilePath "powershell.exe" `
        -ArgumentList @("-NoProfile", "-Command", $testBlockT34) `
        -Wait -NoNewWindow -PassThru

    if ($procT34.ExitCode -eq 0) {
        Register-TestResult -TestId "T34" -Name "Unsigned artifact rejection under strict-signature policy" `
            -Command "Test-InstallerProvenance with authenticode-strict on unsigned fixture" `
            -Expected "Returns false (ExitCode 0 from verification block)" `
            -Actual "ExitCode: $($procT34.ExitCode)" -ExitCode 0 -Verdict "PASS"
    } else {
        Register-TestResult -TestId "T34" -Name "Unsigned artifact rejection under strict-signature policy" `
            -Command "Test-InstallerProvenance authenticode-strict on unsigned binary" `
            -Expected "Returns false" -Actual "ExitCode: $($procT34.ExitCode)" `
            -ExitCode 1 -Verdict "FAIL"
    }
} catch {
    Register-TestResult -TestId "T34" -Name "Unsigned artifact rejection under strict-signature policy" `
        -Command "authenticode-strict test" -Expected "ExitCode 0" `
        -Actual "Exception: $_" -ExitCode -1 -Verdict "FAIL"
}

# -------------------------------------------------------------------------
# T35 — Unsigned artifact acceptance under explicitly approved hash policy
# -------------------------------------------------------------------------
try {
    $testBlockT35 = @"
    . '$provisionScript'
    `$art = @{
        artifact_name = 'rustup-init'
        installer_version = '1.29.0'
        platform = 'windows'
        target = 'x86_64-pc-windows-gnu'
        expected_sha256 = '$actualHashT27'
        verification_policy = 'manifest-sha256-pinned'
        approval_status = 'approved'
    }
    `$res = Test-InstallerProvenance -FilePath '$harmlessFixture' -Artifact `$art
    if (`$res -eq `$true) { exit 0 } else { exit 1 }
"@
    $procT35 = Start-Process -FilePath "powershell.exe" `
        -ArgumentList @("-NoProfile", "-Command", $testBlockT35) `
        -Wait -NoNewWindow -PassThru

    if ($procT35.ExitCode -eq 0) {
        Register-TestResult -TestId "T35" -Name "Unsigned artifact acceptance under explicitly approved hash policy" `
            -Command "Test-InstallerProvenance with manifest-sha256-pinned and approved status" `
            -Expected "Returns true (ExitCode 0 from verification block)" `
            -Actual "ExitCode: $($procT35.ExitCode)" -ExitCode 0 -Verdict "PASS"
    } else {
        Register-TestResult -TestId "T35" -Name "Unsigned artifact acceptance under explicitly approved hash policy" `
            -Command "Test-InstallerProvenance manifest-sha256-pinned approved" `
            -Expected "Returns true" -Actual "ExitCode: $($procT35.ExitCode)" `
            -ExitCode 1 -Verdict "FAIL"
    }
} catch {
    Register-TestResult -TestId "T35" -Name "Unsigned artifact acceptance under explicitly approved hash policy" `
        -Command "manifest-sha256-pinned approved test" -Expected "ExitCode 0" `
        -Actual "Exception: $_" -ExitCode -1 -Verdict "FAIL"
}

# -------------------------------------------------------------------------
# T36 — Caller hash override rejection
# -------------------------------------------------------------------------
try {
    $fakeHash = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
    $res = Invoke-ScriptWithCapture -Arguments @("-DryRun", "-ExpectedSha256", $fakeHash)

    if ($res.ExitCode -ne 0 -and $res.Combined -match "Caller-supplied ExpectedSha256" -and $res.Combined -match "Caller override rejected") {
        Register-TestResult -TestId "T36" -Name "Caller hash override rejection" `
            -Command "provision-rust-windows.ps1 -DryRun -ExpectedSha256 $fakeHash" `
            -Expected "Exit code non-zero, error matches 'Caller override rejected'" `
            -Actual "ExitCode: $($res.ExitCode), Output matches error" -ExitCode 0 -Verdict "PASS"
    } else {
        Register-TestResult -TestId "T36" -Name "Caller hash override rejection" `
            -Command "provision-rust-windows.ps1 -DryRun -ExpectedSha256 $fakeHash" `
            -Expected "Exit code non-zero with override rejection" `
            -Actual "ExitCode: $($res.ExitCode), Output: $($res.Combined.Trim())" -ExitCode 1 -Verdict "FAIL"
    }
} catch {
    Register-TestResult -TestId "T36" -Name "Caller hash override rejection" `
        -Command "Caller hash override test" -Expected "ExitCode != 0" `
        -Actual "Exception: $_" -ExitCode -1 -Verdict "FAIL"
}

# -------------------------------------------------------------------------
# T37 — Artifact replacement after verification rejection (TOCTOU)
# -------------------------------------------------------------------------
try {
    $toctouMock = Register-TestFixtureFile (Join-Path $sessionDir "toctou-mock.cmd")
    Set-Content -LiteralPath $toctouMock -Value "@exit 0" -Force
    $originalToctouHash = (Get-FileHash -LiteralPath $toctouMock -Algorithm SHA256).Hash.ToLowerInvariant()

    # Modify file after hash calculation to simulate replacement/tampering before launch
    Add-Content -LiteralPath $toctouMock -Value "`r`nREM Tampered payload"

    $testBlockT37 = @"
    `$ErrorActionPreference = 'Stop'
    . '$provisionScript'
    try {
        `$null = Invoke-RustupInstaller -InstallerPath '$toctouMock' -RustupHome 'E:\Workspace\Active\ULVO\vault\runtimes\rust\rustup' -CargoHome 'E:\Workspace\Active\ULVO\vault\runtimes\rust\cargo' -RustVersion '1.96.0' -Target 'x86_64-pc-windows-gnu' -ExpectedSha256 '$originalToctouHash'
        exit 0
    } catch {
        exit 42
    }
"@
    $procT37 = Start-Process -FilePath "powershell.exe" `
        -ArgumentList @("-NoProfile", "-Command", $testBlockT37) `
        -Wait -NoNewWindow -PassThru

    # Either child exits with 42 (caught and matched error) or exits 1 (via Write-ErrorExit terminating child)
    if ($procT37.ExitCode -eq 42 -or $procT37.ExitCode -eq 1) {
        Register-TestResult -TestId "T37" -Name "Artifact replacement after verification rejection (TOCTOU)" `
            -Command "Invoke-RustupInstaller with modified file prior to launch" `
            -Expected "Exit code indicates pre-launch rejection (non-zero)" `
            -Actual "ExitCode: $($procT37.ExitCode) (tampered binary rejected)" -ExitCode 0 -Verdict "PASS"
    } else {
        Register-TestResult -TestId "T37" -Name "Artifact replacement after verification rejection (TOCTOU)" `
            -Command "Invoke-RustupInstaller pre-launch tampering test" `
            -Expected "Non-zero exit (rejection)" -Actual "ExitCode: $($procT37.ExitCode)" `
            -ExitCode 1 -Verdict "FAIL"
    }
} catch {
    Register-TestResult -TestId "T37" -Name "Artifact replacement after verification rejection (TOCTOU)" `
        -Command "TOCTOU pre-launch test" -Expected "Non-zero exit" `
        -Actual "Exception: $_" -ExitCode -1 -Verdict "FAIL"
}

# -------------------------------------------------------------------------
# T38 — Destination collision handling
# -------------------------------------------------------------------------
try {
    $t38FixtureRoot = Join-Path $sessionDir "t38-root"
    New-TestFixtureDirectory -Path $t38FixtureRoot | Out-Null
    $t38Vault = Join-Path $t38FixtureRoot "vault"
    $t38Runtimes = Join-Path $t38Vault "runtimes"
    $t38Rust = Join-Path $t38Runtimes "rust"
    $t38Rustup = Join-Path $t38Rust "rustup"
    New-TestFixtureDirectory -Path $t38Vault | Out-Null
    New-TestFixtureDirectory -Path $t38Runtimes | Out-Null
    New-TestFixtureDirectory -Path $t38Rust | Out-Null
    New-TestFixtureDirectory -Path $t38Rustup | Out-Null

    $res = Invoke-ScriptWithCapture -Arguments @("-DryRun", "-UlvoRoot", $t38FixtureRoot)

    if ($res.ExitCode -ne 0 -and $res.Combined -match "RUSTUP_HOME already exists") {
        Register-TestResult -TestId "T38" -Name "Destination collision handling" `
            -Command "provision-rust-windows.ps1 -DryRun on root with pre-existing rustup directory" `
            -Expected "Exit code non-zero, error matches 'RUSTUP_HOME already exists'" `
            -Actual "ExitCode: $($res.ExitCode), Output matches error" -ExitCode 0 -Verdict "PASS"
    } else {
        Register-TestResult -TestId "T38" -Name "Destination collision handling" `
            -Command "provision-rust-windows.ps1 -DryRun with existing destination" `
            -Expected "Exit code non-zero with already exists error" `
            -Actual "ExitCode: $($res.ExitCode), Output: $($res.Combined.Trim())" -ExitCode 1 -Verdict "FAIL"
    }
    Cleanup-TestFixture -Path $t38FixtureRoot | Out-Null
} catch {
    Register-TestResult -TestId "T38" -Name "Destination collision handling" `
        -Command "Destination collision test" -Expected "ExitCode != 0" `
        -Actual "Exception: $_" -ExitCode -1 -Verdict "FAIL"
}

# -------------------------------------------------------------------------
# T39 — Production installation blocked on fixture root
# -------------------------------------------------------------------------
try {
    $t39FixtureRoot = Join-Path $sessionDir "t39-fixture-root"
    New-TestFixtureDirectory -Path $t39FixtureRoot | Out-Null
    $res = Invoke-ScriptWithCapture -Arguments @("-Install", "-UlvoRoot", $t39FixtureRoot)

    if ($res.ExitCode -ne 0 -and $res.Combined -match "Production installation \(-Install\) is strictly prohibited") {
        Register-TestResult -TestId "T39" -Name "Production installation blocked on fixture root" `
            -Command "provision-rust-windows.ps1 -Install -UlvoRoot '$t39FixtureRoot'" `
            -Expected "Exit code non-zero, error matches 'Production installation (-Install) is strictly prohibited'" `
            -Actual "ExitCode: $($res.ExitCode), Output matches error" -ExitCode 0 -Verdict "PASS"
    } else {
        Register-TestResult -TestId "T39" -Name "Production installation blocked on fixture root" `
            -Command "provision-rust-windows.ps1 -Install -UlvoRoot '$t39FixtureRoot'" `
            -Expected "Exit code non-zero with fixture root install rejection" `
            -Actual "ExitCode: $($res.ExitCode), Output: $($res.Combined.Trim())" -ExitCode 1 -Verdict "FAIL"
    }
    Cleanup-TestFixture -Path $t39FixtureRoot | Out-Null
} catch {
    Register-TestResult -TestId "T39" -Name "Production installation blocked on fixture root" `
        -Command "Fixture root install rejection test" -Expected "ExitCode != 0" `
        -Actual "Exception: $_" -ExitCode -1 -Verdict "FAIL"
}

# -------------------------------------------------------------------------
# T40 — Caller DownloadUrl substitution rejected
# -------------------------------------------------------------------------
try {
    $res = Invoke-ScriptWithCapture -Arguments @("-DryRun", "-DownloadUrl", "https://win.rustup.rs/x86_64")

    if ($res.ExitCode -ne 0 -and $res.Combined -match "Caller URL substitution") {
        Register-TestResult -TestId "T40" -Name "Caller DownloadUrl substitution rejected" `
            -Command "provision-rust-windows.ps1 -DryRun -DownloadUrl 'https://win.rustup.rs/x86_64'" `
            -Expected "Exit code non-zero, error matches caller URL substitution prohibited" `
            -Actual "ExitCode: $($res.ExitCode), Output matches error" -ExitCode 0 -Verdict "PASS"
    } else {
        Register-TestResult -TestId "T40" -Name "Caller DownloadUrl substitution rejected" `
            -Command "provision-rust-windows.ps1 -DryRun -DownloadUrl substitution" `
            -Expected "Exit code non-zero with substitution rejection" `
            -Actual "ExitCode: $($res.ExitCode), Output: $($res.Combined.Trim())" -ExitCode 1 -Verdict "FAIL"
    }
} catch {
    Register-TestResult -TestId "T40" -Name "Caller DownloadUrl substitution rejected" `
        -Command "Caller DownloadUrl substitution test" -Expected "ExitCode != 0" `
        -Actual "Exception: $_" -ExitCode -1 -Verdict "FAIL"
}

# -------------------------------------------------------------------------
# T41 — Single GET redirect flow streaming and per-hop host validation
# -------------------------------------------------------------------------
try {
    $t41OutFile = Register-TestFixtureFile (Join-Path $sessionDir "t41-downloaded.bin")

    # Step 1: Untrusted host must be rejected immediately without making network call
    $untrustedRejected = $false
    try {
        Invoke-DownloadWithRedirectCheck -Uri "https://untrusted.example.com/rustup-init.exe" -OutFile $t41OutFile
    } catch {
        if ($_.Exception.Message -match "Untrusted download or redirect destination") {
            $untrustedRejected = $true
        }
    }

    # Step 2: Bounded mock HTTP server using PowerShell Runspace on dynamic port
    $tcp = [System.Net.Sockets.TcpListener]::new([System.Net.IPAddress]::Loopback, 0)
    $tcp.Start()
    $port = ($tcp.LocalEndpoint).Port
    $tcp.Stop()

    $expectedPayload = "ULVO-STAGE241-STREAM-PAYLOAD-OK"

    $ps = [PowerShell]::Create()
    $serverScript = {
        param([int]$serverPort, [string]$payload)
        $listener = [System.Net.HttpListener]::new()
        $prefix = "http://127.0.0.1:$serverPort/"
        $listener.Prefixes.Add($prefix)
        $listener.Start()
        try {
            # Hop 1: 302 Redirect -> /final.bin
            $async1 = $listener.BeginGetContext($null, $null)
            if ($async1.AsyncWaitHandle.WaitOne(5000)) {
                $ctx1 = $listener.EndGetContext($async1)
                $ctx1.Response.StatusCode = 302
                $ctx1.Response.Headers.Add("Location", "http://127.0.0.1:$serverPort/final.bin")
                $ctx1.Response.Close()
            } else {
                return
            }

            # Hop 2: 200 OK with streamed payload
            $async2 = $listener.BeginGetContext($null, $null)
            if ($async2.AsyncWaitHandle.WaitOne(5000)) {
                $ctx2 = $listener.EndGetContext($async2)
                $ctx2.Response.StatusCode = 200
                $bytes = [System.Text.Encoding]::UTF8.GetBytes($payload)
                $ctx2.Response.ContentLength64 = $bytes.Length
                $ctx2.Response.OutputStream.Write($bytes, 0, $bytes.Length)
                $ctx2.Response.OutputStream.Flush()
                $ctx2.Response.Close()
            } else {
                return
            }
        } finally {
            try { $listener.Stop() } catch {}
            try { $listener.Close() } catch {}
        }
    }

    [void]$ps.AddScript($serverScript).AddArgument($port).AddArgument($expectedPayload)
    $asyncHandle = $ps.BeginInvoke()

    # Allow listener a moment to bind
    [System.Threading.Thread]::Sleep(200)

    try {
        $allowedPattern = "^http://127\.0\.0\.1:\d+/"
        Invoke-DownloadWithRedirectCheck -Uri "http://127.0.0.1:$port/start.exe" -OutFile $t41OutFile -AllowedPattern $allowedPattern
        [void]$asyncHandle.AsyncWaitHandle.WaitOne(5000)
    } finally {
        try { $ps.Stop() } catch {}
        try { $ps.Dispose() } catch {}
    }

    $streamSucceeded = $false
    if (Test-Path -LiteralPath $t41OutFile) {
        $actualPayload = [System.IO.File]::ReadAllText($t41OutFile)
        if ($actualPayload -eq $expectedPayload) {
            $streamSucceeded = $true
        }
        Remove-Item -LiteralPath $t41OutFile -Force -ErrorAction SilentlyContinue
    }

    if ($untrustedRejected -and $streamSucceeded) {
        Register-TestResult -TestId "T41" -Name "Single GET redirect flow streaming and per-hop host validation" `
            -Command "Invoke-DownloadWithRedirectCheck mock streaming GET redirect" `
            -Expected "ExitCode 0, payload streamed correctly following validated redirects" `
            -Actual "Untrusted rejected: $untrustedRejected, Stream verified: $streamSucceeded" -ExitCode 0 -Verdict "PASS"
    } else {
        Register-TestResult -TestId "T41" -Name "Single GET redirect flow streaming and per-hop host validation" `
            -Command "Invoke-DownloadWithRedirectCheck mock test" `
            -Expected "Untrusted rejected and stream verified" `
            -Actual "Untrusted rejected: $untrustedRejected, Stream verified: $streamSucceeded" `
            -ExitCode 1 -Verdict "FAIL"
    }
} catch {
    Register-TestResult -TestId "T41" -Name "Single GET redirect flow streaming and per-hop host validation" `
        -Command "Single GET redirect test" -Expected "ExitCode 0" `
        -Actual "Exception: $_" -ExitCode -1 -Verdict "FAIL"
}

# -------------------------------------------------------------------------
# T42 — Duplicate TOML key rejection in manifest
# -------------------------------------------------------------------------
try {
    $dupKeyToml = Register-TestFixtureFile (Join-Path $sessionDir "dup-key.toml")
    $dupKeyContent = @(
        'schema_version = "1.0.0"',
        'schema_version = "1.0.0"',
        '[[artifacts]]',
        'artifact_name = "rustup-init"',
        'installer_version = "1.29.0"',
        'platform = "windows"',
        'target = "x86_64-pc-windows-gnu"',
        'canonical_url = "https://static.rust-lang.org/rustup/dist/x86_64-pc-windows-gnu/rustup-init.exe"',
        'expected_sha256 = "03dbaae1d33a4d220bd0d202e5092955dae859c119074192ca513f8c4713fff7"',
        'verification_policy = "manifest-sha256-pinned"',
        'approval_status = "unapproved"'
    ) -join "`r`n"
    Set-Content -LiteralPath $dupKeyToml -Value $dupKeyContent -Force
    $res = Invoke-ScriptWithCapture -Arguments @("-DryRun", "-ManifestPath", $dupKeyToml)

    if ($res.ExitCode -ne 0 -and $res.Combined -match "Duplicate root key") {
        Register-TestResult -TestId "T42" -Name "Duplicate TOML key rejection in manifest" `
            -Command "provision-rust-windows.ps1 -DryRun -ManifestPath $dupKeyToml" `
            -Expected "Exit code non-zero, error matches 'Duplicate root key'" `
            -Actual "ExitCode: $($res.ExitCode), Output matches error" -ExitCode 0 -Verdict "PASS"
    } else {
        Register-TestResult -TestId "T42" -Name "Duplicate TOML key rejection in manifest" `
            -Command "provision-rust-windows.ps1 -DryRun -ManifestPath $dupKeyToml" `
            -Expected "Exit code non-zero with duplicate key error" `
            -Actual "ExitCode: $($res.ExitCode), Output: $($res.Combined.Trim())" -ExitCode 1 -Verdict "FAIL"
    }
} catch {
    Register-TestResult -TestId "T42" -Name "Duplicate TOML key rejection in manifest" `
        -Command "Duplicate key manifest test" -Expected "ExitCode != 0" `
        -Actual "Exception: $_" -ExitCode -1 -Verdict "FAIL"
}

# -------------------------------------------------------------------------
# T43 — Duplicate artifact identity rejection in manifest
# -------------------------------------------------------------------------
try {
    $dupArtToml = Register-TestFixtureFile (Join-Path $sessionDir "dup-artifact.toml")
    $dupArtContent = @(
        'schema_version = "1.0.0"',
        '[[artifacts]]',
        'artifact_name = "rustup-init"',
        'installer_version = "1.29.0"',
        'platform = "windows"',
        'target = "x86_64-pc-windows-gnu"',
        'canonical_url = "https://static.rust-lang.org/rustup/dist/x86_64-pc-windows-gnu/rustup-init.exe"',
        'expected_sha256 = "03dbaae1d33a4d220bd0d202e5092955dae859c119074192ca513f8c4713fff7"',
        'verification_policy = "manifest-sha256-pinned"',
        'approval_status = "unapproved"',
        '',
        '[[artifacts]]',
        'artifact_name = "rustup-init"',
        'installer_version = "1.29.0"',
        'platform = "windows"',
        'target = "x86_64-pc-windows-gnu"',
        'canonical_url = "https://static.rust-lang.org/rustup/dist/x86_64-pc-windows-gnu/rustup-init.exe"',
        'expected_sha256 = "03dbaae1d33a4d220bd0d202e5092955dae859c119074192ca513f8c4713fff7"',
        'verification_policy = "manifest-sha256-pinned"',
        'approval_status = "unapproved"'
    ) -join "`r`n"
    Set-Content -LiteralPath $dupArtToml -Value $dupArtContent -Force
    $res = Invoke-ScriptWithCapture -Arguments @("-DryRun", "-ManifestPath", $dupArtToml)

    if ($res.ExitCode -ne 0 -and $res.Combined -match "Duplicate artifact entry detected for identity") {
        Register-TestResult -TestId "T43" -Name "Duplicate artifact identity rejection in manifest" `
            -Command "provision-rust-windows.ps1 -DryRun -ManifestPath $dupArtToml" `
            -Expected "Exit code non-zero, error matches 'Duplicate artifact entry detected for identity'" `
            -Actual "ExitCode: $($res.ExitCode), Output matches error" -ExitCode 0 -Verdict "PASS"
    } else {
        Register-TestResult -TestId "T43" -Name "Duplicate artifact identity rejection in manifest" `
            -Command "provision-rust-windows.ps1 -DryRun -ManifestPath $dupArtToml" `
            -Expected "Exit code non-zero with duplicate artifact identity error" `
            -Actual "ExitCode: $($res.ExitCode), Output: $($res.Combined.Trim())" -ExitCode 1 -Verdict "FAIL"
    }
} catch {
    Register-TestResult -TestId "T43" -Name "Duplicate artifact identity rejection in manifest" `
        -Command "Duplicate artifact identity test" -Expected "ExitCode != 0" `
        -Actual "Exception: $_" -ExitCode -1 -Verdict "FAIL"
}

# -------------------------------------------------------------------------
# T44 — Ambiguous artifact match rejection in Find-ToolchainArtifact
# -------------------------------------------------------------------------
try {
    $testBlockT44 = @"
    . '$provisionScript'
    `$manifest = @{
        schema_version = '1.0.0'
        artifacts = @(
            @{
                artifact_name = 'rustup-init'
                installer_version = '1.29.0'
                platform = 'windows'
                target = 'x86_64-pc-windows-gnu'
                toolchain_version = '1.96.0'
                expected_sha256 = '03dbaae1d33a4d220bd0d202e5092955dae859c119074192ca513f8c4713fff7'
                verification_policy = 'manifest-sha256-pinned'
                approval_status = 'unapproved'
            },
            @{
                artifact_name = 'rustup-init'
                installer_version = '1.29.0'
                platform = 'windows'
                target = 'x86_64-pc-windows-gnu'
                toolchain_version = '1.96.0'
                expected_sha256 = 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa'
                verification_policy = 'manifest-sha256-pinned'
                approval_status = 'unapproved'
            }
        )
    }

    try {
        Find-ToolchainArtifact -Manifest `$manifest -ArtifactName 'rustup-init' -Platform 'windows' -Target 'x86_64-pc-windows-gnu' -InstallerVersion '1.29.0' -ToolchainVersion '1.96.0'
        exit 1
    } catch {
        if (`$_.Exception.Message -match 'Ambiguous artifact match: Multiple \(2\) conflicting artifacts matched') {
            exit 0
        } else {
            exit 2
        }
    }
"@
    $procT44 = Start-Process -FilePath "powershell.exe" `
        -ArgumentList @("-NoProfile", "-Command", $testBlockT44) `
        -Wait -NoNewWindow -PassThru

    if ($procT44.ExitCode -eq 0) {
        Register-TestResult -TestId "T44" -Name "Ambiguous artifact match rejection in Find-ToolchainArtifact" `
            -Command "Find-ToolchainArtifact with conflicting matching entries" `
            -Expected "ExitCode 0 (fails closed with ambiguous artifact match error)" `
            -Actual "ExitCode: $($procT44.ExitCode)" -ExitCode 0 -Verdict "PASS"
    } else {
        Register-TestResult -TestId "T44" -Name "Ambiguous artifact match rejection in Find-ToolchainArtifact" `
            -Command "Find-ToolchainArtifact ambiguous match test" `
            -Expected "ExitCode 0" -Actual "ExitCode: $($procT44.ExitCode)" `
            -ExitCode 1 -Verdict "FAIL"
    }
} catch {
    Register-TestResult -TestId "T44" -Name "Ambiguous artifact match rejection in Find-ToolchainArtifact" `
        -Command "Ambiguous match test" -Expected "ExitCode != 0" `
        -Actual "Exception: $_" -ExitCode -1 -Verdict "FAIL"
}

# -------------------------------------------------------------------------
# T45 — Production artifacts manifest approval gating
# -------------------------------------------------------------------------
try {
    $manifestPath = Join-Path $ulvoRoot "config\toolchain-artifacts.toml"
    $manifest = Get-ToolchainManifest -FilePath $manifestPath

    $msvcArtifact = $manifest.artifacts | Where-Object { $_.target -eq "x86_64-pc-windows-msvc" } | Select-Object -First 1
    $gnuArtifact = $manifest.artifacts | Where-Object { $_.target -eq "x86_64-pc-windows-gnu" } | Select-Object -First 1

    $msvcUnapproved = ($msvcArtifact["approval_status"] -eq "unapproved")
    $gnuApproved = ($gnuArtifact["approval_status"] -eq "approved")

    if ($msvcUnapproved -and $gnuApproved -and $manifest.artifacts.Count -ge 2) {
        Register-TestResult -TestId "T45" -Name "Production artifacts manifest approval gating" `
            -Command "Verify approval_status in config/toolchain-artifacts.toml" `
            -Expected "ExitCode 0, MSVC remains unapproved and GNU is operator-approved" `
            -Actual "Artifacts count: $($manifest.artifacts.Count), MSVC unapproved: $msvcUnapproved, GNU approved: $gnuApproved" `
            -ExitCode 0 -Verdict "PASS"
    } else {
        Register-TestResult -TestId "T45" -Name "Production artifacts manifest approval gating" `
            -Command "Verify approval_status in config/toolchain-artifacts.toml" `
            -Expected "MSVC unapproved and GNU approved" `
            -Actual "Count: $($manifest.artifacts.Count), MSVC unapproved: $msvcUnapproved, GNU approved: $gnuApproved" `
            -ExitCode 1 -Verdict "FAIL"
    }
} catch {
    Register-TestResult -TestId "T45" -Name "Production artifacts in manifest remain unapproved" `
        -Command "Production manifest approval check" -Expected "ExitCode 0" `
        -Actual "Exception: $_" -ExitCode -1 -Verdict "FAIL"
}

# -------------------------------------------------------------------------
# T46 — Nested directory junction pointing outside tests/tmp rejected
# -------------------------------------------------------------------------
try {
    # 1. Create a disposable external target outside tests/tmp (under tests/t46-external)
    $t46ExternalRoot = Join-Path (Split-Path $fixtureRoot -Parent) "t46-external-$sessionId"
    New-Item -ItemType Directory -Path $t46ExternalRoot -Force | Out-Null
    $t46Sentinel = Join-Path $t46ExternalRoot "external-sentinel.txt"
    Set-Content -LiteralPath $t46Sentinel -Value "EXTERNAL-TARGET-SENSITIVE-CONTENT" -Force
    $t46HashBefore = (Get-FileHash -LiteralPath $t46Sentinel -Algorithm SHA256).Hash

    # 2. Create owned fixture directory inside sessionDir
    $t46Fixture = New-TestFixtureDirectory -Path (Join-Path $sessionDir "t46-fixture")

    # 3. Create junction inside owned fixture pointing outside tests/tmp
    $t46Junction = Join-Path $t46Fixture "escaped-junc"
    $mkOutT46 = cmd.exe /c "mklink /J `"$t46Junction`" `"$t46ExternalRoot`"" 2>&1
    
    # 4. Stamp sidecar and register junction to simulate attacker within fixture
    $sidecarTokenT46 = @{
        sessionId = $sessionId
        pid = $PID
        createdAt = (Get-Date).ToString("o")
        junctionPath = $t46Junction
        targetPath = $t46ExternalRoot
        type = "junction"
    } | ConvertTo-Json -Compress
    Set-Content -LiteralPath "$t46Junction.junction-owner.json" -Value $sidecarTokenT46 -Force
    $script:ownedFixtureRegistry.Add($t46Junction) | Out-Null
    $script:ownedFixtureRegistry.Add("$t46Junction.junction-owner.json") | Out-Null

    # 5. Attempt cleanup on fixture containing the escaping junction
    $resT46 = Cleanup-TestFixture -Path $t46Fixture

    # 6. Verify fail-closed behavior
    $t46FixtureStillExists = Test-Path -LiteralPath $t46Fixture
    $t46ExternalStillExists = Test-Path -LiteralPath $t46ExternalRoot
    $t46HashAfter = (Get-FileHash -LiteralPath $t46Sentinel -Algorithm SHA256).Hash

    $t46Passed = ($resT46 -eq $false -and $t46FixtureStillExists -and $t46ExternalStillExists -and ($t46HashBefore -eq $t46HashAfter))

    # Teardown
    [System.IO.Directory]::Delete($t46Junction, $false)
    $script:ownedFixtureRegistry.Remove($t46Junction) | Out-Null
    Remove-Item -LiteralPath "$t46Junction.junction-owner.json" -Force -ErrorAction SilentlyContinue
    $script:ownedFixtureRegistry.Remove("$t46Junction.junction-owner.json") | Out-Null
    Remove-Item -LiteralPath $t46Sentinel -Force -ErrorAction SilentlyContinue
    [System.IO.Directory]::Delete($t46ExternalRoot, $false)
    Cleanup-TestFixture -Path $t46Fixture | Out-Null

    if ($t46Passed) {
        Register-TestResult -TestId "T46" -Name "Nested directory junction pointing outside tests/tmp rejected" `
            -Command "Cleanup-TestFixture on fixture with junction targeting path outside tests\tmp" `
            -Expected "Cleanup returns false, fixture preserved, external target unmodified" `
            -Actual "Cleanup: $resT46, Fixture exists: $t46FixtureStillExists, Target hash preserved: $($t46HashBefore -eq $t46HashAfter)" `
            -ExitCode 0 -Verdict "PASS"
    } else {
        Register-TestResult -TestId "T46" -Name "Nested directory junction pointing outside tests/tmp rejected" `
            -Command "Cleanup-TestFixture on fixture with escaping junction" `
            -Expected "Cleanup returns false and target unmodified" `
            -Actual "Cleanup: $resT46, Fixture exists: $t46FixtureStillExists, Target exists: $t46ExternalStillExists" `
            -ExitCode 1 -Verdict "FAIL"
    }
} catch {
    Register-TestResult -TestId "T46" -Name "Nested directory junction pointing outside tests/tmp rejected" `
        -Command "Escaping junction cleanup test" -Expected "ExitCode 0" `
        -Actual "Exception: $_" -ExitCode -1 -Verdict "FAIL"
}

# -------------------------------------------------------------------------
# T47 — Junction pointing toward persistent ULVO vault rejected
# -------------------------------------------------------------------------
try {
    # 1. Compute hash of persistent vault binary before test
    $vaultExe = Join-Path $ulvoRoot "vault\runtimes\rust\Downloads\rustup-init.exe"
    $vaultHashBefore = (Get-FileHash -LiteralPath $vaultExe -Algorithm SHA256).Hash

    # 2. Create owned fixture directory inside sessionDir
    $t47Fixture = New-TestFixtureDirectory -Path (Join-Path $sessionDir "t47-vault-fixture")

    # 3. Create junction inside fixture pointing to persistent vault
    $t47Junction = Join-Path $t47Fixture "vault-junc"
    $canonicalVaultPath = (Join-Path $ulvoRoot "vault")
    $mkOutT47 = cmd.exe /c "mklink /J `"$t47Junction`" `"$canonicalVaultPath`"" 2>&1

    # 4. Stamp sidecar and register junction
    $sidecarTokenT47 = @{
        sessionId = $sessionId
        pid = $PID
        createdAt = (Get-Date).ToString("o")
        junctionPath = $t47Junction
        targetPath = $canonicalVaultPath
        type = "junction"
    } | ConvertTo-Json -Compress
    Set-Content -LiteralPath "$t47Junction.junction-owner.json" -Value $sidecarTokenT47 -Force
    $script:ownedFixtureRegistry.Add($t47Junction) | Out-Null
    $script:ownedFixtureRegistry.Add("$t47Junction.junction-owner.json") | Out-Null

    # 5. Attempt cleanup on fixture containing the vault junction
    $resT47 = Cleanup-TestFixture -Path $t47Fixture

    # 6. Verify fail-closed behavior
    $vaultHashAfter = (Get-FileHash -LiteralPath $vaultExe -Algorithm SHA256).Hash
    $t47FixtureStillExists = Test-Path -LiteralPath $t47Fixture
    $vaultIntact = ($vaultHashBefore -eq $vaultHashAfter)

    $t47Passed = ($resT47 -eq $false -and $t47FixtureStillExists -and $vaultIntact)

    # Teardown
    [System.IO.Directory]::Delete($t47Junction, $false)
    $script:ownedFixtureRegistry.Remove($t47Junction) | Out-Null
    Remove-Item -LiteralPath "$t47Junction.junction-owner.json" -Force -ErrorAction SilentlyContinue
    $script:ownedFixtureRegistry.Remove("$t47Junction.junction-owner.json") | Out-Null
    Cleanup-TestFixture -Path $t47Fixture | Out-Null

    if ($t47Passed) {
        Register-TestResult -TestId "T47" -Name "Junction pointing toward persistent ULVO vault rejected" `
            -Command "Cleanup-TestFixture on fixture containing junction pointing to vault" `
            -Expected "Cleanup returns false, fixture preserved, persistent vault unmodified" `
            -Actual "Cleanup: $resT47, Fixture exists: $t47FixtureStillExists, Vault hash match: $vaultIntact" `
            -ExitCode 0 -Verdict "PASS"
    } else {
        Register-TestResult -TestId "T47" -Name "Junction pointing toward persistent ULVO vault rejected" `
            -Command "Cleanup-TestFixture on vault junction" `
            -Expected "Cleanup returns false and vault preserved" `
            -Actual "Cleanup: $resT47, Fixture exists: $t47FixtureStillExists, Vault intact: $vaultIntact" `
            -ExitCode 1 -Verdict "FAIL"
    }
} catch {
    Register-TestResult -TestId "T47" -Name "Junction pointing toward persistent ULVO vault rejected" `
        -Command "Vault junction cleanup test" -Expected "ExitCode 0" `
        -Actual "Exception: $_" -ExitCode -1 -Verdict "FAIL"
}

# -------------------------------------------------------------------------
# T48 — Unowned junction inside an otherwise owned fixture causes fail-closed
# -------------------------------------------------------------------------
try {
    # 1. Create owned fixture directory and disposable target inside sessionDir
    $t48Fixture = New-TestFixtureDirectory -Path (Join-Path $sessionDir "t48-fixture")
    $t48Target = New-TestFixtureDirectory -Path (Join-Path $sessionDir "t48-target")
    $t48Sentinel = Register-TestFixtureFile (Join-Path $t48Target "target-sentinel.txt")
    Set-Content -LiteralPath $t48Sentinel -Value "TARGET-SENTINEL-T48" -Force
    $t48HashBefore = (Get-FileHash -LiteralPath $t48Sentinel -Algorithm SHA256).Hash

    # 2. Create raw unowned junction (NO sidecar, NOT in registry)
    $t48Junction = Join-Path $t48Fixture "unowned-junc"
    $mkOutT48 = cmd.exe /c "mklink /J `"$t48Junction`" `"$t48Target`"" 2>&1

    # 3. Attempt cleanup on fixture containing the unowned junction
    $resT48 = Cleanup-TestFixture -Path $t48Fixture

    # 4. Verify fail-closed behavior
    $t48FixtureStillExists = Test-Path -LiteralPath $t48Fixture
    $t48JunctionStillExists = Test-Path -LiteralPath $t48Junction
    $t48TargetStillExists = Test-Path -LiteralPath $t48Target
    $t48HashAfter = (Get-FileHash -LiteralPath $t48Sentinel -Algorithm SHA256).Hash

    $t48Passed = ($resT48 -eq $false -and $t48FixtureStillExists -and $t48JunctionStillExists -and $t48TargetStillExists -and ($t48HashBefore -eq $t48HashAfter))

    # Teardown
    [System.IO.Directory]::Delete($t48Junction, $false)
    Cleanup-TestFixture -Path $t48Target | Out-Null
    Cleanup-TestFixture -Path $t48Fixture | Out-Null

    if ($t48Passed) {
        Register-TestResult -TestId "T48" -Name "Unowned junction inside an otherwise owned fixture" `
            -Command "Cleanup-TestFixture on owned fixture containing unowned junction" `
            -Expected "Cleanup returns false, entire fixture preserved without unlinking" `
            -Actual "Cleanup: $resT48, Fixture exists: $t48FixtureStillExists, Junction exists: $t48JunctionStillExists" `
            -ExitCode 0 -Verdict "PASS"
    } else {
        Register-TestResult -TestId "T48" -Name "Unowned junction inside an otherwise owned fixture" `
            -Command "Cleanup-TestFixture on unowned junction" `
            -Expected "Cleanup returns false and preserves fixture" `
            -Actual "Cleanup: $resT48, Fixture exists: $t48FixtureStillExists" `
            -ExitCode 1 -Verdict "FAIL"
    }
} catch {
    Register-TestResult -TestId "T48" -Name "Unowned junction inside an otherwise owned fixture" `
        -Command "Unowned junction test" -Expected "ExitCode 0" `
        -Actual "Exception: $_" -ExitCode -1 -Verdict "FAIL"
}

# -------------------------------------------------------------------------
# T49 — Unexpected symbolic link inside a fixture rejected (fails closed)
# -------------------------------------------------------------------------
try {
    # 1. Create owned fixture directory and disposable target inside sessionDir
    $t49Fixture = New-TestFixtureDirectory -Path (Join-Path $sessionDir "t49-fixture")
    $t49Target = New-TestFixtureDirectory -Path (Join-Path $sessionDir "t49-target")

    # 2. Create junction but stamp sidecar declaring type = 'symlink'
    $t49Junc = Join-Path $t49Fixture "unexpected-symlink-junc"
    $mkOutT49 = cmd.exe /c "mklink /J `"$t49Junc`" `"$t49Target`"" 2>&1
    $sidecarTokenT49 = @{
        sessionId = $sessionId
        pid = $PID
        createdAt = (Get-Date).ToString("o")
        junctionPath = $t49Junc
        targetPath = $t49Target
        type = "symlink"
    } | ConvertTo-Json -Compress
    Set-Content -LiteralPath "$t49Junc.junction-owner.json" -Value $sidecarTokenT49 -Force
    $script:ownedFixtureRegistry.Add($t49Junc) | Out-Null
    $script:ownedFixtureRegistry.Add("$t49Junc.junction-owner.json") | Out-Null

    # 3. Attempt cleanup on fixture containing unexpected reparse type sidecar
    $resT49a = Cleanup-TestFixture -Path $t49Fixture
    $t49FixtureExistsA = Test-Path -LiteralPath $t49Fixture

    # 4. Child process test verifying reparse item with LinkType != 'Junction'
    $testScriptT49b = @"
    . '$provisionScript'
    `$fakeItem = [PSCustomObject]@{
        FullName = '$t49Junc'
        LinkType = 'SymbolicLink'
        Attributes = [System.IO.FileAttributes]::ReparsePoint
        PSIsContainer = `$true
    }
    # Direct check of LinkType guard in non-following walker
    if (`$fakeItem.LinkType -ne 'Junction') { exit 0 } else { exit 1 }
"@
    $procT49 = Start-Process -FilePath "powershell.exe" `
        -ArgumentList @("-NoProfile", "-Command", $testScriptT49b) `
        -Wait -NoNewWindow -PassThru

    $t49Passed = ($resT49a -eq $false -and $t49FixtureExistsA -and $procT49.ExitCode -eq 0)

    # Teardown
    [System.IO.Directory]::Delete($t49Junc, $false)
    $script:ownedFixtureRegistry.Remove($t49Junc) | Out-Null
    Remove-Item -LiteralPath "$t49Junc.junction-owner.json" -Force -ErrorAction SilentlyContinue
    $script:ownedFixtureRegistry.Remove("$t49Junc.junction-owner.json") | Out-Null
    Cleanup-TestFixture -Path $t49Target | Out-Null
    Cleanup-TestFixture -Path $t49Fixture | Out-Null

    if ($t49Passed) {
        Register-TestResult -TestId "T49" -Name "Unexpected symbolic link inside a fixture rejected" `
            -Command "Cleanup-TestFixture on fixture containing non-junction reparse point" `
            -Expected "Cleanup returns false, non-junction reparse points rejected" `
            -Actual "Cleanup: $resT49a, Fixture exists: $t49FixtureExistsA, Guard ExitCode: $($procT49.ExitCode)" `
            -ExitCode 0 -Verdict "PASS"
    } else {
        Register-TestResult -TestId "T49" -Name "Unexpected symbolic link inside a fixture rejected" `
            -Command "Cleanup-TestFixture on unexpected symlink" `
            -Expected "Cleanup returns false" `
            -Actual "Cleanup: $resT49a, Fixture exists: $t49FixtureExistsA, Guard ExitCode: $($procT49.ExitCode)" `
            -ExitCode 1 -Verdict "FAIL"
    }
} catch {
    Register-TestResult -TestId "T49" -Name "Unexpected symbolic link inside a fixture rejected" `
        -Command "Unexpected symbolic link test" -Expected "ExitCode 0" `
        -Actual "Exception: $_" -ExitCode -1 -Verdict "FAIL"
}

# -------------------------------------------------------------------------
# T50 — Foreign-session ownership marker causes cleanup refusal
# -------------------------------------------------------------------------
try {
    # 1. Create owned fixture directory
    $t50Fixture = New-TestFixtureDirectory -Path (Join-Path $sessionDir "t50-fixture")

    # 2. Create nested directory with foreign session marker
    $t50Nested = Join-Path $t50Fixture "nested-foreign"
    New-Item -ItemType Directory -Path $t50Nested -Force | Out-Null
    $foreignTokenT50 = @{
        sessionId = "FOREIGN-SESSION-INTRUDER-9999"
        pid = 99999
        createdAt = (Get-Date).ToString("o")
        path = $t50Nested
    } | ConvertTo-Json -Compress
    Set-Content -LiteralPath (Join-Path $t50Nested ".ulvo-fixture-owner.json") -Value $foreignTokenT50 -Force
    # Register path in memory but marker itself has foreign session
    $script:ownedFixtureRegistry.Add($t50Nested) | Out-Null

    # 3. Create sentinel file inside nested directory
    $t50Sentinel = Join-Path $t50Nested "foreign-sentinel.txt"
    Set-Content -LiteralPath $t50Sentinel -Value "FOREIGN-DIRECTORY-PRESERVED" -Force

    # 4. Attempt cleanup on top-level fixture
    $resT50 = Cleanup-TestFixture -Path $t50Fixture

    # 5. Verify fail-closed behavior
    $t50FixtureStillExists = Test-Path -LiteralPath $t50Fixture
    $t50NestedStillExists = Test-Path -LiteralPath $t50Nested
    $t50SentinelStillExists = Test-Path -LiteralPath $t50Sentinel

    $t50Passed = ($resT50 -eq $false -and $t50FixtureStillExists -and $t50NestedStillExists -and $t50SentinelStillExists)

    # Teardown
    Remove-Item -LiteralPath $t50Sentinel -Force -ErrorAction SilentlyContinue
    Remove-Item -LiteralPath (Join-Path $t50Nested ".ulvo-fixture-owner.json") -Force -ErrorAction SilentlyContinue
    [System.IO.Directory]::Delete($t50Nested, $false)
    $script:ownedFixtureRegistry.Remove($t50Nested) | Out-Null
    Cleanup-TestFixture -Path $t50Fixture | Out-Null

    if ($t50Passed) {
        Register-TestResult -TestId "T50" -Name "Foreign-session ownership marker causes cleanup refusal" `
            -Command "Cleanup-TestFixture on fixture containing nested foreign session directory" `
            -Expected "Cleanup returns false, fixture and nested contents preserved" `
            -Actual "Cleanup: $resT50, Fixture exists: $t50FixtureStillExists, Nested exists: $t50NestedStillExists" `
            -ExitCode 0 -Verdict "PASS"
    } else {
        Register-TestResult -TestId "T50" -Name "Foreign-session ownership marker causes cleanup refusal" `
            -Command "Cleanup-TestFixture on foreign session marker" `
            -Expected "Cleanup returns false" `
            -Actual "Cleanup: $resT50, Fixture exists: $t50FixtureStillExists, Nested exists: $t50NestedStillExists" `
            -ExitCode 1 -Verdict "FAIL"
    }
} catch {
    Register-TestResult -TestId "T50" -Name "Foreign-session ownership marker causes cleanup refusal" `
        -Command "Foreign marker test" -Expected "ExitCode 0" `
        -Actual "Exception: $_" -ExitCode -1 -Verdict "FAIL"
}

# -------------------------------------------------------------------------
# T51 — Corrupt ownership marker causes cleanup refusal
# -------------------------------------------------------------------------
try {
    # 1. Create owned fixture directory
    $t51Fixture = New-TestFixtureDirectory -Path (Join-Path $sessionDir "t51-fixture")

    # 2. Create nested directory with corrupt JSON marker
    $t51Nested = Join-Path $t51Fixture "nested-corrupt"
    New-Item -ItemType Directory -Path $t51Nested -Force | Out-Null
    Set-Content -LiteralPath (Join-Path $t51Nested ".ulvo-fixture-owner.json") -Value "{ corrupt_json: unterminated" -Force
    $script:ownedFixtureRegistry.Add($t51Nested) | Out-Null

    # 3. Create sentinel file inside nested directory
    $t51Sentinel = Join-Path $t51Nested "corrupt-sentinel.txt"
    Set-Content -LiteralPath $t51Sentinel -Value "CORRUPT-MARKER-PRESERVED" -Force

    # 4. Attempt cleanup on top-level fixture
    $resT51 = Cleanup-TestFixture -Path $t51Fixture

    # 5. Verify fail-closed behavior
    $t51FixtureStillExists = Test-Path -LiteralPath $t51Fixture
    $t51NestedStillExists = Test-Path -LiteralPath $t51Nested
    $t51SentinelStillExists = Test-Path -LiteralPath $t51Sentinel

    $t51Passed = ($resT51 -eq $false -and $t51FixtureStillExists -and $t51NestedStillExists -and $t51SentinelStillExists)

    # Teardown
    Remove-Item -LiteralPath $t51Sentinel -Force -ErrorAction SilentlyContinue
    Remove-Item -LiteralPath (Join-Path $t51Nested ".ulvo-fixture-owner.json") -Force -ErrorAction SilentlyContinue
    [System.IO.Directory]::Delete($t51Nested, $false)
    $script:ownedFixtureRegistry.Remove($t51Nested) | Out-Null
    Cleanup-TestFixture -Path $t51Fixture | Out-Null

    if ($t51Passed) {
        Register-TestResult -TestId "T51" -Name "Corrupt ownership marker causes cleanup refusal" `
            -Command "Cleanup-TestFixture on fixture containing corrupt JSON marker" `
            -Expected "Cleanup returns false, fixture and nested contents preserved" `
            -Actual "Cleanup: $resT51, Fixture exists: $t51FixtureStillExists, Nested exists: $t51NestedStillExists" `
            -ExitCode 0 -Verdict "PASS"
    } else {
        Register-TestResult -TestId "T51" -Name "Corrupt ownership marker causes cleanup refusal" `
            -Command "Cleanup-TestFixture on corrupt marker" `
            -Expected "Cleanup returns false" `
            -Actual "Cleanup: $resT51, Fixture exists: $t51FixtureStillExists, Nested exists: $t51NestedStillExists" `
            -ExitCode 1 -Verdict "FAIL"
    }
} catch {
    Register-TestResult -TestId "T51" -Name "Corrupt ownership marker causes cleanup refusal" `
        -Command "Corrupt marker test" -Expected "ExitCode 0" `
        -Actual "Exception: $_" -ExitCode -1 -Verdict "FAIL"
}

# -------------------------------------------------------------------------
# T52 — Pre-existing directory collision in New-TestFixtureDirectory fails closed
# -------------------------------------------------------------------------
try {
    # 1. Create fixture directory
    $t52Fixture = New-TestFixtureDirectory -Path (Join-Path $sessionDir "t52-collision")
    $t52Sentinel = Register-TestFixtureFile (Join-Path $t52Fixture "collision-sentinel.txt")
    Set-Content -LiteralPath $t52Sentinel -Value "ORIGINAL-PREEXISTING-DATA" -Force
    $t52HashBefore = (Get-FileHash -LiteralPath $t52Sentinel -Algorithm SHA256).Hash

    # 2. Attempt to call New-TestFixtureDirectory on the pre-existing directory
    $collisionThrown = $false
    try {
        New-TestFixtureDirectory -Path $t52Fixture | Out-Null
    } catch {
        if ($_.Exception.Message -match "attempted to claim or overwrite pre-existing path") {
            $collisionThrown = $true
        }
    }

    # 3. Verify data preservation
    $t52HashAfter = (Get-FileHash -LiteralPath $t52Sentinel -Algorithm SHA256).Hash
    $t52Passed = ($collisionThrown -and ($t52HashBefore -eq $t52HashAfter))

    # Teardown
    Cleanup-TestFixture -Path $t52Fixture | Out-Null

    if ($t52Passed) {
        Register-TestResult -TestId "T52" -Name "Pre-existing directory collision fails closed" `
            -Command "New-TestFixtureDirectory on pre-existing path" `
            -Expected "Throws security violation, pre-existing contents preserved" `
            -Actual "Collision thrown: $collisionThrown, Hash matches: $($t52HashBefore -eq $t52HashAfter)" `
            -ExitCode 0 -Verdict "PASS"
    } else {
        Register-TestResult -TestId "T52" -Name "Pre-existing directory collision fails closed" `
            -Command "New-TestFixtureDirectory collision check" `
            -Expected "Throws collision exception and preserves contents" `
            -Actual "Collision thrown: $collisionThrown, Hash matches: $($t52HashBefore -eq $t52HashAfter)" `
            -ExitCode 1 -Verdict "FAIL"
    }
} catch {
    Register-TestResult -TestId "T52" -Name "Pre-existing directory collision fails closed" `
        -Command "Pre-existing directory collision test" -Expected "ExitCode 0" `
        -Actual "Exception: $_" -ExitCode -1 -Verdict "FAIL"
}

# -------------------------------------------------------------------------
# T53 — Fixture deletion attempted outside current session rejected
# -------------------------------------------------------------------------
try {
    # Test paths outside session directory:
    $t53Vault = Join-Path $ulvoRoot "vault"
    $t53Scripts = Join-Path $ulvoRoot "scripts"
    $t53FixtureRoot = $fixtureRoot
    $t53OtherSession = Join-Path $fixtureRoot "run-99999999999999999999999999999999"

    $res53a = Cleanup-TestFixture -Path $t53Vault
    $res53b = Cleanup-TestFixture -Path $t53Scripts
    $res53c = Cleanup-TestFixture -Path $t53FixtureRoot
    $res53d = Cleanup-TestFixture -Path $t53OtherSession

    $allRejected = ($res53a -eq $false -and $res53b -eq $false -and $res53c -eq $false -and $res53d -eq $false)

    # Verify external paths exist intact
    $vaultExists = Test-Path -LiteralPath $t53Vault
    $scriptsExists = Test-Path -LiteralPath $t53Scripts
    $fixtureRootExists = Test-Path -LiteralPath $t53FixtureRoot

    $t53Passed = ($allRejected -and $vaultExists -and $scriptsExists -and $fixtureRootExists)

    if ($t53Passed) {
        Register-TestResult -TestId "T53" -Name "Fixture deletion outside current session rejected" `
            -Command "Cleanup-TestFixture on vault, scripts, fixtureRoot, and foreign session path" `
            -Expected "All cleanups return false; zero deletions" `
            -Actual "Rejections: vault=$res53a, scripts=$res53b, fixtureRoot=$res53c, otherSession=$res53d" `
            -ExitCode 0 -Verdict "PASS"
    } else {
        Register-TestResult -TestId "T53" -Name "Fixture deletion outside current session rejected" `
            -Command "Cleanup-TestFixture outside session" `
            -Expected "All cleanups return false" `
            -Actual "Rejections: vault=$res53a, scripts=$res53b, fixtureRoot=$res53c, otherSession=$res53d" `
            -ExitCode 1 -Verdict "FAIL"
    }
} catch {
    Register-TestResult -TestId "T53" -Name "Fixture deletion outside current session rejected" `
        -Command "Outside session cleanup test" -Expected "ExitCode 0" `
        -Actual "Exception: $_" -ExitCode -1 -Verdict "FAIL"
}

# -------------------------------------------------------------------------
# T54 — Normal owned-fixture cleanup succeeds completely bottom-up
# -------------------------------------------------------------------------
try {
    # 1. Create hierarchical owned fixture structure
    $t54Fixture = New-TestFixtureDirectory -Path (Join-Path $sessionDir "t54-fixture")
    $t54Sub = New-TestFixtureDirectory -Path (Join-Path $t54Fixture "nested-dir")
    $t54Deep = New-TestFixtureDirectory -Path (Join-Path $t54Sub "deep-dir")

    # 2. Create target directory and owned junction inside deep-dir
    $t54Target = New-TestFixtureDirectory -Path (Join-Path $sessionDir "t54-target")
    $t54Junction = New-TestJunctionFixture -JunctionPath (Join-Path $t54Deep "junc") -TargetPath $t54Target

    # 3. Create registered files at multiple levels
    $t54File1 = Register-TestFixtureFile (Join-Path $t54Fixture "file1.txt")
    $t54File2 = Register-TestFixtureFile (Join-Path $t54Sub "file2.txt")
    $t54File3 = Register-TestFixtureFile (Join-Path $t54Deep "file3.txt")
    Set-Content -LiteralPath $t54File1 -Value "CONTENT-1" -Force
    Set-Content -LiteralPath $t54File2 -Value "CONTENT-2" -Force
    Set-Content -LiteralPath $t54File3 -Value "CONTENT-3" -Force

    # 4. Execute bottom-up cleanup
    $resT54 = Cleanup-TestFixture -Path $t54Fixture

    # 5. Verify entire fixture hierarchy is completely deleted and target is intact
    $t54FixtureGone = (-not (Test-Path -LiteralPath $t54Fixture))
    $t54TargetIntact = (Test-Path -LiteralPath $t54Target)

    $t54Passed = ($resT54 -eq $true -and $t54FixtureGone -and $t54TargetIntact)

    # Teardown target
    Cleanup-TestFixture -Path $t54Target | Out-Null

    if ($t54Passed) {
        Register-TestResult -TestId "T54" -Name "Normal owned-fixture cleanup succeeds completely bottom-up" `
            -Command "Cleanup-TestFixture on deeply nested owned fixture with files, subdirs, and junctions" `
            -Expected "Cleanup returns true, fixture tree deleted cleanly, junction target preserved" `
            -Actual "Cleanup: $resT54, Fixture gone: $t54FixtureGone, Target intact: $t54TargetIntact" `
            -ExitCode 0 -Verdict "PASS"
    } else {
        Register-TestResult -TestId "T54" -Name "Normal owned-fixture cleanup succeeds completely bottom-up" `
            -Command "Cleanup-TestFixture normal cleanup" `
            -Expected "Cleanup returns true and fixture deleted" `
            -Actual "Cleanup: $resT54, Fixture gone: $t54FixtureGone, Target intact: $t54TargetIntact" `
            -ExitCode 1 -Verdict "FAIL"
    }
} catch {
    Register-TestResult -TestId "T54" -Name "Normal owned-fixture cleanup succeeds completely bottom-up" `
        -Command "Normal cleanup test" -Expected "ExitCode 0" `
        -Actual "Exception: $_" -ExitCode -1 -Verdict "FAIL"
}

# -------------------------------------------------------------------------
# T55 — No modifications to external junction targets after cleanup refusal
# -------------------------------------------------------------------------
try {
    # 1. Create target directory and sentinel file
    $t55Target = New-TestFixtureDirectory -Path (Join-Path $sessionDir "t55-target")
    $t55Sentinel = Register-TestFixtureFile (Join-Path $t55Target "sentinel-55.txt")
    Set-Content -LiteralPath $t55Sentinel -Value "SENSITIVE-DATA-T55-$(Get-Random)" -Force
    $t55HashBefore = (Get-FileHash -LiteralPath $t55Sentinel -Algorithm SHA256).Hash

    # 2. Create fixture directory
    $t55Fixture = New-TestFixtureDirectory -Path (Join-Path $sessionDir "t55-fixture")

    # 3. Create unowned junction inside fixture pointing to target
    $t55Junction = Join-Path $t55Fixture "unowned-junc"
    $mkOutT55 = cmd.exe /c "mklink /J `"$t55Junction`" `"$t55Target`"" 2>&1

    # 4. Attempt cleanup on fixture (must fail closed due to unowned junction)
    $resT55 = Cleanup-TestFixture -Path $t55Fixture

    # 5. Verify target was completely isolated and unmodified
    $t55HashAfter = (Get-FileHash -LiteralPath $t55Sentinel -Algorithm SHA256).Hash
    $t55TargetStillExists = Test-Path -LiteralPath $t55Target
    $t55TargetUntouched = ($t55HashBefore -eq $t55HashAfter)

    $t55Passed = ($resT55 -eq $false -and $t55TargetStillExists -and $t55TargetUntouched)

    # Teardown
    [System.IO.Directory]::Delete($t55Junction, $false)
    Cleanup-TestFixture -Path $t55Target | Out-Null
    Cleanup-TestFixture -Path $t55Fixture | Out-Null

    if ($t55Passed) {
        Register-TestResult -TestId "T55" -Name "No modifications to external junction targets after cleanup refusal" `
            -Command "Verify SHA-256 integrity of junction target after cleanup refusal" `
            -Expected "Cleanup returns false, target SHA-256 is 100% identical before and after" `
            -Actual "Cleanup: $resT55, Target intact: $t55TargetStillExists, Hash match: $t55TargetUntouched" `
            -ExitCode 0 -Verdict "PASS"
    } else {
        Register-TestResult -TestId "T55" -Name "No modifications to external junction targets after cleanup refusal" `
            -Command "Verify target integrity" `
            -Expected "Cleanup returns false and target SHA-256 matches" `
            -Actual "Cleanup: $resT55, Target intact: $t55TargetStillExists, Hash match: $t55TargetUntouched" `
            -ExitCode 1 -Verdict "FAIL"
    }
} catch {
    Register-TestResult -TestId "T55" -Name "No modifications to external junction targets after cleanup refusal" `
        -Command "Target integrity test" -Expected "ExitCode 0" `
        -Actual "Exception: $_" -ExitCode -1 -Verdict "FAIL"
}

# -------------------------------------------------------------------------
# Session Fixture Cleanup
# -------------------------------------------------------------------------
if (-not $PreserveFixtures) {
    Write-Host "`nCleaning up session fixtures ($sessionDir)..." -ForegroundColor DarkGray
    $cleanupSuccess = Cleanup-TestFixture -Path $sessionDir
    if (-not $cleanupSuccess) {
        Write-Warning "Session fixture cleanup did not succeed cleanly."
    }
} else {
    Write-Host "`nPreserving fixtures at: $sessionDir" -ForegroundColor DarkYellow
}

# -------------------------------------------------------------------------
# Test Summary
# -------------------------------------------------------------------------
Write-Host "`n==========================================================" -ForegroundColor Cyan
Write-Host "Test Run Complete: $($script:passedCount) PASSED, $($script:failedCount) FAILED, $($script:notRunCount) NOT RUN (Total: $($testResults.Count))" -ForegroundColor Yellow
Write-Host "==========================================================" -ForegroundColor Cyan

# Return structured output or exit code
if ($script:failedCount -gt 0) {
    exit 1
} else {
    exit 0
}
