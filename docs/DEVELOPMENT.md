# ULVO Developer Guide

This document describes how to set up the development environment, build the native Windows binary, run automated tests, and contribute to ULVO.

---

## 1. Prerequisites

- **Operating System**: Windows 10 or Windows 11 (NTFS volume recommended for native file identity and hardlink testing).
- **Rust Toolchain**: Rust 1.96.0+ (or the ULVO-managed Rust toolchain in `vault/runtimes/rust`).
- **Target**: `x86_64-pc-windows-gnu`.
- **C Linker**: MSYS2 UCRT64 GCC linker (`gcc.exe`) located at `C:\msys64\ucrt64\bin` (or on system PATH).

---

## 2. Environment Configuration

To build with the ULVO-managed native Windows toolchain without altering your global environment:

```powershell
# Set process-local environment variables in Windows PowerShell
$env:CARGO_HOME = "$PWD\vault\runtimes\rust\cargo"
$env:RUSTUP_HOME = "$PWD\vault\runtimes\rust\rustup"
$env:PATH = "$PWD\vault\runtimes\rust\cargo\bin;C:\msys64\ucrt64\bin;C:\msys64\usr\bin;" + $env:PATH
```

---

## 3. Building ULVO

### Check Workspace Compilation
```powershell
cargo check --workspace --locked --target x86_64-pc-windows-gnu
```

### Build Debug Binary
```powershell
cargo build --workspace --locked --target x86_64-pc-windows-gnu
```
The compiled executable will be placed at:
`target\x86_64-pc-windows-gnu\debug\ulvo.exe`

### Build Release Binary
```powershell
cargo build --workspace --locked --release --target x86_64-pc-windows-gnu
```
The release binary will be placed at:
`target\x86_64-pc-windows-gnu\release\ulvo.exe`

---

## 4. Running Tests

All automated tests across the workspace can be executed natively:

```powershell
cargo test --workspace --locked --target x86_64-pc-windows-gnu
```

### Running Targeted Crate Tests
- **Core models and registry logic**:
  ```powershell
  cargo test -p ulvo-core --target x86_64-pc-windows-gnu
  ```
- **Filesystem identity and adapters**:
  ```powershell
  cargo test -p ulvo-adapters --target x86_64-pc-windows-gnu
  ```
- **Discovery and workspace audits**:
  ```powershell
  cargo test -p ulvo-discovery --target x86_64-pc-windows-gnu
  ```
- **CLI integration tests**:
  ```powershell
  cargo test -p ulvo-cli --target x86_64-pc-windows-gnu
  ```

---

## 5. Test Safety Invariants: `SafeTestFixture`

When writing tests that interact with the filesystem:
- **Never** use hardcoded paths or paths directly in the source tree.
- Use `SafeTestFixture`, which creates a uniquely named directory in the system temp directory containing process PID and timestamp tokens.
- An ownership sentinel file `.ulvo_test_sentinel` is written to the fixture root.
- The `Drop` implementation strictly verifies the sentinel and process PID before recursively deleting the test fixture.

```rust
#[test]
fn test_my_feature() {
    let fixture = SafeTestFixture::new("my_feature");
    let test_file = fixture.path.join("data.txt");
    std::fs::write(&test_file, b"test payload").unwrap();
    // Test logic here...
    // Fixture is safely cleaned up on drop
}
```

---

## 6. Code Architecture Guidelines

1. **`ulvo-core` is Pure**: Never import filesystem I/O, process spawning, or networking into `ulvo-core`. All domain models must remain deterministic, serializable, and easily testable.
2. **Handle Reparse Points First**: Any directory walking routine in `ulvo-adapters` or `ulvo-discovery` must explicitly reject reparse points (`is_reparse_point`) before recursing.
3. **No Unchecked Unwraps**: Use explicit error handling (`Result<T, E>`) for all I/O and parsing operations.
4. **Disclose Limitations**: If an algorithm relies on a heuristic or cannot guarantee race freedom, document it clearly in code comments and architecture specifications.
