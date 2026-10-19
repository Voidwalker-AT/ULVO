$ErrorActionPreference = "Stop"

$env:CARGO_HOME = "E:\Workspace\Active\ULVO\vault\runtimes\rust\cargo"
$env:RUSTUP_HOME = "E:\Workspace\Active\ULVO\vault\runtimes\rust\rustup"
$env:PATH = "E:\Workspace\Active\ULVO\vault\runtimes\rust\cargo\bin;C:\msys64\ucrt64\bin;$($env:PATH)"

$cargoExe = "E:\Workspace\Active\ULVO\vault\runtimes\rust\cargo\bin\cargo.exe"
Write-Host "Building ulvo release binary..." -ForegroundColor Cyan
& $cargoExe build --release --bin ulvo
if ($LASTEXITCODE -ne 0) {
    throw "Build failed with exit code $LASTEXITCODE"
}
Write-Host "Build successful." -ForegroundColor Green
