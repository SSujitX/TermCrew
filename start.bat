@echo off
title TermCrew
echo.
echo  Starting TermCrew (local web mode)
echo.

start "TermCrew Backend" cmd /k "cd /d %~dp0backend && cargo run"
timeout /t 2 /nobreak >nul
start "TermCrew Frontend" cmd /k "cd /d %~dp0frontend && bun run dev"

echo  Backend  http://127.0.0.1:3001
echo  Frontend http://localhost:5173
echo.
