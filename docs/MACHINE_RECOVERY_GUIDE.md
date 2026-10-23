# ULVO Machine Recovery & Operational Runbook

For current project-registry corruption handling, stale-writer retry rules, token rotation limits, and storage accounting semantics, see [workstation hardening](WORKSTATION_HARDENING.md). The machine-specific versions and measurements below are historical snapshots, not current health evidence.

**Document Version:** 1.0.0  
**Target Host:** Windows 11 (x86_64)  
**Host Hardware:** NVIDIA GeForce RTX 2050 Laptop GPU (Driver: 596.49)  
**Primary Working Directory:** `E:\Workspace\Active\ULVO`  
**Primary Vault Location:** `E:\Workspace\Active\ULVO\vault`

---

## 1. Quick Verification & System Health

To confirm the current operational health of the ULVO control plane at any time:

```powershell
# 1. Run System Doctor
E:\Workspace\Active\ULVO\bin\ulvo.exe doctor

# 2. Check Background Agent Status
E:\Workspace\Active\ULVO\bin\ulvo.exe agent status

# 3. Check Managed & External Runtimes
E:\Workspace\Active\ULVO\bin\ulvo.exe inventory

# 4. Check Free Storage Geometry
Get-CimInstance Win32_LogicalDisk | Select-Object DeviceID, @{N='FreeGB';E={[math]::Round($_.FreeSpace/1e9,2)}}, @{N='FreeGiB';E={[math]::Round($_.FreeSpace/1073741824,2)}}
```

---

## 2. Background Reconciliation Agent Service

ULVO includes a background reconciliation agent (`ulvo-agent.exe`) that listens on loopback TCP port `48590` using a persistent bearer token stored in `vault/agent.token`.

### Starting the Agent Daemon
```powershell
# Launch agent in background
Start-Process -FilePath "E:\Workspace\Active\ULVO\target\release\ulvo-agent.exe" -ArgumentList "run" -WindowStyle Hidden
```

### Checking Status & Triggering Reconciliation
```powershell
# Query status
E:\Workspace\Active\ULVO\bin\ulvo.exe agent status

# Force immediate lease and cache reconciliation
E:\Workspace\Active\ULVO\bin\ulvo.exe agent reconcile
```

### Stopping the Agent
```powershell
# Graceful shutdown via authenticated IPC
E:\Workspace\Active\ULVO\bin\ulvo.exe agent stop
```

---

## 3. Desktop GUI Control Center

The desktop GUI application is located at:
`E:\Workspace\Active\ULVO\target\release\ulvo-desktop.exe`

### Launching the Desktop Application
```powershell
# Launch GUI
Start-Process -FilePath "E:\Workspace\Active\ULVO\target\release\ulvo-desktop.exe"
```
Or launch via the portable distribution shortcut:
`E:\Workspace\Active\ULVO\dist\ulvo-v1.2.0-windows-x64-portable\ULVO Control Center.bat`

---

## 4. Execution Gateway Quick Reference

To execute any language runtime without polluting global Windows PATH:

```powershell
# Python via Vault CPython 3.11.16
E:\Workspace\Active\ULVO\bin\ulvo.exe exec --tool python python --version

# Rust via Vault Rust 1.96.0
E:\Workspace\Active\ULVO\bin\ulvo.exe exec --tool cargo cargo --version

# C/C++ via Vault MSYS2 GCC 14.2.0
E:\Workspace\Active\ULVO\bin\ulvo.exe exec --tool gcc gcc --version

# Go via Gateway
E:\Workspace\Active\ULVO\bin\ulvo.exe exec --tool go go version

# Ruby via Gateway
E:\Workspace\Active\ULVO\bin\ulvo.exe exec --tool ruby ruby --version

# Node.js via Gateway
E:\Workspace\Active\ULVO\bin\ulvo.exe exec --tool node node --version

# Java OpenJDK via Gateway
E:\Workspace\Active\ULVO\bin\ulvo.exe exec --tool java java -version
```

---

## 5. Rebuilding Binaries from Source

If binaries ever need to be recompiled on this machine:

```powershell
# Set environment to vault-managed Rust toolchain
$env:CARGO_HOME = "E:\Workspace\Active\ULVO\vault\runtimes\rust\cargo"
$env:RUSTUP_HOME = "E:\Workspace\Active\ULVO\vault\runtimes\rust\rustup"
$env:PATH = "E:\Workspace\Active\ULVO\vault\runtimes\rust\cargo\bin;E:\Workspace\Active\ULVO\vault\toolchains\msys2\ucrt64\bin;$env:PATH"

# Build release workspace binaries (ulvo.exe, ulvo-agent.exe)
cargo build --release --workspace

# Rebuild release gateway in bin/
powershell.exe -NoProfile -ExecutionPolicy Bypass -File scripts\build-release-gateway.ps1
```

---

## 6. Running Test & Acceptance Suites

```powershell
# 1. Full Cargo Workspace Unit Tests (128 tests)
powershell.exe -NoProfile -ExecutionPolicy Bypass -File scripts\run-workspace-tests.ps1

# 2. Complete Real Project & Polyglot Acceptance (NexusNode, CrossModalLab CUDA, AstraRun, C++, Go, Java)
powershell.exe -NoProfile -ExecutionPolicy Bypass -File scripts\test-real-project-and-lifecycle-acceptance.ps1

# 3. Language Gateway Acceptance (7 ecosystems)
powershell.exe -NoProfile -ExecutionPolicy Bypass -File scripts\test-language-gateway-acceptance.ps1

# 4. GUI Complete Acceptance & Telemetry
powershell.exe -NoProfile -ExecutionPolicy Bypass -File scripts\test-gui-complete-acceptance.ps1

# 5. Installer / Uninstaller Security Defenses (7 suites)
powershell.exe -NoProfile -ExecutionPolicy Bypass -File scripts\test-installer-lifecycle.ps1
```

---

## 7. Troubleshooting

### Port 48590 Conflict
If the background agent fails to start with `Address already in use`:
```powershell
Get-NetTCPConnection -LocalPort 48590 -ErrorAction SilentlyContinue | Select-Object OwningProcess
Stop-Process -Id <OwningProcess> -Force
```

### Locked Ephemeral Environments
If an ephemeral virtualenv in `vault/environments/` cannot be unlinked due to a lingering process:
```powershell
# Reconcile leases and purge dead locks
E:\Workspace\Active\ULVO\bin\ulvo.exe env audit
E:\Workspace\Active\ULVO\bin\ulvo.exe env prune
```

### Shared Package Store Read-Only Flags
Files in `vault/packages/python/shared` are protected with NTFS `ReadOnly` attributes to guarantee immutability. If an external tool attempts in-place mutation, it will be rejected. Never remove these attributes manually.
