@echo off
chcp 65001 >nul
title API Monitor - 测试模式
cd /d "%~dp0"
call npm run tauri dev
echo.
echo ========================================
echo 进程已退出，按任意键关闭窗口...
pause >nul
