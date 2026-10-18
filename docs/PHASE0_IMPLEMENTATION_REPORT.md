# ULVO Phase 0 implementation report

Date: 2026-10-16. Repository: E:\Workspace\Active\ULVO.

## Status

**Partial implementation, not full Phase 0 acceptance.** Four-crate read-only foundation builds and passes Linux/WSL tests. Windows-native execution, complete WSL distribution discovery, MSYS2 execution, and OS-enforced read-only subprocess isolation remain incomplete.

## Architecture implemented

Core deterministic models/version parsing/dedup; native-header and bounded-process adapters; bounded toolchain and VS Code reference discovery; native manifest detection and strict ULVO TOML parser; clap human/JSON CLI. Backend identities stay separate. Ownership supports external/experimental/managed/unknown but discovery currently uses unknown. Configured references are never executed. No package installation/environment provisioning/GC/MCP/service/database code.

## Raw build and test results

Historical initial validation: Rust/Cargo 1.96.0, x86_64-unknown-linux-gnu. Initial final `cargo test --workspace --locked`: exit 0, 18 passed, 0 failed (6 adapters, 2 CLI, 2 core, 8 discovery). `cargo build --workspace --locked`: exit 0. Python functional validator: exit 0, nine journeys. Raw logs: `docs/evidence/10-final-tests.txt`, `11-final-build.txt`, `12-functional.txt`; per-command stdout/stderr also retained. Subsequent hardening results are recorded separately; old evidence is not overwritten.

First run compiled core but failed doctest launch because rustdoc was missing on PATH; fixed by explicit existing RUSTDOC path. Initial bridge shell invocation and a read-only inline Python quoting attempt failed before intended operations; corrected inspections and report generation followed. No installations or elevated fallback were used.

Exact build environment and commands appear in PHASE0_VALIDATION.md. Writable Cargo home `.cargo-local` and build outputs `target-local` are Git-excluded. Cargo's registry cache did not use the existing WSL Cargo home. Rust and rustdoc are explicit existing executable paths; rustup installation unchanged by targeted operations.

## Executed functional commands

- `/mnt/e/Workspace/Active/ULVO/target-local/debug/ulvo --help` → exit 0
- `/mnt/e/Workspace/Active/ULVO/target-local/debug/ulvo --version` → exit 0
- `/mnt/e/Workspace/Active/ULVO/target-local/debug/ulvo inventory --all-backends --json` → exit 0
- `/mnt/e/Workspace/Active/ULVO/target-local/debug/ulvo inventory --all-backends --verify --json` → exit 0
- `/mnt/e/Workspace/Active/ULVO/target-local/debug/ulvo inspect /mnt/e/Workspace/Active/ULVO/tests/fixtures/valid` → exit 0
- `/mnt/e/Workspace/Active/ULVO/target-local/debug/ulvo inspect /mnt/e/Workspace/Active/ULVO/tests/fixtures/valid --json` → exit 0
- `/mnt/e/Workspace/Active/ULVO/target-local/debug/ulvo inspect /mnt/e/Workspace/Active/ULVO/tests/fixtures/invalid --json` → exit 1
- `/mnt/e/Workspace/Active/ULVO/target-local/debug/ulvo inspect /mnt/e/Workspace/Active/ULVO/tests/fixtures/missing --json` → exit 1
- `/mnt/e/Workspace/Active/ULVO/target-local/debug/ulvo install anything` → exit 2

Inventory: 34 candidate records across Windows, WSL, MSYS2 labels. Only WSL probes can be verified by this Linux binary. Verified versions observed:

- cargo: 1.77.0-nightly (wsl)
- rustc: 1.77.0-nightly (wsl)
- cargo: 1.78.0-nightly (wsl)
- rustc: 1.78.0-nightly (wsl)
- cargo: 1.80.0-nightly (wsl)
- rustc: 1.80.0-nightly (wsl)
- cargo: 1.98.0-nightly (wsl)
- rustc: 1.98.0-nightly (wsl)
- cargo: 1.96.0 (wsl)
- rustc: 1.96.0 (wsl)
- node: 22.22.1 (wsl)
- python3: 3.14.4 (wsl)

Three synthetic fixture content hashes matched before/after. JSON inventory, valid inspection, and controlled invalid/missing errors were parsed. Unsupported installation command rejected with exit 2.

## Supported and unsupported functionality

Supported: metadata inventory, optional conservative native version checks, ELF/PE architecture, deterministic domain output, backend/distribution identity, Unix device/inode alias consolidation, selected known installation roots/PATH, strict-JSON VS Code profile references, recognized top-level native manifests, schema-version-1 ULVO TOML, human and JSON output.

