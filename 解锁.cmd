@echo off
echo ======================================
echo 调用已存在 unlock-kaoyan-focus.ps1
echo ======================================
echo.

:: 当前目录下的ps1脚本
set "PS1_FILE=%~dp0unlock-kaoyan-focus.ps1"

if not exist "%PS1_FILE%" (
    echo 错误：找不到脚本文件：%PS1_FILE%
    echo 请把此cmd和unlock-kaoyan-focus.ps1放在同一个文件夹！
    goto end
)

echo 正在执行PowerShell脚本，绕过执行策略限制...
echo.

:: Bypass 绕过本地脚本权限限制，不需要手动修改系统策略
powershell -ExecutionPolicy Bypass -File "%PS1_FILE%"

echo.
echo 执行完成。

:end
echo 按任意键退出...
pause >nul