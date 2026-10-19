$ErrorActionPreference = "Stop"

$env:CARGO_HOME = "E:\Workspace\Active\ULVO\vault\runtimes\rust\cargo"
$env:RUSTUP_HOME = "E:\Workspace\Active\ULVO\vault\runtimes\rust\rustup"
$env:PATH = "E:\Workspace\Active\ULVO\vault\runtimes\rust\cargo\bin;E:\Workspace\Active\ULVO\vault\toolchains\msys2\ucrt64\bin;C:\msys64\ucrt64\bin;$($env:PATH)"

$cargoExe = "E:\Workspace\Active\ULVO\vault\runtimes\rust\cargo\bin\cargo.exe"
Write-Host "Running all workspace unit tests..." -ForegroundColor Cyan
& $cargoExe test --workspace
if ($LASTEXITCODE -ne 0) {
    throw "Cargo workspace tests failed with exit code $LASTEXITCODE"
}
Write-Host "All workspace unit tests passed cleanly." -ForegroundColor Green
