# Phase 0 security

## Implemented boundaries

No mutating CLI command exists. Inspection reads only recognized top-level manifests and ULVO TOML; it never evaluates scripts, shells, package hooks, lockfile resolution, or runtime commands supplied by a project. VS Code paths are data-only and unverified. Read limits and traversal checks apply to configuration input. Discovery is bounded to PATH and selected known roots, not entire drives.

Verification is opt-in. Tool/argument pairs are fixed in source, executable paths are canonicalized, conservative installation roots are required, canonical names are checked, and ELF/PE headers are required. The Linux build rejects Windows/MSYS2 execution. `clangd` is a language server, not a compiler. `.NET` version execution is disabled due to potential first-run state mutation.

Probe stdin is null. Stdout/stderr are concurrently drained with 64 KiB retained per stream. Direct child waits are bounded and killed on timeout. Capture return waits are bounded.

## Limits: do not treat this as a sandbox

A native binary from a recognized directory is not cryptographically authenticated. Directory trust, filename checks and a header are not sufficient against a malicious administrator or user-controlled installation. Version output is not proof of tool identity or general usability. Processes inherit environment and current directory; a runtime may load plugins/configuration or have side effects. The program does not enforce OS-level filesystem/network write restrictions on subprocesses. Only the direct child is terminated; descendants and blocked reader threads are not guaranteed to be reaped. Filesystem I/O can block; no inventory-wide timeout exists. Windows containment and process behavior are not natively tested.

Therefore strict no-write use should omit `--verify`. Verification needs a future sandboxed execution backend before claiming enforceable read-only behavior for arbitrary installed binaries.

Traversal checks are path-based, not race-proof handle-relative operations. A concurrent attacker can change paths after checking. On WSL, Windows junction metadata may not be exposed as Unix symlinks. Repository mutation checks additionally inspected Windows ancestors and repository reparse points, but no adversarial concurrency/TOCTOU guarantee is claimed.

## Implementation-session boundary

Approved writes: source, docs, synthetic fixtures, evidence, `.cargo-local`, `target-local`, and Git operations inside E:\Workspace\Active\ULVO. Windows ancestors were checked for reparse points before creation; WSL build writes were gated on the physical repository path and symlink-free output trees. No installations/upgrades, PATH/registry/ACL changes, existing experiment mutations, global package-cache changes, or unrelated project edits were targeted.

Official crates.io dependencies were downloaded under the local Cargo home. Cargo executes third-party build/proc-macro code and is not an OS sandbox. No independent whole-machine before/after audit proves that third-party code or version probes made zero global writes. Synthetic project content hashes were unchanged. Existing projects and experiments were deliberately not opened for modification.

Future strengthening: environment sanitization, explicit binary approval/identity, job/process-group termination, cancellable filesystem I/O, Windows handle-based containment, protected writable temporary roots, and OS-enforced write restrictions.
