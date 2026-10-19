$ErrorActionPreference = "Stop"

$env:CARGO_HOME = "E:\Workspace\Active\ULVO\vault\runtimes\rust\cargo"
$env:RUSTUP_HOME = "E:\Workspace\Active\ULVO\vault\runtimes\rust\rustup"
$env:PATH = "E:\Workspace\Active\ULVO\vault\runtimes\rust\cargo\bin;E:\Workspace\Active\ULVO\vault\toolchains\msys2\ucrt64\bin;$($env:PATH)"

$cargoExe = "E:\Workspace\Active\ULVO\vault\runtimes\rust\cargo\bin\cargo.exe"
Write-Host "Building release gateway binary..." -ForegroundColor Cyan
& $cargoExe build --release --bin ulvo
if ($LASTEXITCODE -ne 0) {
    throw "Cargo build --release failed with exit code $LASTEXITCODE"
}

$releaseExe = "E:\Workspace\Active\ULVO\target\release\ulvo.exe"
if (-not (Test-Path $releaseExe)) {
    throw "Release binary not found at $releaseExe"
}

Copy-Item -Path $releaseExe -Destination "E:\Workspace\Active\ULVO\bin\ulvo.exe" -Force
Copy-Item -Path $releaseExe -Destination "E:\Workspace\Active\ULVO\vault\bin\ulvo.exe" -Force

Write-Host "Staged release binary to bin\ulvo.exe and vault\bin\ulvo.exe successfully." -ForegroundColor Green
