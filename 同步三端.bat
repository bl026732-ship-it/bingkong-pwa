@echo off
chcp 65001 >nul
setlocal
cd /d "%~dp0"
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0sync-three.ps1" %*
set RC=%ERRORLEVEL%
echo.
if not "%RC%"=="0" echo   [进程退出码 %RC%]  出了点问题，看上面提示。
pause
exit /b %RC%
