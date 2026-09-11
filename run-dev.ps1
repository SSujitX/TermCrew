# TermCrew launcher
Write-Host "==================================================" -ForegroundColor Cyan
Write-Host " Starting TermCrew (local web mode)" -ForegroundColor Cyan
Write-Host "==================================================" -ForegroundColor Cyan

$CurrentDir = Get-Location

# 1. Start Rust Backend in a separate window
Write-Host "[1/2] Starting TermCrew backend on http://127.0.0.1:3001..." -ForegroundColor Green
$BackendProcess = Start-Process powershell -ArgumentList "-NoExit", "-Command", "cd '$CurrentDir\backend'; cargo run" -PassThru

# Wait until the backend answers before starting Vite, so the first page load
# doesn't hit the proxy with ECONNRESET while cargo is still compiling.
Write-Host "Waiting for backend to become ready (first build can take a while)..." -ForegroundColor Gray
$BackendReady = $false
for ($i = 0; $i -lt 120; $i++) {
    Start-Sleep -Seconds 2
    try {
        $response = Invoke-WebRequest -Uri "http://127.0.0.1:3001/api/agents" -UseBasicParsing -TimeoutSec 2
        if ($response.StatusCode -eq 200) { $BackendReady = $true; break }
    } catch {
        # Not up yet (or an old instance is rebuilding) — keep waiting
    }
    if ($BackendProcess.HasExited) {
        Write-Host "Backend window exited early — check the backend window for a compile or port-bind error." -ForegroundColor Red
        break
    }
}

if (-not $BackendReady) {
    Write-Host "Backend did not become ready in time. Starting Vite anyway; refresh the page once the backend is up." -ForegroundColor Yellow
} else {
    Write-Host "Backend is ready." -ForegroundColor Green
}

# 2. Start Frontend Vite Dev Server
Write-Host "[2/2] Starting TermCrew UI on http://localhost:5173..." -ForegroundColor Green
Set-Location -Path "$CurrentDir\frontend"
bun run dev
