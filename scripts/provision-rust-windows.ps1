<#PSScriptInfo

.VERSION 1.1

.GUID
b8e8e8e8-e8e8-e8e8-e8e8-e8e8e8e8e8e8

.AUTHOR
ULVO Development Team

.COMPANYNAME

.COPYRIGHT

.TAGS
ULVO, Rust, Windows, Provisioning, Hardened

.LICENSEURI

.PROJECTURI

.ICONURI

.EXTERNALMODULEDEPENDENCIES

.REQUIREDSCRIPTS

.EXTERNALSCRIPTDEPENDENCIES

.RELEASENOTES
ULVO Windows Rust Toolchain Provisioning Script - Hardened Edition
#>

<#
.SYNOPSIS
    Provisions a Windows-native Rust toolchain under ULVO's vault architecture.

.DESCRIPTION
    This script provisions a Rust toolchain (rustc, cargo, rustup) under
    E:\Workspace\Active\ULVO\vault\runtimes\rust\ for Windows-native builds.
    
    The script enforces:
    - Dry-run mode (guaranteed non-mutating: no downloads, writes, or installations)
    - Strict path containment within E:\Workspace\Active\ULVO\vault\runtimes\rust\
    - Reparse point (junction/symlink) rejection across the path hierarchy
    - Sibling-prefix and traversal injection rejection
    - Non-overlapping, non-nested RUSTUP_HOME and CARGO_HOME validation
    - Parameter format allowlists against command injection
    - Dual-aspect installer provenance verification (Authenticode + official source)
    - Subprocess execution isolation with full environment restoration
    - In-memory standard output and error capture (no disk leakage or file overwrite)
    - Full PowerShell 5.1 compatibility
    
    SAFETY RESTRICTIONS:
    - Does NOT modify global PATH, registry, or ACLs
    - Does NOT install Rust without explicit user approval
    - Does NOT download or execute rustup-init.exe in dry-run mode
    - Does NOT touch existing C:\msys64 or WSL installations
    - Does NOT modify existing ULVO experiments (E:\ULVO_TEST, E:\WVB-Lab)
    - All custom paths must remain strictly inside the approved Rust vault area
    - Fail-closed security design on any ambiguous or inaccessible metadata

.PARAMETER DryRun
    If specified, performs path and configuration validation only.
    Guaranteed zero writes, zero downloads, zero executions.

.PARAMETER RustVersion
    Rust toolchain version to install (default: 1.96.0). Must match SemVer or channel.

.PARAMETER Target
    Target triple (default: x86_64-pc-windows-gnu). Must match alphanumeric triple pattern.

