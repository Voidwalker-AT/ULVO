# Fast toolchain directory measurement using .NET EnumerateFiles
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

function Fast-MeasureDir([string]$path) {
    if (-not (Test-Path $path)) { return @{ Bytes = 0; Files = 0; Exists = $false } }
    $item = Get-Item -LiteralPath $path -Force
    if (-not $item.PSIsContainer) {
        return @{ Bytes = $item.Length; Files = 1; Exists = $true }
    }
    $totalBytes = [long]0
    $totalFiles = [long]0
    try {
        $di = New-Object System.IO.DirectoryInfo($path)
        foreach ($f in $di.EnumerateFiles("*", [System.IO.SearchOption]::AllDirectories)) {
            $totalBytes += $f.Length
            $totalFiles += 1
        }
    } catch {
        # Fallback for permission errors
    }
    return @{ Bytes = $totalBytes; Files = $totalFiles; Exists = $true }
}

$results = @()
foreach ($k in $toolchainPaths.Keys) {
    $p = $toolchainPaths[$k]
    $m = Fast-MeasureDir $p
    $results += [PSCustomObject]@{
        Name = $k
        Path = $p
        Exists = $m.Exists
        Bytes = $m.Bytes
        SizeMB = [math]::Round($m.Bytes / 1048576, 2)
        SizeGB = [math]::Round($m.Bytes / 1073741824, 3)
        Files = $m.Files
    }
}

$results | Format-Table Name, Exists, SizeMB, SizeGB, Files -AutoSize | Out-String | Write-Host
$results | Export-Clixml -Path "E:\Workspace\Active\ULVO\vault\scratch\measured_external_toolchains.xml"
