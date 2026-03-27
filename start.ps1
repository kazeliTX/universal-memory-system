# UMMS Startup Script
# Usage: .\start.ps1 [mode]
#   .\start.ps1       - Start all (Core + Dashboard + Chat)
#   .\start.ps1 core  - Core Service only
#   .\start.ps1 dev   - Core + Dashboard

param([string]$Mode = "all")

$ErrorActionPreference = "Stop"
$ProjectRoot = $PSScriptRoot

Write-Host ""
Write-Host "  ========================================" -ForegroundColor Cyan
Write-Host "    UMMS - Universal Memory Management" -ForegroundColor Cyan
Write-Host "  ========================================" -ForegroundColor Cyan
Write-Host ""

# Load .env
$envFile = Join-Path $ProjectRoot ".env"
if (Test-Path $envFile) {
    foreach ($line in Get-Content $envFile) {
        if ($line -match '^\s*([^#][^=]+)=(.+)$') {
            [Environment]::SetEnvironmentVariable($Matches[1].Trim(), $Matches[2].Trim(), "Process")
        }
    }
    Write-Host "  [OK] .env loaded" -ForegroundColor Green
}
else {
    Write-Host "  [!!] .env not found" -ForegroundColor Yellow
}

function Start-Core {
    $listening = Get-NetTCPConnection -LocalPort 8720 -ErrorAction SilentlyContinue
    if ($listening) {
        Write-Host "  [--] Core Service already running (port 8720)" -ForegroundColor Yellow
        return
    }
    Write-Host "  [..] Starting Core Service (port 8720)..." -ForegroundColor Cyan
    Start-Process -FilePath "cargo" -ArgumentList "run","-p","umms-server" -WorkingDirectory $ProjectRoot -WindowStyle Minimized

    for ($i = 0; $i -lt 30; $i++) {
        Start-Sleep -Seconds 1
        try {
            $r = Invoke-WebRequest -Uri "http://127.0.0.1:8720/api/health" -UseBasicParsing -TimeoutSec 2
            Write-Host "  [OK] Core Service ready" -ForegroundColor Green
            return
        }
        catch { }
    }
    Write-Host "  [!!] Core Service startup timeout (30s)" -ForegroundColor Red
}

function Start-Dashboard {
    $listening = Get-NetTCPConnection -LocalPort 5173 -ErrorAction SilentlyContinue
    if ($listening) {
        Write-Host "  [--] Dashboard already running (port 5173)" -ForegroundColor Yellow
        return
    }
    $dir = Join-Path $ProjectRoot "dashboard"
    if (-not (Test-Path (Join-Path $dir "node_modules"))) {
        Write-Host "  [..] Installing Dashboard deps..." -ForegroundColor Cyan
        Start-Process -FilePath "npm" -ArgumentList "install" -WorkingDirectory $dir -Wait -NoNewWindow
    }
    Write-Host "  [..] Starting Dashboard (port 5173)..." -ForegroundColor Cyan
    Start-Process -FilePath "npm" -ArgumentList "run","dev" -WorkingDirectory $dir -WindowStyle Minimized
    Start-Sleep -Seconds 3
    Write-Host "  [OK] Dashboard ready" -ForegroundColor Green
}

function Start-ChatClient {
    $listening = Get-NetTCPConnection -LocalPort 5174 -ErrorAction SilentlyContinue
    if ($listening) {
        Write-Host "  [--] Chat already running (port 5174)" -ForegroundColor Yellow
        return
    }
    $dir = Join-Path $ProjectRoot "chat"
    if (-not (Test-Path (Join-Path $dir "node_modules"))) {
        Write-Host "  [..] Installing Chat deps..." -ForegroundColor Cyan
        Start-Process -FilePath "npm" -ArgumentList "install" -WorkingDirectory $dir -Wait -NoNewWindow
    }
    Write-Host "  [..] Starting Chat (port 5174)..." -ForegroundColor Cyan
    Start-Process -FilePath "npm" -ArgumentList "run","dev" -WorkingDirectory $dir -WindowStyle Minimized
    Start-Sleep -Seconds 3
    Write-Host "  [OK] Chat ready" -ForegroundColor Green
}

switch ($Mode.ToLower()) {
    "core" { Start-Core }
    "dev"  { Start-Core; Start-Dashboard }
    "chat" { Start-Core; Start-ChatClient }
    "all"  { Start-Core; Start-Dashboard; Start-ChatClient }
    default {
        Write-Host "  Unknown mode: $Mode (use: all, core, dev, chat)" -ForegroundColor Red
        exit 1
    }
}

Write-Host ""
Write-Host "  ----------------------------------------" -ForegroundColor DarkGray
Write-Host "  Core:      http://127.0.0.1:8720" -ForegroundColor White
if ($Mode -eq "all" -or $Mode -eq "dev") {
    Write-Host "  Dashboard: http://127.0.0.1:5173" -ForegroundColor White
}
if ($Mode -eq "all" -or $Mode -eq "chat") {
    Write-Host "  Chat:      http://127.0.0.1:5174" -ForegroundColor White
}
Write-Host "  ----------------------------------------" -ForegroundColor DarkGray
Write-Host "  Stop all:  .\stop.ps1" -ForegroundColor DarkGray
Write-Host ""
