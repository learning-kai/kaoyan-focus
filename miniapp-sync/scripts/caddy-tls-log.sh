#!/usr/bin/env bash
# ============================================================================
#  查看 Caddy 最近记录的 TLS 握手详情（排查「真机 TLS handshake failed」用）
#
#  前提：/etc/caddy/Caddyfile 头部已有 global { debug }（否则 Caddy 不记握手日志）
#
#  用法：
#    bash scripts/caddy-tls-log.sh              # 最近 15 分钟
#    bash scripts/caddy-tls-log.sh aliyun 60    # 指定 ssh 主机名与分钟数
#
#  读法：
#    client_hello 里的 SupportedVersions = [772,771] 表示客户端支持 TLS1.3+1.2，
#    只有 [771,770,769] 说明客户端最高只到 TLS 1.2。
#    若看到 "matched certificate in cache" 之后紧跟 "connection reset by peer"，
#    说明 Caddy 已正常回包、包在路径上被中间设备 RST——问题不在服务端。
# ============================================================================
set -uo pipefail

SSH_HOST="${1:-aliyun}"
MINUTES="${2:-15}"

ssh -o BatchMode=yes -o ConnectTimeout=15 "$SSH_HOST" \
  "sudo -n journalctl -u caddy --since '${MINUTES} min ago' --no-pager 2>/dev/null \
   | grep -i -E 'tls.handshake|client_hello|TLS handshake error|connection reset' \
   | tail -40"
