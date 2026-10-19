# Gravitas Decision Record: Universal Content-Addressable Store & Multi-Language Adoption

## Context
ULVO Stage 2.9.4 mandates the implementation of:
1. Machine-wide development toolchain discovery across Windows PATH, standard installation directories, user profiles, Scoop, MSYS2, VS Code, and WSL metadata.
2. A formal Language Adoption Registry governing 9 lifecycle states (`EXTERNAL_DISCOVERED` through `MANAGED` / `BLOCKED`).
3. A Universal Content-Addressable Dependency Store (CAS) using cryptographic SHA-256 identifiers, multi-attribute compatibility metadata (ecosystem, ABI, architecture, CUDA compute capability), atomic transactional publication, reference counting, and corruption verification.
4. Correction of unsafe Python/GPU package sharing in `ProjectOrchestrator::link_shared_packages`, replacing directory copy assumptions with distribution-verified artifacts and explicit error handling.
5. First-class adoption of Ruby (4.0.1) and Go (1.26.5) alongside managed MSYS2 (UCRT64 C/C++).
6. Gateway execution via `ulvo exec --tool <name>` with clean, process-local environments and Windows verbatim path (`\\?\`) normalization.

## Architectural Decisions

### 1. Universal Content-Addressable Storage (CAS) Coordination
- **Action**: Implement `CasArtifactDescriptor`, `CasStorageCoordinator`, and `CasManifest` in `crates/ulvo-core/src/cas.rs` and `crates/ulvo-adapters/src/cas_storage.rs`.
- **Reason**: Ecosystems have specialized package layouts (`uv-cache`, `pnpm-store`, `cargo registry`, `go/pkg/mod`, `gems`, `pacman`). Replacing them with a single monolithic format breaks native tools. Coordinating them under a unified cryptographic identity layer gives cross-ecosystem visibility and safe deduplication without breaking package managers.
- **Evidence**: Deduplication requires SHA-256 content verification, reference tracking, and zero deletion of actively referenced artifacts.
- **Risk**: Hardlink cross-volume limits.
- **Mitigation**: Detect volume boundaries; use atomic hardlinks when on the same NTFS volume (`E:`), falling back to verified copy with transaction logs.

### 2. Transactional Python/GPU Package Sharing
- **Action**: Deprecate indiscriminate tree-copy in `ProjectOrchestrator::link_shared_packages`. Require manifest/wheel metadata validation (`.dist-info`), Python ABI matching (`cp311`), CUDA capability matching (`sm_86`), and atomic hardlink materialization with explicit error propagation.
- **Reason**: Indiscriminately copying `site-packages` corrupts environments with mismatched native binary extensions, breaks GPU acceleration, and masks failures.
- **Evidence**: Real GPU projects (`Academic\sem5\DL\exp1`) run PyTorch and torchvision with CUDA 12.4. Mismatched DLLs crash with WinError 126 or CUDA driver errors.
- **Risk**: Rejection of unverified local packages.
- **Mitigation**: Retain explicit diagnostics; allow fallback to direct uv-isolated materialization when shared artifact verification does not pass.

### 3. Extended Path Normalization for External Toolchains (e.g. GCC/MinGW)
- **Action**: Normalize paths passed to external toolchain compilers and execution gateway by stripping `\\?\` verbatim prefixes.
- **Reason**: MinGW GCC cannot parse Windows verbatim extended-length prefixes (`\\?\E:\...`), resulting in `fatal error: cannot execute 'cc1': CreateProcess: No such file or directory`.
- **Evidence**: Verified with PowerShell reproducer: `\\?\...gcc.exe` fails; normalized `E:\...gcc.exe` executes and compiles successfully.
- **Risk**: Paths exceeding 260 characters on legacy Windows without long path support.
- **Mitigation**: ULVO workspace is located at `E:\Workspace\Active\ULVO`, well within standard MAX_PATH limits.

### 4. Non-Destructive Multi-Language Adoption
- **Action**: Adopt external installations (Ruby, Go, JDK, MSYS2) into the Vault Registry using `AdoptionStrategy::ReferenceExternal` or `AdoptionStrategy::CopyRelocatable` without modifying global Windows PATH or registry.
- **Reason**: Global PATH mutations cause machine-wide regressions for user processes.
- **Evidence**: Process-local environment injection via `GatewayResolver` isolates runtime toolchains per process invocation.
