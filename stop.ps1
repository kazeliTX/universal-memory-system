# UMMS Stop Script
# Usage: .\stop.ps1

Write-Host ""
Write-Host "  Stopping UMMS services..." -ForegroundColor Yellow

$stopped = 0

foreach ($port in @(8720, 5173, 5174)) {
    $conns = Get-NetTCPConnection -LocalPort $port -ErrorAction SilentlyContinue
    foreach ($c in $conns) {
        $proc = Get-Process -Id $c.OwningProcess -ErrorAction SilentlyContinue
        if ($proc -and $proc.ProcessName -ne "System") {
            Stop-Process -Id $proc.Id -Force -ErrorAction SilentlyContinue
            Write-Host "  [OK] Port $port stopped ($($proc.ProcessName), PID $($proc.Id))" -ForegroundColor Green
            $stopped++
        }
    }
}

if ($stopped -eq 0) {
    Write-Host "  [--] No running UMMS services found" -ForegroundColor DarkGray
}

Write-Host ""
