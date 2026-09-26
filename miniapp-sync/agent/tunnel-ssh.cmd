@echo off
REM ============================================================================
REM  考研专注 · SSH 隧道守护脚本（Windows）
REM
REM  作用：把服务器上的同步服务端口通过 SSH 转发到本机 127.0.0.1:8787，
REM        当直连 HTTPS 被网络环境干扰时，让同步代理改走这条隧道。
REM        隧道断开后会自动重连（每 10 秒检查一次）。
REM
REM  使用前提：
REM    1. 本机已能用 SSH 登录服务器（密钥免密更佳）
REM    2. 服务器上同步服务监听 127.0.0.1:8787（默认配置即是）
REM
REM  用法：
REM    修改下面的 SSH_TARGET 和 LOCAL_PORT 后双击运行，或加入任务计划程序开机启动。
REM    然后二选一：
REM      A. 把 agent/config.json 的 server.baseUrl 改成 http://127.0.0.1:8787
REM      B. 保持 baseUrl 为 HTTPS，把 server.fallback 设为
REM         { "enabled": true, "baseUrl": "http://127.0.0.1:8787", "afterFailures": 3 }
REM         这样直连正常时走公网，被干扰时自动回落到隧道。
REM ============================================================================

set SSH_TARGET=root@你的服务器地址
set LOCAL_PORT=8787
set REMOTE_PORT=8787

title 考研专注 SSH 隧道守护 (%LOCAL_PORT% -^> %SSH_TARGET%)

:loop
echo [%date% %time%] 正在建立隧道 %LOCAL_PORT% -^> %SSH_TARGET%:%REMOTE_PORT%
ssh -N -o ServerAliveInterval=30 -o ServerAliveCountMax=3 -o ExitOnForwardFailure=yes ^
    -L %LOCAL_PORT%:127.0.0.1:%REMOTE_PORT% %SSH_TARGET%
echo [%date% %time%] 隧道已断开，10 秒后重连...
timeout /t 10 /nobreak >nul
goto loop
