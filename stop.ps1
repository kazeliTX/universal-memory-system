# UMMS 一键停止脚本
# 用法: .\stop.ps1

Write-Host ""
Write-Host "  停止 UMMS 服务..." -ForegroundColor Yellow

$stopped = 0

# 停止 Core Service (umms-server / umms_server)
Get-Process -Name "umms-server", "umms_server" -ErrorAction SilentlyContinue | ForEach-Object {
    Stop-Process -Id $_.Id -Force
    $stopped++
    Write-Host "  [OK] Core Service 已停止 (PID $($_.Id))" -ForegroundColor Green
}

# 停止占用 8720 端口的进程
Get-NetTCPConnection -LocalPort 8720 -ErrorAction SilentlyContinue | ForEach-Object {
    $proc = Get-Process -Id $_.OwningProcess -ErrorAction SilentlyContinue
    if ($proc -and $proc.ProcessName -ne "System") {
        Stop-Process -Id $proc.Id -Force -ErrorAction SilentlyContinue
        $stopped++
        Write-Host "  [OK] 端口 8720 进程已停止 ($($proc.ProcessName), PID $($proc.Id))" -ForegroundColor Green
    }
}

# 停止占用 5173 端口的进程 (Dashboard)
Get-NetTCPConnection -LocalPort 5173 -ErrorAction SilentlyContinue | ForEach-Object {
    $proc = Get-Process -Id $_.OwningProcess -ErrorAction SilentlyContinue
    if ($proc) {
        Stop-Process -Id $proc.Id -Force -ErrorAction SilentlyContinue
        $stopped++
        Write-Host "  [OK] Dashboard (端口 5173) 已停止" -ForegroundColor Green
    }
}

# 停止占用 5174 端口的进程 (Chat)
Get-NetTCPConnection -LocalPort 5174 -ErrorAction SilentlyContinue | ForEach-Object {
    $proc = Get-Process -Id $_.OwningProcess -ErrorAction SilentlyContinue
    if ($proc) {
        Stop-Process -Id $proc.Id -Force -ErrorAction SilentlyContinue
        $stopped++
        Write-Host "  [OK] Chat (端口 5174) 已停止" -ForegroundColor Green
    }
}

if ($stopped -eq 0) {
    Write-Host "  [--] 没有运行中的 UMMS 服务" -ForegroundColor DarkGray
}

Write-Host ""
