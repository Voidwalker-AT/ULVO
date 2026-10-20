$ErrorActionPreference = "Stop"

$zip = "E:\Workspace\Active\ULVO\dist\ulvo-v1.2.0-windows-x64-portable.zip"
$outDir = "E:\Workspace\Active\ULVO\dist\test-portable-extracted"

if (Test-Path $outDir) {
    Remove-Item -Recurse -Force $outDir
}

Write-Host "Extracting portable release ZIP: $zip" -ForegroundColor Cyan
Expand-Archive -Path $zip -DestinationPath $outDir

Write-Host "Extracted files in bin:"
Get-ChildItem -Path "$outDir\bin" | Select-Object Name, Length

$testExe = "$outDir\bin\ulvo-desktop.exe"
Write-Host "Launching extracted executable: $testExe" -ForegroundColor Cyan
$p = Start-Process -FilePath $testExe -PassThru
Start-Sleep -Seconds 5

Write-Host "Process HasExited: $($p.HasExited)"
if ($p.HasExited) {
    Write-Host "ExitCode: $($p.ExitCode)"
    throw "Extracted executable failed to stay running!"
} else {
    Write-Host "[VERIFIED PASS] Extracted portable binary launched and is actively running with PID $($p.Id)!" -ForegroundColor Green
    $p.Kill()
}

Remove-Item -Recurse -Force $outDir
Write-Host "Cleaned up temporary test directory."
