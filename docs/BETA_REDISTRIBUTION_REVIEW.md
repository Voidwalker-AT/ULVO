# ULVO v1.2.0-beta.1 redistribution review

Windows 11 x64 Early Access / proposed GitHub Pre-release. Reviewed 2026-10-24.
**Publication status: HOLD for human legal and release review.** This is a
technical evidence review, not counsel's opinion or complete license clearance.

## Distribution boundary and graph

The portable application ships three unchanged ULVO executable files, Microsoft's
x64 WebView2 loader, first-party scripts/documentation, and attribution/source
materials. It does not ship Python, Node, Cargo, rustc, GCC, package caches, a
WebView2 browser runtime, user projects or vault data. Statically linked runtime
code and generated/embedded assets still require review even when a compiler or
build tool is not distributed.

[DEPENDENCIES.json](../legal/DEPENDENCIES.json) enumerates all 462 third-party
Cargo.lock entries and 62 npm lock entries: **524 components**. The seven
first-party workspace entries are recorded in the Cargo graph, under ULVO's MIT
license. Target-filtered Cargo metadata contains 269 workspace/third-party
packages. The inventory preserves package versions, dependency edges, features,
registry checksums, declared license expressions, roles and notice hashes.

| Role combination | Components | Treatment |
| --- | ---: | --- |
| Runtime only | 57 | Potentially linked Rust code or embedded npm production code |
| Build + runtime | 132 | Both contexts; conservatively include notices |
| Build + development + runtime | 6 | Both contexts; conservatively include notices |
| Development + runtime | 5 | Runtime obligations still reviewed |
| Build only | 89 | Build tools/proc macros; not shipped as executable dependencies |
| Non-host optional build | 35 | Platform-specific npm tooling; not shipped |
| Not selected for Windows target | 200 | Cargo lock entries outside the target resolve graph |

Runtime is a conservative closure, **not exact linked membership**. Traversal
starts at CLI, agent and desktop normal dependencies; build edges and proc-macro
targets switch to the build role. Workspace dev edges are separately identified.
Cargo metadata can unify features between host and target contexts, so a package
may be over-included. Build tools can generate code: their output is not assumed
exempt merely because a tool has a build role. npm production dependencies are
React, React DOM, scheduler and the two Tauri API/plugin packages; Vite, TypeScript,
Tauri CLI, rolldown and lightningcss are build tooling.

All **200 conservatively runtime-classified third-party components** have collected
license/notice text. There are 37 absent local texts for nonshipped optional/build
tools, explicitly enumerated in the inventory. Their declared licenses remain
recorded. The 200 non-target Cargo entries have not had their license texts or
license metadata reviewed: no claim of clearance for another platform or a future
distribution of development tools is made. Full unfiltered offline Cargo metadata
was unavailable because non-target packages are not all cached; Cargo.lock still
provides their complete version/dependency enumeration. No lockfile was changed.

## Text and attribution provenance

[THIRD-PARTY-NOTICES.txt](../legal/THIRD-PARTY-NOTICES.txt) preserves root and nested
LICENSE, COPYING, COPYRIGHT and NOTICE materials available in the selected local
Cargo/npm packages, including copyright holders and disclaimers. Every collected
text has its SHA-256 and registry or upstream source recorded. All 262 selected
third-party Cargo source archives were verified against Cargo.lock and their
collected license bytes compared with the original archive where present. The
five runtime npm tarballs were retrieved from their exact lockfile registry URLs,
verified against lockfile integrity values, and their license bytes matched.

Missing Rust texts were resolved from the upstream repository commit named by
each registry package's `.cargo_vcs_info.json`, rather than an arbitrary current
branch: alloc-stdlib (Dropbox), tauri-plugin (Tauri), and the three webview2-rs
crates. The selectors package declares MPL-2.0 and its source headers confirm it;
the unmodified license text was retrieved from Mozilla. Failed upstream filename
guesses are retained privately; none was substituted with invented text.

Practical conditions reviewed, subject to exact component terms:

| License family in selected graph | Distribution treatment |
| --- | --- |
| MIT, BSD, ISC, Zlib | Preserve applicable attribution, license, disclaimer and non-endorsement/alteration notices |
| Apache-2.0 | Preserve license, relevant attribution/NOTICE and modification notices; no trademark grant assumed |
| OR dual/multiple licenses | Retain supplied alternatives; do not interpret OR as requiring all alternatives or as removing component notices |
| AND expressions, including Unicode conjunctions | Retain all applicable texts; an OR branch does not erase an AND obligation |
| Unicode-3.0 | Preserve Unicode license/data notices with affected ICU/Unicode components |
| MPL-2.0 | Preserve notices and license; make covered source available and tell recipients how to obtain it |
| CC0, MIT-0, 0BSD, Unlicense alternatives | Retain provided texts; no additional patent/trademark rights inferred |

