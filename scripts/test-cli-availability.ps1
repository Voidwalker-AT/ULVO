$ErrorActionPreference = "Continue"

Write-Host "============================================================" -ForegroundColor Cyan
Write-Host "    ULVO Machine-Wide CLI Availability Test                " -ForegroundColor Cyan
Write-Host "============================================================" -ForegroundColor Cyan

$testDirs = @("C:\", "E:\", "C:\Windows\Temp")

foreach ($dir in $testDirs) {
    Write-Host "`n>>> Testing from directory: $dir" -ForegroundColor Yellow
    
    # 1. Test Windows PowerShell (5.1)
    Write-Host "--- Windows PowerShell (5.1) ---" -ForegroundColor Cyan
    $p5Cmd = "& { Set-Location '$dir'; Write-Host ('PWD: ' + (Get-Location)); Get-Command ulvo -All | Select-Object -Property Name, Source, CommandType; where.exe ulvo; ulvo --version }"
    $p5Out = & powershell.exe -NoProfile -Command $p5Cmd 2>&1
    Write-Host ($p5Out -join "`n")
    
    # 2. Test PowerShell 7+ (pwsh)
    Write-Host "--- PowerShell 7+ (pwsh) ---" -ForegroundColor Cyan
    $pwshCmd = "& { Set-Location '$dir'; Write-Host ('PWD: ' + (Get-Location)); Get-Command ulvo -All | Select-Object -Property Name, Source, CommandType; where.exe ulvo; ulvo --version }"
    $pwshOut = & pwsh.exe -NoProfile -Command $pwshCmd 2>&1
    Write-Host ($pwshOut -join "`n")
}

Write-Host "`n>>> Registry User PATH:" -ForegroundColor Yellow
$userPath = [Environment]::GetEnvironmentVariable('Path', [EnvironmentVariableTarget]::User)
Write-Host $userPath

Write-Host "`n>>> Registry Machine PATH:" -ForegroundColor Yellow
$machinePath = [Environment]::GetEnvironmentVariable('Path', [EnvironmentVariableTarget]::Machine)
Write-Host $machinePath
