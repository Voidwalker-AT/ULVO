# ULVO v1.2.0 — Human Acceptance Testing Protocol & Release Gate
**Project:** ULVO — Universal Language & Virtual Environment Orchestrator  
**Target Release:** `v1.2.0`  
**Platform:** Windows 10/11 x64  
**Release Distribution:** `dist/ulvo-v1.2.0-windows-x64-portable.zip`  
**SHA-256 Checksum:** `63062d0a42c24fde12fddb62728c9b66c67ee3573f878bf011637c398d35c618`  

---

## 1. Instructions for the Human Tester

This document defines the formal, human-in-the-loop verification protocol for ULVO v1.2.0.
Before the stable release tag is published on GitHub, you must execute the 15 acceptance steps below.

> [!IMPORTANT]
> **Safety Guarantees in Place:**
> - Zero modification of user source code, Git repositories, or existing user projects.
> - Zero deletion of existing production vault runtimes or cached model weights (`E:\cache\huggingface`).
> - Hard 500 MB physical free space safety floor strictly enforced.
> - All tests run either read-only, in ephemeral environments, or in isolated test directories.

---

## 2. Deterministic 15-Step Acceptance Test Plan

### Step 1: Storage Preflight & Safety Reserve Verification
- **Purpose:** Ensure drive storage capacity is healthy and well above the 500 MB safety floor before executing any operations.
- **Action (PowerShell):**
  ```powershell
  Get-Volume -DriveLetter E | Select-Object DriveLetter, FileSystem, @{N='FreeGB';E={[math]::Round($_.SizeRemaining/1GB, 3)}}
  ```
- **Expected Outcome:** Drive `E:` available space is reported, and free space is well above 22.0 GB.
- **Red-Flag Failure Indicators:** Free space is less than 500 MB; volume cannot be accessed; error querying disk geometry.

---

### Step 2: Version & CLI Banner Output
- **Purpose:** Verify binary identification and version consistency across all interfaces.
- **Action:**
  ```powershell
  E:\Workspace\Active\ULVO\target\release\ulvo.exe --version
  E:\Workspace\Active\ULVO\target\release\ulvo.exe --help
  ```
- **Expected Outcome:** Prints `ulvo 1.2.0`. `--help` lists 13 top-level commands (`inventory`, `inspect`, `vault`, `workspace`, `project`, `run`, `env`, `migrate`, `toolchain`, `gpu`, `exec`, `doctor`, `catalog`, `agent`, `cache`, `storage`).
- **Red-Flag Failure Indicators:** Reports version other than `1.2.0`; crashes or exits with non-zero exit code.

---

### Step 3: System-Wide Doctor Diagnostic
- **Purpose:** Verify vault durability, execution gateway, runtime availability, GPU telemetry, and storage integrity.
- **Action:**
  ```powershell
  E:\Workspace\Active\ULVO\target\release\ulvo.exe doctor
  ```
- **Expected Outcome:** All diagnostic sections pass with green `[OK]`. Vault Durability reports persistent registry (`vault_registry.json`), Execution Gateway is operational, GPU is detected (NVIDIA GeForce RTX 2050), and priority ecosystems (Python, Rust, MSYS2, Ruby) resolve.
- **Red-Flag Failure Indicators:** Red `[FAIL]` status on vault durability or execution gateway; registry corruption errors.

---

### Step 4: Universal Toolchain Inventory Scan
- **Purpose:** Audit external and managed development toolchains across Windows PATH, MSYS2, and known installations.
- **Action:**
  ```powershell
  E:\Workspace\Active\ULVO\target\release\ulvo.exe toolchain inventory --verify
  ```
- **Expected Outcome:** Scans system and reports managed and external toolchains (Rust, GCC, G++, Python, Go, Ruby, Java, Node.js). Exit code is `0`.
- **Red-Flag Failure Indicators:** Hanging scan; unhandled panic on missing paths; incorrect ownership classification.

---

### Step 5: Universal Catalog & Search Resolution
- **Purpose:** Query the offline/online development toolchain catalog and resolve package requests.
- **Action:**
  ```powershell
  E:\Workspace\Active\ULVO\target\release\ulvo.exe catalog list
  E:\Workspace\Active\ULVO\target\release\ulvo.exe catalog search ninja
  ```
- **Expected Outcome:** `catalog list` displays 10 verified catalog entries (`python`, `uv`, `rust`, `msys2`, `go`, `ruby`, `node`, `java`, `cmake`, `ninja`). `catalog search ninja` resolves to Ninja Fast Build Tool with recommended version `1.12.1`.
- **Red-Flag Failure Indicators:** Catalog list is empty; search crashes on valid query; invalid JSON output.

---

