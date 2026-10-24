# Troubleshooting and recovery

Run `ulvo doctor --deep`, then `ulvo logs list` and `ulvo logs show <operation-id>`. Select an explicit absolute configuration root when diagnosing a separate installation. Doctor checks metadata readiness; it does not certify an arbitrary project's ABI, GPU execution or dependency compatibility. A missing optional language runtime is not proof that bootstrap failed.

A setup journal in `applying` or `verify` can be resumed with its reviewed ID. If preferences changed concurrently, publication and rollback stop for review. Do not erase the journal to bypass that check. A corrupted preferences file must be restored from a trusted backup before normal path resolution; automatic preferences repair is not implemented.

Registry repair is deliberately narrow. `ulvo repair --plan --backup <absolute-valid-project-registry>` reviews a replacement for the owned project registry. `ulvo repair --apply <reviewed-id>` rechecks target and backup hashes, saves the original bytes, and atomically replaces that registry. Stale plans fail. Missing backups, arbitrary files, runtime reinstall and project data repair are not automatic.

Installer recovery uses its transaction journal. Keep the journal, manifest and previous package together for diagnosis. Uninstall retains modified and unknown files and vault data. Do not recursively delete directories to resolve ownership failures. Rollback must use the exact installation boundary and preserve external runtimes and native locks.

For an offline GUI failure, check that WebView2 Evergreen is installed. The portable archive contains production assets and its loader, not a WebView2 runtime installer. CLI logs and setup remain available. If discovery is partial or permissions are denied, supply explicit project/workspace paths; do not grant broader access to make an inventory appear complete.

Retirement remains blocked without real project execution, consumer identification, native metadata preservation, ABI/GPU checks and a tested rollback receipt. A successful resolver call or connector write is insufficient. No external runtime or legacy environment removal is authorized by setup consent.