.PARAMETER DownloadUrl
    URL for rustup-init.exe (default: https://win.rustup.rs/x86_64).
    Must use HTTPS and originate from win.rustup.rs or static.rust-lang.org.

.PARAMETER RustupHome
    RUSTUP_HOME directory (default: $approvedRustArea\rustup).
    Must be strictly inside the approved Rust vault area and cannot overlap CARGO_HOME.

.PARAMETER CargoHome
    CARGO_HOME directory (default: $approvedRustArea\cargo).
    Must be strictly inside the approved Rust vault area and cannot overlap RUSTUP_HOME.

.PARAMETER UlvoRoot
    ULVO repository root (default: E:\Workspace\Active\ULVO).
    Restricted to repository root or disposable test fixture paths under tests\tmp.

.OUTPUTS
    Writes diagnostic output to stdout.
    Exits with code 0 on success, non-zero on any validation or installation failure.
#>

[CmdletBinding()]
param(
    [Switch]
    $DryRun,

    [string]
    $RustVersion = "1.96.0",

    [string]
    $Target = "x86_64-pc-windows-gnu",

    [string]
    $DownloadUrl,

    [string]
    $RustupHome,

    [string]
    $CargoHome,

    [string]
    $UlvoRoot = "E:\Workspace\Active\ULVO",

    [Switch]
    $Install,

    [string]
    $ExpectedSha256,

    [string]
    $ManifestPath,

    [string]
    $InstallerVersion = "1.29.0"
)

#region Security and Diagnostic Helpers

function Write-Diagnostic {
    param([string]$Message, [string]$Prefix = "[DIAG]")
    Write-Host "$Prefix $Message"
}

function Write-ErrorExit {
    param([string]$Message, [int]$ExitCode = 1)
    Write-Error "$Message"
    exit $ExitCode
}

function Test-ReparsePoint {
    param([string]$Path)
    try {
        if (-not (Test-Path -LiteralPath $Path)) {
            return $false
        }
        $item = Get-Item -LiteralPath $Path -Force -ErrorAction Stop
        if ($item.Attributes -band [System.IO.FileAttributes]::ReparsePoint) {
            return $true
        }
        return $false
    } catch {
        # Treat inaccessible metadata as an error (fail-closed)
        Write-Diagnostic "Cannot access path metadata for reparse inspection: $Path ($($_.Exception.Message))"
        return $true
    }
}

function Test-SafePathContainment {
    <#
    .SYNOPSIS
        Validates that a path is an absolute, non-traversing descendant of an approved root.
    .DESCRIPTION
        Enforces:
        - Non-empty, valid path characters
        - No alternate data streams (no colons beyond drive specifier)
        - Absolute, rooted drive path (^[a-zA-Z]:[\\/])
        - No relative, drive-relative, UNC, extended-length (\\?\), or device (\\.\) paths
        - Rejection of '.' and '..' traversal segments
        - Strict canonical containment within ApprovedArea with trailing boundary (no sibling collisions)
        - Rejection of the ApprovedArea root itself
        - Reparse point (junction/symlink) detection on all existing ancestor components
        - Rejection of existing files where directories are required
        - Fail-closed on inaccessible metadata
    #>
    param(
        [string]$Path,
        [string]$ApprovedArea,
        [string]$ParameterName
    )

    if ([string]::IsNullOrWhiteSpace($Path)) {
        Write-ErrorExit "$ParameterName cannot be null or empty."
    }

    # 1. Invalid path characters
    $invalidChars = [System.IO.Path]::GetInvalidPathChars()
    if ($Path.IndexOfAny($invalidChars) -ge 0) {
        Write-ErrorExit "$ParameterName contains invalid path characters: '$Path'"
    }

    # 2. Reject UNC paths (\\server\share), extended-length (\\?\), and device paths (\\.\)
    if ($Path.StartsWith('\\')) {
        Write-ErrorExit "$ParameterName cannot be a UNC, extended-length, or device path: '$Path'"
    }

    # 3. Reject Alternate Data Streams (colon after drive specifier)
    if ($Path.Length -gt 2) {
        $remaining = $Path.Substring(2)
        if ($remaining.Contains(':')) {
            Write-ErrorExit "$ParameterName cannot contain alternate data streams: '$Path'"
        }
    }

    # 4. Require absolute, rooted drive path (e.g. E:\... or E:/...)
    # Rejects relative paths (vault\rust) and drive-relative paths (E:vault\rust)
    if ($Path -notmatch '^[a-zA-Z]:[\\/]') {
        Write-ErrorExit "$ParameterName must be an absolute, rooted drive path (e.g. E:\...): '$Path'"
    }

    # 5. Reject explicit traversal tokens ('.' or '..')
    $normalizedSeparators = $Path.Replace('/', '\')
    $segments = $normalizedSeparators.Split('\')
    foreach ($seg in $segments) {
        if ($seg -eq '.' -or $seg -eq '..') {
            Write-ErrorExit "$ParameterName contains traversal components ('.' or '..'): '$Path'"
        }
    }

    # 6. Canonicalize path
    try {
        $canonicalPath = [System.IO.Path]::GetFullPath($Path).TrimEnd('\')
        $canonicalApproved = [System.IO.Path]::GetFullPath($ApprovedArea).TrimEnd('\')
    } catch {
        Write-ErrorExit "Failed to canonicalize path for $ParameterName : $_"
    }

    # 7. Sibling prefix safety & strict approved area containment
    $approvedBoundary = $canonicalApproved + '\'
    
    # Reject the approved root itself (must be a descendant)
    if ($canonicalPath.Equals($canonicalApproved, [System.StringComparison]::OrdinalIgnoreCase)) {
        Write-ErrorExit "$ParameterName cannot be the approved root directory itself ($ApprovedArea); it must be a subdirectory within it."
    }

    # Must start with approvedBoundary (case-insensitive)
    if (-not $canonicalPath.StartsWith($approvedBoundary, [System.StringComparison]::OrdinalIgnoreCase)) {
        Write-ErrorExit "$ParameterName must be inside the approved Rust resource area: '$canonicalApproved'. Provided: '$canonicalPath'"
    }

    # 8. Reparse point and ancestor inspection on existing hierarchy
    $components = @()
    $curr = $canonicalPath
    $driveRoot = [System.IO.Path]::GetPathRoot($curr)
    while ($curr -and $curr -ne $driveRoot) {
        $components = @($curr) + $components
        $curr = [System.IO.Path]::GetDirectoryName($curr)
    }

    foreach ($comp in $components) {
        $compExists = $false
        try {
            $compExists = Test-Path -LiteralPath $comp -ErrorAction Stop
        } catch {
            Write-ErrorExit "Cannot access metadata for existing path component: '$comp'. Failing closed."
        }

        if ($compExists) {
            try {
                $item = Get-Item -LiteralPath $comp -Force -ErrorAction Stop
            } catch {
                Write-ErrorExit "Cannot access metadata for existing path component: '$comp'. Failing closed."
            }

            if ($item.Attributes -band [System.IO.FileAttributes]::ReparsePoint) {
                Write-ErrorExit "Reparse point (symlink/junction) detected in path hierarchy at: '$comp'"
            }

            if (-not ($item.Attributes -band [System.IO.FileAttributes]::Directory)) {
                Write-ErrorExit "Expected directory but found file at path component: '$comp'"
            }

            # Verify that canonical path has not been replaced or diverted
            try {
                $actualCanonical = [System.IO.Path]::GetFullPath($item.FullName).TrimEnd('\')
                $compCanonical = [System.IO.Path]::GetFullPath($comp).TrimEnd('\')
                if (-not $actualCanonical.Equals($compCanonical, [System.StringComparison]::OrdinalIgnoreCase)) {
                    Write-ErrorExit "Path identity mismatch or path replacement detected for component: '$comp' (resolved: '$actualCanonical'). Failing closed."
                }
            } catch {
                Write-ErrorExit "Failed to verify canonical consistency for component '$comp': $($_.Exception.Message). Failing closed."
            }
        }
    }

    return $canonicalPath
}

function Validate-InstallationPaths {
    param(
        [string]$RustupHome,
        [string]$CargoHome,
        [string]$ApprovedArea
    )

    $canonicalRustup = Test-SafePathContainment -Path $RustupHome -ApprovedArea $ApprovedArea -ParameterName "RUSTUP_HOME"
    $canonicalCargo = Test-SafePathContainment -Path $CargoHome -ApprovedArea $ApprovedArea -ParameterName "CARGO_HOME"

    # Reject identical paths
    if ($canonicalRustup.Equals($canonicalCargo, [System.StringComparison]::OrdinalIgnoreCase)) {
        Write-ErrorExit "RUSTUP_HOME and CARGO_HOME must be different paths: '$canonicalRustup'"
    }

    # Reject nested paths (CARGO_HOME inside RUSTUP_HOME or vice-versa)
    $rustupBoundary = $canonicalRustup + '\'
    $cargoBoundary = $canonicalCargo + '\'

    if ($canonicalCargo.StartsWith($rustupBoundary, [System.StringComparison]::OrdinalIgnoreCase)) {
        Write-ErrorExit "CARGO_HOME cannot be nested under RUSTUP_HOME: '$canonicalCargo' under '$canonicalRustup'"
    }

    if ($canonicalRustup.StartsWith($cargoBoundary, [System.StringComparison]::OrdinalIgnoreCase)) {
        Write-ErrorExit "RUSTUP_HOME cannot be nested under CARGO_HOME: '$canonicalRustup' under '$canonicalCargo'"
    }

    # Reject pre-existing destination directories
    if (Test-Path -LiteralPath $canonicalRustup) {
        Write-ErrorExit "RUSTUP_HOME already exists: '$canonicalRustup'"
    }

    if (Test-Path -LiteralPath $canonicalCargo) {
        Write-ErrorExit "CARGO_HOME already exists: '$canonicalCargo'"
    }
}

function Get-FreeSpaceGB {
    param([string]$Drive)
    try {
        $volume = Get-Volume | Where-Object { $_.DriveLetter -eq $Drive }
        if ($volume -and $volume.SizeRemaining -gt 0) {
            $gb = $volume.SizeRemaining / 1GB
            return [math]::Round($gb, 2)
        }

        $driveObj = Get-CimInstance Win32_LogicalDisk -Filter "DeviceID='${Drive}:'" -ErrorAction Stop
        if ($driveObj -and $driveObj.FreeSpace -gt 0) {
            $gb = $driveObj.FreeSpace / 1GB
            return [math]::Round($gb, 2)
        }

        Write-Diagnostic "FreeSpace query returned zero or null for drive $Drive"
        return -1
    } catch {
        Write-Diagnostic "Drive free space query failed: $_"
        return -1
    }
}

function Test-AuthenticodeSignature {
    param([string]$FilePath)

    # Trust model documentation:
    # 1. The binary must be Authenticode-signed and trusted by the Windows certificate store.
    # 2. Publisher identity must strictly match CN=Rust Foundation and O=Rust Foundation.
    # 3. Known limitations:
    #    - Authenticode does not pin specific release versions.
    #    - Upstream official distribution provides authoritative SHA256 checksums alongside
    #      every release at https://static.rust-lang.org/rustup/dist/<target>/rustup-init.exe.sha256.
    #    - Fails closed on untrusted, corrupted, or unsigned binaries.

    try {
        if (-not (Test-Path -LiteralPath $FilePath)) {
            Write-Diagnostic "Installer file does not exist: $FilePath"
            return $false
        }

        $signature = Get-AuthenticodeSignature -FilePath $FilePath -ErrorAction Stop

        if ($signature.Status -ne "Valid") {
            Write-Diagnostic "Authenticode signature status: $($signature.Status)"
            Write-Diagnostic "Status message: $($signature.StatusMessage)"
            return $false
        }

        $subject = $signature.SignerCertificate.Subject
        if ($subject -notmatch 'CN=Rust Foundation' -or $subject -notmatch 'O=Rust Foundation') {
            Write-Diagnostic "Untrusted publisher subject: $subject"
            return $false
        }

        Write-Diagnostic "Authenticode signature verified."
        Write-Diagnostic "  Publisher  : $subject"
        Write-Diagnostic "  Thumbprint : $($signature.SignerCertificate.Thumbprint)"
        return $true
    } catch {
        Write-Diagnostic "Authenticode verification exception: $_"
        return $false
    }
}

function Get-ToolchainManifest {
    <#
    .SYNOPSIS
        Parses and validates a toolchain artifact manifest (TOML format) in pure PowerShell 5.1.
    .DESCRIPTION
        Enforces schema version 1.0.0, required artifact fields, 64-character lowercase SHA-256 digests,
        and allowed verification policies and approval statuses.
        Rejects duplicate keys, duplicate artifact identities, and malformed syntax.
    #>
    param([string]$FilePath)

    if (-not (Test-Path -LiteralPath $FilePath)) {
        throw "Toolchain manifest not found at: '$FilePath'"
    }

    $lines = @(Get-Content -LiteralPath $FilePath -ErrorAction Stop)
    $manifest = @{
        artifacts = [System.Collections.ArrayList]::new()
    }

    $currentSection = $null
    $currentArtifact = $null
    $inArray = $false
    $arrayKey = $null
    $arrayValues = [System.Collections.ArrayList]::new()

    for ($i = 0; $i -lt $lines.Count; $i++) {
        $line = $lines[$i].Trim()
        if ([string]::IsNullOrWhiteSpace($line) -or $line.StartsWith("#")) {
            continue
        }

        # Multiline array continuation
        if ($inArray) {
            if ($line.EndsWith("]")) {
                $inArray = $false
                $val = $line.TrimEnd(']').Trim()
                if ($val) {
                    $valClean = $val.Trim('"', "'", ' ', ',')
                    if ($valClean) { [void]$arrayValues.Add($valClean) }
                }
                if ($currentArtifact) {
                    $currentArtifact[$arrayKey] = @($arrayValues)
                }
                $arrayValues = [System.Collections.ArrayList]::new()
                $arrayKey = $null
            } else {
                $valClean = $line.Trim('"', "'", ' ', ',')
                if ($valClean) { [void]$arrayValues.Add($valClean) }
            }
            continue
        }

        # Array of tables: [[artifacts]]
        if ($line -eq "[[artifacts]]") {
            $currentArtifact = [ordered]@{}
            [void]$manifest.artifacts.Add($currentArtifact)
            $currentSection = "artifact"
            continue
        }

        # Table header [section]
        if ($line -match '^\[([a-zA-Z0-9_.-]+)\]$') {
            $currentSection = $matches[1]
            continue
        }

        # Key = Value
        if ($line -match '^([a-zA-Z0-9_]+)\s*=\s*(.*)$') {
            $key = $matches[1].Trim()
            $rawVal = $matches[2].Trim()

            # Multiline array start: key = [
            if ($rawVal.StartsWith("[") -and -not $rawVal.EndsWith("]")) {
                $inArray = $true
                $arrayKey = $key
                $arrayValues = [System.Collections.ArrayList]::new()
                $inside = $rawVal.Substring(1).Trim()
                if ($inside) {
                    $valClean = $inside.Trim('"', "'", ' ', ',')
                    if ($valClean) { [void]$arrayValues.Add($valClean) }
                }
                continue
            }

            # Inline array: [ "a", "b" ]
            if ($rawVal.StartsWith("[") -and $rawVal.EndsWith("]")) {
                $content = $rawVal.Substring(1, $rawVal.Length - 2).Trim()
                $items = @()
                if ($content) {
                    $split = $content.Split(',')
                    foreach ($s in $split) {
                        $cleaned = $s.Trim().Trim('"', "'")
                        if ($cleaned) { $items += $cleaned }
                    }
                }
                $parsedVal = $items
            }
            # Quoted string
            elseif ($rawVal.StartsWith('"') -and $rawVal.EndsWith('"') -and $rawVal.Length -ge 2) {
                $parsedVal = $rawVal.Substring(1, $rawVal.Length - 2)
            }
            elseif ($rawVal.StartsWith("'") -and $rawVal.EndsWith("'") -and $rawVal.Length -ge 2) {
                $parsedVal = $rawVal.Substring(1, $rawVal.Length - 2)
            }
            # Booleans and numbers
            elseif ($rawVal -eq "true") { $parsedVal = $true }
            elseif ($rawVal -eq "false") { $parsedVal = $false }
            elseif ($rawVal -match '^\d+$') { $parsedVal = [int64]$rawVal }
            else {
                throw "Invalid TOML syntax in manifest at line $($i + 1): '$line'"
            }

            if ($currentSection -eq "artifact" -and $currentArtifact -ne $null) {
                if ($currentArtifact.Contains($key)) {
                    throw "Invalid TOML syntax in manifest at line $($i + 1): Duplicate key '$key' detected in artifact table."
                }
                $currentArtifact[$key] = $parsedVal
            } else {
                if ($manifest.ContainsKey($key)) {
                    throw "Invalid TOML syntax in manifest at line $($i + 1): Duplicate root key '$key' detected in manifest."
                }
                $manifest[$key] = $parsedVal
            }
            continue
        }

        throw "Invalid TOML syntax in manifest at line $($i + 1): '$line'"
    }

    # Schema version check
    if (-not $manifest.ContainsKey("schema_version") -or -not $manifest["schema_version"]) {
        throw "Manifest missing required 'schema_version'."
    }
    if ($manifest["schema_version"] -ne "1.0.0") {
        throw "Unsupported manifest schema version: '$($manifest["schema_version"])'. Supported version is '1.0.0'."
    }

    # Validate each artifact
    if ($manifest.artifacts.Count -eq 0) {
        throw "Manifest contains no artifacts."
    }

    $seenIdentities = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::OrdinalIgnoreCase)

    foreach ($art in $manifest.artifacts) {
        $requiredFields = @("artifact_name", "installer_version", "platform", "target", "canonical_url", "expected_sha256", "verification_policy", "approval_status")
        foreach ($rf in $requiredFields) {
            if (-not $art.Contains($rf) -or [string]::IsNullOrWhiteSpace($art[$rf])) {
                if ($rf -eq "expected_sha256") {
                    throw "Artifact '$($art.artifact_name)' is missing required 'expected_sha256' digest."
                }
                throw "Artifact '$($art.artifact_name)' is missing required field '$rf'."
            }
        }

        # Enforce unique artifact identity
        $id = "$($art['artifact_name'])::$($art['platform'])::$($art['target'])::$($art['installer_version'])"
        if ($seenIdentities.Contains($id)) {
            throw "Ambiguous manifest: Duplicate artifact entry detected for identity '$id'."
        }
        [void]$seenIdentities.Add($id)

        $sha = $art["expected_sha256"].ToLowerInvariant()
        if ($sha -notmatch '^[a-f0-9]{64}$') {
            throw "Malformed SHA-256 digest in manifest for artifact '$($art.artifact_name)': '$($art.expected_sha256)'"
        }
        $art["expected_sha256"] = $sha

        $policy = $art["verification_policy"]
        if ($policy -ne "manifest-sha256-pinned" -and $policy -ne "authenticode-strict") {
            throw "Invalid verification_policy '$policy' for artifact '$($art.artifact_name)'. Must be 'manifest-sha256-pinned' or 'authenticode-strict'."
        }

        $status = $art["approval_status"]
        if ($status -ne "approved" -and $status -ne "unapproved") {
            throw "Invalid approval_status '$status' for artifact '$($art.artifact_name)'. Must be 'approved' or 'unapproved'."
        }
    }

    return $manifest
}

function Find-ToolchainArtifact {
    <#
    .SYNOPSIS
        Selects an artifact entry from the toolchain manifest matching criteria.
        Rejects ambiguous duplicate artifact matches.
    #>
    param(
        [hashtable]$Manifest,
        [string]$ArtifactName = "rustup-init",
        [string]$Platform = "windows",
        [string]$Target,
        [string]$InstallerVersion,
        [string]$ToolchainVersion
    )

    if (-not $Manifest -or -not $Manifest.artifacts) {
        throw "Invalid or empty toolchain manifest."
    }

    $targetMatches = [System.Collections.ArrayList]::new()
    $platformMatches = [System.Collections.ArrayList]::new()

    foreach ($art in $Manifest.artifacts) {
        if ($art["artifact_name"] -eq $ArtifactName) {
            if ($art["platform"] -eq $Platform) {
                [void]$platformMatches.Add($art)
                if ($art["target"] -eq $Target) {
                    [void]$targetMatches.Add($art)
                }
            }
        }
    }

    if ($platformMatches.Count -eq 0) {
        throw "Platform mismatch: No artifact found for platform '$Platform'."
    }

    if ($targetMatches.Count -eq 0) {
        throw "Target / platform mismatch: No artifact found for target '$Target' on platform '$Platform'."
    }

    $matches = [System.Collections.ArrayList]::new()

    foreach ($art in $targetMatches) {
        $matchesInstaller = $true
        $matchesToolchain = $true

        if ($InstallerVersion -and $art["installer_version"] -ne $InstallerVersion) {
            $matchesInstaller = $false
        }
        if ($ToolchainVersion -and $art.Contains("toolchain_version") -and $art["toolchain_version"] -ne $ToolchainVersion) {
            $matchesToolchain = $false
        }

        if ($matchesInstaller -and $matchesToolchain) {
            [void]$matches.Add($art)
        }
    }

    if ($matches.Count -eq 0) {
        throw "Artifact version mismatch: No matching artifact found for target '$Target' with installer version '$InstallerVersion' / toolchain version '$ToolchainVersion'."
    }

    if ($matches.Count -gt 1) {
        throw "Ambiguous artifact match: Multiple ($($matches.Count)) conflicting artifacts matched target '$Target' with installer version '$InstallerVersion' / toolchain version '$ToolchainVersion'."
    }

    return $matches[0]
}

function Test-RedirectDestination {
    param(
        [string]$DestinationUri,
        [string]$AllowedPattern = '^https://(?:win\.rustup\.rs|static\.rust-lang\.org)/'
    )
    if ([string]::IsNullOrWhiteSpace($DestinationUri)) {
        return $false
    }
    if ($DestinationUri -notmatch $AllowedPattern) {
        return $false
    }
    return $true
}

function Invoke-DownloadWithRedirectCheck {
    param(
        [string]$Uri,
        [string]$OutFile,
        [string]$AllowedPattern = '^https://(?:win\.rustup\.rs|static\.rust-lang\.org)/',
        [int]$MaxRedirects = 5
    )

    $currentUri = $Uri
    $redirectCount = 0

    while ($redirectCount -le $MaxRedirects) {
        if (-not (Test-RedirectDestination -DestinationUri $currentUri -AllowedPattern $AllowedPattern)) {
            throw "Untrusted download or redirect destination: '$currentUri'. Allowed pattern: '$AllowedPattern'"
        }

        $request = [System.Net.HttpWebRequest]::Create($currentUri)
        $request.AllowAutoRedirect = $false
        $request.Method = "GET"
        $request.Timeout = 30000
        $request.UserAgent = "ULVO-Provisioning/1.1"

        try {
            $response = [System.Net.HttpWebResponse]$request.GetResponse()
            $statusCode = [int]$response.StatusCode

            if ($statusCode -ge 300 -and $statusCode -lt 400) {
                $location = $response.Headers["Location"]
                $response.Close()
                $response.Dispose()

                if ([string]::IsNullOrWhiteSpace($location)) {
                    throw "HTTP redirect response ($statusCode) missing Location header."
                }

                $targetUri = [System.Uri]::new([System.Uri]::new($currentUri), $location).AbsoluteUri
                if (-not (Test-RedirectDestination -DestinationUri $targetUri -AllowedPattern $AllowedPattern)) {
                    throw "Unexpected or untrusted redirect destination: '$targetUri'"
                }

                Write-Diagnostic "  Followed GET redirect: $currentUri -> $targetUri"
                $currentUri = $targetUri
                $redirectCount++
                continue
            }

            if ($statusCode -eq 200) {
                Write-Diagnostic "  Streaming payload from validated GET response: $currentUri"
                $responseStream = $response.GetResponseStream()
                $fileStream = [System.IO.File]::Create($OutFile)
                try {
                    $responseStream.CopyTo($fileStream)
                } finally {
                    $fileStream.Flush()
                    $fileStream.Dispose()
                    $responseStream.Dispose()
                    $response.Close()
                    $response.Dispose()
                }
                return
            }

            $response.Close()
            $response.Dispose()
            throw "Unexpected HTTP status code $statusCode from '$currentUri'"
        } catch [System.Net.WebException] {
            $webEx = $_.Exception
            if ($webEx.Response) {
                $httpResp = [System.Net.HttpWebResponse]$webEx.Response
                $statusCode = [int]$httpResp.StatusCode
                if ($statusCode -ge 300 -and $statusCode -lt 400) {
                    $location = $httpResp.Headers["Location"]
                    $httpResp.Close()
                    $httpResp.Dispose()
                    if ([string]::IsNullOrWhiteSpace($location)) {
                        throw "HTTP redirect response ($statusCode) missing Location header."
                    }
                    $targetUri = [System.Uri]::new([System.Uri]::new($currentUri), $location).AbsoluteUri
                    if (-not (Test-RedirectDestination -DestinationUri $targetUri -AllowedPattern $AllowedPattern)) {
                        throw "Unexpected or untrusted redirect destination: '$targetUri'"
                    }
                    Write-Diagnostic "  Followed GET redirect: $currentUri -> $targetUri"
                    $currentUri = $targetUri
                    $redirectCount++
                    continue
                }
                $httpResp.Close()
                $httpResp.Dispose()
            }
            throw "HTTP GET request failed for '$currentUri': $($webEx.Message)"
        }
    }

    if ($redirectCount -gt $MaxRedirects) {
        throw "Exceeded maximum redirect count ($MaxRedirects) while resolving '$Uri'"
    }
}

function Test-InstallerProvenance {
    param(
        [string]$FilePath,
        [hashtable]$Artifact
    )

    if (-not (Test-Path -LiteralPath $FilePath)) {
        Write-Diagnostic "Installer file does not exist: $FilePath"
        return $false
    }

    if (-not $Artifact) {
        Write-Diagnostic "Missing artifact specification for provenance verification."
        return $false
    }

    $policy = $Artifact["verification_policy"]
    $approvalStatus = $Artifact["approval_status"]
    $expectedSha256 = $Artifact["expected_sha256"]

    Write-Diagnostic "Evaluating installer provenance:"
    Write-Diagnostic "  Verification Policy : $policy"
    Write-Diagnostic "  Approval Status     : $approvalStatus"
    Write-Diagnostic "  Expected SHA-256    : $expectedSha256"

    # 1. Compute SHA-256 digest
    $actualHash = (Get-FileHash -LiteralPath $FilePath -Algorithm SHA256).Hash.ToLowerInvariant()
    Write-Diagnostic "  Computed SHA-256    : $actualHash"

    if (-not $actualHash.Equals($expectedSha256.ToLowerInvariant(), [System.StringComparison]::OrdinalIgnoreCase)) {
        Write-Diagnostic "  FAIL: Computed SHA-256 does not match expected hash!"
        return $false
    }

    # 2. Check Authenticode signature status
    $sig = Get-AuthenticodeSignature -FilePath $FilePath -ErrorAction SilentlyContinue
    $isAuthenticodeValid = $false

    if ($sig -and $sig.Status -eq "Valid") {
        $subject = $sig.SignerCertificate.Subject
        if ($subject -match 'CN=Rust Foundation' -and $subject -match 'O=Rust Foundation') {
            $isAuthenticodeValid = $true
            Write-Diagnostic "  Authenticode signature: VALID (Rust Foundation)"
        } else {
            Write-Diagnostic "  Authenticode signature: Untrusted publisher ($subject)"
        }
    } else {
        $statusStr = if ($sig) { $sig.Status.ToString() } else { "Unknown" }
        Write-Diagnostic "  Authenticode signature: $statusStr"
    }

    # 3. Policy evaluation
    $hasAttachedSignature = ($sig -ne $null -and $sig.SignerCertificate -ne $null)

    if ($policy -eq "authenticode-strict") {
        if (-not $isAuthenticodeValid) {
            Write-Diagnostic "  FAIL: Policy 'authenticode-strict' requires valid Authenticode signature from Rust Foundation. Unsigned or invalid binary rejected."
            return $false
        }
    } elseif ($policy -eq "manifest-sha256-pinned") {
        # If binary has an attached signature certificate but it is invalid or untrusted, reject
        if ($hasAttachedSignature -and -not $isAuthenticodeValid) {
            Write-Diagnostic "  FAIL: Binary contains an invalid or untrusted Authenticode signature ($($sig.Status))."
            return $false
        }
        # If unsigned, allowed ONLY if artifact is explicitly approved
        if (-not $isAuthenticodeValid -and $approvalStatus -ne "approved") {
            Write-Diagnostic "  FAIL: Binary is not Authenticode-signed and artifact approval_status is '$approvalStatus'. Unattended execution prohibited."
            return $false
        }
    } else {
        Write-Diagnostic "  FAIL: Unknown verification policy '$policy'."
        return $false
    }

    if ($approvalStatus -ne "approved") {
        Write-Diagnostic "  FAIL: Artifact approval_status is '$approvalStatus'. Execution blocked."
        return $false
    }

    Write-Diagnostic "  Provenance verification SUCCESS."
    return $true
}

function Invoke-RustupInstaller {
    param(
        [string]$InstallerPath,
        [string]$RustupHome,
        [string]$CargoHome,
        [string]$RustVersion,
        [string]$Target,
        [string]$ExpectedSha256
    )

    # Pre-launch binary integrity verification (Anti-replacement / TOCTOU mitigation)
    if ($ExpectedSha256) {
        if (-not (Test-Path -LiteralPath $InstallerPath)) {
            Write-ErrorExit "Pre-launch security failure: Installer binary not found at '$InstallerPath'"
        }
        $preLaunchHash = (Get-FileHash -LiteralPath $InstallerPath -Algorithm SHA256).Hash.ToLowerInvariant()
        if (-not $preLaunchHash.Equals($ExpectedSha256.ToLowerInvariant(), [System.StringComparison]::OrdinalIgnoreCase)) {
            Write-ErrorExit "Pre-launch security violation: Installer binary modified after verification! Expected: '$ExpectedSha256', Actual: '$preLaunchHash'. Aborting."
        }
        Write-Diagnostic "Pre-launch binary integrity verified (SHA-256: $preLaunchHash)."
    }

    $originalRustupHome = $env:RUSTUP_HOME
    $originalCargoHome = $env:CARGO_HOME
    $originalPath = $env:PATH

    try {
        $env:RUSTUP_HOME = $RustupHome
        $env:CARGO_HOME = $CargoHome

        $installerArgs = @(
            "-y",
            "--default-host", $Target,
            "--default-toolchain", $RustVersion,
            "--profile", "minimal",
            "--no-modify-path",
            "-q"
        )

        Write-Diagnostic "Installing Rust $RustVersion with target $Target"
        Write-Diagnostic "  RUSTUP_HOME: $RustupHome"
        Write-Diagnostic "  CARGO_HOME : $CargoHome"
        Write-Diagnostic "  Arguments  : $($installerArgs -join ' ')"

        $process = Start-Process -FilePath $InstallerPath -ArgumentList $installerArgs `
            -Wait -NoNewWindow -PassThru

        return $process.ExitCode
    } finally {
        if ($null -ne $originalRustupHome) {
            $env:RUSTUP_HOME = $originalRustupHome
        } else {
            Remove-Item env:RUSTUP_HOME -ErrorAction SilentlyContinue
        }

        if ($null -ne $originalCargoHome) {
            $env:CARGO_HOME = $originalCargoHome
        } else {
            Remove-Item env:CARGO_HOME -ErrorAction SilentlyContinue
        }

        $env:PATH = $originalPath
    }
}

function Invoke-ToolchainCommand {
    param(
        [string]$FilePath,
        [string[]]$ArgumentList,
        [int]$TimeoutMs = 15000
    )

    $psi = New-Object System.Diagnostics.ProcessStartInfo
    $psi.FileName = $FilePath
    $psi.Arguments = ($ArgumentList -join " ")
    $psi.UseShellExecute = $false
    $psi.RedirectStandardOutput = $true
    $psi.RedirectStandardError = $true
    $psi.CreateNoWindow = $true

    $proc = New-Object System.Diagnostics.Process
    $proc.StartInfo = $psi

    $stdoutBuilder = New-Object System.Text.StringBuilder
    $stderrBuilder = New-Object System.Text.StringBuilder

    try {
        [void]$proc.Start()
        $stdoutTask = $proc.StandardOutput.ReadToEndAsync()
        $stderrTask = $proc.StandardError.ReadToEndAsync()

        if (-not $proc.WaitForExit($TimeoutMs)) {
            try { $proc.Kill() } catch {}
            return @{
                ExitCode = -1
                Stdout = ""
                Stderr = "Process timed out after $TimeoutMs ms"
            }
        }
        [void]$stdoutTask.Wait()
        [void]$stderrTask.Wait()

        return @{
            ExitCode = $proc.ExitCode
            Stdout = $stdoutTask.Result
            Stderr = $stderrTask.Result
        }
    } catch {
        return @{
            ExitCode = -1
            Stdout = ""
            Stderr = $_.Exception.Message
        }
    } finally {
        $proc.Dispose()
    }
}

function Test-ToolchainComponents {
    param(
        [string]$RustupHome,
        [string]$CargoHome,
        [string]$RustVersion,
        [string]$Target
    )

    $originalPath = $env:PATH
    $originalRustupHome = $env:RUSTUP_HOME
    $originalCargoHome = $env:CARGO_HOME

    try {
        $env:RUSTUP_HOME = $RustupHome
        $env:CARGO_HOME = $CargoHome
        $env:PATH = "$CargoHome\bin;$originalPath"

        $rustcPath = "$CargoHome\bin\rustc.exe"
        $cargoPath = "$CargoHome\bin\cargo.exe"
        $rustupPath = "$CargoHome\bin\rustup.exe"

        if (-not (Test-Path -LiteralPath $rustcPath)) {
            return "rustc.exe not found at '$rustcPath'"
        }
        if (-not (Test-Path -LiteralPath $cargoPath)) {
            return "cargo.exe not found at '$cargoPath'"
        }
        if (-not (Test-Path -LiteralPath $rustupPath)) {
            return "rustup.exe not found at '$rustupPath'"
        }

        # Check rustc --version
        $res = Invoke-ToolchainCommand -FilePath $rustcPath -ArgumentList @("--version")
        if ($res.ExitCode -ne 0) {
            return "rustc --version failed with exit code $($res.ExitCode): $($res.Stderr)"
        }
        if ($res.Stdout -notmatch [regex]::Escape($RustVersion)) {
            return "rustc version mismatch: expected '$RustVersion', got '$($res.Stdout.Trim())'"
        }

        # Check cargo --version
        $res = Invoke-ToolchainCommand -FilePath $cargoPath -ArgumentList @("--version")
        if ($res.ExitCode -ne 0) {
            return "cargo --version failed with exit code $($res.ExitCode): $($res.Stderr)"
        }

        # Check rustup show (default host)
        $res = Invoke-ToolchainCommand -FilePath $rustupPath -ArgumentList @("show")
        if ($res.ExitCode -ne 0) {
            return "rustup show failed with exit code $($res.ExitCode): $($res.Stderr)"
        }
        $defaultHostMatch = Select-String -InputObject $res.Stdout -Pattern "Default host:\s*([^\r\n]+)"
        if (-not $defaultHostMatch) {
            return "Default host not found in rustup show output: $($res.Stdout)"
        }
        $defaultHost = $defaultHostMatch.Matches[0].Groups[1].Value.Trim()
        if ($defaultHost -ne $Target) {
            return "Default host mismatch: expected '$Target', got '$defaultHost'"
        }

        # Check rustc --print target-libdir
        $res = Invoke-ToolchainCommand -FilePath $rustcPath -ArgumentList @("--print", "target-libdir")
        if ($res.ExitCode -ne 0) {
            return "rustc --print target-libdir failed with exit code $($res.ExitCode): $($res.Stderr)"
        }
        $libDir = $res.Stdout.Trim()
        if (-not (Test-Path -LiteralPath $libDir)) {
            return "Target libdir not found: '$libDir'"
        }

        $libstdFiles = Get-ChildItem -LiteralPath $libDir -Filter "libstd-*.rlib" -ErrorAction SilentlyContinue
        if (-not $libstdFiles -or $libstdFiles.Count -eq 0) {
            return "libstd-*.rlib not found in '$libDir'"
        }

        Write-Diagnostic "Toolchain verification passed."
        return $null
    } finally {
        $env:PATH = $originalPath
        if ($null -ne $originalRustupHome) {
            $env:RUSTUP_HOME = $originalRustupHome
        } else {
            Remove-Item env:RUSTUP_HOME -ErrorAction SilentlyContinue
        }
        if ($null -ne $originalCargoHome) {
            $env:CARGO_HOME = $originalCargoHome
        } else {
            Remove-Item env:CARGO_HOME -ErrorAction SilentlyContinue
        }
    }
}

#endregion

#region Main Execution

if ($MyInvocation.InvocationName -ne '.' -and (-not $MyInvocation.Line -or $MyInvocation.Line -notmatch '^\s*\.\s+')) {

Write-Diagnostic "ULVO Windows Rust Provisioning v1.1 (Hardened)"
Write-Diagnostic "=============================================="

# Resolve and validate ULVO root
try {
    $canonicalUlvoRoot = [System.IO.Path]::GetFullPath($UlvoRoot).TrimEnd('\')
    $defaultUlvoCanonical = [System.IO.Path]::GetFullPath("E:\Workspace\Active\ULVO").TrimEnd('\')
    $testTmpCanonical = [System.IO.Path]::GetFullPath("E:\Workspace\Active\ULVO\tests\tmp").TrimEnd('\')

    $isProductionRoot = $canonicalUlvoRoot.Equals($defaultUlvoCanonical, [System.StringComparison]::OrdinalIgnoreCase)
    $isTestFixtureRoot = $canonicalUlvoRoot.StartsWith($testTmpCanonical + '\', [System.StringComparison]::OrdinalIgnoreCase)

    if (-not ($isProductionRoot -or $isTestFixtureRoot)) {
        Write-ErrorExit "Invalid UlvoRoot: '$UlvoRoot'. Must be 'E:\Workspace\Active\ULVO' or under 'tests\tmp'."
    }
} catch {
    Write-ErrorExit "Invalid UlvoRoot path specification: '$UlvoRoot'"
}

# Define approved Rust vault resource area
$approvedRustArea = "$canonicalUlvoRoot\vault\runtimes\rust"
$defaultRustupHome = "$approvedRustArea\rustup"
$defaultCargoHome = "$approvedRustArea\cargo"
$downloadDir = "$approvedRustArea\Downloads"
$installerPath = "$downloadDir\rustup-init.exe"

# Resolve actual RUSTUP_HOME and CARGO_HOME
if (-not $RustupHome) { $RustupHome = $defaultRustupHome }
if (-not $CargoHome) { $CargoHome = $defaultCargoHome }

# Validate input arguments against injection
if ($RustVersion -notmatch '^(?:stable|beta|nightly|\d+\.\d+\.\d+(?:-[a-zA-Z0-9.-]+)?)$') {
    Write-ErrorExit "Invalid RustVersion format: '$RustVersion'"
}

if ($Target -notmatch '^[a-zA-Z0-9_-]+$') {
    Write-ErrorExit "Invalid Target format: '$Target'"
}

if ($DownloadUrl -and $DownloadUrl -notmatch '^https://(?:win\.rustup\.rs|static\.rust-lang\.org)/') {
    Write-ErrorExit "Untrusted DownloadUrl: '$DownloadUrl'. Must use HTTPS from win.rustup.rs or static.rust-lang.org"
}

# Resolve Manifest Path
if (-not $ManifestPath) {
    $candidateManifest = "$canonicalUlvoRoot\config\toolchain-artifacts.toml"
    if (Test-Path -LiteralPath $candidateManifest) {
        $ManifestPath = $candidateManifest
    } elseif ($isTestFixtureRoot) {
        $ManifestPath = "$defaultUlvoCanonical\config\toolchain-artifacts.toml"
    } else {
        $ManifestPath = $candidateManifest
    }
}

Write-Diagnostic "Loading toolchain artifact manifest: $ManifestPath"
try {
    $manifest = Get-ToolchainManifest -FilePath $ManifestPath
} catch {
    Write-ErrorExit "Manifest verification failed: $_"
}

# Select approved artifact from manifest
try {
    $artifact = Find-ToolchainArtifact -Manifest $manifest -ArtifactName "rustup-init" -Platform "windows" -Target $Target -InstallerVersion $InstallerVersion -ToolchainVersion $RustVersion
} catch {
    Write-ErrorExit "$_"
}

# Enforce canonical URL from reviewed manifest and reject caller substitution
$downloadTargetUrl = $artifact["canonical_url"]
if ($DownloadUrl -and -not $DownloadUrl.Equals($downloadTargetUrl, [System.StringComparison]::OrdinalIgnoreCase)) {
    Write-ErrorExit "Security violation: Caller-supplied DownloadUrl ('$DownloadUrl') does not match reviewed manifest canonical_url ('$downloadTargetUrl'). Caller URL substitution is prohibited."
}
$DownloadUrl = $downloadTargetUrl

# Enforce hash provenance and reject caller hash overrides
$manifestSha256 = $artifact["expected_sha256"]
if ($ExpectedSha256) {
    $callerShaClean = $ExpectedSha256.Trim().ToLowerInvariant()
    if (-not $callerShaClean.Equals($manifestSha256, [System.StringComparison]::OrdinalIgnoreCase)) {
        Write-ErrorExit "Security violation: Caller-supplied ExpectedSha256 ('$ExpectedSha256') does not match reviewed manifest hash ('$manifestSha256'). Caller override rejected."
    }
} else {
    $ExpectedSha256 = $manifestSha256
}

Write-Diagnostic "Configuration:"
Write-Diagnostic "  ULVO Root     : $canonicalUlvoRoot"
Write-Diagnostic "  Approved Area : $approvedRustArea"
Write-Diagnostic "  RUSTUP_HOME   : $RustupHome"
Write-Diagnostic "  CARGO_HOME    : $CargoHome"
Write-Diagnostic "  Rust Version  : $RustVersion"
Write-Diagnostic "  Target        : $Target"
Write-Diagnostic "  Download URL  : $downloadTargetUrl"
Write-Diagnostic "  Artifact      : $($artifact.artifact_name) v$($artifact.installer_version)"
Write-Diagnostic "  Expected SHA  : $manifestSha256"
Write-Diagnostic "  Trust Policy  : $($artifact.verification_policy)"
Write-Diagnostic "  Approval      : $($artifact.approval_status)"

# Gate 1: Free space check
Write-Diagnostic "Gate 1: Free space verification"
$targetDrive = ([System.IO.Path]::GetPathRoot($approvedRustArea)).Substring(0, 1)
$freeSpaceGB = Get-FreeSpaceGB -Drive $targetDrive
if ($freeSpaceGB -lt 2.0) {
    Write-ErrorExit "Insufficient space on ${targetDrive}: ($freeSpaceGB GB free, 2GB required)"
}
Write-Diagnostic "  Free space on ${targetDrive}:: $freeSpaceGB GB"

# Gate 2: Path validation and containment
Write-Diagnostic "Gate 2: Path validation and containment check"
Validate-InstallationPaths -RustupHome $RustupHome -CargoHome $CargoHome -ApprovedArea $approvedRustArea

# Gate 3: Dry-run check (Guaranteed non-mutating)
if ($DryRun) {
    Write-Diagnostic "[DRY-RUN] Manifest loaded and validated ($ManifestPath)"
    Write-Diagnostic "[DRY-RUN] Artifact selected: $($artifact.artifact_name) v$($artifact.installer_version) ($Target)"
    Write-Diagnostic "[DRY-RUN] Expected SHA-256: $manifestSha256"
    Write-Diagnostic "[DRY-RUN] Verification policy: $($artifact.verification_policy)"
    Write-Diagnostic "[DRY-RUN] Approval status: $($artifact.approval_status)"
    Write-Diagnostic "[DRY-RUN] Path validation passed"
    Write-Diagnostic "[DRY-RUN] Free space check passed (${freeSpaceGB} GB)"
    Write-Diagnostic "[DRY-RUN] Genuinely non-mutating: zero downloads, zero writes, zero installations"
    exit 0
}

# Gate 3.5: Production execution approval & artifact approval check
if (-not $Install) {
    Write-ErrorExit "Production provisioning blocked: Requires explicit -Install switch. For non-mutating validation, run with -DryRun."
}

if (-not $isProductionRoot) {
    Write-ErrorExit "Security violation: Production installation (-Install) is strictly prohibited with an UlvoRoot under test fixtures ('$canonicalUlvoRoot'). UlvoRoot must be the canonical production root 'E:\Workspace\Active\ULVO'."
}

if ($artifact["approval_status"] -ne "approved") {
    Write-ErrorExit "Production provisioning blocked: Artifact '$($artifact.artifact_name)' approval status is '$($artifact.approval_status)'. Unapproved artifacts cannot be executed."
}

# Gate 4: Download rustup-init.exe with redirect verification
Write-Diagnostic "Gate 4: Downloading rustup-init.exe with redirect verification"
if (-not (Test-Path -LiteralPath $downloadDir)) {
    New-Item -ItemType Directory -Path $downloadDir -Force | Out-Null
    Write-Diagnostic "  Created download directory: $downloadDir"
}

try {
    Write-Diagnostic "  Downloading from $downloadTargetUrl"
    Invoke-DownloadWithRedirectCheck -Uri $downloadTargetUrl -OutFile $installerPath
    Write-Diagnostic "  Download complete"
} catch {
    Write-ErrorExit "Download failed: $_"
}

$fileInfo = Get-Item -LiteralPath $installerPath
if ($fileInfo.Length -lt 1MB) {
    Write-ErrorExit "Installer file too small: $($fileInfo.Length) bytes"
}
Write-Diagnostic "  File size: $($fileInfo.Length / 1MB) MB"

# Gate 4.5: Installer provenance verification
Write-Diagnostic "Gate 4.5: Installer provenance verification"
if (-not (Test-InstallerProvenance -FilePath $installerPath -Artifact $artifact)) {
    Write-ErrorExit "Installer provenance verification failed. Failing closed."
}

# Gate 5: Toolchain installation
Write-Diagnostic "Gate 5: Installing Rust toolchain"
$exitCode = Invoke-RustupInstaller -InstallerPath $installerPath `
    -RustupHome $RustupHome -CargoHome $CargoHome `
    -RustVersion $RustVersion -Target $Target `
    -ExpectedSha256 $manifestSha256

if ($exitCode -ne 0) {
    Write-ErrorExit "Installation failed with exit code $exitCode"
}

# Gate 6: Toolchain verification
Write-Diagnostic "Gate 6: Toolchain verification"
$errorMsg = Test-ToolchainComponents -RustupHome $RustupHome -CargoHome $CargoHome `
    -RustVersion $RustVersion -Target $Target

if ($errorMsg) {
    Write-ErrorExit "Toolchain verification failed: $errorMsg"
}

Write-Diagnostic "[SUCCESS] Toolchain provisioned successfully"
Write-Diagnostic "  RUSTUP_HOME: $RustupHome"
Write-Diagnostic "  CARGO_HOME : $CargoHome"
Write-Diagnostic "  rustc: $CargoHome\bin\rustc.exe"
Write-Diagnostic "  cargo: $CargoHome\bin\cargo.exe"
Write-Diagnostic "  Target: $Target"

} # End of if ($MyInvocation.InvocationName -ne '.')

#endregion
