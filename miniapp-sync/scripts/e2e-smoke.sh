#!/usr/bin/env bash
# ============================================================================
#  考研专注同步 · 本地端到端冒烟测试
#
#  一条命令跑通三端：真实数据库副本 -> PC 代理 -> 同步服务 -> 小程序逻辑。
#  全程不碰真实数据库（用的是 %APPDATA% 下真实库的**副本**），也不连公网。
#
#  用法：
#    bash scripts/e2e-smoke.sh              # 默认端口 8788
#    bash scripts/e2e-smoke.sh 8899         # 指定端口
#
#  结束时会自动停掉测试服务端并清理临时数据目录。
# ============================================================================
set -u
# 必须开 pipefail：下面多处用 `命令 | tail` 收尾输出，
# 不开的话 `$?` 取到的是 tail 的退出码，被测命令失败也会被判成成功。
set -o pipefail

PORT="${1:-8788}"
TOKEN="test-token-local"
PAGE_SIZE=300
SERVER_PID=""
AGENT_CONFIG=""

# 测试数据目录放在系统临时目录，避免污染工程目录 / 触发网盘上传。
# 注意要转成 Windows 风格的正斜杠路径（C:/Users/...）：node 拿到 /tmp/xxx
# 会按当前盘符解析，和 bash 看到的不是同一个目录。
to_win() { cygpath -m "$1" 2>/dev/null || echo "$1"; }
SMOKE_DIR="$(to_win "${TEMP:-/tmp}")/kaoyan-sync-smoke-$PORT"
AGENT_STATE_DIR="$(to_win "${TEMP:-/tmp}")/kaoyan-sync-test"

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT" || exit 1

DB_SRC="${APPDATA}/com.kaoyan.focus/kaoyan-focus.sqlite3"
# 副本也放临时目录：放在工程目录里(百度网盘同步盘)会让每次写入都触发一次网盘上传，
# 82MB 的库能白白拖掉几十秒
DB_DST="$SMOKE_DIR/test.sqlite3"

red()   { printf '\033[31m%s\033[0m\n' "$*"; }
green() { printf '\033[32m%s\033[0m\n' "$*"; }
step()  { printf '\n\033[36m=== %s ===\033[0m\n' "$*"; }

cleanup() {
  if [ -n "${SERVER_PID:-}" ] && kill -0 "$SERVER_PID" 2>/dev/null; then
    kill "$SERVER_PID" 2>/dev/null
    wait "$SERVER_PID" 2>/dev/null
  fi
  rm -rf "$SMOKE_DIR"
  rm -f "$AGENT_CONFIG"
}
trap cleanup EXIT

# ------------------------------------------------- 1/6 真机连接诊断（不需要服务端）
# 先跑这一步：它纯本地、秒级完成，能在起服务端之前就把
# 「连接失败被吞掉 / 永远停在连接中」这类问题暴露出来。
step "1/6 真机连接诊断逻辑测试（本地）"
if ! node tests/mini-connect-diagnostic-test.js 2>&1 | tail -3; then
  red "真机连接诊断测试失败"; exit 1
fi

# ---------------------------------------------------------------- 2/6 数据库副本
step "2/6 准备数据库副本"
if [ ! -f "$DB_SRC" ]; then
  red "找不到真实数据库: $DB_SRC"
  exit 1
fi
rm -rf "$SMOKE_DIR"
mkdir -p "$SMOKE_DIR"
# 用 SQLite 自己的在线备份接口做一致性副本，**不要用 cp**：
#   1. 桌面端 24h 在跑，-wal / -shm 被它独占，cp 会报 cannot stat；
#   2. 只拷主文件会丢掉还在 WAL 里未 checkpoint 的最近提交；
#   3. backup() 走 SQLite 层逐页复制，拿到的永远是事务一致快照，且是只读打开。
if ! python - "$DB_SRC" "$DB_DST" <<'PY'
import os, sqlite3, sys
src, dst = sys.argv[1], sys.argv[2]
uri = 'file:///' + src.replace('\\', '/').lstrip('/') + '?mode=ro'
src_con = sqlite3.connect(uri, uri=True)
dst_con = sqlite3.connect(dst)
with dst_con:
    src_con.backup(dst_con)
