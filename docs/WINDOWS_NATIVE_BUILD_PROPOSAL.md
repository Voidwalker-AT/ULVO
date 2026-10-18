# Native Windows ulvo.exe — research-only proposal

Status: no Windows build, installation, target addition, toolchain upgrade, or native executable validation performed. This document does not authorize any of those actions. Phase 0 remains a Linux-tested foundation.

## Observed local availability

- Ubuntu's existing Rust/Cargo 1.96.0 builds this workspace for x86_64-unknown-linux-gnu.
- Inspected installed rustup toolchain rustlib directories contained Linux, bare-metal, or UEFI targets, but no Windows target standard-library directory.
- Windows command lookup did not find rustc/cargo/cl/link/clang. This does not prove their absence everywhere.
- Known MSYS2 UCRT64 and MINGW64 locations contain gcc.exe and ld.exe; UCRT64 also exposes x86_64-w64-mingw32-gcc.exe in command lookup.
- Known UCRT64, MINGW64, CLANG64 bin checks found no rustc.exe or cargo.exe.
- Existing vswhere.exe returned [] for `-all -products * -format json`. This means no instances returned by that discovery query, not a drive-wide proof that no SDK/compiler files exist.
- GCC, Binutils, mingw-w64 and SDK versions/usability were not verified by execution. No WSL-hosted cross-linker was found at the inspected /usr/bin candidate paths.

Conclusion: no complete existing-only Rust-to-Windows build route is established. GCC cannot replace rustc. A missing Windows standard library is a blocker for this std-using workspace even though Rust can generate target machine code.

## Official documented requirements

1. rustup target add supplies the target Rust standard library, not necessarily a linker/C toolchain: https://rust-lang.github.io/rustup/cross-compilation.html
2. Rust Windows GNU documentation specifies PE binaries, cross-compilation support, and MSVCRT as the default; its current reference C toolchain is GCC 14.2, Binutils 2.44, mingw-w64 12.0.0. The page also warns about target-maintainer status. These current requirements must be checked against the exact chosen Rust release: https://doc.rust-lang.org/rustc/platform-support/windows-gnu.html
3. MSVC is recommended for Windows software interoperability and requires compatible Visual Studio linker/libraries: https://rust-lang.github.io/rustup/installation/windows.html
4. MSYS2 UCRT64 and legacy MINGW64 target different C runtimes. Do not mix CRT-specific objects/static libraries. MINGW64 was deprecated in March 2026, so existing availability is not a recommendation for a new long-term installation: https://www.msys2.org/docs/environments/
5. Microsoft compiler/linker/SDK environment requirements: https://learn.microsoft.com/en-us/cpp/build/building-on-the-command-line?view=msvc-170
6. Microsoft installation discovery guidance: https://devblogs.microsoft.com/cppblog/finding-the-visual-c-compiler-tools-in-visual-studio-2017/

## Candidate routes and gates

### A — Windows MSVC (preferred product-validation baseline)
Requires a verified Windows-hosted Rust compiler, its x86_64-pc-windows-msvc std, compatible linker and SDK libraries. None is established available by current checks. First request bounded read-only discovery of any user-specified existing installation. If missing, obtain explicit approval for exact official components and repository-local installation/writable state, or use a separately approved existing development machine. Do not run installers or alter defaults as part of this proposal.

### B — Windows GNU using existing MSYS2 compiler
Existing MINGW64 GCC may be a compatible candidate for stock MSVCRT-based Rust GNU, but versions/import libraries remain unverified and the environment is deprecated. UCRT64 must not be selected solely because GCC appears first on PATH: prove compatibility with the chosen Rust distribution/runtime. A Windows Rust compiler/std is still missing from inspected locations. Request approval for a pinned, official repository-local Rust toolchain if no existing compiler is located. Do not mix UCRT64 and MINGW64 library paths.

### C — WSL cross-build
Reuses the verified Linux rustc, but needs matching Windows std and a compatible linker/import libraries. Those are not present at checked WSL locations. Calling Windows GCC from WSL is an unverified interop route with argument/path and DLL-resolution issues, not an established solution. A Linux-hosted cross-toolchain is more predictable but would require explicit acquisition approval. Do not modify existing rustup home using target add; propose a separately scoped local sysroot/toolchain instead. Build-std/nightly bootstrapping is not recommended for this narrow Phase 0 milestone.

## Future build commands — templates only, NOT executed

After the selected compiler, target std, linker/CRT and physical containment are verified and approved, use repository-local writable Cargo home, target directory and TEMP/TMP. Use explicit existing executable paths and process-local variables; no global PATH or configuration changes.

```powershell
# Run from the guarded repository root, AFTER toolchain-specific approval.
$env:CARGO_HOME = Join-Path $PWD '.cargo-local-windows'
$env:CARGO_TARGET_DIR = Join-Path $PWD 'target-local-windows'
$env:RUSTC = $ApprovedRustcPath
$env:RUSTDOC = $ApprovedRustdocPath
& $ApprovedCargoPath build --locked --offline -p ulvo-cli --target $ApprovedWindowsTarget
```

For GNU, configure `CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER` with a verified compatible absolute linker path. For MSVC, use the approved compiler/SDK's process-local environment; Windows link.exe naming alone is not enough. Offline build may fail if Windows-only dependency sources are not cached; ask before any new downloads. Do not automatically fall back to installations or unbounded searches. The proposed Windows cache/target names need ignore rules approved before actual use; currently only existing Linux paths and default target directories are excluded.

## Separate artifact and native runtime acceptance

1. Capture exact compiler/Cargo/target/linker/SDK/CRT versions, dependency sources, argv, raw output and exit codes.
2. Inspect PE signature and x86_64 machine type; hash the artifact and inspect DLL imports. Renaming the Linux ELF does not create ulvo.exe.
3. Execute the resulting binary directly from Windows PowerShell: help, version, metadata-only inventory JSON, synthetic valid/invalid/missing inspection, and unsupported-command rejection. No automatic --verify.
4. Test Windows paths with spaces/Unicode, drive-relative/UNC paths, junction/reparse rejection, and genuine Windows file identities under an explicitly approved fixture scope.
5. Prove host_backend=windows, no inherited Linux-host assumptions, fixture hashes unchanged, and process exit codes correct. Observe writes in the approved scope; do not infer no global side effects without suitable observation.
6. Windows-to-WSL and MSYS2 execution still require their own implementation/acceptance; producing PE alone does not complete backend discovery.

Recommendation: locate an existing compatible Windows Rust/MSVC installation first through bounded approved discovery; otherwise request explicit provision approval. Do not attempt native or cross compilation with an incomplete guessed toolchain. No installation, build, commit, or publication is authorized by this document.
