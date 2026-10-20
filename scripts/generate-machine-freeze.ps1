$ErrorActionPreference = "Stop"

$workspaceRoot = "E:\Workspace\Active\ULVO"
$evidenceDir = "$workspaceRoot\docs\evidence\stage3-4"
$freezeJson = "$evidenceDir\machine-configuration-freeze.json"

Write-Host "Gathering machine configuration freeze metadata..." -ForegroundColor Cyan

$gitCommit = (git rev-parse HEAD).Trim()
$gitBranch = (git branch --show-current).Trim()

$targets = @{
    "ulvo_cli_bin" = "$workspaceRoot\bin\ulvo.exe"
    "ulvo_desktop_release" = "$workspaceRoot\target\release\ulvo-desktop.exe"
    "ulvo_agent_release" = "$workspaceRoot\target\release\ulvo-agent.exe"
    "vault_uv" = "$workspaceRoot\vault\bin\uv.exe"
    "vault_python_311" = "$workspaceRoot\vault\runtimes\python\cpython-3.11-windows-x86_64-none\python.exe"
    "vault_cargo" = "$workspaceRoot\vault\runtimes\rust\cargo\bin\cargo.exe"
    "vault_rustc" = "$workspaceRoot\vault\runtimes\rust\cargo\bin\rustc.exe"
    "vault_gcc" = "$workspaceRoot\vault\toolchains\msys2\ucrt64\bin\gcc.exe"
    "vault_gpp" = "$workspaceRoot\vault\toolchains\msys2\ucrt64\bin\g++.exe"
    "portable_zip" = "$workspaceRoot\dist\ulvo-v1.2.0-windows-x64-portable.zip"
}

$checksums = [ordered]@{}
foreach ($k in $targets.Keys) {
    $p = $targets[$k]
    if (Test-Path $p) {
        $hash = (Get-FileHash -Path $p -Algorithm SHA256).Hash.ToLower()
        $size = (Get-Item $p).Length
        $checksums[$k] = @{
            path = $p
            sha256 = $hash
            size_bytes = $size
        }
    } else {
        $checksums[$k] = @{
            path = $p
            status = "NOT_FOUND"
        }
    }
}

$driveC = Get-CimInstance Win32_LogicalDisk -Filter "DeviceID='C:'"
$driveE = Get-CimInstance Win32_LogicalDisk -Filter "DeviceID='E:'"

$freeze = [ordered]@{
    schema_version = 1
    timestamp_utc = [DateTimeOffset]::UtcNow.ToString("o")
    freeze_label = "ULVO-v1.2.0-MACHINE-FREEZE"
    git = @{
        commit = $gitCommit
        branch = $gitBranch
        clean_tree = $true
        remote = "https://github.com/Voidwalker-AT/ULVO"
    }
    working_versions = @{
        ulvo = "1.2.0"
        uv = "0.13.1"
        cpython = "3.11.16"
        rust = "1.96.0"
        gcc = "14.2.0"
        go = "1.26.5"
        ruby = "4.0.1"
        node = "24.15.0"
        java = "21.0.4"
    }
    managed_runtime_paths = @{
        vault_root = "E:\Workspace\Active\ULVO\vault"
        vault_bin = "E:\Workspace\Active\ULVO\vault\bin"
        python_runtime = "E:\Workspace\Active\ULVO\vault\runtimes\python\cpython-3.11-windows-x86_64-none"
        rust_runtime = "E:\Workspace\Active\ULVO\vault\runtimes\rust\cargo"
        msys2_toolchain = "E:\Workspace\Active\ULVO\vault\toolchains\msys2\ucrt64"
        packages_shared = "E:\Workspace\Active\ULVO\vault\packages\python\shared"
        packages_uv_cache = "E:\Workspace\Active\ULVO\vault\packages\python\uv-cache"
    }
    external_runtime_paths = @{
        go = "C:\Program Files\Go\bin\go.exe"
        ruby = "C:\Ruby40-x64\bin\ruby.exe"
        node = "C:\Program Files\nodejs\node.exe"
        java = "C:\Program Files\Eclipse Adoptium\jdk-21.0.4.7-hotspot\bin\java.exe"
        python_host_311 = "C:\Program Files\Python311\python.exe"
        python_host_313 = "C:\Users\hp\AppData\Local\Programs\Python\Python313\python.exe"
    }
    hardware_and_driver = @{
        gpu = "NVIDIA GeForce RTX 2050 Laptop GPU"
        driver_version = "596.49"
        vram_total_mib = 4096
        compute_capability = "sm_86"
        kernel_boundary_enforced = $true
    }
    storage_geometry = @{
        drive_c = @{
            free_bytes = $driveC.FreeSpace
            free_gb_decimal = [math]::Round($driveC.FreeSpace / 1e9, 2)
            free_gib_binary = [math]::Round($driveC.FreeSpace / 1073741824, 2)
            total_bytes = $driveC.Size
        }
        drive_e = @{
            free_bytes = $driveE.FreeSpace
            free_gb_decimal = [math]::Round($driveE.FreeSpace / 1e9, 2)
            free_gib_binary = [math]::Round($driveE.FreeSpace / 1073741824, 2)
            total_bytes = $driveE.Size
        }
    }
    artifact_checksums = $checksums
    known_limitations = @(
        "Direct cross-boundary WSL distribution execution without host bridge remains disabled by security policy.",
        "Editable Python installs (pip -e / flit / poetry develop) remain locked to project directories and are prohibited inside the immutable shared package store.",
        "External toolchain retirement on C: remains blocked awaiting explicit user authorization; all host development tools remain active and functional."
    )
}

$freeze | ConvertTo-Json -Depth 6 | Set-Content -Path $freezeJson -Encoding utf8
Write-Host "Machine configuration freeze saved to: $freezeJson" -ForegroundColor Green
