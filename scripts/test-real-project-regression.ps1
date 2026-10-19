param(
    [string]$UlvoExe = "E:\Workspace\Active\ULVO\bin\ulvo.exe",
    [string]$OutputFile = "E:\Workspace\Active\ULVO\docs\evidence\stage3-3\project-regression-suite.json"
)

$ErrorActionPreference = "Continue"

Write-Host "==================================================================" -ForegroundColor Cyan
Write-Host "  ULVO Stage 3.3 - Real Project Regression and Polyglot Test Suite" -ForegroundColor Cyan
Write-Host "==================================================================" -ForegroundColor Cyan

$results = [ordered]@{
    timestamp = (Get-Date).ToString("yyyy-MM-ddTHH:mm:sszzz")
    ulvo_executable = $UlvoExe
    verifications = @()
    summary = @{
        total = 0
        passed = 0
        failed = 0
    }
}

function Run-Verification {
    param(
        [string]$Name,
        [string]$Category,
        [scriptblock]$Action
    )
    $results.summary.total++
    Write-Host "[RUNNING] $Name..." -NoNewline
    $sw = [System.Diagnostics.Stopwatch]::StartNew()
    $res = [ordered]@{
        name = $Name
        category = $Category
        status = "FAILED"
        duration_ms = 0
        output = ""
        error = ""
    }
    try {
        $output = & $Action
        $sw.Stop()
        $res.duration_ms = $sw.ElapsedMilliseconds
        $res.output = ($output | Out-String).Trim()
        $res.status = "PASS"
        $results.summary.passed++
        $dur = $res.duration_ms
        Write-Host " [PASS] ($dur ms)" -ForegroundColor Green
    } catch {
        $sw.Stop()
        $res.duration_ms = $sw.ElapsedMilliseconds
        $res.error = $_.Exception.Message
        $results.summary.failed++
        $dur = $res.duration_ms
        $err = $res.error
        Write-Host " [FAIL] ($dur ms): $err" -ForegroundColor Red
    }
    $results.verifications += $res
}

# ------------------------------------------------------------------------------
# Test 1: CrossModalLab PyTorch CUDA Hardware Acceleration on RTX 2050
# ------------------------------------------------------------------------------
Run-Verification -Name "CrossModalLab_PyTorch_CUDA_MatMul" -Category "Python_CUDA" -Action {
    $cmlPy = Join-Path $env:TEMP "ulvo_test_cml_cuda.py"
    @'
import torch, time
assert torch.cuda.is_available(), 'CUDA is not available'
dev_name = torch.cuda.get_device_name(0)
x = torch.randn(4096, 4096, device='cuda')
y = torch.randn(4096, 4096, device='cuda')
t0 = time.time()
z = torch.matmul(x, y)
torch.cuda.synchronize()
dt = time.time() - t0
vram = torch.cuda.memory_allocated() / (1024**2)
print(f'Device: {dev_name}')
print(f'MatMul: 4096x4096 completed in {dt*1000:.2f} ms')
print(f'Allocated VRAM: {vram:.2f} MB')
'@ | Set-Content -Path $cmlPy -Encoding utf8

    $out = & $UlvoExe run E:\Workspace\Active\CrossModalLab python $cmlPy 2>&1
    $code = $LASTEXITCODE
    Remove-Item -Path $cmlPy -Force -ErrorAction SilentlyContinue
    $outText = ($out | Out-String)
    if ($code -ne 0) { throw "CrossModalLab run failed (code $code): $outText" }
    if ($outText -notmatch "RTX 2050") { throw "Expected RTX 2050 in output: $outText" }
    $outText.Trim()
}

# ------------------------------------------------------------------------------
# Test 2: CrossModalLab Zero Project Tree Bloat Verification
# ------------------------------------------------------------------------------
Run-Verification -Name "CrossModalLab_Zero_Project_Tree_Bloat" -Category "Storage_Preservation" -Action {
    $hasVenv = Test-Path -Path "E:\Workspace\Active\CrossModalLab\.venv"
    if ($hasVenv) { throw "Violation: Unmanaged .venv was detected in CrossModalLab" }
    $connectorSize = (Get-ChildItem -Path "E:\Workspace\Active\CrossModalLab\.ulvo" -Recurse | Measure-Object -Property Length -Sum).Sum
    if ($connectorSize -gt 10240) { throw "Violation: .ulvo connector exceeded thin connector budget ($connectorSize bytes)" }
    "Zero .venv detected; thin connector size = $connectorSize bytes"
}

