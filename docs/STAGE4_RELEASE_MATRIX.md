# Stage 4 release qualification

Status vocabulary: VERIFIED means evidence exists for the stated scope; FAILED means an observed failing check; BLOCKED means a required environment or approval is unavailable; NOT TESTED means no acceptance claim. This candidate is not universally Windows-ready and must not receive a stable tag or release.

| Gate | Status | Scope / remaining acceptance |
| --- | --- | --- |
| Shared paths, metadata-only setup, plan ID, ownership refusal | VERIFIED | Automated host tests; CLI and GUI use the shared service |
| Bounded logs, redaction, export, repair stale-plan protection | VERIFIED | Automated service tests; manual review still required before export |
| Connector-only batch migration | VERIFIED | Synthetic legacy Python fixture; native locks and legacy files retained |
| Functional retirement / external installation deletion | BLOCKED | Exact-resource approval and independent functional/consumer/rollback receipts absent |
| Full Rust workspace regression | VERIFIED | Development host; no substitute for clean Windows |
| Production frontend build | VERIFIED | Embedded asset build; independent GUI acceptance remains required |
| Eleven-stage GUI setup and control-center transition | VERIFIED | Host-only production executable, isolated config/vault, tool-free PATH; discovery and agent opt-out retained |
| Installer recovery and selective uninstall | VERIFIED | Seven isolated host groups, including poisoned journal, traversal/ADS refusal and retained user/vault files |
| Warm metadata CLI startup | VERIFIED | Ten isolated host samples: 39–69 ms, including bounded event recording; single hardware class |
| Final PowerShell packaging invocation and packaged root installer | BLOCKED | Host PowerShell startup stalled even for a trivial fresh command; earlier seven-group installer receipts remain valid for their stated source scope. Root installer pairing fix needs acceptance |
| Portable ZIP fallback assembly | VERIFIED | ZIP CRC and per-file SHA-256 checked against staged source bytes. This does not qualify the PowerShell packaging script or installer |
| Optional agent and redirected CLI lifetime | VERIFIED | Host-only isolated Unicode vault: CLI pipe closes while daemon lives, ephemeral-port IPC responds, clean stop; 1.8 s startup, 1.3 MiB private memory / 12.3 MiB working set, 0 measured CPU seconds over ten idle seconds |
| All native package managers on a clean Windows installation | NOT TESTED | Capture whitelist and gateway integration are not ecosystem certification |
| Physical storage accuracy across different filesystems | NOT TESTED | No new cross-filesystem qualification evidence |
| Multiple hardware performance classes | BLOCKED | Independently controlled hardware unavailable |

All required independent Windows scenarios remain BLOCKED until independently controlled environments and signed-off receipts exist:

| Scenario | Status | Required environment |
| --- | --- | --- |
| A: no development tools | BLOCKED | Clean Windows 11 |
| B: existing Python and Node | BLOCKED | Independent Windows 11 |
| C: conflicting runtimes | BLOCKED | Independent Windows 11 |
| D: no NVIDIA GPU | BLOCKED | Independent Windows 11 |
| E: low RAM / low storage | BLOCKED | Controlled constrained Windows 11 |
| F: interrupted installation/download | BLOCKED | Clean Windows fault injection; metadata setup performs no downloads |
| G: corrupt registry / partial vault | BLOCKED | Independent Windows recovery; host fixture tests alone do not satisfy this |
| H: legacy .venv projects | BLOCKED | Independent project execution and rollback |
| I: spaces / Unicode | BLOCKED | Independent Windows install and execution |
| J: offline startup / diagnostics | BLOCKED | Independent offline Windows with declared WebView2 prerequisite |

Performance acceptance targets for qualification are warm metadata CLI p95 <=250 ms, bounded first-run tool discovery <=3 s, agent steady idle private bytes <=32 MiB and <=1% of one CPU over 10 s, GUI idle private bytes <=300 MiB, connector-only migration metadata <=100 KiB/project and event store <=2 MiB owned data. Targets are not measurements. Cold materialization, native caches and GPU workloads require separate workload-specific receipts; do not count workload memory as ULVO overhead.

Host-only receipts, timings, candidate hashes and precise limitations are kept in the private Stage 4 readiness report. Host isolation never changes a BLOCKED independent scenario to VERIFIED.