Unsupported/incomplete: native Windows build/run, Windows file-ID hardlink deduplication, Windows-to-WSL launcher/cross-distribution native discovery, MSYS2 execution context, complete versioned Python/registry/user-location discovery, E:-managed/experimental runtimes, general VS Code JSONC and runtime arrays/extensions, complete vendor/ownership policies, runtime constraint resolution, native manifest semantic parsing, total inventory timeout, OS-enforced subprocess no-write isolation.

All non-Windows hosts are currently treated as WSL under this prototype contract. Hardening sorts and deduplicates immediate directory/PATH candidates before limits; immediate-directory enumeration reads all entries to select deterministically, without recursive scans. Alias consolidation retains original observations in `evidence`, unions sources/capabilities, prefers verified summaries, and reports version conflicts. Configured references stay separate from filesystem observations. Broad usability must not be inferred from a successful version probe.

## Security checks and limitations

Windows repository ancestors and existing reparse points were checked before creation; WSL writes checked physical repository path and symlink-free output trees. No project scripts executed; VS Code configuration remains data-only. No existing experiment directories or global package caches were targeted. No PATH, registry, ACL, WSL packages/config, or installed toolchain changes were performed by targeted commands. Only approved crates.io downloads and contained files/builds occurred.

No whole-machine audit demonstrates that all third-party build/probe code made zero global writes. Version probes inherit environment/current directory and are not a sandbox; direct-child timeout is enforced, descendant cleanup is incomplete. Filesystem reads are not cancellable. Path guards have TOCTOU limits; WSL may not expose all Windows junctions as symlinks. Read-only-by-default inventory is the defensible strict mode. Review SECURITY.md before using --verify on untrusted installations.

## Dependencies and provenance

Direct external dependencies: serde (domain serialization), serde_json (JSON), toml (strict manifest parsing), clap (CLI arguments). Exact direct and transitive versions below. Registry sources validated against official crates.io index; Cargo.lock records checksums. No alternate Git dependency source.

Table coverage: all 41 locked packages (four workspace and 37 registry packages), including serde_spanned and strsim. No dependencies were added by the hardening pass.

| Package | Version | Source |
|---|---|---|
| anstream | 1.0.0 | registry+https://github.com/rust-lang/crates.io-index |
| anstyle | 1.0.14 | registry+https://github.com/rust-lang/crates.io-index |
| anstyle-parse | 1.0.0 | registry+https://github.com/rust-lang/crates.io-index |
| anstyle-query | 1.1.5 | registry+https://github.com/rust-lang/crates.io-index |
| anstyle-wincon | 3.0.11 | registry+https://github.com/rust-lang/crates.io-index |
| clap | 4.6.7 | registry+https://github.com/rust-lang/crates.io-index |
| clap_builder | 4.6.7 | registry+https://github.com/rust-lang/crates.io-index |
| clap_derive | 4.6.7 | registry+https://github.com/rust-lang/crates.io-index |
| clap_lex | 1.1.1 | registry+https://github.com/rust-lang/crates.io-index |
| colorchoice | 1.0.5 | registry+https://github.com/rust-lang/crates.io-index |
| equivalent | 1.0.3 | registry+https://github.com/rust-lang/crates.io-index |
| hashbrown | 0.17.1 | registry+https://github.com/rust-lang/crates.io-index |
| heck | 0.5.0 | registry+https://github.com/rust-lang/crates.io-index |
| indexmap | 2.14.2 | registry+https://github.com/rust-lang/crates.io-index |
| is_terminal_polyfill | 1.70.2 | registry+https://github.com/rust-lang/crates.io-index |
| itoa | 1.0.18 | registry+https://github.com/rust-lang/crates.io-index |
| memchr | 2.8.3 | registry+https://github.com/rust-lang/crates.io-index |
| once_cell_polyfill | 1.70.2 | registry+https://github.com/rust-lang/crates.io-index |
| proc-macro2 | 1.0.107 | registry+https://github.com/rust-lang/crates.io-index |
| quote | 1.0.47 | registry+https://github.com/rust-lang/crates.io-index |
| serde | 1.0.229 | registry+https://github.com/rust-lang/crates.io-index |
| serde_core | 1.0.229 | registry+https://github.com/rust-lang/crates.io-index |
| serde_derive | 1.0.229 | registry+https://github.com/rust-lang/crates.io-index |
| serde_json | 1.0.154 | registry+https://github.com/rust-lang/crates.io-index |
| serde_spanned | 0.6.9 | registry+https://github.com/rust-lang/crates.io-index |
| strsim | 0.11.1 | registry+https://github.com/rust-lang/crates.io-index |
| syn | 3.0.9 | registry+https://github.com/rust-lang/crates.io-index |
| toml | 0.8.23 | registry+https://github.com/rust-lang/crates.io-index |
| toml_datetime | 0.6.11 | registry+https://github.com/rust-lang/crates.io-index |
| toml_edit | 0.22.27 | registry+https://github.com/rust-lang/crates.io-index |
| toml_write | 0.1.2 | registry+https://github.com/rust-lang/crates.io-index |
| ulvo-adapters | 0.1.0 | workspace |
| ulvo-cli | 0.1.0 | workspace |
| ulvo-core | 0.1.0 | workspace |
| ulvo-discovery | 0.1.0 | workspace |
| unicode-ident | 1.0.27 | registry+https://github.com/rust-lang/crates.io-index |
| utf8parse | 0.2.2 | registry+https://github.com/rust-lang/crates.io-index |
| windows-link | 0.2.1 | registry+https://github.com/rust-lang/crates.io-index |
| windows-sys | 0.61.2 | registry+https://github.com/rust-lang/crates.io-index |
| winnow | 0.7.15 | registry+https://github.com/rust-lang/crates.io-index |
| zmij | 1.0.23 | registry+https://github.com/rust-lang/crates.io-index |

