# Phase 0 hardening review — 2026-10-16

Status: focused hardening completed and Linux-tested; Phase 0 remains provisionally accepted, not a completed Windows-first prototype. No commit, stage, push, publication, Phase 1, installation or global configuration change performed.

## Changes

Modified six existing files:
- .gitignore: default target/ at any depth, __pycache__/ and *.py[cod] exclusions.
- crates/ulvo-core/src/lib.rs: retained original observations in evidence; stable primary summary; union sources/capabilities; version-conflict diagnostic; transitive alias grouping; configured references separated; backend/distribution separation retained; three regression tests.
- crates/ulvo-discovery/src/lib.rs: sort/deduplicate immediate-child and absolute PATH candidates before limits; stable known roots/settings order; initialize evidence; oversized/reversed selection regression.
- tests/validate.py: derive guarded workspace root from script, require fresh output directory, exclusive writes, eight metadata-only CLI journeys, no automatic inventory verification.
- tests/report.py: guarded relocatable root, new output required, historical report labeling, exclude Python cache inventory. Never overwrite the reviewed report by default.
- docs/PHASE0_IMPLEMENTATION_REPORT.md: complete 41-package table coverage, missing created-file inventory correction, historical/new-result distinction, updated ordering/evidence semantics.

New source/review files:
- tests/repo_guard.py: existing script and expected workspace sentinels, ancestor symlink/reparse rejection, repository-contained outputs, exclusive fresh directories/files.
- tests/test_hardening.py: six helper regressions, unique retained fixture directories; mocked link metadata rejection (not native junction proof).
- docs/PHASE0_HARDENING_REPORT.md (this report).
- docs/WINDOWS_NATIVE_BUILD_PROPOSAL.md (research only).

All new baseline/logs/functional stdout/stderr/synthetic regression files are under docs/evidence/hardening-20261016; see changed-files.txt there for exact paths. Existing raw evidence and Cargo.lock verified unchanged by SHA256 baseline. Synthetic regression fixture files intentionally remain; no cleanup/deletion performed.

## Actual verification

- Focused core/discovery log: 14 passed. The initial surrounding invocation timed out although its retained log showed successful tests; not treated as a clean invocation success.
- First Python shell precheck failed due to command quoting; 02-python.txt retains failure status.
- First actual Python run: five passed, missing-script regression failed. Guard tightened to require helper file existence; fixtures made unique per run. Failure retained in 03-python.txt.
- Final Python helper regressions: 6 passed, exit 0; 04-python.txt.
- Full Rust workspace suite: 22 passed, 0 failed, exit 0 (adapters 6, CLI 2, core 5, discovery 9); 05-workspace.txt.
- Linux workspace build: exit 0; 06-build.txt.
- Metadata-only CLI validator: eight journeys, expected exit codes 0/1/2, exit 0; 07-functional.txt and functional/functional.json.
- Inventory returned 34 records across three backend labels, none verified; fixtures unchanged. Existing adapter unit tests still include fixed Python version and sleep timeout controls; no automatic inventory --verify was enabled.

## Exact successful commands

Run inside Ubuntu from the physically guarded repository. Verify Windows reparse ancestors and WSL output containment first; no command here authorizes writes through links. Existing Rust/Cargo paths were used, not installed or upgraded.

```sh
export CARGO_HOME="$PWD/.cargo-local"
export CARGO_TARGET_DIR="$PWD/target-local"
export RUSTC=/home/anmol/.cargo/bin/rustc
export RUSTDOC=/home/anmol/.cargo/bin/rustdoc
/home/anmol/.cargo/bin/cargo test --workspace --locked --offline
/home/anmol/.cargo/bin/cargo build --workspace --locked --offline
/usr/bin/python3 -B tests/test_hardening.py
/usr/bin/python3 -B tests/validate.py --output docs/evidence/hardening-20261016/functional
```

The shown functional destination already exists: reproduction must choose a NEW contained output directory. Regression fixture directories are unique; old logs stay intact. Raw runs redirected into fresh numbered log files and appended explicit exit_code values. Cargo.lock/dependencies unchanged; no downloads needed for the Linux build.

## Behavior and trade-offs

Summary versions prefer Verified observations then Present/Failed/Unsupported/Configured, with deterministic tie-breaking; conflicting original versions/status/diagnostics remain in evidence. Identical normalized observations are deduplicated, not their evidence distinctions. Evidence is an additive JSON field; original logs retain the earlier schema shape. Native configured-reference status is not upgraded through merging. The merging routine is intended for internally generated observations, not an untrusted externally supplied registry.

Deterministic child selection now reads all immediate entries before retaining the first 128 sorted paths. This removes filesystem-order dependence but increases enumeration/memory cost for large directories; no recursive drive scans or extra roots were added. PATH lexical selection is inventory policy, not shell executable-resolution precedence. No total inventory timeout is provided.

Shared guards retain path-based TOCTOU limitations and WSL Windows-junction metadata limitations; Python link regression uses mocked metadata. Repository relocation is tested through a synthetic structurally equivalent workspace, not a second actual build. Helper roots are derived from files plus ULVO workspace/.git sentinels; callers must still approve the physical repository before execution.

## Remaining acceptance gaps

Windows-native build/run and file-ID tests; true cross-distribution WSL discovery; MSYS2 execution; complete VS Code JSONC/runtime-array/extensions support; automatic ownership/vendor policy; native manifest semantic resolution; OS-enforced subprocess no-write isolation/descendant cleanup; race-proof Windows containment; total filesystem/inventory bounds; x86_64-specific existing adapter test portability; whole-machine no-global-side-effect evidence.

The older architecture/requirements/validation documents describe the original milestone in places; this review and corrected implementation report supersede original enumeration and source-only-merge descriptions. They were not broadly rewritten in this scoped pass.

Raw inventories remain private in the local repository. No external party received repository contents. Research retrieved only public documentation. Windows proposal establishes no complete available-tool-only route; missing Windows Rust/std and unverified linker/runtime compatibility require approval-gated follow-up.
