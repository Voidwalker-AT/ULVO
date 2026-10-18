import json
import pathlib
import tomllib
import argparse
from repo_guard import repository_root, check_tree, write_new

root = repository_root(__file__)
check_tree(root, ('crates', 'docs', 'tests'))
parser = argparse.ArgumentParser()
parser.add_argument('--output', required=True, help='New repository-contained historical report snapshot')
args = parser.parse_args()
lock = tomllib.loads((root / 'Cargo.lock').read_text())
packages = lock['package']
for package in packages:
    source = package.get('source')
    if source and source != 'registry+https://github.com/rust-lang/crates.io-index':
        raise RuntimeError(f'unapproved dependency source: {source}')
functional = json.loads((root / 'docs/evidence/09-functional.json').read_text())
inventory = json.loads((root / 'docs/evidence/inventory-verified.stdout.txt').read_text())
files = sorted(str(p.relative_to(root)) for base in ('crates', 'docs', 'tests')
               for p in (root / base).rglob('*') if p.is_file()
               and '__pycache__' not in p.parts and p.suffix not in ('.pyc', '.pyo'))
files += ['Cargo.toml', 'Cargo.lock', '.gitignore', 'README.md']
commands = '\n'.join('- `' + ' '.join(x['command']) + '` → exit ' + str(x['exit_code'])
                     for x in functional['results'] if 'command' in x)
deps = '\n'.join(f'| {p["name"]} | {p["version"]} | {p.get("source", "workspace")} |'
                 for p in packages)
verified = '\n'.join(f'- {p["tool"]}: {p["version"]} ({p["backend"]})'
                     for p in inventory['toolchains'] if p['verification'] == 'verified')
text = f'''# ULVO Phase 0 implementation report

Date: 2026-10-16. Historical initial validation; paths in raw evidence identify the original machine.

## Status

**Partial implementation, not full Phase 0 acceptance.** Four-crate read-only foundation builds and passes Linux/WSL tests. Windows-native execution, complete WSL distribution discovery, MSYS2 execution, and OS-enforced read-only subprocess isolation remain incomplete.

## Architecture implemented

Core deterministic models/version parsing/dedup; native-header and bounded-process adapters; bounded toolchain and VS Code reference discovery; native manifest detection and strict ULVO TOML parser; clap human/JSON CLI. Backend identities stay separate. Ownership supports external/experimental/managed/unknown but discovery currently uses unknown. Configured references are never executed. No package installation/environment provisioning/GC/MCP/service/database code.

## Raw build and test results

Toolchain verified: Rust/Cargo 1.96.0, x86_64-unknown-linux-gnu. Final `cargo test --workspace --locked`: exit 0, 18 passed, 0 failed (6 adapters, 2 CLI, 2 core, 8 discovery). `cargo build --workspace --locked`: exit 0. Python functional validator: exit 0, nine journeys. Raw logs: `docs/evidence/10-final-tests.txt`, `11-final-build.txt`, `12-functional.txt`; per-command stdout/stderr also retained.

First run compiled core but failed doctest launch because rustdoc was missing on PATH; fixed by explicit existing RUSTDOC path. Initial bridge shell invocation and a read-only inline Python quoting attempt failed before intended operations; corrected inspections and report generation followed. No installations or elevated fallback were used.

Exact build environment and commands appear in PHASE0_VALIDATION.md. Writable Cargo home `.cargo-local` and build outputs `target-local` are Git-excluded. Cargo's registry cache did not use the existing WSL Cargo home. Rust and rustdoc are explicit existing executable paths; rustup installation unchanged by targeted operations.

## Executed functional commands

{commands}

Inventory: {len(inventory['toolchains'])} candidate records across Windows, WSL, MSYS2 labels. Only WSL probes can be verified by this Linux binary. Verified versions observed:

{verified}

Three synthetic fixture content hashes matched before/after. JSON inventory, valid inspection, and controlled invalid/missing errors were parsed. Unsupported installation command rejected with exit 2.

## Supported and unsupported functionality

Supported: metadata inventory, optional conservative native version checks, ELF/PE architecture, deterministic domain output, backend/distribution identity, Unix device/inode alias consolidation, selected known installation roots/PATH, strict-JSON VS Code profile references, recognized top-level native manifests, schema-version-1 ULVO TOML, human and JSON output.

Unsupported/incomplete: native Windows build/run, Windows file-ID hardlink deduplication, Windows-to-WSL launcher/cross-distribution native discovery, MSYS2 execution context, complete versioned Python/registry/user-location discovery, E:-managed/experimental runtimes, general VS Code JSONC and runtime arrays/extensions, complete vendor/ownership policies, runtime constraint resolution, native manifest semantic parsing, total inventory timeout, OS-enforced subprocess no-write isolation.

All non-Windows hosts are currently treated as WSL under this prototype contract. This generator summarizes the original validation evidence, not a fresh build or current acceptance. Consult the hardening review for subsequent behavior changes. Broad usability must not be inferred from a successful version probe.

## Security checks and limitations

Windows repository ancestors and existing reparse points were checked before creation; WSL writes checked physical repository path and symlink-free output trees. No project scripts executed; VS Code configuration remains data-only. No existing experiment directories or global package caches were targeted. No PATH, registry, ACL, WSL packages/config, or installed toolchain changes were performed by targeted commands. Only approved crates.io downloads and contained files/builds occurred.

No whole-machine audit demonstrates that all third-party build/probe code made zero global writes. Version probes inherit environment/current directory and are not a sandbox; direct-child timeout is enforced, descendant cleanup is incomplete. Filesystem reads are not cancellable. Path guards have TOCTOU limits; WSL may not expose all Windows junctions as symlinks. Read-only-by-default inventory is the defensible strict mode. Review SECURITY.md before using --verify on untrusted installations.

## Dependencies and provenance

Direct external dependencies: serde (domain serialization), serde_json (JSON), toml (strict manifest parsing), clap (CLI arguments). Exact direct and transitive versions below. Registry sources validated against official crates.io index; Cargo.lock records checksums. No alternate Git dependency source.

| Package | Version | Source |
|---|---|---|
{deps}

## Files created

All source/documentation/test/evidence files are new; repository initially contained only .git. Cargo cache and target artifacts are intentionally excluded from the list and Git.

''' + '\n'.join('- `' + f + '`' for f in sorted(set(files + ['docs/PHASE0_IMPLEMENTATION_REPORT.md']))) + '''

## Git diff and requested lockfile commit

Initial tracked files: none; working tree clean. Implementation files remain untracked pending review; plain git diff therefore does not describe them. See docs/evidence/13-git-review.txt for status and untracked-source line/file summary. Cargo.lock is generated and retained, but **not committed**: repository/global Git user.name and user.email queries returned no values. No author identity was invented or global Git configuration changed. The requested lockfile commit requires an approved author name/email; no unrelated files were staged or committed.

## Next recommended milestone

Complete Windows-native and true multi-backend execution support with a reviewed toolchain provision, job-object/process-group confinement, sanitized probe environments, handle-relative containment, Windows file identities, full VS Code configuration coverage, and policy-based ownership. Validate all acceptance criteria before declaring Phase 0 complete. Resolve Git identity to commit Cargo.lock without unrelated changes.
'''
write_new(root, args.output, text)
print('Dependency source validation passed:', sum('source' in p for p in packages), 'registry packages')
print('Implementation report generated; verified WSL tools:', len([p for p in inventory['toolchains'] if p['verification'] == 'verified']))
