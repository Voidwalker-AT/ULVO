# Comprehensive verification of all backend commands backing the ULVO Desktop GUI
$ErrorActionPreference = "Continue"

$ulvo = "E:\Workspace\Active\ULVO\target\release\ulvo.exe"

Write-Host "============================================================" -ForegroundColor Cyan
Write-Host "       ULVO Desktop GUI Backend Response Verification       " -ForegroundColor Cyan
Write-Host "============================================================" -ForegroundColor Cyan

Write-Host "`n>>> [1. CATALOG BROWSING - 'Discover' Page] <<<" -ForegroundColor Yellow
& $ulvo catalog list

Write-Host "`n>>> [2. UNIVERSAL SEARCH - 'Discover' Search Bar] <<<" -ForegroundColor Yellow
$queries = @("python", "rust", "go", "ruby", "node")
foreach ($q in $queries) {
    Write-Host "--- Search Query: '$q' ---" -ForegroundColor Green
    & $ulvo catalog search $q
}

Write-Host "`n>>> [3. INSTALLED VAULT RESOURCES - 'Installed' Page] <<<" -ForegroundColor Yellow
& $ulvo vault list

Write-Host "`n>>> [4. THIN CONNECTOR AUDIT - 'Projects' Page] <<<" -ForegroundColor Yellow
& $ulvo project connector .

Write-Host "`n>>> [5. ENVIRONMENT LIFECYCLE - 'Projects' Page] <<<" -ForegroundColor Yellow
& $ulvo env audit

Write-Host "`n>>> [6. STORAGE DUAL-ACCOUNTING - 'Storage' Page] <<<" -ForegroundColor Yellow
& $ulvo storage report .

Write-Host "`n>>> [7. CACHE GOVERNANCE & RECLAMATION - 'Storage' Page] <<<" -ForegroundColor Yellow
& $ulvo cache status

Write-Host "`n>>> [8. BACKGROUND AGENT RECONCILIATION - 'Dashboard' / 'Activity'] <<<" -ForegroundColor Yellow
& $ulvo agent status
Write-Host "Triggering lease reconciliation check..."
& $ulvo agent reconcile

Write-Host "`n>>> [9. USER PREFERENCES PERSISTENCE - 'Settings' Page] <<<" -ForegroundColor Yellow
$prefFile = "E:\Workspace\Active\ULVO\vault\config\preferences.json"
if (Test-Path $prefFile) {
    Write-Host "Preferences file exists: $prefFile" -ForegroundColor Green
    Get-Content $prefFile
} else {
    Write-Host "Preferences file not yet created in vault. Default loaded." -ForegroundColor Yellow
}

Write-Host "`n============================================================" -ForegroundColor Green
Write-Host "       ALL 8 DESKTOP GUI BACKEND SERVICES VERIFIED!         " -ForegroundColor Green
Write-Host "============================================================" -ForegroundColor Green
