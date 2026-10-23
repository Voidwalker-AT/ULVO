# ADR-002: Fail-closed workstation persistence and cleanup

Status: accepted for release-candidate hardening, 2026-10-24.

## Context

Background watchers, the CLI, and the desktop can update project metadata concurrently. A malformed registry must not become an empty catalog. Cleanup plans and stored paths can be stale or modified. Logical lengths do not prove physical allocation, and filesystem notifications are unaligned byte buffers.

## Decision

- Preserve corrupt or unsupported project registries and return an actionable error.
- On Windows, serialize project commits with an OS lock and reject stale snapshots. Callers reload and replay rather than overwriting another writer's state.
- Flush staged file contents before atomic replacement, use extended Windows paths, and capture native errors before cleanup calls can overwrite them.
- Validate all CAS cleanup candidates against current ownership and content-address paths before deleting any candidate. Reject reparse traversal and abort if the intent cannot be durably journaled.
- Count unique file identities for allocation telemetry. Keep unlinked allocation estimates separate from observed volume free-space changes.
- Decode notification fields as bytes, preserve pending-I/O buffers until completion or process termination, and exclude generated vault/cache activity from project watching.
- Bound agent request reads, reassemble fragmented TCP messages, serialize daemon ownership per vault, and generate new bearer tokens from the OS random source.

## Consequences and limits

Stale writes can return retry errors; this is preferable to silent data loss. Windows is the verified locking target. The existing non-Windows file-lock implementation remains insufficient for concurrent writes. Native file-data allocation does not include filesystem metadata or prove recoverability. Filesystem permissions remain part of the trust boundary; static path checks do not establish immunity to every adversarial concurrent path replacement. Token migration and external toolchain retirement remain explicit operational steps.