### Step 6: Multi-Language Gateway Execution
- **Purpose:** Verify tool execution through ULVO gateway across non-Python ecosystems without manual PATH manipulation.
- **Action:**
  ```powershell
  E:\Workspace\Active\ULVO\target\release\ulvo.exe exec --tool go -- go version
  E:\Workspace\Active\ULVO\target\release\ulvo.exe exec --tool ruby -- ruby -v
  E:\Workspace\Active\ULVO\target\release\ulvo.exe exec --tool gcc -- gcc --version
  ```
- **Expected Outcome:**
  - Go prints `go version go1.26.5 windows/amd64`.
  - Ruby prints `ruby 4.0.1 ... [x64-mingw-ucrt]`.
  - GCC prints `gcc.exe (Rev3, Built by MSYS2 project) 14.2.0`.
  - All exit with code `0`.
- **Red-Flag Failure Indicators:** Tool not found error; execution timeout; non-zero exit code.

---

### Step 7: Project Connector & Dependency Inspection
- **Purpose:** Inspect project metadata, language requirements, and dependency manifests.
- **Action:**
  ```powershell
  E:\Workspace\Active\ULVO\target\release\ulvo.exe project connector E:\Workspace\Active\NexusNode\server
  ```
- **Expected Outcome:** Resolves project `NexusNode/server`, detects Python runtime requirement (3.11), lists dependency manifests (`pyproject.toml` / `requirements.txt`), and shows resolution status.
- **Red-Flag Failure Indicators:** Failure to locate project manifests; unhandled IO errors.

---

### Step 8: Ephemeral Project Execution Without `.venv` Pollution
- **Purpose:** Verify that running Python scripts through ULVO creates zero project-local `.venv` folders and cleans up.
- **Action:**
  ```powershell
  E:\Workspace\Active\ULVO\target\release\ulvo.exe run E:\Workspace\Active\NexusNode\server -- python -c "import sys; print('ULVO_RUN_OK', sys.version_info[:2])"
  Test-Path "E:\Workspace\Active\NexusNode\server\.venv"
  ```
- **Expected Outcome:** Prints `ULVO_RUN_OK (3, 11)`. `Test-Path` returns `False` (zero project pollution).
- **Red-Flag Failure Indicators:** `.venv` created inside `NexusNode\server`; script execution fails; exit code non-zero.

---

### Step 9: Deep Learning Framework & CUDA GPU Acceleration
- **Purpose:** Verify PyTorch execution with hardware CUDA acceleration on the host GPU.
- **Action:**
  ```powershell
  E:\Workspace\Active\ULVO\target\release\ulvo.exe run E:\Workspace\Academic\sem5\DL\exp1 -- python -c "import torch; print('TORCH:', torch.__version__, 'CUDA:', torch.cuda.is_available(), 'GPU:', torch.cuda.get_device_name(0))"
  ```
- **Expected Outcome:** Prints `TORCH: 2.6.0+cu124 CUDA: True GPU: NVIDIA GeForce RTX 2050`. Exit code is `0`.
- **Red-Flag Failure Indicators:** `CUDA: False`; GPU device not detected; driver link error.

---

### Step 10: Environment Lifecycle Audit & Safe Quiescence Pruning
- **Purpose:** Audit active/ephemeral environments in the vault and reclaim inactive ones without touching running processes.
- **Action:**
  ```powershell
  E:\Workspace\Active\ULVO\target\release\ulvo.exe env audit
  E:\Workspace\Active\ULVO\target\release\ulvo.exe env prune
  ```
- **Expected Outcome:** `audit` lists environment counts and physical storage. `prune` reports scan and reclamation results, strictly preserving unknown or active lease directories.
- **Red-Flag Failure Indicators:** Deletion of directories outside the vault; failure to observe active leases; crash on unowned directories.

---

### Step 11: Package Cache Governance & Audit
- **Purpose:** Inspect physical allocation across package caches (`uv-cache`, shared packages, Cargo).
- **Action:**
  ```powershell
  E:\Workspace\Active\ULVO\target\release\ulvo.exe cache status
  ```
- **Expected Outcome:** Shows itemized physical allocation for `uv-cache`, shared immutable packages (e.g. PyTorch), and Cargo crates cache.
- **Red-Flag Failure Indicators:** Negative storage values; unhandled panic; failure to read cache directory.

---

### Step 12: Content-Addressable Storage (CAS) Integrity Verification
- **Purpose:** Verify that CAS dependency store maintains immutable artifact addressing.
- **Action:**
  ```powershell
  E:\Workspace\Active\ULVO\target\release\ulvo.exe storage cache-audit
  ```
- **Expected Outcome:** Audits package cache components and verifies CAS integrity. Exit code is `0`.
- **Red-Flag Failure Indicators:** Checksum mismatch; reported repository corruption.

---