No explicit license election changing upstream terms has been made. Slash-form
legacy expressions and generated-code licensing are retained for human review;
they are not automatically treated as an SPDX compatibility decision.

## MPL source availability

The five Rust MPL components are cssparser 0.37.0, cssparser-macros 0.7.1,
dtoa-short 0.3.5, option-ext 0.2.0 and selectors 0.38.0. Their **original,
unmodified registry source archives** are included in
[MPL-SOURCE-ARCHIVES.zip](../legal/MPL-SOURCE-ARCHIVES.zip), including the build-only
macro package to avoid relying on an exclusion. Every .crate SHA-256 was checked
against Cargo.lock. Extract the ZIP, then use a tar-compatible tool to extract each
.crate. Sources remain available under their upstream MPL terms; ULVO adds no
restriction to those rights. Registry URLs and hashes are also in the inventory.
No third-party source patch was made in this remediation.

Mozilla's [MPL FAQ](https://www.mozilla.org/en-US/MPL/2.0/FAQ/) explains executable
distribution source availability. The license itself is included in legal/native.
Human review should confirm that these source materials cover any generated or
modified MPL code actually present in the binaries.

For independent source review, retrieve each inventory source URL and verify its
recorded Cargo.lock SHA-256 or npm lock integrity value before inspecting license
files. For missing-package supplements, use the exact commit-pinned URLs in
`retrieved_upstream`; compare file SHA-256. The collection helper
`scripts/review-distribution-dependencies.py` accepts explicit offline metadata,
reviewed supplement and new output paths. It refuses to overwrite prior evidence.
It does not replace the separate registry integrity or human legal review.

## Microsoft loader and runtime

The shipped loader's version resource is **1.0.3800.47**, x64. Its SHA-256 is
`86545b66cdb0603bc26b626fb9ad610cb6e71f28d468f5ea66df23b03dda96d5`.
It is byte-identical to `build/native/x64/WebView2Loader.dll` in Microsoft's
[exact NuGet package](https://www.nuget.org/packages/Microsoft.Web.WebView2/1.0.3800.47).
The downloaded package SHA-256 is
`56c9f26bdd07916a2d1949fb58a5c7e434dfa1173577dca879206050c4e718db`.
The package's actual LICENSE.txt and NOTICE.txt are reproduced unmodified in
legal/native. The LICENSE permits source/binary redistribution subject to its
notice, disclaimer and non-endorsement conditions. This conclusion is specific
to that exact package and loader; no license for an unrelated SDK/runtime is assumed.

The browser runtime is **not redistributed**. Users must install WebView2
Evergreen separately. Microsoft's [distribution documentation](https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/distribution)
distinguishes the architecture-specific loader from the browser runtime. No
runtime installer, fixed-version browser or Microsoft endorsement is included.

## Rust, GNU and native code

Rust 1.96.0 GNU was the qualified toolchain. Included native materials comprise
the installed distribution's complete `COPYRIGHT-library.html`, plus Rust 1.96.0
MIT, Apache and COPYRIGHT files from the official version tag. Rust's
[licensing guidance](https://rustc-dev-guide.rust-lang.org/licenses.html) explicitly
points to component-specific exceptions; the standard library cannot be treated
as covered solely by ULVO's MIT file.

The binaries' PE import tables identify Windows/UCRT system libraries and the
desktop loader. No separately shipped GNU runtime DLL is required by those import
tables. This does **not** rule out static libgcc, MinGW CRT, compiler-builtins or
other runtime code. GCC 14.2 license/exception texts and locally installed MSYS2
UCRT64 CRT/header/winpthread notices are included conservatively. These copied
native notices do not prove the exact object-level linker inputs of the original
build or the provenance of Rust's bundled GNU support libraries.

The [GCC Runtime Library Exception](https://www.gnu.org/licenses/gcc-exception-3.1.en.html)
is conditional on covered files and eligible compilation processes. It is not a
blanket exemption for arbitrary GPL code. GCC itself and binutils are not shipped.
No new binary was compiled, relinked or stripped during this remediation.

## Remaining human review / publication blockers

1. **Native provenance:** obtain/confirm the original linker inputs and applicable
   Rust GNU bundled support-library notices, including any compiler-builtins,
   MinGW CRT and libgcc code. The available PE imports and broad notice bundle do
   not independently establish exact membership or exception eligibility.
2. **Legal sign-off:** review all applicable expressions, retained NOTICE files,
   generated-code obligations and MPL source correspondence. This report is an
   engineering audit, not an independent legal review by counsel.
3. **Technical acceptance:** independent Windows scenarios remain blocked, and
   the packaged root installer remains experimental. See the verification matrix.

The license-text gaps for potentially shipped Cargo/npm components and the exact
loader have been remediated. **Complete redistribution clearance is not claimed.**
Do not publish the candidate until the responsible human has resolved or explicitly
accepted the outstanding legal/release questions; do not describe uncertainty as
verified legal compatibility.
