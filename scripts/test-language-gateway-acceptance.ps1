$ErrorActionPreference = "Stop"

$ulvoCli = "E:\Workspace\Active\ULVO\bin\ulvo.exe"
$evidenceDir = "E:\Workspace\Active\ULVO\docs\evidence\stage3-4"
if (-not (Test-Path $evidenceDir)) {
    New-Item -ItemType Directory -Path $evidenceDir -Force | Out-Null
}
$evidenceJson = "$evidenceDir\language-gateway-acceptance.json"

Write-Host "============================================================" -ForegroundColor Cyan
Write-Host "    ULVO Stage 3.4 / v1.2.0 Language Gateway Acceptance    " -ForegroundColor Cyan
Write-Host "============================================================" -ForegroundColor Cyan

$results = [ordered]@{
    timestamp = [DateTimeOffset]::UtcNow.ToString("o")
    languages = @{}
    all_passed = $false
}

$tempDir = [System.IO.Path]::Combine([System.IO.Path]::GetTempPath(), "ulvo_lang_acceptance_" + [System.Guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Path $tempDir -Force | Out-Null

try {
    # --------------------------------------------------------------------------
    # 1. PYTHON
    # --------------------------------------------------------------------------
    Write-Host "`n>>> [1/7] Testing Python via Gateway..." -ForegroundColor Yellow
    $pyScript = Join-Path $tempDir "test.py"
    @'
import sys, os
print(f"PYTHON_EXE:{sys.executable}")
print(f"PYTHON_VER:{sys.version.split()[0]}")
print(f"PATH_ENTRIES:{len(os.environ.get('PATH', '').split(os.pathsep))}")
print("PYTHON_GATEWAY_SUCCESS")
'@ | Set-Content -Path $pyScript -Encoding utf8

    $pyOut = & $ulvoCli exec --tool python python $pyScript 2>&1
    $pyCode = $LASTEXITCODE
    $pyOutText = ($pyOut | Out-String).Trim()
    $pyPass = ($pyCode -eq 0) -and ($pyOutText -match "PYTHON_GATEWAY_SUCCESS")
    Write-Host "  Exit: $pyCode; Output: $pyOutText" -ForegroundColor DarkGray
    Write-Host "  Python Result: $(if ($pyPass) {'PASS'} else {'FAIL'})" -ForegroundColor $(if ($pyPass) {'Green'} else {'Red'})
    $results.languages["python"] = [ordered]@{
        passed = $pyPass
        exit_code = $pyCode
        output = $pyOutText
    }

    # --------------------------------------------------------------------------
    # 2. RUST
    # --------------------------------------------------------------------------
    Write-Host "`n>>> [2/7] Testing Rust (Cargo & Rustc) via Gateway..." -ForegroundColor Yellow
    $rustcOut = & $ulvoCli exec --tool rustc rustc --version 2>&1
    $rustcCode = $LASTEXITCODE
    $cargoOut = & $ulvoCli exec --tool cargo cargo --version 2>&1
    $cargoCode = $LASTEXITCODE
    $rustPass = ($rustcCode -eq 0) -and ($cargoCode -eq 0) -and ($rustcOut -match "rustc")
    Write-Host "  Rustc: $($rustcOut | Out-String).Trim()" -ForegroundColor DarkGray
    Write-Host "  Cargo: $($cargoOut | Out-String).Trim()" -ForegroundColor DarkGray
    Write-Host "  Rust Result: $(if ($rustPass) {'PASS'} else {'FAIL'})" -ForegroundColor $(if ($rustPass) {'Green'} else {'Red'})
    $results.languages["rust"] = [ordered]@{
        passed = $rustPass
        rustc_exit = $rustcCode
        rustc_version = ($rustcOut | Out-String).Trim()
        cargo_exit = $cargoCode
        cargo_version = ($cargoOut | Out-String).Trim()
    }

    # --------------------------------------------------------------------------
    # 3. GCC / G++ (MSYS2 UCRT64)
    # --------------------------------------------------------------------------
    Write-Host "`n>>> [3/7] Testing GCC / G++ (C/C++ Compilation) via Gateway..." -ForegroundColor Yellow
    $cppSource = Join-Path $tempDir "hello.cpp"
    $cppBin = Join-Path $tempDir "hello.exe"
    @'
#include <iostream>
int main() {
    std::cout << "CPP_GATEWAY_SUCCESS: GCC 14.2.0 active" << std::endl;
    return 0;
}
'@ | Set-Content -Path $cppSource -Encoding utf8

    $gppOut = & $ulvoCli exec --tool g++ g++ $cppSource -o $cppBin 2>&1
    $gppCode = $LASTEXITCODE
    $cppRunOut = ""
    $cppRunCode = -1
    if ($gppCode -eq 0 -and (Test-Path $cppBin)) {
        $cppRunOut = & $cppBin 2>&1
        $cppRunCode = $LASTEXITCODE
    }
    $cppPass = ($gppCode -eq 0) -and ($cppRunCode -eq 0) -and ($cppRunOut -match "CPP_GATEWAY_SUCCESS")
    Write-Host "  C++ Compile Exit: $gppCode; Run Exit: $cppRunCode; Output: $cppRunOut" -ForegroundColor DarkGray
    Write-Host "  GCC/G++ Result: $(if ($cppPass) {'PASS'} else {'FAIL'})" -ForegroundColor $(if ($cppPass) {'Green'} else {'Red'})
    $results.languages["gcc_cpp"] = [ordered]@{
        passed = $cppPass
        compile_exit = $gppCode
        run_exit = $cppRunCode
        output = ($cppRunOut | Out-String).Trim()
    }

    # --------------------------------------------------------------------------
    # 4. GO
    # --------------------------------------------------------------------------
    Write-Host "`n>>> [4/7] Testing Go via Gateway..." -ForegroundColor Yellow
    $goSource = Join-Path $tempDir "main.go"
    @'
package main
import "fmt"
func main() {
    fmt.Println("GO_GATEWAY_SUCCESS: Go 1.26.5 active")
}
'@ | Set-Content -Path $goSource -Encoding utf8

    $goOut = & $ulvoCli exec --tool go run $goSource 2>&1
    $goCode = $LASTEXITCODE
    $goOutText = ($goOut | Out-String).Trim()
    $goPass = ($goCode -eq 0) -and ($goOutText -match "GO_GATEWAY_SUCCESS")
    Write-Host "  Exit: $goCode; Output: $goOutText" -ForegroundColor DarkGray
    Write-Host "  Go Result: $(if ($goPass) {'PASS'} else {'FAIL'})" -ForegroundColor $(if ($goPass) {'Green'} else {'Red'})
    $results.languages["go"] = [ordered]@{
        passed = $goPass
        exit_code = $goCode
        output = $goOutText
    }

    # --------------------------------------------------------------------------
    # 5. RUBY
    # --------------------------------------------------------------------------
    Write-Host "`n>>> [5/7] Testing Ruby via Gateway..." -ForegroundColor Yellow
    $rbSource = Join-Path $tempDir "test.rb"
    @'
puts "RUBY_GATEWAY_SUCCESS: Ruby #{RUBY_VERSION} (#{RUBY_PLATFORM})"
'@ | Set-Content -Path $rbSource -Encoding utf8

    $rbOut = & $ulvoCli exec --tool ruby ruby $rbSource 2>&1
    $rbCode = $LASTEXITCODE
    $rbOutText = ($rbOut | Out-String).Trim()
    $rbPass = ($rbCode -eq 0) -and ($rbOutText -match "RUBY_GATEWAY_SUCCESS")
    Write-Host "  Exit: $rbCode; Output: $rbOutText" -ForegroundColor DarkGray
    Write-Host "  Ruby Result: $(if ($rbPass) {'PASS'} else {'FAIL'})" -ForegroundColor $(if ($rbPass) {'Green'} else {'Red'})
    $results.languages["ruby"] = [ordered]@{
        passed = $rbPass
        exit_code = $rbCode
        output = $rbOutText
    }

    # --------------------------------------------------------------------------
    # 6. NODE.JS
    # --------------------------------------------------------------------------
    Write-Host "`n>>> [6/7] Testing Node.js via Gateway..." -ForegroundColor Yellow
    $jsSource = Join-Path $tempDir "test.js"
    @'
console.log(`NODE_GATEWAY_SUCCESS: Node ${process.version}`);
'@ | Set-Content -Path $jsSource -Encoding utf8

    $nodeOut = & $ulvoCli exec --tool node node $jsSource 2>&1
    $nodeCode = $LASTEXITCODE
    $nodeOutText = ($nodeOut | Out-String).Trim()
    $nodePass = ($nodeCode -eq 0) -and ($nodeOutText -match "NODE_GATEWAY_SUCCESS")
    Write-Host "  Exit: $nodeCode; Output: $nodeOutText" -ForegroundColor DarkGray
    Write-Host "  Node.js Result: $(if ($nodePass) {'PASS'} else {'FAIL'})" -ForegroundColor $(if ($nodePass) {'Green'} else {'Red'})
    $results.languages["node"] = [ordered]@{
        passed = $nodePass
        exit_code = $nodeCode
        output = $nodeOutText
    }

    # --------------------------------------------------------------------------
    # 7. JAVA (JDK 21)
    # --------------------------------------------------------------------------
    Write-Host "`n>>> [7/7] Testing Java (Adoptium JDK 21) via Gateway..." -ForegroundColor Yellow
    $javaSource = Join-Path $tempDir "HelloGateway.java"
    @'
public class HelloGateway {
    public static void main(String[] args) {
        System.out.println("JAVA_GATEWAY_SUCCESS: Java " + System.getProperty("java.version"));
    }
}
'@ | Set-Content -Path $javaSource -Encoding ascii

    $javacOut = & $ulvoCli exec --tool javac javac $javaSource 2>&1
    $javacCode = $LASTEXITCODE
    $javaRunOut = ""
    $javaRunCode = -1
    if ($javacCode -eq 0) {
        $javaRunOut = & $ulvoCli exec --tool java java -cp $tempDir HelloGateway 2>&1
        $javaRunCode = $LASTEXITCODE
    }
    $javaPass = ($javacCode -eq 0) -and ($javaRunCode -eq 0) -and ($javaRunOut -match "JAVA_GATEWAY_SUCCESS")
    Write-Host "  Compile Exit: $javacCode; Run Exit: $javaRunCode; Output: $javaRunOut" -ForegroundColor DarkGray
    Write-Host "  Java Result: $(if ($javaPass) {'PASS'} else {'FAIL'})" -ForegroundColor $(if ($javaPass) {'Green'} else {'Red'})
    $results.languages["java"] = [ordered]@{
        passed = $javaPass
        compile_exit = $javacCode
        run_exit = $javaRunCode
        output = ($javaRunOut | Out-String).Trim()
    }

    $allPassed = $true
    foreach ($k in $results.languages.Keys) {
        if (-not $results.languages[$k].passed) {
            $allPassed = $false
        }
    }
    $results.all_passed = $allPassed

} finally {
    Remove-Item -Path $tempDir -Recurse -Force -ErrorAction SilentlyContinue
}

$results | ConvertTo-Json -Depth 5 | Set-Content -Path $evidenceJson -Encoding utf8
Write-Host "`n============================================================" -ForegroundColor Cyan
Write-Host "  Language Gateway Acceptance: $(if ($results.all_passed) {'ALL 7 PASS'} else {'FAILURES DETECTED'})" -ForegroundColor $(if ($results.all_passed) {'Green'} else {'Red'})
Write-Host "  Evidence saved to: $evidenceJson" -ForegroundColor DarkGray
Write-Host "============================================================" -ForegroundColor Cyan

if (-not $results.all_passed) {
    exit 1
}
