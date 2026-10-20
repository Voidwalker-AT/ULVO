$ErrorActionPreference = "Continue"

Write-Host "============================================================" -ForegroundColor Cyan
Write-Host "  Simulating Fresh Shell with Registry PATH Environment     " -ForegroundColor Cyan
Write-Host "============================================================" -ForegroundColor Cyan

$machinePath = [Environment]::GetEnvironmentVariable('Path', [EnvironmentVariableTarget]::Machine)
$userPath = [Environment]::GetEnvironmentVariable('Path', [EnvironmentVariableTarget]::User)
$freshPath = "$userPath;$machinePath"

Write-Host "Fresh combined PATH starts with:"
Write-Host ($freshPath.Substring(0, [math]::Min(120, $freshPath.Length)))

$testDirs = @("C:\", "E:\", "E:\Workspace", "C:\Windows\Temp")

foreach ($dir in $testDirs) {
    Write-Host "`n============================================================" -ForegroundColor Yellow
    Write-Host ">>> Testing from directory: $dir" -ForegroundColor Yellow
    Write-Host "============================================================" -ForegroundColor Yellow
    
    # 1. Test Windows PowerShell (5.1) with fresh PATH
    Write-Host "--- Windows PowerShell (5.1) ---" -ForegroundColor Cyan
    $pinfo = New-Object System.Diagnostics.ProcessStartInfo
    $pinfo.FileName = "powershell.exe"
    $pinfo.Arguments = "-NoProfile -Command `"Set-Location '$dir'; Get-Command ulvo -All | Select-Object -Property Name, Source, CommandType; where.exe ulvo; ulvo --version; ulvo doctor`""
    $pinfo.RedirectStandardOutput = $true
    $pinfo.RedirectStandardError = $true
    $pinfo.UseShellExecute = $false
    $pinfo.EnvironmentVariables["Path"] = $freshPath
    
    $p = [System.Diagnostics.Process]::Start($pinfo)
    $stdout = $p.StandardOutput.ReadToEnd()
    $stderr = $p.StandardError.ReadToEnd()
    $p.WaitForExit()
    Write-Host $stdout
    if ($stderr) { Write-Host "STDERR: $stderr" -ForegroundColor Red }

    # 2. Test PowerShell 7 (pwsh) with fresh PATH
    Write-Host "--- PowerShell 7+ (pwsh) ---" -ForegroundColor Cyan
    $pinfo7 = New-Object System.Diagnostics.ProcessStartInfo
    $pinfo7.FileName = "pwsh.exe"
    $pinfo7.Arguments = "-NoProfile -Command `"Set-Location '$dir'; Get-Command ulvo -All | Select-Object -Property Name, Source, CommandType; where.exe ulvo; ulvo --version; ulvo doctor`""
    $pinfo7.RedirectStandardOutput = $true
    $pinfo7.RedirectStandardError = $true
    $pinfo7.UseShellExecute = $false
    $pinfo7.EnvironmentVariables["Path"] = $freshPath
    
    $p7 = [System.Diagnostics.Process]::Start($pinfo7)
    $stdout7 = $p7.StandardOutput.ReadToEnd()
    $stderr7 = $p7.StandardError.ReadToEnd()
    $p7.WaitForExit()
    Write-Host $stdout7
    if ($stderr7) { Write-Host "STDERR: $stderr7" -ForegroundColor Red }
}
