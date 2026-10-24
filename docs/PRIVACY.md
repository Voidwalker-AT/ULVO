# Discovery and diagnostics

First-run discovery is opt-in, metadata-only and bounded to conventional tool locations. It does not scan Documents or an entire disk, run discovered executable versions, modify external installations or send inventory to a service. Depth, directory, entry and time limits may produce partial results. Versions, architecture and absent tools are not inferred from a filename. Workspace review is explicit and bounded. External references remain external unless ownership evidence proves otherwise.

Operational events are local JSON records with an operation ID, component, timestamp, classification and redacted message. The store retains at most 100 owned records, 2 MiB and 14 days, with 32 KiB per record. Rotation touches recognized ULVO event files only. Unknown files are retained and excluded from the budget. A bounded directory inspection refuses an excessive unknown-file population. Crash events omit panic payloads, locals and arguments.

`logs capture --tool <supported-name> -- <arguments>` explicitly runs a noninteractive native tool through the gateway. Supported names include uv, pip, npm, npx, pnpm, cargo, rustup, go, gem, mvn, gradle and pacman. Capture drains bounded stdout/stderr, applies a 1–300 second timeout and kills the supervised job on timeout. Arguments are not logged. Native tools can still contact their configured registries or modify their own working state; first-run consent does not authorize capture.

Redaction removes lines containing common credential, token, URL and authorization markers and masks the current user profile. It cannot recognize every arbitrary secret. Review exported diagnostics before sharing. `logs export <new-absolute-file>` refuses overwrite and exports recognized events rather than environment variables or a full machine inventory. No automatic telemetry upload is implemented.

Project identities, configured absolute paths, native manifests and private test evidence can still contain sensitive data. Local evidence under `scripts/output` and built distributions are ignored by Git. Public documentation must not include a machine inventory, tokens or private project data.
