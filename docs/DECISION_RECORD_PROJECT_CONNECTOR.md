# GRAVITAS DECISION RECORD — ULVO Lightweight Project Integration Contract

## 1. Action
Enforce a lightweight project integration contract across every supported ecosystem (Python, Node.js, Rust, C/C++, Java, Go, Ruby).
1. Define the formal Thin Project Connector Contract in `ulvo-core::manifest` and `ulvo-discovery::project_lifecycle`.
2. Enforce strict `.ulvo` directory constraints: normal target size < 100 KiB, hard limit <= 10 MiB, and zero prohibited artifacts (no virtual environments, runtimes, package caches, or build outputs).
3. Extend `ProjectManifest` to declare ecosystem specifications referencing native manifests (`requirements.txt`, `package.json`, `Cargo.toml`, etc.) without duplicating dependency payloads.
4. Ensure all materialized execution environments reside exclusively in `vault/environments/<ecosystem>/` and never inside `<project>/`.
5. Enhance `ulvo project connector` and `ulvo project status` CLI subcommands with `--json` schema output.
6. Build comprehensive test fixtures and unit/integration tests validating all 15 acceptance criteria.

## 2. Reason
ULVO's governing philosophy requires:
- STORE ONCE
- ISOLATE PER PROJECT
- MATERIALIZE ON DEMAND
- EXECUTE THROUGH ULVO
- RECLAIM SAFELY

Without a strict connector contract, projects risk treating `.ulvo` as an environment directory rather than a lightweight connector, or polluting project directories with persistent virtual environments (`.venv`, `node_modules`). Standardizing on `<project>/.ulvo/manifest.toml` under 100 KiB guarantees zero workspace bloat, rapid cloning, and deterministic vault-backed execution.

## 3. Evidence
- Current project lifecycle in `crates/ulvo-discovery/src/project_lifecycle.rs` only models Python via `UvManager`.
- Current `ProjectManifest` uses `#[serde(deny_unknown_fields)]` and lacks typed ecosystem blocks for Node.js, Rust, C/C++, Java, Go, and Ruby.
- Existing projects (`NexusNode/server`, `termux-ai-workstation`, `AstraRun-Anti`, `AstraRun-MiMo`) each successfully execute via ULVO with a small `.ulvo/manifest.toml` (< 2 KiB), proving that the thin connector model is both practical and effective.

## 4. Risk & Mitigations
- **Risk**: Adding new fields to `ProjectManifest` could break existing manifests if `#[serde(deny_unknown_fields)]` rejects them.
  **Mitigation**: Use optional fields with `#[serde(default)]` and preserve all existing Python, GPU, and lifecycle fields so all existing manifests remain 100% valid.
- **Risk**: Enforcing a 10 MiB limit could fail on edge cases if user projects put accidental files into `.ulvo`.
  **Mitigation**: Fail-closed with actionable diagnostics explaining the exact file that violated the connector contract and how to remediate.
- **Risk**: Mutating global state.
  **Mitigation**: Strict invariant: zero writes to global PATH, Windows registry, or system environment. All execution uses process-local PATH and environment tables.
