# ULVO Control Center — GUI Architecture & Branding Specification

**Document Version:** 1.0.0  
**Status:** Non-Executable Architectural Specification (Stage 2.9.1 Milestone Deliverable)  
**Target Platform:** Windows 11 Desktop (64-bit)  
**Primary Framework Feasibility:** Tauri 2.0 (Rust backend + Web frontend)  

---

## 1. Executive Summary & Design Vision

ULVO (**Universal Language & Virtual Environment Orchestrator**) operates under the foundational doctrine:

> **STORE ONCE. ISOLATE PER PROJECT. MATERIALIZE ON DEMAND. EXECUTE THROUGH ULVO. RECLAIM SAFELY.**

While the native Windows CLI (`ulvo.exe`) serves automated CI/CD and terminal-centric workflows, the **ULVO Control Center** provides a high-density, real-time visual cockpit for developers managing multi-language runtimes, large GPU model environments, project leases, and workstation storage deduplication.

The GUI does **not** implement separate execution or lifecycle logic. It directly binds to `ulvo-core`, `ulvo-discovery`, and `ulvo-adapters`, guaranteeing absolute behavioral equivalence between the CLI and GUI.

---

## 2. Branding & Symbol System

### 2.1 The ULVO Gateway Vault Symbol

The official ULVO visual symbol represents an interlocking hexagonal vault gateway:

```
          /─────────────\
         /   /───────\   \
        /   /    ▲    \   \
       /   /    / \    \   \
      |   |    /___\    |   |
      |   |   [  ●  ]   |   |
      |   |    \   /    |   |
       \   \    \_/    /   /
        \   \         /   /
         \   \───────/   /
          \─────────────/
```

- **Outer Hexagon:** Represents the **Single-Root Vault** (`E:\Workspace\Active\ULVO\vault`), enforcing strict filesystem confinement and isolation boundaries.
- **Inner Prism:** Represents the **Execution Gateway**, resolving multi-language tools and process-local environments on demand.
- **Central Core (●):** Represents the **Immutable Shared Package Store**, achieving single-instance physical storage with zero-copy hardlinked materialization.

### 2.2 Color System (Dark-First Visual Language)

Designed specifically for developers in low-light environments, with high-contrast functional color coding:

| Token Name | Hex Code | Purpose |
|:---|:---|:---|
| `--color-bg-base` | `#08090C` | Deep Void Black (App background) |
| `--color-bg-surface` | `#11141A` | Elevated Panels and Card containers |
| `--color-bg-subtle` | `#1A1F29` | Table row hover, input backgrounds |
| `--color-border` | `#262E3D` | Structural dividers and container borders |
| `--color-cyan-glow` | `#00E5FF` | Active Gateway execution, Primary Accent |
| `--color-emerald` | `#00E676` | Verified / Managed / Quiescent / Passing |
| `--color-amber` | `#FFB300` | Warning / Staging / External Discovered |
| `--color-crimson` | `#FF1744` | Error / Blocked Capacity / Mismatch |
| `--color-gpu-green` | `#76B900` | NVIDIA Hardware & CUDA Telemetry |
| `--color-text-main` | `#ECEFF4` | Primary typography |
| `--color-text-muted`| `#8892B0` | Metadata, paths, timestamps |

---

## 3. Control Center Information Architecture

The application layout follows a three-pane modular cockpit:

```
┌──────────────────────────────────────────────────────────────────────────────────────┐
│ [ULVO Vault Symbol]  ULVO Control Center v0.3.0        [Drive E: 7.87 GB Free] [● OK]│
├──────────────┬───────────────────────────────────────────────────────────────────────┤
│ NAVIGATION   │ MAIN WORKSPACE VIEW                                                   │
│              │                                                                       │
│ ◈ Projects   │ [Projects Dashboard]                                                  │
│   • Active   │ ┌───────────────────────────────────────────────────────────────────┐ │
│   • Leased   │ │ Academic\sem5\DL\exp1   [GPU sm_86] [Python 3.11]  [EPHEMERAL]    │ │
│              │ │ State: Quiescent  | Allocated: 4.62 GB (Shared: 4.62 GB) [0 Bytes]│ │
│ ◈ Runtimes   │ ├───────────────────────────────────────────────────────────────────┤ │
│   • Python   │ │ NexusNode\server        [Web Server] [Python 3.11] [MANAGED]      │ │
│   • Rust     │ │ State: Active (PID 4912) | Lease Expires: 14m 22s                 │ │
│   • Node.js  │ └───────────────────────────────────────────────────────────────────┘ │
│   • Toolchains│                                                                       │
│              │ [GPU Hardware & Compatibility Inspector]                              │
│ ◈ GPU Copilot│ ┌───────────────────────────────────────────────────────────────────┐ │
│ ◈ Storage    │ │ NVIDIA GeForce RTX 2050 (4096 MiB VRAM) | Driver 596.49 (CUDA 13.2)│ │
│ ◈ Adoption   │ │ PyTorch 2.6.0+cu124 Build: COMPATIBLE | sm_86: NATIVE             │ │
│ ◈ Security   │ └───────────────────────────────────────────────────────────────────┘ │
├──────────────┴───────────────────────────────────────────────────────────────────────┤
│ STATUS BAR: Vault: E:\Workspace\Active\ULVO\vault | Process-Local Gateway Active     │
└──────────────────────────────────────────────────────────────────────────────────────┘
```