rows = dst_con.execute('select count(*) from sync_meta').fetchone()[0]
dst_con.close()
src_con.close()
print('  一致性副本已生成，sync_meta %d 行' % rows)
PY
then
  red "数据库副本生成失败：$DB_SRC"; exit 1
fi
green "已复制 $(du -h "$DB_DST" | cut -f1) -> $DB_DST"

# ---------------------------------------------------------------- 3/6 起服务端
step "3/6 启动测试服务端 (:$PORT)"
SYNC_TOKEN="$TOKEN" SYNC_PORT="$PORT" SYNC_DATA_DIR="$SMOKE_DIR" \
  node server/index.js > "$SMOKE_DIR/server.log" 2>&1 &
SERVER_PID=$!

for _ in $(seq 1 40); do
  if grep -q '同步服务已启动' "$SMOKE_DIR/server.log" 2>/dev/null; then break; fi
  if ! kill -0 "$SERVER_PID" 2>/dev/null; then
    red "服务端启动失败："; cat "$SMOKE_DIR/server.log"; exit 1
  fi
  sleep 0.5
done
# 确认真的在监听，别让后面几步对着空气重试几十秒
if ! grep -q '同步服务已启动' "$SMOKE_DIR/server.log" 2>/dev/null; then
  red "服务端 20 秒内没有就绪："; cat "$SMOKE_DIR/server.log"; exit 1
fi
green "服务端已就绪 (pid=$SERVER_PID)"

# ---------------------------------------------------------------- 4/6 代理灌数据
step "4/6 PC 代理全量推送（本地 -> 服务端）"
rm -rf "$AGENT_STATE_DIR"
# config.test.json 里的端口是写死的，这里按实际端口生成一份临时配置。
# dbPath 必须写成绝对路径：代理是按**配置文件所在目录**解析相对路径的。
AGENT_CONFIG="$ROOT/agent/config.test.gen.json"
python - "$ROOT/agent/config.test.json" "$AGENT_CONFIG" "$PORT" "$DB_DST" <<'PY'
import json, sys
src, dst, port, db = sys.argv[1], sys.argv[2], sys.argv[3], sys.argv[4]
cfg = json.load(open(src, encoding='utf-8'))
cfg['server']['baseUrl'] = 'http://127.0.0.1:%s' % port
cfg['local']['dbPath'] = db
json.dump(cfg, open(dst, 'w', encoding='utf-8'), ensure_ascii=False, indent=2)
print('  临时配置已生成: %s (baseUrl=%s)' % (dst, cfg['server']['baseUrl']))
PY

if ! timeout 300 python agent/sync_agent.py --config "$AGENT_CONFIG" --once --resync 2>&1 | tail -4; then
  red "代理执行失败"; exit 1
fi

# ---------------------------------------------------------------- 5) 核对服务端
step "5/6 核对服务端实体统计"
python - "$PORT" "$TOKEN" <<'PY'
import json, sys, urllib.request
port, token = sys.argv[1], sys.argv[2]
opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))   # 必须绕开系统代理
req = urllib.request.Request('http://127.0.0.1:%s/snapshot' % port,
                             headers={'Authorization': 'Bearer ' + token})
data = json.loads(opener.open(req, timeout=30).read().decode())
total = 0
for etype, bucket in sorted(data['entities'].items()):
    live = [k for k, v in bucket.items() if not v.get('deletedAt')]
    tomb = len(bucket) - len(live)
    total += len(live)
    print('  %-20s 存活=%-6d 墓碑=%d' % (etype, len(live), tomb))
print('  存活实体合计: %d  (serverRev=%s)' % (total, data.get('serverRev')))
PY

# ---------------------------------------------------------------- 6) 小程序逻辑
step "6/6 小程序端逻辑冒烟测试"
node tests/mini-smoke-test.js \
  --url "ws://127.0.0.1:$PORT/ws" \
  --rest "http://127.0.0.1:$PORT" \
  --token "$TOKEN" \
  --page-size "$PAGE_SIZE"
RC=$?

if [ "$RC" -eq 0 ]; then green "\n全部通过"; else red "\n存在失败项 (exit=$RC)"; fi
exit "$RC"