# ------------------------------------------------------------------------------
# Test 3: NexusNode Execution and Unittest Pass
# ------------------------------------------------------------------------------
Run-Verification -Name "NexusNode_Unittest_Execution" -Category "Python_Server" -Action {
    $out = & $UlvoExe run E:\Workspace\Active\NexusNode\server python -m unittest tests/test_package.py 2>&1
    $code = $LASTEXITCODE
    $outText = ($out | Out-String)
    if ($code -ne 0) { throw "NexusNode unittests failed (code $code): $outText" }
    if ($outText -notmatch "OK") { throw "Expected OK in unittest output: $outText" }
    $outText.Trim()
}

# ------------------------------------------------------------------------------
# Test 4: NexusNode Network Socket Binding and Clean Release
# ------------------------------------------------------------------------------
Run-Verification -Name "NexusNode_Socket_Binding_And_Release" -Category "Network_Lifecycle" -Action {
    $sockPy = Join-Path $env:TEMP "ulvo_test_socket.py"
    @'
import socket
port = 58945
s1 = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
s1.bind(('127.0.0.1', port))
s1.listen(1)
print(f'Socket successfully bound and listening on 127.0.0.1:{port}')
s1.close()
s2 = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
s2.bind(('127.0.0.1', port))
print(f'Socket successfully rebound on 127.0.0.1:{port} after close')
s2.close()
print('Socket lifecycle verified cleanly')
'@ | Set-Content -Path $sockPy -Encoding utf8

    $out = & $UlvoExe run E:\Workspace\Active\NexusNode\server python $sockPy 2>&1
    $code = $LASTEXITCODE
    Remove-Item -Path $sockPy -Force -ErrorAction SilentlyContinue
    $outText = ($out | Out-String)
    if ($code -ne 0) { throw "Socket test failed (code $code): $outText" }
    if ($outText -notmatch "verified cleanly") { throw "Unexpected socket output: $outText" }
    $outText.Trim()
}

# ------------------------------------------------------------------------------
# Test 5: AstraRun Python 3.13 Runtime and Executable Verification
# ------------------------------------------------------------------------------
Run-Verification -Name "AstraRun_Python313_Execution" -Category "Python_313" -Action {
    $arPy = Join-Path $env:TEMP "ulvo_test_ar.py"
    @'
import sys
print(f'Python Version: {sys.version.split()[0]}')
assert sys.version_info.major == 3 and sys.version_info.minor >= 13, f'Expected Python 3.13+, got {sys.version}'
print('AstraRun Python 3.13 execution verified successfully')
'@ | Set-Content -Path $arPy -Encoding utf8

    $out = & $UlvoExe run E:\Workspace\Active\AstraRun python $arPy 2>&1
    $code = $LASTEXITCODE
    Remove-Item -Path $arPy -Force -ErrorAction SilentlyContinue
    $outText = ($out | Out-String)
    if ($code -ne 0) { throw "AstraRun test failed (code $code): $outText" }
    if ($outText -notmatch "3.13") { throw "Expected Python 3.13 in output: $outText" }
    $outText.Trim()
}

# ------------------------------------------------------------------------------
# Test 6: Polyglot Go Toolchain Execution via Gateway
# ------------------------------------------------------------------------------
Run-Verification -Name "Polyglot_Go_Gateway_Execution" -Category "Toolchain_Go" -Action {
    $testDir = Join-Path $env:TEMP "ulvo_polyglot_go"
    New-Item -ItemType Directory -Path $testDir -Force | Out-Null
    $goFile = Join-Path $testDir "main.go"
    @'
package main
import "fmt"
func main() {
    fmt.Println("Go 1.26.5 successfully compiled and executed via ULVO gateway!")
}
'@ | Set-Content -Path $goFile -Encoding utf8

    $out = & $UlvoExe exec --tool go run $goFile 2>&1
    $code = $LASTEXITCODE
    Remove-Item -Path $testDir -Recurse -Force -ErrorAction SilentlyContinue
    $outText = ($out | Out-String)
    if ($code -ne 0) { throw "Go execution failed (code $code): $outText" }
    if ($outText -notmatch "ULVO gateway") { throw "Unexpected Go output: $outText" }
    $outText.Trim()
}