### Step 13: Background Reconciliation Agent Verification
- **Purpose:** Verify status reporting and IPC communication with the ULVO background reconciliation agent.
- **Action:**
  ```powershell
  E:\Workspace\Active\ULVO\target\release\ulvo.exe agent status
  ```
- **Expected Outcome:** Returns agent status (`ONLINE` or `OFFLINE (graceful direct mode)`). Never panics.
- **Red-Flag Failure Indicators:** Hanging execution; unhandled connection panic.

---

### Step 14: Desktop GUI Cockpit Launch, Visual Exploration & Graceful Window Close
- **Purpose:** Verify that the native desktop application (`ulvo-desktop.exe`) renders its visual window, loads backend data, and closes cleanly.
- **Action:**
  1. Launch `E:\Workspace\Active\ULVO\target\release\ulvo-desktop.exe` (or run `powershell -File E:\Workspace\Active\ULVO\scripts\test-desktop-gui.ps1`).
  2. Visually confirm:
     - Window title: `ULVO Control Center`.
     - Cockpit tabs: Overview, Inventory, Catalog, Projects, Environments, Cache, Storage, Settings.
     - Dark modern UI layout with status pills (`Vault: OK`, `GPU: ACTIVE`).
  3. Click the close button (`X`) or allow the test script to issue `WM_CLOSE`.
- **Expected Outcome:** Window renders promptly (<2.5 seconds), memory footprint is bounded (~160 MB combined with WebView2), and closes gracefully upon `WM_CLOSE`.
- **Red-Flag Failure Indicators:** Blank white window; WebView2 crash dialog; UI freezes; process refuses to exit on close.

---

### Step 15: Transactional Installer Lifecycle & Safe Uninstallation
- **Purpose:** Verify transactional install, atomic directory swap, manifest validation, and clean uninstallation without deleting vault data.
- **Action:**
  ```powershell
  powershell.exe -NoProfile -ExecutionPolicy Bypass -File "E:\Workspace\Active\ULVO\scripts\test-installer-lifecycle.ps1"
  ```
- **Expected Outcome:** All 7 test suites pass cleanly:
  - Clean install -> PASS
  - Artifact verification -> PASS
  - Transactional upgrade with separate staging -> PASS
  - Interrupted transaction crash recovery -> PASS
  - Uninstaller security defenses (blocks path traversal & sibling attacks) -> PASS
  - Hash-mismatch and user file preservation -> PASS
  - Clean removal with vault data strictly preserved -> PASS
- **Red-Flag Failure Indicators:** Any test fails; user configuration deleted; vault data purged; registry keys left dangling.

---

## 3. Human Acceptance Report Template

Please copy and fill out this verification table upon completing the tests above:

```markdown
### ULVO v1.2.0 Human Acceptance Sign-Off

**Tester Name:** [ Your Name / Handle ]  
**Test Date & Time:** [ YYYY-MM-DD HH:MM UTC ]  
**Test Environment:** Windows 11 x64 (Build: 22631+), NVIDIA RTX 2050  
**Distribution ZIP Tested:** `dist/ulvo-v1.2.0-windows-x64-portable.zip`  
**Distribution Checksum Verified:** [ YES / NO ]  

| Step # | Test Name | Result (PASS / FAIL) | Observations / Notes |
|:------:|:----------|:--------------------:|:---------------------|
| 1 | Storage Preflight & Safety Reserve | PASS / FAIL | |
| 2 | Version & CLI Banner Output | PASS / FAIL | |
| 3 | System-Wide Doctor Diagnostic | PASS / FAIL | |
| 4 | Universal Toolchain Inventory Scan | PASS / FAIL | |
| 5 | Universal Catalog & Search Resolution | PASS / FAIL | |
| 6 | Multi-Language Gateway Execution | PASS / FAIL | |
| 7 | Project Connector Inspection | PASS / FAIL | |
| 8 | Ephemeral Project Run (Zero .venv) | PASS / FAIL | |
| 9 | PyTorch + CUDA GPU Acceleration | PASS / FAIL | |
| 10 | Environment Lifecycle & Prune | PASS / FAIL | |
| 11 | Package Cache Governance | PASS / FAIL | |
| 12 | CAS Store Integrity | PASS / FAIL | |
| 13 | Background Agent Verification | PASS / FAIL | |
| 14 | Desktop GUI Cockpit & Close | PASS / FAIL | |
| 15 | Transactional Installer & Uninstaller | PASS / FAIL | |

### Release Decision:
- [ ] **APPROVED FOR STABLE RELEASE:** The tested distribution meets all production reliability, security, and performance standards. Proceed with GitHub `v1.2.0` release creation and tag publishing.
- [ ] **REJECTED / ACTION REQUIRED:** The following issues were discovered and must be addressed before release: [ Specify issues ]

**Signature:** ___________________________  
**Date:** _______________________________
```