## Files created

All source/documentation/test/evidence files are new; repository initially contained only .git. Cargo cache and target artifacts are intentionally excluded from the list and Git.

- `.gitignore`
- `Cargo.lock`
- `Cargo.toml`
- `README.md`
- `crates/ulvo-adapters/Cargo.toml`
- `crates/ulvo-adapters/src/lib.rs`
- `crates/ulvo-cli/Cargo.toml`
- `crates/ulvo-cli/src/main.rs`
- `crates/ulvo-core/Cargo.toml`
- `crates/ulvo-core/src/lib.rs`
- `crates/ulvo-discovery/Cargo.toml`
- `crates/ulvo-discovery/src/lib.rs`
- `docs/ARCHITECTURE.md`
- `docs/PHASE0_IMPLEMENTATION_REPORT.md`
- `docs/PHASE0_VALIDATION.md`
- `docs/REQUIREMENTS.md`
- `docs/SECURITY.md`
- `docs/decisions/ADR-001.md`
- `docs/evidence/01-core.txt`
- `docs/evidence/02-core.txt`
- `docs/evidence/03-adapters.txt`
- `docs/evidence/04-discovery.txt`
- `docs/evidence/05-workspace.txt`
- `docs/evidence/06-build.txt`
- `docs/evidence/07-safety-tests.txt`
- `docs/evidence/08-build.txt`
- `docs/evidence/09-functional.json`
- `docs/evidence/10-final-tests.txt`
- `docs/evidence/11-final-build.txt`
- `docs/evidence/12-functional.txt`
- `docs/evidence/13-git-review.txt`
- `docs/evidence/help.stderr.txt`
- `docs/evidence/help.stdout.txt`
- `docs/evidence/inspect-human.stderr.txt`
- `docs/evidence/inspect-human.stdout.txt`
- `docs/evidence/inspect.stderr.txt`
- `docs/evidence/inspect.stdout.txt`
- `docs/evidence/invalid.stderr.txt`
- `docs/evidence/invalid.stdout.txt`
- `docs/evidence/inventory-verified.stderr.txt`
- `docs/evidence/inventory-verified.stdout.txt`
- `docs/evidence/inventory.stderr.txt`
- `docs/evidence/inventory.stdout.txt`
- `docs/evidence/missing.stderr.txt`
- `docs/evidence/missing.stdout.txt`
- `docs/evidence/mutating-rejected.stderr.txt`
- `docs/evidence/mutating-rejected.stdout.txt`
- `docs/evidence/version.stderr.txt`
- `docs/evidence/version.stdout.txt`
- `tests/fixtures/invalid/.ulvo/manifest.toml`
- `tests/fixtures/valid/.ulvo/manifest.toml`
- `tests/fixtures/valid/package.json`
- `tests/report.py`
- `tests/repo_guard.py`
- `tests/test_hardening.py`
- `tests/validate.py`

The new private hardening evidence and review files are inventoried in the hardening report; build/cache artifacts remain excluded.

## Git diff and requested lockfile commit

Initial tracked files: none; working tree clean. Implementation files remain untracked pending review; plain git diff therefore does not describe them. See docs/evidence/13-git-review.txt for status and untracked-source line/file summary. Cargo.lock is generated and retained, but **not committed**: repository/global Git user.name and user.email queries returned no values. No author identity was invented or global Git configuration changed. The requested lockfile commit requires an approved author name/email; no unrelated files were staged or committed.

## Next recommended milestone

Complete Windows-native and true multi-backend execution support with a reviewed toolchain provision, job-object/process-group confinement, sanitized probe environments, handle-relative containment, Windows file identities, full VS Code configuration coverage, and policy-based ownership. Validate all acceptance criteria before declaring Phase 0 complete. Resolve Git identity to commit Cargo.lock without unrelated changes.
