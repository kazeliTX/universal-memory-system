# UMMS 一键启动脚本
# 用法: .\start.ps1 [模式]
#   .\start.ps1         - 启动全部 (Core + Dashboard + Chat)
#   .\start.ps1 core    - 仅启动 Core Service
#   .\start.ps1 dev     - 启动 Core + Dashboard (无 Chat)

param(
    [string]$Mode = "all"
)

$ErrorActionPreference = "Stop"
$ProjectRoot = $PSScriptRoot

Write-Host ""
Write-Host "  ========================================" -ForegroundColor Cyan
Write-Host "    UMMS - Universal Memory Management" -ForegroundColor Cyan
Write-Host "  ========================================" -ForegroundColor Cyan
Write-Host ""

# 加载环境变量
$envFile = Join-Path $ProjectRoot ".env"
if (Test-Path $envFile) {
    Get-Content $envFile | ForEach-Object {
        if ($_ -match '^\s*([^#][^=]+)=(.+)$') {
            [Environment]::SetEnvironmentVariable($Matches[1].Trim(), $Matches[2].Trim(), "Process")
        }
    }
    Write-Host "  [OK] .env 已加载" -ForegroundColor Green
} else {
    Write-Host "  [!!] .env 未找到, GEMINI_API_KEY 可能未设置" -ForegroundColor Yellow
}

# 检查端口占用
function Test-Port($port) {
    $conn = Get-NetTCPConnection -LocalPort $port -ErrorAction SilentlyContinue
    return $null -ne $conn
}

# 启动 Core Service
function Start-Core {
    if (Test-Port 8720) {
        Write-Host "  [--] Core Service 已在运行 (端口 8720)" -ForegroundColor Yellow
        return
    }
    Write-Host "  [..] 启动 Core Service (端口 8720)..." -ForegroundColor Cyan
    Start-Process -FilePath "cargo" `
        -ArgumentList "run", "-p", "umms-server" `
        -WorkingDirectory $ProjectRoot `
        -WindowStyle Minimized

    # 等待服务就绪
    $retries = 0
    while ($retries -lt 30) {
        Start-Sleep -Seconds 1
        try {
            $null = Invoke-WebRequest -Uri "http://127.0.0.1:8720/api/health" -UseBasicParsing -TimeoutSec 2
            Write-Host "  [OK] Core Service 就绪" -ForegroundColor Green
            return
        } catch {
            $retries++
        }
    }
    Write-Host "  [!!] Core Service 启动超时 (30s)" -ForegroundColor Red
}

# 启动 Dashboard
function Start-Dashboard {
    if (Test-Port 5173) {
        Write-Host "  [--] Dashboard 已在运行 (端口 5173)" -ForegroundColor Yellow
        return
    }
    $dashDir = Join-Path $ProjectRoot "dashboard"
    if (-not (Test-Path (Join-Path $dashDir "node_modules"))) {
        Write-Host "  [..] 安装 Dashboard 依赖..." -ForegroundColor Cyan
        Start-Process -FilePath "npm" -ArgumentList "install" -WorkingDirectory $dashDir -Wait -NoNewWindow
    }
    Write-Host "  [..] 启动 Dashboard (端口 5173)..." -ForegroundColor Cyan
    Start-Process -FilePath "npm" `
        -ArgumentList "run", "dev" `
        -WorkingDirectory $dashDir `
        -WindowStyle Minimized
    Start-Sleep -Seconds 2
    Write-Host "  [OK] Dashboard 就绪" -ForegroundColor Green
}

# 启动 Chat
function Start-Chat {
    if (Test-Port 5174) {
        Write-Host "  [--] Chat 已在运行 (端口 5174)" -ForegroundColor Yellow
        return
    }
    $chatDir = Join-Path $ProjectRoot "chat"
    if (-not (Test-Path (Join-Path $chatDir "node_modules"))) {
        Write-Host "  [..] 安装 Chat 依赖..." -ForegroundColor Cyan
        Start-Process -FilePath "npm" -ArgumentList "install" -WorkingDirectory $chatDir -Wait -NoNewWindow
    }
    Write-Host "  [..] 启动 Chat 客户端 (端口 5174)..." -ForegroundColor Cyan
    Start-Process -FilePath "npm" `
        -ArgumentList "run", "dev" `
        -WorkingDirectory $chatDir `
        -WindowStyle Minimized
    Start-Sleep -Seconds 2
    Write-Host "  [OK] Chat 客户端就绪" -ForegroundColor Green
}

# 按模式启动
switch ($Mode.ToLower()) {
    "core" {
        Start-Core
    }
    "dev" {
        Start-Core
        Start-Dashboard
    }
    "all" {
        Start-Core
        Start-Dashboard
        Start-Chat
    }
    "chat" {
        Start-Core
        Start-Chat
    }
    default {
        Write-Host "  未知模式: $Mode" -ForegroundColor Red
        Write-Host "  可用模式: all, core, dev, chat" -ForegroundColor Yellow
        exit 1
    }
}

Write-Host ""
Write-Host "  ----------------------------------------" -ForegroundColor DarkGray
Write-Host "  Core Service:  http://127.0.0.1:8720" -ForegroundColor White

if ($Mode -in "all", "dev") {
    Write-Host "  Dashboard:     http://127.0.0.1:5173" -ForegroundColor White
}
if ($Mode -in "all", "chat") {
    Write-Host "  Chat:          http://127.0.0.1:5174" -ForegroundColor White
}

Write-Host "  ----------------------------------------" -ForegroundColor DarkGray
Write-Host ""
Write-Host "  按 Ctrl+C 不会停止后台服务" -ForegroundColor DarkGray
Write-Host "  停止全部: .\stop.ps1" -ForegroundColor DarkGray
Write-Host ""