---

## 4. Key Cockpit Modules

### 4.1 Project Environment Dashboard
- **Active vs. Quiescent Lifecycle**: Displays real-time process leases, tracking active OS process IDs via Windows Job Objects.
- **Specification Invalidation Indicator**: Highlights when `requirements.txt` or `manifest.toml` has been edited, warning that next execution will re-materialize.
- **One-Click Reclaim**: Safe reclamation button disabled while active leases remain.

### 4.2 GPU Compatibility & Hardware Inspector
- **Physical Device Telemetry**: Real-time read of GPU model, VRAM utilization, PCIe Bus ID, and NVIDIA display driver version.
- **Framework Build Inspector**: Distinguishes CPU-only wheels (`+cpu`) from CUDA-enabled wheels (`+cu124`), highlighting ABI tags and missing CUDA DLLs.
- **Kernel vs. User-space Boundary**: Visual indicator affirming that display drivers remain system-managed, preventing unauthorized relocation attempts.

### 4.3 Storage & Deduplication Matrix
- **Dual Storage Accounting**: Side-by-side display of:
  - Total Logical Package Footprint (e.g. 18.4 GB across projects).
  - Single-Instance Vault Physical Allocation (e.g. 4.62 GB in shared package store).
  - Net Storage Recovered via Zero-Copy Hardlinks (e.g. 13.78 GB saved).
- **Volume Free-Space Monitor**: Tracks drive free space with a hard safety reserve floor (500 MB), preventing accidental disk saturation.

### 4.4 Toolchain Adoption & Staged Migration Cockpit
Visualizes the 9-stage adoption state machine:
```
[EXTERNAL_DISCOVERED] ➔ [COMPATIBILITY_ASSESSED] ➔ [ADOPTION_PLANNED]
         ➔ [STAGING] ➔ [VERIFIED] ➔ [MANAGED]
         └─ (Or [EXTERNAL_RETAINED] / [BLOCKED])
```
- Dry-run verification commands displayed prior to any state transition.
- Explicit block on direct filesystem movement without verified relocatability.

---

## 5. Rust Core Integration Architecture (Tauri 2.0 Feasibility)

### 5.1 Architecture Model
The GUI will be constructed using **Tauri 2.0**:
- **Backend**: Native Rust binary importing `ulvo-core`, `ulvo-discovery`, and `ulvo-adapters` as internal Cargo dependencies.
- **Frontend**: Lightweight React/TypeScript UI rendered in Windows WebView2 (`Microsoft.Web.WebView2.Core`).
- **Communication**: Zero-copy JSON IPC commands (`tauri::command`), ensuring sub-millisecond query latency.

### 5.2 Windows Build Environment Compatibility
- **Current Compiler**: `x86_64-pc-windows-gnu` (MSYS2 UCRT64 GCC 14.2.0).
- **Tauri Feasibility Assessment**:
  1. Tauri 2.0 supports Windows with WebView2 natively.
  2. While MSVC (`x86_64-pc-windows-msvc`) is the default upstream recommendation for Tauri on Windows due to Windows SDK header access, Tauri *can* be built with `x86_64-pc-windows-gnu` provided MSYS2 UCRT64 packages (`mingw-w64-ucrt-x86_64-webview2-loader`) are present.
  3. Recommendation: Keep core crates purely platform-abstracted. When implementing the GUI in a future milestone, configure the Tauri runner to consume the workspace libraries without altering CLI dependencies.
