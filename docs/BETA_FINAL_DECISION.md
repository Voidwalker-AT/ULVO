# v1.2.0-beta.1 qualification decision — 2026-10-31

**Decision: NOT READY for publication. No tag or release is authorized.**

The exact portable archive remains unchanged at SHA-256
`98faebbb1f791e403dff647bf85f309cf2f1fa17ad70dfd0f07d9d13b662f8dd`.
Its packaging source is `c3543cbd459e9e5d133c31d72d1db7d5c31221a1`;
the executable source remains `d112aac3c13c54022ba54e78488de49e019147ff`.
Subsequent source-export rules and this qualification record do not change those
bytes or rebuild any executable. The application version is 1.2.0; the proposed
public tag is v1.2.0-beta.1, Windows 11 x64 Early Access, GitHub Pre-release.

| Check | Status | Evidence scope |
| --- | --- | --- |
| ZIP integrity, allowlist, extraction and file hashes | PASS | Fresh checks of the exact archive; all 48 files accounted for |
| CLI and isolated optional agent lifecycle | PASS | Fresh bounded commands; no development server |
| GUI setup review/apply, close and relaunch | PASS | Actual native rendering and screenshots; isolated vault |
| Dashboard, Discover, Installed, Projects, Storage, GPU view, Activity, Settings | PASS | Rendering/navigation only; empty/unavailable states observed |
| Adoption with discovery opted out | FAIL | Blank main pane reproduced on two launches |
| GPU telemetry data availability | BLOCKED | View renders but query did not populate telemetry on this isolated test configuration |
| Current live registry structural/semantic integrity | PASS | Valid schema, unique entries, existing managed connectors; unchanged from available post-anomaly snapshot |
| Cause of original registry hash change | BLOCKED | Pre-mutation bytes unavailable; no invented historical diff or automatic restoration |
| Existing managed research project | PASS | Fresh project doctor, connector inspection and execution; locks/connector unchanged |
| Native object-level provenance and legal approval | BLOCKED | MinGW/Rust support symbols and tool identities available; original complete linker map/argv unavailable |
| Independent Windows A–J and packaged root installer | BLOCKED | No independent-machine qualification; root installer remains experimental |

The Adoption failure is localized to `apps/ulvo-desktop/src/App.tsx`: discovery
opt-out prevents the adoption report from loading, while the view renders only
when that report exists. There is no disabled/empty fallback. This mission does
not fix or rebuild it because executable changes are prohibited. Do not treat
window/process existence as successful GUI acceptance.

Historical workstation reports remain in Git. Source exports also exclude the
older root storage baseline, which was missed by the earlier documentation-only
export exclusions. Exclusion does not erase public history; historical profile
and workspace references require a separate human privacy decision. No real
credential was identified by the bounded reachable-history pattern review; a
known synthetic diagnostic fixture is not a deployed credential.

The original 225 workspace tests, final 31 service plus 2 transport tests, and
19 bounded research-project regression tests are inherited receipts, not new
runs in this mission. Full model/GPU research campaigns were not performed.
Unsigned executables, separately installed WebView2 Evergreen, incomplete broad
ecosystem qualification and unresolved native redistribution questions remain
explicit limitations. Technical evidence is not human legal authorization.

The responsible human should withhold publication. Any Adoption fix, rebuild and
new binary qualification must be separately authorized and bounded; no automatic
development phase, stable release, master merge or history rewrite follows this
decision.