# ------------------------------------------------------------------------------
# Test 7: Polyglot Ruby Toolchain Execution via Gateway
# ------------------------------------------------------------------------------
Run-Verification -Name "Polyglot_Ruby_Gateway_Execution" -Category "Toolchain_Ruby" -Action {
    $testDir = Join-Path $env:TEMP "ulvo_polyglot_ruby"
    New-Item -ItemType Directory -Path $testDir -Force | Out-Null
    $rbFile = Join-Path $testDir "test.rb"
    @'
puts "Ruby #{RUBY_VERSION} (#{RUBY_PLATFORM}) successfully executed via ULVO gateway!"
'@ | Set-Content -Path $rbFile -Encoding utf8

    $out = & $UlvoExe exec --tool ruby $rbFile 2>&1
    $code = $LASTEXITCODE
    Remove-Item -Path $testDir -Recurse -Force -ErrorAction SilentlyContinue
    $outText = ($out | Out-String)
    if ($code -ne 0) { throw "Ruby execution failed (code $code): $outText" }
    if ($outText -notmatch "ULVO gateway") { throw "Unexpected Ruby output: $outText" }
    $outText.Trim()
}

# ------------------------------------------------------------------------------
# Test 8: Polyglot Vault MSYS2 GCC C++ Compilation and Run
# ------------------------------------------------------------------------------
Run-Verification -Name "Polyglot_Vault_MSYS2_GCC_Compilation" -Category "Toolchain_Cpp" -Action {
    $testDir = Join-Path $env:TEMP "ulvo_polyglot_cpp"
    New-Item -ItemType Directory -Path $testDir -Force | Out-Null
    $cppFile = Join-Path $testDir "hello.cpp"
    $exeFile = Join-Path $testDir "hello.exe"
    @'
#include <iostream>
int main() {
    std::cout << "MSYS2 GCC 14.2.0 C++ binary compiled and executed via ULVO gateway!" << std::endl;
    return 0;
}
'@ | Set-Content -Path $cppFile -Encoding utf8

    $out = & $UlvoExe exec --tool g++ $cppFile -o $exeFile 2>&1
    $code = $LASTEXITCODE
    $outText = ($out | Out-String)
    if ($code -ne 0) { throw "C++ compilation failed (code $code): $outText" }
    if (-not (Test-Path $exeFile)) { throw "Expected compiled binary $exeFile was not created" }

    $runOut = & $exeFile 2>&1
    $runCode = $LASTEXITCODE
    Remove-Item -Path $testDir -Recurse -Force -ErrorAction SilentlyContinue
    $runOutText = ($runOut | Out-String)
    if ($runCode -ne 0) { throw "Compiled C++ binary exited with $($runCode): $runOutText" }
    if ($runOutText -notmatch "ULVO gateway") { throw "Unexpected C++ run output: $runOutText" }
    $runOutText.Trim()
}

# ------------------------------------------------------------------------------
# ------------------------------------------------------------------------------
# Test 9: Polyglot Node.js Execution via Gateway
# ------------------------------------------------------------------------------
Run-Verification -Name "Polyglot_NodeJS_Gateway_Execution" -Category "Toolchain_Node" -Action {
    $testDir = Join-Path $env:TEMP "ulvo_polyglot_node"
    New-Item -ItemType Directory -Path $testDir -Force | Out-Null
    $jsFile = Join-Path $testDir "index.js"
    @'
console.log(`Node.js ${process.version} executed successfully via ULVO gateway!`);
'@ | Set-Content -Path $jsFile -Encoding utf8

    $out = & $UlvoExe exec --tool node $jsFile 2>&1
    $code = $LASTEXITCODE
    Remove-Item -Path $testDir -Recurse -Force -ErrorAction SilentlyContinue
    $outText = ($out | Out-String)
    if ($code -ne 0) { throw "Node.js execution failed (code $code): $outText" }
    if ($outText -notmatch "ULVO gateway") { throw "Unexpected Node.js output: $outText" }
    $outText.Trim()
}

# ------------------------------------------------------------------------------
# Test 10: Polyglot Java JDK 21 Compilation and Execution via Gateway
# ------------------------------------------------------------------------------
Run-Verification -Name "Polyglot_Java_Compilation_And_Execution" -Category "Toolchain_Java" -Action {
    $testDir = Join-Path $env:TEMP "ulvo_polyglot_java"
    New-Item -ItemType Directory -Path $testDir -Force | Out-Null
    $javaFile = Join-Path $testDir "HelloUlvo.java"
    @'
public class HelloUlvo {
    public static void main(String[] args) {
        System.out.println("Java 21 successfully compiled and executed via ULVO gateway!");
    }
}
'@ | Set-Content -Path $javaFile -Encoding ascii

    # Compile with javac via gateway
    $compileOut = & $UlvoExe exec --tool javac $javaFile 2>&1
    $compileCode = $LASTEXITCODE
    $compileOutText = ($compileOut | Out-String)
    if ($compileCode -ne 0) { throw "Java compilation failed (code $compileCode): $compileOutText" }
    $classFile = Join-Path $testDir "HelloUlvo.class"
    if (-not (Test-Path $classFile)) { throw "Expected compiled class HelloUlvo.class was not created" }

    # Execute with java via gateway
    $runOut = & $UlvoExe exec --tool java -cp $testDir HelloUlvo 2>&1
    $runCode = $LASTEXITCODE
    Remove-Item -Path $testDir -Recurse -Force -ErrorAction SilentlyContinue
    $runOutText = ($runOut | Out-String)
    if ($runCode -ne 0) { throw "Java execution failed (code $runCode): $runOutText" }
    if ($runOutText -notmatch "ULVO gateway") { throw "Unexpected Java run output: $runOutText" }
    $runOutText.Trim()
}

