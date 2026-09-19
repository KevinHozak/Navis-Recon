@echo off
setlocal
set "ROOT=%~dp0"

if exist "%ROOT%Navis-Recon.exe" (
  start "Navis-Recon" "%ROOT%Navis-Recon.exe"
  exit /b 0
)

if exist "%ROOT%frontend\src-tauri\target\release\navis-recon.exe" (
  start "Navis-Recon" "%ROOT%frontend\src-tauri\target\release\navis-recon.exe"
  exit /b 0
)

echo Navis-Recon.exe was not found. Run: cd frontend ^&^& npm run desktop:build
exit /b 1
