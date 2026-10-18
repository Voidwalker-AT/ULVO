# Phase 0 validation

Verified toolchain: rustc 1.96.0 (ac68faa20 2026-05-25), cargo 1.96.0 (30a34c682 2026-05-25), host x86_64-unknown-linux-gnu, LLVM 22.1.2. Python validator: Python 3.14.4. Native Windows Rust was not available on PATH and was not installed.

## Exact final commands inside Ubuntu

```sh
cd /mnt/e/Workspace/Active/ULVO
test "$(pwd -P)" = /mnt/e/Workspace/Active/ULVO
test -z "$(find crates docs tests .cargo-local target-local -type l -print -quit)"
export CARGO_HOME="$PWD/.cargo-local"
export CARGO_TARGET_DIR="$PWD/target-local"
export RUSTC=/home/anmol/.cargo/bin/rustc
export RUSTDOC=/home/anmol/.cargo/bin/rustdoc
/home/anmol/.cargo/bin/cargo test --workspace --locked > docs/evidence/10-final-tests.txt 2>&1
/home/anmol/.cargo/bin/cargo build --workspace --locked > docs/evidence/11-final-build.txt 2>&1
/usr/bin/python3 -B tests/validate.py > docs/evidence/12-functional.txt 2>&1
```

Final commands returned exit code 0. Suite: adapters 6, CLI 2, core 2, discovery 8 = 18 tests; no failed tests. Zero doctests exist. Debug binary built successfully. These are Linux results only.

Validator executes nine journeys, recording exact argv and exit codes: help 0, version 0, metadata inventory 0, verified inventory 0, human inspection 0, JSON inspection 0, malformed manifest 1, missing project 1, unsupported install command 2. Both inventories contained 34 records and Windows/WSL/MSYS2 labels. Assertions require WSL verified records and prohibit foreign verified records. JSON schema parses. Three fixture hashes match before/after.

## Raw evidence

- 01-core.txt: first core build compiled; doctest launch failed because rustdoc was not on PATH. No success was claimed for that run.
- 02-core.txt: absolute RUSTDOC fixes the launch; two core tests pass.
- 03-adapters.txt: initial three adapter tests pass.
- 04-discovery.txt: seven initial discovery tests pass.
- 05-workspace.txt / 06-build.txt: initial full suite/build.
- 07-safety-tests.txt / 08-build.txt: 17-test safety refinement.
- 10-final-tests.txt / 11-final-build.txt: final 18-test suite/build.
- 09-functional.json / 12-functional.txt: argv, exit codes and fixture hashes.
- *.stdout.txt / *.stderr.txt: raw CLI output per journey.

Timeout and capture-limit tests run; native Python version/header check runs. Windows-path and MSYS2 distinctions are fixture tests; no Windows/MSYS2 compiler execution was performed. Symlink/reparse rejection exists but adversarial race/junction and Windows-native safety tests are still incomplete. Missing Git identity blocks the requested lockfile commit; do not fabricate one.
