@echo off
cd /d "%~dp0"
where py >nul 2>nul
if %errorlevel% equ 0 (
  py -3 serve.py %*
) else (
  python serve.py %*
)
if errorlevel 1 (
  echo Install Python 3.10 or newer, then open Start-Shop.bat again.
  pause
)
