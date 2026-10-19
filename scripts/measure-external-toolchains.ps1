$ErrorActionPreference = "SilentlyContinue"

$toolchainPaths = [ordered]@{
    "MSYS2 (C:\msys64)" = "C:\msys64"
    "Ruby (C:\Ruby40-x64)" = "C:\Ruby40-x64"
    "Go (C:\Program Files\Go)" = "C:\Program Files\Go"
    "Node.js (C:\Program Files\nodejs)" = "C:\Program Files\nodejs"
    "Java Temurin JDK 21 (C:\Program Files\Eclipse Adoptium\jdk-21.0.4.7-hotspot)" = "C:\Program Files\Eclipse Adoptium\jdk-21.0.4.7-hotspot"
    "Python 3.10 (C:\Program Files\Python310)" = "C:\Program Files\Python310"
    "Python 3.11 (C:\Users\hp\AppData\Local\Programs\Python\Python311)" = "C:\Users\hp\AppData\Local\Programs\Python\Python311"
    "Python 3.13 (C:\Users\hp\AppData\Local\Programs\Python\Python313)" = "C:\Users\hp\AppData\Local\Programs\Python\Python313"
    "uv standalone (C:\Users\hp\.local\bin\uv.exe)" = "C:\Users\hp\.local\bin\uv.exe"
    "uv cache (C:\Users\hp\scoop\persist\uv\cache)" = "C:\Users\hp\scoop\persist\uv\cache"
    "uv python versions (C:\Users\hp\scoop\persist\uv\python\versions)" = "C:\Users\hp\scoop\persist\uv\python\versions"
    "Cargo Host Dir (C:\Users\hp\.cargo)" = "C:\Users\hp\.cargo"
    "Rustup Host Dir (C:\Users\hp\.rustup)" = "C:\Users\hp\.rustup"
}

$results = @()
foreach ($name in $toolchainPaths.Keys) {
    $p = $toolchainPaths[$name]
    if (Test-Path $p) {
        $item = Get-Item $p
        if ($item.PSIsContainer) {
            $meas = Get-ChildItem -Path $p -Recurse -Force -ErrorAction SilentlyContinue | Measure-Object -Property Length -Sum
            $bytes = if ($meas.Sum) { $meas.Sum } else { 0 }
            $count = if ($meas.Count) { $meas.Count } else { 0 }
        } else {
            $bytes = $item.Length
            $count = 1
        }
        $results += [PSCustomObject]@{
            Name = $name
            Path = $p
            Exists = $true
            Bytes = $bytes
            SizeMB = [math]::Round($bytes / 1MB, 2)
            SizeGB = [math]::Round($bytes / 1GB, 3)
            Files = $count
        }
    } else {
        $results += [PSCustomObject]@{
            Name = $name
            Path = $p
            Exists = $false
            Bytes = 0
            SizeMB = 0
            SizeGB = 0
            Files = 0
        }
    }
}

$results | Format-Table Name, Exists, SizeMB, SizeGB, Files -AutoSize | Out-String | Write-Host
$results | Export-Clixml -Path "E:\Workspace\Active\ULVO\vault\scratch\measured_external_toolchains.xml"
