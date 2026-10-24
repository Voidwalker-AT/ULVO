# Beta verification matrix

Candidate: **v1.2.0-beta.1 / Windows 11 x64 Early Access / GitHub Pre-release**.
The release-side `candidate-verification.json` is the authoritative receipt for
the exact ZIP hash. It is generated after extraction from the corrected archive.
An absent or failing receipt means the candidate has not passed acceptance.

| Check | Evidence and scope |
| --- | --- |
| Immutable reference | Original Stage 4 ZIP SHA-256 must remain a02dc550c88ed47fc2f847a4943580a15540af703ffe2255a3de1b68e2e9c995; source d112aac retained |
| Binary preservation | All three executables and loader must equal reference entry bytes; no rebuild or stripping |
| Archive integrity | Rerun CRC, duplicate/path/ADS/traversal/symlink checks, extraction and every file hash |
| Packaging contents | Rerun complete allowlist, MIT, notices, MPL source checksums, manifest/provenance and relative documentation links |
| Public data | Rerun credential-pattern scan and private-file/path review; no proof of arbitrary secret absence |
| Portable CLI/setup | Rerun extracted CLI with isolated absolute Unicode config/vault and System32-only PATH; no toolchain on PATH |
| Optional agent | Rerun start, redirected CLI completion while daemon lives, status/IPC and stop against isolated vault only |
| Standalone GUI | Rerun extracted production executable; inspect native window with WebView2 already installed, no Vite server |
| Vault/installation preservation | Rerun owned test sentinel preservation, before/after installed binary hashes and protected registry/connector/lock hashes |
| Existing research project | Rerun 19 bounded portability/scorer-contract tests through extracted ULVO using the existing managed environment; no training/model campaign |
| Workspace unit tests | INHERITED: 225 host Rust workspace tests; final services 31 + transport 2 after last code changes; not rerun for documentation-only remediation |
| Frontend/release build | INHERITED: production frontend and final GNU release builds; binary identity is verified anew |
| GPU preflight | INHERITED: bounded existing-project GPU execution; no new GPU compatibility certification |
| Installer fixtures | INHERITED: seven isolated script fixture groups; final packaged root pair not accepted |
| Packaged root installer | BLOCKED / experimental: previous host PowerShell startup timeout; no installer execution claimed by this remediation |
| Independent Windows A–J | BLOCKED: no independently controlled clean/conflicting/no-GPU/constrained/offline machines supplied |
| Legal clearance | BLOCKED pending human review of native linker provenance and applicable redistribution obligations |

The private historical evidence remains unchanged and is not distributed because
it includes local paths and inventories. Public provenance records give content
hashes of inherited receipts so a maintainer can correlate retained evidence.
No measurement is a guarantee across hardware. Universal Windows compatibility,
all package managers, independent offline startup and the root installer are not
described as verified. No stable release or automatic merge is authorized.
