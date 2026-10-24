# Windows qualification candidate

Stage 4 is a development qualification candidate, not a stable release. Windows 11 x64 is the intended target. The desktop needs Microsoft WebView2 Evergreen already installed; the bundled loader DLL does not include that runtime. CLI setup and diagnostics do not require WebView2, Python, Node, Rust, an agent, a GPU or a development server. Bootstrap does not download prerequisites.

Extract the portable archive into a writable directory. Run `bin\ulvo-desktop.exe` or `bin\ulvo.exe setup status`. Production frontend assets are embedded; do not start Vite. Portable application files and per-user configuration are separate. Moving the portable directory does not move an explicitly selected vault.

For per-user installation, run `powershell -NoProfile -File .\install.ps1 -InstallDir <absolute-path> -VaultPath <absolute-vault>`. Use `-NoPath`, `-NoShortcuts` and `-NoRegistry` to opt out of those integrations. User PATH registration is optional; Machine PATH is never required. The background agent is opt-in with `-EnableAgent`. Restart the application after changing the agent preference. The installer stages and swaps versioned application files, records a recovery journal, and preserves the vault on update/uninstall. Keep the previous package for rollback. Install and uninstall through the scripts shipped with that package; do not manually remove a vault.

The shared configuration order is CLI `--vault-root` / `--config-root`, process `ULVO_VAULT_ROOT` / `ULVO_CONFIG_ROOT`, per-user preferences, per-install preferences, a proven legacy vault adjacent to the installation, then the native Windows LocalAppData/ULVO default. CWD is not a vault search root. Invalid preferences fail closed. Explicit paths must be absolute, non-root and free of reparse-point ancestors. Spaces and Unicode are supported in metadata; independent Windows acceptance remains blocked.

The first-run wizard explains privacy, optional metadata discovery, capacity, vault location, ecosystems, migration limits and review before apply. It writes metadata only. Ecosystem selections are preferences, not runtime installation. Native manifests and locks remain authoritative. The CLI equivalent is:

```powershell
ulvo --config-root <absolute-config> setup plan --vault <absolute-vault> --workspace-root <absolute-workspace> --ecosystem python
ulvo --config-root <absolute-config> setup apply <reviewed-plan-id>
ulvo --config-root <absolute-config> setup status
```

Use `--allow-discovery` only after reviewing the privacy policy. An interrupted apply resumes with the same approved ID. `setup cancel` restores unchanged pre-apply preferences where safe and retains directories; completed setup cannot be cancelled. Setup refuses to claim a nonempty directory without ULVO ownership evidence. Metadata journals have a 512 KiB cap, the plan reserves 1 MiB plus 500 MB free storage, and no runtime download size is guessed.

Migration review accepts explicit projects or a bounded workspace scan. `migrate batch-review --project <absolute-project>` produces a review ID. `migrate batch-apply <id> --project <selected-project>` creates only eligible ULVO connectors; legacy environments, manifests and locks are retained. Unsupported, ML-sensitive or incomplete metadata stays blocked. No connector receipt proves functional equivalence or authorizes retirement.

Build tooling is required only for maintainers. `scripts/package-release.ps1` packages prebuilt release binaries without deploying into the live installation. It refuses an existing output directory, emits SHA-256 checksums, and marks the package as non-stable. `-ForceRebuild` uses explicitly configured Cargo/npm; it does not install build tools.

Updates retain the previous application tree in an exact `.ulvo_backup_<transaction>` sibling for reviewed rollback. This includes modified files with installer-owned names. Backup disk usage is additional to the active application; backup retirement requires separate review. Recovery retains unjournaled sibling directories and refuses journals whose paths differ from the exact installation transaction.
