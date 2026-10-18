# Phase 0 requirements status

| Requirement | Status and evidence |
|---|---|
| Four-crate workspace builds | Pass on x86_64-unknown-linux-gnu; evidence/11-final-build.txt |
| Automated tests | 18 passed, zero failed; evidence/10-final-tests.txt |
| inventory, inspect, help/version | Executed Linux CLI journeys; evidence/09-functional.json |
| Human and JSON output | Human inspection/help and JSON inventory/inspection/errors executed |
| Windows/WSL/MSYS2 distinction | 34 real candidate records across three labels, path tests; foreign usability not verified |
| All WSL distributions/native toolchains | Partial: current WSL native/rustup inventory; cross-distribution discovery not implemented |
| Windows native execution | Incomplete: no Windows Rust installed, no Windows binary built or executed |
| MSYS2 execution backend | Incomplete: classified/discovered, execution rejected |
| Runtime versions/architecture/vendor | Versions probed selectively, ELF/PE architecture; vendor limited, unknown values explicit |
| Ownership categories | Model includes all four; automatic policy classification incomplete, candidates remain unknown |
| Underlying executable deduplication | Unix device/inode plus canonical paths; Windows hardlink file-ID support incomplete |
| VS Code profiles/config | Selected strict-JSON path references and bounded profiles; JSONC, arrays and extensions incomplete |
| Native project manifests | Top-level name detection only, no resolved dependencies invented |
| ULVO manifest and invalid input | Strict schema-version-1 TOML parser, invalid inputs controlled |
| Read-only behavior | No mutating CLI, fixture hashes unchanged; opt-in subprocess no-write isolation not enforced |
| Global/real-project state unchanged | No targeted mutation; no whole-machine before/after audit |
| No unapproved installation/destruction | No such command executed; only approved crates.io downloads and contained build writes |
| Raw evidence/docs | Included under docs/evidence and required documents |
| Cargo.lock generated/committed | Generated and official sources recorded; commit blocked pending real Git author identity |

**Overall Phase 0 acceptance remains incomplete.** A functional independently reviewable Linux/WSL foundation is delivered; native Windows and complete multi-backend safety/coverage require further implementation and approval for any required toolchain provision.