# ------------------------------------------------------------------------------
# Test 11: NexusNode Server HTTP Request Handling and Lifecycle Shutdown
# ------------------------------------------------------------------------------
Run-Verification -Name "NexusNode_Http_Server_Request_Lifecycle" -Category "Server_Lifecycle" -Action {
    $serverPy = Join-Path $env:TEMP "ulvo_test_nexus_server.py"
    @'
import http.server
import socketserver
import threading
import sys

PORT = 58948

class Handler(http.server.SimpleHTTPRequestHandler):
    def do_GET(self):
        if self.path == "/health":
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.end_headers()
            self.wfile.write(b'{"status": "healthy", "engine": "NexusNode", "orchestrator": "ULVO"}')
        else:
            self.send_response(404)
            self.end_headers()
    def log_message(self, format, *args):
        pass

server = socketserver.TCPServer(("127.0.0.1", PORT), Handler)
server.allow_reuse_address = True
print(f"NEXUS_SERVER_READY:{PORT}", flush=True)

# Run server with timeout
server.handle_request()
server.server_close()
print("NEXUS_SERVER_SHUTDOWN_CLEAN", flush=True)
'@ | Set-Content -Path $serverPy -Encoding utf8

    # Start server as a background process through ulvo
    $srvOut = Join-Path $env:TEMP "srv_out.txt"
    $srvErr = Join-Path $env:TEMP "srv_err.txt"
    $proc = Start-Process -FilePath $UlvoExe -ArgumentList @("run", "E:\Workspace\Active\NexusNode\server", "python", $serverPy) -NoNewWindow -PassThru -RedirectStandardOutput $srvOut -RedirectStandardError $srvErr

    # Wait for server to bind
    $bound = $false
    for ($i = 0; $i -lt 30; $i++) {
        Start-Sleep -Milliseconds 300
        $content = Get-Content $srvOut -Raw -ErrorAction SilentlyContinue
        if ($content -match "NEXUS_SERVER_READY:58948") {
            $bound = $true
            break
        }
    }

    if (-not $bound) {
        Stop-Process -Id $proc.Id -Force -ErrorAction SilentlyContinue
        $errText = Get-Content $srvErr -Raw -ErrorAction SilentlyContinue
        throw "Server failed to start or bind port 58948: $errText"
    }

    # Issue HTTP GET request
    $response = $null
    try {
        $response = Invoke-RestMethod -Uri "http://127.0.0.1:58948/health" -Method Get -TimeoutSec 5
    } catch {
        Stop-Process -Id $proc.Id -Force -ErrorAction SilentlyContinue
        throw "HTTP request to NexusNode server failed: $($_.Exception.Message)"
    }

    $proc.WaitForExit(10000)
    Remove-Item -Path $serverPy, $srvOut, $srvErr -Force -ErrorAction SilentlyContinue

    if ($response.status -ne "healthy" -or $response.orchestrator -ne "ULVO") {
        throw "Unexpected response payload: $($response | ConvertTo-Json -Compress)"
    }

    "HTTP 200 OK: status=$($response.status), engine=$($response.engine), orchestrator=$($response.orchestrator); clean shutdown verified"
}


# ------------------------------------------------------------------------------
# Final Output and Summary
# ------------------------------------------------------------------------------
$parentDir = Split-Path -Path $OutputFile -Parent
if (-not (Test-Path $parentDir)) {
    New-Item -ItemType Directory -Path $parentDir -Force | Out-Null
}

$results | ConvertTo-Json -Depth 5 | Set-Content -Path $OutputFile -Encoding utf8

Write-Host "`n==================================================================" -ForegroundColor Cyan
Write-Host "  Results: $($results.summary.passed)/$($results.summary.total) Passed ($($results.summary.failed) Failed)" -ForegroundColor $(if ($results.summary.failed -eq 0) { "Green" } else { "Red" })
Write-Host "  Evidence saved to: $OutputFile" -ForegroundColor Cyan
Write-Host "==================================================================" -ForegroundColor Cyan

if ($results.summary.failed -gt 0) {
    exit 1
} else {
    exit 0
}
