# 考研专注 · 桌面端 ↔ 微信小程序 实时同步

把电脑上「考研专注」桌面端的本地数据，实时同步到手机微信小程序，并支持手机回写。

三端职责：`本地 SQLite（事实来源）` ↔ `公网中转服务（唯一仲裁者）` ↔ `小程序（轻量客户端）`。

---

## 1. 整体方案

```
┌──────────────── 电脑（Windows，桌面端常驻）────────────────┐
│  kaoyan-focus.sqlite3   ← 桌面端事实来源                    │
│        ▲ 写回（字段白名单）        │ 扫描 diff（2s 轮询）      │
│        │                          ▼                         │
│  agent/sync_agent.py  ──REST/SSE──┐                        │
└───────────────────────────────────┼────────────────────────┘
                                    │  HTTPS
                        ┌───────────▼────────────┐
                        │  server/（Node + ws）   │
                        │  快照 + oplog + 仲裁    │
                        │  token 鉴权             │
                        └───────────┬────────────┘
                                    │  WSS
                        ┌───────────▼────────────┐
                        │  微信小程序             │
                        │  WS 长连接 + 本地缓存   │
                        └────────────────────────┘
```

**为什么是星型而不是直连**：微信小程序禁止访问 `http://` 和内网 IP，手机和电脑往往也不在同一网络，所以必须有一个公网域名的 WSS 端点做中转。服务端因此成为**唯一仲裁者**：所有冲突都在服务端裁决，两端只接受裁决结果，不会出现两端各自算出一版真相的情况。

**为什么选自建服务器而不是托管云服务**（你有服务器和域名的前提下）：

| 维度 | 自建服务器（本方案） | 托管云服务 |
| --- | --- | --- |
| 成本 | 已有服务器，边际成本 0 | 通常按调用量/存储计费，长连接心跳会持续计费 |
| 实时性 | WebSocket 常驻，端到端 <1s | 多数托管 DB 只能轮询，秒级到十秒级 |
| 数据归属 | 学习记录/复盘内容全在自己机器上 | 数据经过第三方平台 |
| 协议控制 | 自定义 oplog / 版本号 / 冲突策略 | 只能用平台提供的能力 |
| 运维 | 自己管证书、进程、备份 | 平台托管 |

结论：本项目数据量小（约 3400 条实体）、但需要常驻长连接和自定义冲突策略，且已有服务器与域名 → 自建明显更合适。

---

## 2. 同步范围

本地数据位置：`%APPDATA%\com.kaoyan.focus\kaoyan-focus.sqlite3`（SQLite，WAL 模式）。

| 实体 | 本地表 | 方向 | 小程序可写字段 |
| --- | --- | --- | --- |
| `subject` 科目 | `subjects` | 双向（PC 权威） | 只读 |
| `checklist_task` 清单任务 | `checklist_tasks` | 双向 | 标题 / 备注 / 截止 / 完成 / 排序 |
| `today_plan_item` 今日任务 | `today_plan_items` | 双向 | 标题 / 备注 / 截止 / 完成 / 排序 |
| `schedule_block` 课表块 | `schedule_blocks` | 双向（PC 权威） | 仅状态 |
| `schedule_template` 课表模板 | `schedule_templates` | 双向（PC 权威） | 只读 |
| `daily_review` 每日复盘 | `daily_reviews` | 双向 | 总结 / 卡点 / 明日重点 / 评分 |
| `weekly_review` 周复盘 | `weekly_reviews` | 双向 | 同上 |
| `focus_session` 专注记录 | `focus_sessions` | PC → 手机 | 只读 |
| `study_mode` 学习模式 | `study_modes` | PC → 手机 | 只读 |
| `app_event` 干扰事件 | `app_events` | PC → 手机（近 3 天） | 只读 |
| `settings` 设置 | `settings` | **不同步** | — |

**明确排除**：`settings` 表内含 WebDAV 密码、飞书 secret、SMTP 密码，绝不进入同步通道。服务端还做了第二道拦截：字段名命中 `password|secret|token|api_key|credential|smtp|webdav` 等模式的一律丢弃（已实测）。

`sync_id` 复用桌面端已有的 `sync_meta` 表，桌面端自带的对象存储同步与本项目共用同一套 ID，互不冲突；缺失时才由代理补发 `uuid` 形式的 ID。

---

## 3. 同步方式与延迟

| 环节 | 机制 | 目标延迟 |
| --- | --- | --- |
| 电脑本地变更检测 | 每 2s 扫描一次业务表，按内容指纹 diff | ≤2s |
| 电脑 → 服务端 | HTTP POST 批量推送（单批 200 条） | 即时 |
| 服务端 → 手机 | WebSocket 广播 | <1s |
| 手机 → 服务端 | WebSocket 立即推送（先入本地发件箱） | <1s |
| 服务端 → 电脑 | SSE 长连接推送 | <1s |
| 一致性兜底 | 每 5 分钟摘要比对 | 5 分钟 |

**实测**（本机服务端 + 数据库副本）：手机端改一条清单任务 → 服务端接受 → 电脑端写回 SQLite，全程 <1s；电脑 → 手机方向 <3s。

触发方式三选一都支持：**变更监听式**（2s 轮询 diff，默认）、**推送式**（SSE/WS）、**手动式**（小程序下拉刷新触发摘要校验，PC 端 `--resync` 强制全量重推）。

---

## 4. 冲突处理

裁决全部在服务端，规则按优先级：

| 优先级 | 规则 | 含义 |
| --- | --- | --- |
| 1 | 权限 | PC 权威实体（专注记录 / 学习模式 / 干扰事件）不接受小程序写入，直接 `rejected` |
| 2 | 强制重同步 | PC 端 `force=true` 的全量重推直接胜出（本地 SQLite 是事实来源） |
| 3 | 幂等 | 内容指纹相同 → `duplicate`，不产生新版本 |
| 4 | 快进 | `baseRev` 命中当前版本 → 无并发，直接接受 |
| 5 | LWW | 更新时间新者胜；时间相同则指纹大者胜（确定性 tie-break） |

- **版本记录**：每个实体保留最近 5 个历史版本（字段快照 + 设备 + 时间），服务实体回滚排查。
- **冲突提示**：落败方会收到 `conflict` 回执，含胜出版本的字段；小程序端自动回退本地乐观更新，弹出「N 条修改被电脑端新版覆盖」，并在「我的」页保留最近 50 条冲突记录。
- **PC 端落败时**：主动拉取服务端版本写回本地 SQLite，避免下一轮继续推出去形成拉锯。

> **小程序写入必须按字段合并，不能整体替换。**
> 小程序只能写 `miniWritable` 白名单里的字段（`schema.js`），所以它的 op 到了服务端只剩这些字段。
> 如果拿这份过滤后的字段**整体覆盖**实体，电脑端写入的其它字段就被抹掉了——
> 例如小程序只改了个标题，`checklist_task.column_name` 会一起消失。
> 后果是服务端版本与两端都不一致：摘要校验来回抖动、小程序白白拉全量、新设备拿到的快照还缺字段。
> 所以 `store.js#applyOps` 对非 PC 角色做 `{ ...current.fields, ...sanitized }` 合并，
> 并且 **`hash` 由合并后的字段算出**，保证「存下来的实体」与「记录的指纹」始终一致。
> PC 角色推的是完整字段集，不需要合并（整体替换即等价）。

---

## 5. 运行与通信环境

| 端 | 运行环境 | 通道 | 鉴权 |
| --- | --- | --- | --- |
| 电脑 | Python 3.9+（仅标准库，零依赖）常驻进程 | HTTPS REST 上行 + SSE 下行 | `Authorization: Bearer <token>` |
| 服务端 | Node 18+，仅依赖 `ws`；建议 nginx 终结 TLS | 监听 `127.0.0.1:8787` | 同 token；WS 走 query（小程序不能自定义 Header） |
| 小程序 | 微信（真机） | WSS `wss://<域名>/sync/ws` | query 携带 token + deviceId + role |

域名要求：**国内服务器域名需 ICP 备案**，并在微信公众平台 → 开发管理 → 服务器域名里，把域名同时加进 **socket 合法域名**和 **request 合法域名**（后者用于手动一致性校验，可省略）。

安全建议：token 用 32 位随机串，三处保持一致；只开放 443；定期轮换 token；服务端数据目录定期备份。

---

## 6. 异常处理

| 场景 | 策略 |
| --- | --- |
| 断线重连 | 指数退避 1s→2s→4s…→30s + 抖动；小程序前后台切换、网络恢复、心跳超时（45s）都会触发重连 |
| 离线期间数据 | 手机端改动写入本地发件箱（持久化），连接恢复后自动补发；服务端按 `opId` 去重，重复补发安全 |
| 增量同步 | 客户端带 `cursor`（oplog 序号）拉取增量；**必须按响应里的 `nextCursor`（= 本批最后一条 op 的 seq）推进**，直接跳到 `serverRev` 会永久跳过中间那段。cursor 早于日志窗口时服务端返回 `stale`，客户端改拉全量快照 |
| 大批量下发 | 快照按 `snapshotPageSize` 分页（PC/小程序都支持），弱网下单次响应更小、更易重试成功；增量按 `deltaBatchSize` 分页，`truncated=true` 时循环续拉 |
| 失败重试 | 推送失败不推进本地状态指纹 → 下一轮自动重推；单批失败重试 3 次（1s/2s/4s）后放弃，避免阻塞后续 |
| 一致性校验 | 每 5 分钟比对全量摘要（FNV-1a-64，三端同算法）。**只统计存活实体，墓碑不参与**——见下方说明 |
| 幂等性 | `opId` 全局去重（最近 2 万条）；实体内容指纹相同视为同一版本，不产生新版本 |
| 并发写本地库 | 与桌面端进程同时访问同一 SQLite，靠 WAL + `busy_timeout=10s` 避免锁死；代理只写白名单字段 |
| 单点故障 | 服务端快照 `state.json`（原子写）+ 追加式 `oplog.jsonl`；进程退出/重启后自动恢复 |
| 服务端遗留脏实体 | `--prune-orphans` 找出「服务端有、本地 `sync_meta` 没有」的实体并推墓碑清理 |

> **为什么摘要必须排除墓碑？**
> 墓碑（已删除实体）的 `deletedAt` 是各端各自记的本地时刻，天然不可能相同。
> 若把墓碑算进摘要，三端永远对不上，表现为每 5 分钟误触发一次「强制全量重推」——
> 白白刷满带宽，还会把删除操作反复来回推。
> 排除墓碑不会漏判：只要一方删了、另一方没删，存活集合就会多一条，摘要照样能发现。
> 三端口径必须一致：`server/store.js#digest`、`miniapp/services/store.js#digest`、
> `agent/protocol.py#digest_of`（PC 端本来就只扫业务表，天然不含墓碑）。

---

## 7. 目录结构

```
miniapp-sync/
├── README.md                  本文件（方案 + 部署）
├── package.json               统一脚本入口
├── server/                    公网中转服务（Node）
│   ├── config.js              ★ 服务端集中配置（端口 / token / 存储 / 限流）
│   ├── schema.js              ★ 实体规则：同步范围、权威方、小程序可写字段、敏感字段黑名单
│   ├── protocol.js            ★ 三端共用的指纹与冲突仲裁算法
│   ├── store.js               快照 + oplog 持久化
│   ├── index.js               HTTP REST + SSE + WebSocket
│   └── package.json
├── agent/                     PC 端同步代理（Python，零第三方依赖）
│   ├── config.json            ★ PC 端集中配置（数据库路径 / 服务端地址 / 轮询周期 / 代理 / 开关）
│   ├── sync_agent.py          主循环：扫描 → 推送 → 写回 → 校验
│   ├── local_store.py         SQLite 读写：实体翻译、sync_id 复用、白名单写回
│   ├── protocol.py            ★ 三端共用的指纹算法
│   ├── transport.py           REST + SSE 客户端（代理控制 / IPv4 强制 / 错误分类 / 退避重连）
│   ├── doctor.py              网络体检：DNS → TCP → TLS → HTTPS → 大报文 → SSE，逐层给结论
│   ├── tunnel-ssh.cmd         SSH 隧道保活（本机 TLS 被网络中断时的兜底通道）
│   └── config.test.json       测试用配置（指向数据库副本，不碰真实数据）
├── miniapp/                   微信小程序
│   ├── config.js              ★ 小程序端集中配置（wsUrl / token / 周期）
│   ├── app.js / app.json / app.wxss
│   ├── services/
│   │   ├── protocol.js        ★ 三端共用的指纹算法
│   │   ├── store.js           本地缓存 + 业务选择器
│   │   └── sync-service.js    WS 客户端 + 发件箱 + 重连 + 校验
│   ├── pages/today/           今日：学习状态、倒计时、今日计划
│   ├── pages/tasks/           清单：勾选、新增、删除
│   ├── pages/review/          复盘：今日总结 / 卡点 / 明日重点 / 评分
│   └── pages/mine/            我的：连接状态、连接诊断、摘要校验、冲突记录、缓存管理
├── scripts/
│   ├── e2e-smoke.sh           一条命令跑通三端冒烟测试（一致性副本，不碰真实数据）
│   └── caddy-tls-log.sh       一键读取服务器 Caddy 的 TLS 握手 debug 日志
└── tests/
    ├── mini-smoke-test.js     小程序逻辑冒烟测试（Node 模拟微信环境，需服务端）
    ├── mini-connect-diagnostic-test.js  真机连接失败的可诊断性测试（纯本地，无需服务端）
    ├── ws_handshake_probe.py  生产 WSS 端点握手自检（TLS/证书链/HTTP 状态行）
    └── cert_chain_audit.py    证书链审计：下发链、所需信任锚、可选 SSL Labs 第三方评估
```

标 ★ 的是配置项集中地，改配置只动这四个文件。

> **重要**：`server/protocol.js`、`agent/protocol.py`、`miniapp/services/protocol.js` 三处的指纹算法必须逐字一致，改一处必须同步三处，否则一致性校验会永远误报。
> 同理，三处的 `digest` **口径**（哪些实体算进去）也必须一致——见第 6 节的墓碑说明。

---

## 8. 部署步骤

### 8.1 服务端（你的服务器）

```bash
cd miniapp-sync/server
npm install                 # 只有 ws 一个依赖
# 编辑 config.js：改 auth.token（32 位随机串）
mkdir -p /opt/kaoyan-sync && cp -r . /opt/kaoyan-sync/
cd /opt/kaoyan-sync && SYNC_TOKEN=你的token node index.js
```

用 systemd 托管（`/etc/systemd/system/kaoyan-sync.service`）：

```ini
[Unit]
Description=Kaoyan Focus Sync Server
After=network.target

[Service]
Type=simple
WorkingDirectory=/opt/kaoyan-sync
Environment=SYNC_TOKEN=你的token
Environment=SYNC_PORT=8787
Environment=SYNC_DATA_DIR=/opt/kaoyan-sync/data
ExecStart=/usr/bin/node index.js
Restart=always
RestartSec=3

[Install]
WantedBy=multi-user.target
```

nginx 反代（SSE 必须关缓冲，WS 必须带 Upgrade）：

```nginx
server {
    listen 443 ssl http2;
    server_name sync.example.com;

    ssl_certificate     /etc/letsencrypt/live/sync.example.com/fullchain.pem;
    ssl_certificate_key /etc/letsencrypt/live/sync.example.com/privkey.pem;

    location /sync/ {
        proxy_pass http://127.0.0.1:8787/;
        proxy_http_version 1.1;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection "upgrade";
        proxy_set_header Host $host;
        proxy_read_timeout 3600s;
        proxy_send_timeout 3600s;
        proxy_buffering off;              # SSE 必须
        chunked_transfer_encoding off;
    }
}
```

自检：`curl https://sync.example.com/sync/health` 应返回 `{"ok":true,...}`。

### 8.2 电脑端代理（Windows）

```bash
cd miniapp-sync/agent
# 编辑 config.json：
#   server.baseUrl = https://sync.example.com/sync
#   server.token   = 与服务端一致
#   local.dbPath   = auto（自动定位 %APPDATA%\com.kaoyan.focus\kaoyan-focus.sqlite3）
#   local.statePath / local.logPath = 同步盘之外的目录（否则每次写状态都会触发网盘上传）
python sync_agent.py --once --dry-run     # 先体检：只扫描，不写远端也不写本地
python sync_agent.py                      # 常驻运行
```

开机自启（任务计划程序，登录时触发，无窗口）：

```bat
schtasks /create /tn "考研专注同步代理" /sc onlogon /rl highest ^
  /tr "\"C:\Path\To\pythonw.exe\" \"E:\...\miniapp-sync\agent\sync_agent.py\""
```

常用参数：`--once` 只跑一轮 / `--dry-run` 体检 / `--resync` 强制以本地为准全量覆盖服务端。

> **状态与日志不要放在同步盘里**（百度网盘 / OneDrive / Dropbox）。代理每轮都会更新
> `statePath`，放在同步盘会让网盘每次上传，一轮同步能拖到几十秒。默认已放到
> `%LOCALAPPDATA%\kaoyan-focus-sync\`。

### 8.2.1 网络异常定位（doctor.py）

连不上先别改代码，跑体检：

```bash
python agent/doctor.py                 # 用正式配置
python agent/doctor.py --config agent/config.test.json
```

它按 8 段逐层推进并各给结论：**配置解析 → 本地库 → DNS → TCP 443 → TLS 握手/证书 →
HTTPS 三路（直连 / 系统代理 / 自定义代理）→ 大报文读取 → SSE 长连接**。
每一段失败会明确标出是 `dns` / `tcp` / `tls` / `timeout` / `http` 哪一类，
并在结尾给出可执行建议。

### 8.2.2 代理模式下 TLS 被中断怎么办

国内网络环境下常见「curl 和浏览器都通，但代理进程报 TLS 中断」。传输层提供了三个开关
（都在 `config.json` 的 `server` 段）：

| 配置 | 默认 | 说明 |
| --- | --- | --- |
| `proxyMode` | `none` | `none` 直连忽略系统代理 / `auto` 跟随系统代理 / `custom` 用 `proxyUrl` |
| `proxyUrl` | `""` | 仅 `proxyMode=custom` 生效，如 `http://127.0.0.1:8297` |
| `forceIpv4` | `false` | 强制只用 IPv4（排除 AAAA 记录解析到不可达地址导致握手中断） |

`proxyMode` 默认 `none` 是刻意的：环境变量里存在 `http_proxy` 时，Node/Python 的
默认代理会把到自有域名的 TLS 也塞进代理，而该代理往往不支持你域名上的证书链，
表现就是「握手被中断」。指向 `127.0.0.1` / 内网地址的请求始终直连，不受此开关影响。

如果本机到公网域名的 TLS 确实被网络环境阻断（例如公司网络做了 SNI 过滤），用 SSH 隧道兜底：

```bat
agent\tunnel-ssh.cmd   REM 把服务器的 8787 端口映射到本机 127.0.0.1:8787，断线每 10s 自动重连
```

然后把 `server.baseUrl` 指向 `http://127.0.0.1:8787/sync`（服务端需监听 `127.0.0.1`）。
也可以不改主配置，改用 `fallback` 块：`enabled=true` + `afterFailures=3`，
连续失败 3 次后自动切到本地隧道地址，`probeIntervalMs` 之后回探主地址。

### 8.3 小程序端

1. 微信开发者工具 → 导入项目 → 目录选 `miniapp-sync/miniapp`，填你的 AppID。
2. 编辑 `miniapp/config.js`：`wsUrl` 改成 `wss://sync.example.com/sync/ws`，`token` 与服务端一致。
3. 公众平台后台 → 开发管理 → 服务器域名 → **socket 合法域名** 添加 `sync.example.com`。
4. 真机调试/预览。开发者工具里可勾选「不校验合法域名」先跑通，上线前必须配好域名。

### 8.3.1 真机连不上（模拟器正常）—— 只看到「连接中」

**先看小程序自己报的错，再决定往哪边查。** 两种 errMsg 对应两条完全不同的根因：

| errMsg | 根因方向 | 去哪查 |
|---|---|---|
| `url not in domain list` / 提到域名 | socket 合法域名白名单 | 见「情形 A」 |
| `TLS handshake failed` | TLS 握手被掐断（白名单已通过） | 见「情形 B」 |

这正是「我的 → 连接诊断」卡片存在的意义：**没有它你只能看到一个转圈的「连接中」**。

#### 情形 A：`url not in domain list` —— 域名白名单

**这是最常见的坑，根因基本只有一个：域名白名单。**

| | 开发者工具 | 真机（预览 / 体验版 / 正式版） |
|---|---|---|
| 域名校验 | **不校验**（`project.config.json` 的 `"urlCheck": false`、以及「详情 → 本地设置 → 不校验合法域名」都只作用于工具） | **强制校验** socket 合法域名，绕不过 |
| `ws://` 明文 | 允许 | 一律拒绝 |
| IP / localhost | 允许 | 非法，只接受域名 |

所以「模拟器能连、真机不能连」不是网络问题，也不是服务端问题，而是真机多了
一道开发者工具没有的校验。**`urlCheck: false` 对真机完全无效。**

#### 排查顺序

**第 0 步：用「打开调试」做一次性判定（最快，不改任何代码）**

手机端打开小程序 → 右上角「···」→ **开发调试** → 打开调试 → 小程序会自动重启。
如果是 **开发版/体验版**，开启调试后微信**会关闭域名校验**。

- 打开调试后能连上 → **确认就是域名白名单问题**，去第 3 步配置即可
- 打开调试后还是连不上 → 不是白名单问题，去第 2 步看小程序报的具体错误

> 这条只适合做判定，调试模式在正式版上不存在，不能当解决方案。
> 反过来说，**「真机调试」（remote debug）能连 ≠ 正式能用**，必须用不带调试的
> 预览/体验版复验一次。

**第 1 步：确认服务端本身没问题**（排除法，避免在服务端上浪费时间）

```bash
python tests/ws_handshake_probe.py
```

这个脚本用**与小程序完全相同**的 URL、`Origin: https://servicewechat.com` 和手机 UA
直连生产端点，把 TLS 版本、证书链、HTTP 状态行原样打印出来。看到
`HTTP/1.1 101 Switching Protocols` 就说明服务端、Caddy、证书、token 全部正常，
问题 100% 在小程序侧。

> 注意路径：小程序的 `wsUrl` 必须是 `wss://api.skyhold.cloud/sync/ws`。
> Caddy 会把 `/sync` 前缀剥掉再转发给 Node，根路径 `/ws` 是被**别的服务**占用的，
> 不要图省事把 `/sync` 去掉。

**第 2 步：看小程序自己报的错**（已内置，不用装调试插件）

连不上时首页会直接显示一行橙色提示，写明原因和该去改什么；
「我的」页 → **连接诊断**卡片会给出完整信息：

- 连接地址（token 已脱敏）：确认真的指向 `wss://.../sync/ws`
- 最近一次失败的**原始 errMsg**（例如 `connectSocket:fail url not in domain list`）
- 对应的处理建议

典型 errMsg 与含义：

| errMsg 关键字 | 含义 | 处理 |
|---|---|---|
| `not in domain list` / 域名 | 不在 socket 合法域名白名单 | 见第 3 步 |
| `域名协议头非法` | 白名单里填了 `wss://` 前缀或路径 | 只填纯域名 |
| 401 / `unauthorized` | token 不匹配 | 核对 `miniapp/config.js` 与服务端 |
| 证书 / `cert` / `tls` | 证书链不完整、自签名或 TLS < 1.2 | 换可信 CA 证书，确认链完整 |
| `connect-timeout` | 15 秒内没完成握手 | 优先查白名单，其次换 4G/5G 试网络 |

**第 3 步：配置 socket 合法域名**

路径：微信公众平台 → 开发管理 → 开发设置 → 服务器域名 → **socket 合法域名** → 修改。

填写格式非常严格，**填错会报「域名协议头非法」且不提示哪部分违规**：

```
✅ api.skyhold.cloud          ← 只填纯域名主体
❌ wss://api.skyhold.cloud    ← 不能带协议头
❌ api.skyhold.cloud:443      ← 默认端口不用写（非 443 才写 host:port）
❌ api.skyhold.cloud/sync/ws  ← 不能带路径
```

同时必须满足四个硬条件，缺一不可：

1. 域名**已完成 ICP 备案**（未备案的域名根本填不进去；个人主体的备案多数不被接受）
2. 协议必须 `wss://`（明文 `ws://` 在 DNS 解析前就被底层引擎拦掉）
3. TLS ≥ 1.2（本项目实测 TLS 1.3，满足）
4. 证书由可信 CA 签发、**证书链完整**、SAN 覆盖该域名（本项目实测 Let's Encrypt 单域名证书，链长 4 层，满足）

配置保存后约 5 分钟生效。**改完必须重新上传代码**（体验版/正式版才会用上新域名），
真机不会自动刷新白名单。

> 如果你用的是**测试号**：微信对测试号不校验安全域名，那就不是白名单问题，
> 请回到第 2 步看实际 errMsg。
> 另外 `api.skyhold.cloud` 这类 `.cloud` 后缀域名要能填进后台，前提是它已经完成
> ICP 备案——填不进去通常就是没备案或备案主体与小程序主体对不上。

#### 情形 B：`TLS handshake failed` —— 握手在路径上被掐断

白名单已经通过（否则不会走到 TLS）。**此时不要再去改域名配置**，按下面顺序查：

> ⚠️ **2026-09-26 最终定案（经浏览器判别实验确认）**：真机 `TLS handshake failed`
> 的根因是 **H2：微信内置 BoringSSL 对 Let's Encrypt 的 ECDSA 交叉签名链
> （leaf ← YE2 ← Root YE ← ISRG Root X1）校验失败，客户端主动 RST**。
> 判别方法：用手机**浏览器**（不是微信）打开 `https://api.skyhold.cloud/sync/health`
> ——能打开说明系统信任证书，问题锁定在微信自己的 TLS 栈（H2）；
> 打不开才是链路级拦截（H1，只能备案/换 IP/Cloudflare）。
> **修复：Caddy 站点块加 `tls { key_type rsa2048 }` 换 RSA 链**（leaf ← R10/R11 ←
> ISRG Root X1，链更短、根更普及），并清掉旧证书存储强制重签。
> 注意：「TLS 1.3 证书加密所以放行、1.2 明文证书被拦」的早期理论**已被数据推翻**
> ——手机 ClientHello 明确含 TLS 1.3（772）却 34/34 全部被 RST，而电脑宽带 1.3 放行。
> 备案（8.3.2）仍是小程序**正式发布**的硬门槛，但不是本次握手失败的直接原因。

**第 1 步：先确认服务端没问题（服务器回环测试）**

```bash
ssh <你的服务器> "echo | openssl s_client -connect 127.0.0.1:443 -servername api.skyhold.cloud -tls1_2 2>&1 | grep 'Cipher is'"
```

能输出 `Cipher is ECDHE-ECDSA-AES128-GCM-SHA256` 就说明 Caddy 的 TLS 1.2 正常。
再测 `-tls1_3` 作对照。两条都通 → **服务端没有任何问题，不用改配置**。

**第 2 步：打开 Caddy debug，看客户端到底发了什么、错在哪一环**

Caddy 默认日志级别**不记录 TLS 握手失败**——「日志里没有」不代表「没到达」，
必须临时开启 debug：

```bash
ssh <你的服务器> "sudo -n cp /etc/caddy/Caddyfile /etc/caddy/Caddyfile.bak-debug-\$(date +%Y%m%d-%H%M%S)"
ssh <你的服务器> "printf '{\n\tdebug\n}\n\n' | sudo -n tee /tmp/g.tmp >/dev/null; \
                  sudo -n sh -c 'cat /tmp/g.tmp /etc/caddy/Caddyfile > /tmp/n && sudo -n cp /tmp/n /etc/caddy/Caddyfile'"
ssh <你的服务器> "sudo -n caddy validate --config /etc/caddy/Caddyfile --adapter caddyfile && sudo -n systemctl reload caddy"
```

然后让手机点「我的 → 立即重连」，读日志：

```bash
bash scripts/caddy-tls-log.sh <ssh主机名> 15
```

看三个字段就能定位：

| 日志现象 | 含义 |
|---|---|
| 没有任何该客户端的记录 | ClientHello 根本没到服务器（本地网络/防火墙拦了） |
| `no certificate matching TLS ClientHello` 且 `ServerName` 为空 | 客户端没带 SNI，或 SNI 对不上 |
| `SupportedVersions:[771,770,769]` | 客户端最高只支持 **TLS 1.2** |
| `matched certificate in cache` 之后紧跟 `connection reset by peer` | **Caddy 已正常回包，包在路径上被中间设备 RST** —— 服务端无责 |

**第 3 步：区分「客户端能力」与「路径拦截」**

- `SupportedVersions` 含 772（TLS 1.3）却失败 → 客户端能力没问题，是路径在拦；
- 只有 771/770/769（最高 TLS 1.2）→ 该客户端只能走 TLS 1.2，若路径拦 1.2 则必然失败。

实测结论（2026-09-26，本机到 `api.skyhold.cloud`）：

- 服务器回环 TLS 1.2 / 1.3 都正常；
- 本机到 **同 IP 全部子域名** 的 TLS 1.2 稳定失败（读到 0 字节 EOF），重复 3 次一致；
- 本机到 baidu.com / cloudflare.com 的 TLS 1.2 正常（cloudflare 同为 ECDSA 证书）；
- Caddy 日志显示 ClientHello 到达、证书匹配成功、**回包被 `connection reset by peer`**。

→ 结论：**宽带出口到这台服务器 IP 的 TLS 1.2 握手被中间设备 RST**。
TLS 1.3 的证书是加密的、DPI 看不到内容所以放行；TLS 1.2 的证书是明文，被读后注 RST。
电脑端不受影响（Python/Chromium 都协商 TLS 1.3），手机 WeChat 的 WSS 若只协商到
TLS 1.2 就必然失败。

**第 4 步：怎么办**

1. **判别实验（30 秒，一锤定音）**：手机**浏览器**（Safari/Chrome，不是微信）打开
   `https://api.skyhold.cloud/sync/health`：
   - **能打开**（看到 ok）→ 系统信任证书，是微信内置 TLS 栈不认这条 ECDSA 交叉签名链
     → **换 RSA 证书**（见下方第 5 步，5 分钟修好）；
   - 打不开 → 链路级拦截未备案域名 → 只能：备案 / 换服务器 IP / Cloudflare 橙云代理。
2. 手机切 4G/5G 再连（20 秒验证）；检查路由器的「安全防护/广告过滤/家长控制」类功能。
3. 长期方案：把域名套 Cloudflare（橙云），TLS 那一跳改在 CF 边缘终结，支持 WebSocket 代理。
4. 排查完成后**记得移除 Caddyfile 里的 `debug`**（日志量很大）：
   `sudo -n sed -i '1,3{/^{$/,/^}$/d}' /etc/caddy/Caddyfile && sudo -n systemctl reload caddy`
   （或直接从备份恢复：`sudo -n cp /etc/caddy/Caddyfile.bak-debug-* /etc/caddy/Caddyfile` 后 reload。）

**第 5 步：换 RSA 证书（H2 确认后的修法，2026-09-26 已在本项目执行）**

```bash
ssh <你的服务器> "sudo -n cp /etc/caddy/Caddyfile /etc/caddy/Caddyfile.bak-rsa-\$(date +%Y%m%d-%H%M%S)"
# 1) 在目标站点块内加 tls 配置（Caddy 2.6.2 支持）：
ssh <你的服务器> "sudo -n sed -i '/^api\\.skyhold\\.cloud {\$/a\\\\ttls {\n\t\tkey_type rsa2048\n\t}' /etc/caddy/Caddyfile"
ssh <你的服务器> "sudo -n caddy validate --config /etc/caddy/Caddyfile --adapter caddyfile"
# 2) 移走旧 ECDSA 证书存储，强制 Caddy 重签（否则会继续用缓存里的旧证书）：
ssh <你的服务器> "sudo -n mv /var/lib/caddy/.local/share/caddy/certificates/<LE目录>/<域名> \
  /var/lib/caddy/.local/share/caddy/certificates/<LE目录>/<域名>.ecdsa.bak-<日期>"
ssh <你的服务器> "sudo -n systemctl reload caddy"
# 3) 验证：公钥算法应为 rsaEncryption，链长 3（leaf ← R10/R11 ← ISRG Root X1）
ssh <你的服务器> "echo | openssl s_client -connect 127.0.0.1:443 -servername api.skyhold.cloud 2>/dev/null | openssl x509 -noout -text | grep 'Public Key Algorithm'"
```

换完后本机复核：TLS 正常、`/sync/health` 返回 ok 即可让真机重试。
> 注意：服务器上其它同 IP 子域（dav6/imgbed/prices）仍是 ECDSA——若它们也要在
> 微信生态内访问，需逐个站点块加同样的 `tls { key_type rsa2048 }`。

> 顺带说明：`python tests/cert_chain_audit.py --ssllabs` 可以从第三方视角看证书链与
> 协议支持，但 **SSL Labs 等境外探测会被阿里云安全组拦下**（报
> `Failed to communicate with the secure server`），这项结果不能当作服务端故障的证据。

### 8.3.2 域名备案（境内服务器的硬门槛，也是 TLS 被重置的根因）

**只要服务器在中国大陆，域名就必须 ICP 备案。** 这不只是「微信后台填不进去」的问题，
而是会直接导致本方案第 8.3.1 情形 B 的 `connection reset`：

| 现象 | 机制 |
|---|---|
| 同一 IP 的所有子域名都可能失败 | 备案按域名（含子域）逐一登记，同 IP 其它未备案子域一样可能被拦 |
| 服务器本机/回环测试正常 | 拦截发生在运营商/云厂商的网络边界，不在本机 |
| SSL Labs 等境外探测连不上 | 另有境外访问限制，此项不能当证据 |

> ⚠️ **机制修正（2026-09-26）**：早期记录的「TLS 1.3 证书加密所以放行、
> TLS 1.2 明文证书被读取核对备案号」的理论**已被数据推翻**（手机端协商 TLS 1.3
> 也 100% 被 RST，且手机浏览器能正常访问）。真机 `TLS handshake failed` 的实锤
> 根因是**微信内置 BoringSSL 不认 ECDSA 交叉签名链**（见 8.3.1 情形 B 第 5 步），
> 与备案无关。备案拦截的准确机制随运营商/地区而异，未备案域名在**部分网络路径**
> 上确实会被阻断（本机宽带出口的 TLS 1.2 被重置即为实例），但不要再用「明文证书」
> 理论去解释握手失败——先用 8.3.1 第 4 步的浏览器判别实验分流。

阿里云官方对这一症状的说明：*"配置完 HTTPS 后访问出现连接重置，域名没有备案……
未备案的域名直接通过 HTTPS 访问时，会被网络服务商阻断，表现为连接重置。"*

#### 备案前置条件（缺一不可）

1. 服务器在中国大陆（本项目：阿里云杭州 ✓）
2. 域名后缀在工信部批准列表内 —— **`.cloud` 已于 2018-10 获批，可以备案**
3. **域名在国内注册商注册**（阿里云/腾讯云等）。若在 Cloudflare/Namecheap 等境外注册商，
   需先**转入国内注册商**，否则实名核验过不了
4. 备案主体与小程序主体一致：个人小程序用**个人备案**

#### 两条路径怎么选

| | 路径一：备案现有 `skyhold.cloud` | 路径二：注册新域名（`.com`/`.cn`）再备案 |
|---|---|---|
| 个人主体是否支持 | `.cloud` 有服务商称仅限企业/单位备案，**需先向阿里云备案客服确认** | `.com`/`.cn` 个人备案 100% 支持，零风险 |
| 备案期间是否断服 | **会断**（备案期间域名需停止对外访问，1–3 周） | **不断**（新域名不解析即可，现有服务照常跑） |
| 成本 | 免费（域名若需转入注册商则有续费费） | 域名约 ¥30–80/年 |

**推荐路径二**：注册一个便宜的新域名专供小程序，先备案它；备下来后
Caddy 加一个站点块 → 改 `miniapp/config.js` 的 `wsUrl`/`restUrl` →
微信后台配置 socket 合法域名。现有 `api.skyhold.cloud` 电脑端代理可继续用，
之后想统一再「新增备案」挂到同一个主体下（已有主体后新增会快很多）。

#### 备案完成后的配置顺序

1. 域名 ICP 备案通过（拿到 `X ICP备 XXXXXXXX 号`）
2. **小程序备案**：mp.weixin.qq.com → 设置 → 基本设置 → 小程序备案
   （2023-09 起新小程序必须备案，它要求服务器域名已 ICP 备案）
3. 微信后台 → 开发管理 → 服务器域名 → **socket 合法域名** 填纯域名
4. 改 `miniapp/config.js` 的 `wsUrl` / `restUrl`，重新上传体验版
5. **主域名备案不覆盖子域名**：`example.com` 备了，`api.example.com` 仍需单独登记

#### 备案之前能做什么

**先分清两道彼此独立的关卡**——「没备案的域名可以开发调试」这句话只对第一道成立：

| 关卡 | 谁在拦 | 「打开调试」能否绕过 |
|---|---|---|
| ① 微信客户端的域名校验 | 微信 App | **能**。开发版/体验版开启调试后不校验 socket 合法域名（这正是未备案域名也能在开发阶段连上的原因） |
| ② 网络层的备案拦截 | 运营商 / 云厂商 | **不能**。与微信无关，看的是 TLS 1.2 明文证书里的域名，调试开关管不着 |

**怎么判断卡在哪一关**：看小程序报的 errMsg。

- `url not in domain list` → 卡在①，开调试即可绕过；
- `TLS handshake failed` → **①已经过了**，卡在②，调试无能为力。先用 8.3.1 情形 B
  第 4 步的浏览器判别实验分流：能开浏览器 → 换 RSA 证书即可修（本项目 2026-09-26
  已这样做并生效）；打不开 → 只能备案或套 Cloudflare。

其它临时办法：

- 电脑端代理不受影响（Python 协商 TLS 1.3），可照常用。
- 手机换 4G/5G 试试——若那家运营商不执行 TLS 1.2 备案拦截就能用，但不可靠、非长久之计。
- 不想自己备案的替代架构：把同步服务迁到**微信云托管**（腾讯云的默认域名已备案，
  小程序 `wx.callContainer` 免白名单；WebSocket 需单独确认其支持情况），属于较大的架构改动。

#### 附带说明：关于「小游戏/小程序」的两点细节

- 预览二维码时，预览弹窗里也有一个「不校验合法域名」勾选项，勾上可以让**这一次预览**
  跳过校验，用来先验证代码逻辑。但它只对当次预览有效，不能替代白名单配置，更不能上线。
- 真机调试（remote debug）模式下工具会把「不校验合法域名」透传给手机，
  所以**真机调试能连不代表正式能用**，一定要用不带调试的预览/体验版复验一次。

#### 代码侧已做的兜底

即使白名单没配好，也不会再出现「永远连接中、什么都不知道」的情况：

1. **握手看门狗**（`timing.connectWatchdogMs`，默认 15s）：超时未 `onOpen` 就判定失败、
   记录原因并重连，把「无限连接中」变成一次可诊断的失败。
2. **原始 errMsg 全程留痕**：`fail`、`onError`、`send` 失败、看门狗超时四条路径都会记下
   原始报错并翻译成人话，`sync.lastErrorInfo()` 可读。
3. **配置自检**：公网地址若是明文 `ws://` 或裸 IP，直接报配置错误，不空转重连；
   本机/内网地址（`ws://127.0.0.1`、`192.168.x.x`）放行，不影响本地联调与冒烟测试。
4. **过期回调隔离**：被替换掉的旧 socket，其迟到的 `close`/`error` 不会改乱新连接状态。

对应回归测试（本地、不需要服务端）：

```bash
node tests/mini-connect-diagnostic-test.js
```

### 8.4 冒烟测试

一条命令跑通三端（推荐）：

```bash
bash scripts/e2e-smoke.sh            # 默认端口 8788
bash scripts/e2e-smoke.sh 8899       # 指定端口
```

脚本自动完成：真机连接诊断逻辑测试（纯本地）→ 生成真实数据库的**事务一致性副本**
→ 起测试服务端（独立数据目录，不动线上）→ PC 代理全量推送 → 打印服务端各实体存活/墓碑统计
→ 跑小程序逻辑测试 → 收工清理。

> 副本用 SQLite 的在线备份接口生成，**不是 `cp`**。桌面端 24 小时在跑，`-wal`/`-shm`
> 被它独占，`cp` 会失败；而且只拷主文件会丢掉还在 WAL 里未 checkpoint 的最近提交。

想手动分步跑也可以：

```bash
# 0) 先用 SQLite 在线备份做一致性副本（别放工程目录——同步盘会让每次写入都拖慢几十秒）
mkdir -p "%TEMP%\kaoyan-sync-test"
python -c "import sqlite3,os; s=sqlite3.connect('file:///'+os.environ['APPDATA'].replace('\\','/')+'/com.kaoyan.focus/kaoyan-focus.sqlite3?mode=ro',uri=True); d=sqlite3.connect(os.environ['TEMP']+'/kaoyan-sync-test/test.sqlite3'); d.__enter__(); s.backup(d)"
# 1) 起服务端（REST 在根路径，WS 在任意以 /ws 结尾的路径）
cd server && SYNC_TOKEN=test-token-local SYNC_PORT=8788 node index.js
# 2) 用数据库副本跑 PC 代理（不碰真实数据）
python agent/sync_agent.py --config agent/config.test.json --once --resync
# 3) 跑小程序逻辑测试
node tests/mini-smoke-test.js --url ws://127.0.0.1:8788/ws \
     --rest http://127.0.0.1:8788 --token test-token-local --page-size 300
```

已实测通过的断言：

- **真机连接诊断**（21 项）：失败留痕、看门狗兜底、配置自检、过期回调隔离、本地地址放行
- **小程序端逻辑**（7 项）：WebSocket 握手、分页全量快照完整落地、手机端乐观更新、
  手机端写入被服务端接受、两端摘要一致、越权写入被拒、PC 权威实体拒绝手机端写入

### 8.4.1 生产端点连通性自检

```bash
python tests/ws_handshake_probe.py
```

模拟小程序真机的握手请求直连生产 WSS，打印 TLS 版本、证书链与 HTTP 状态行。
「真机连不上」时**先跑这个**，用来区分「服务端问题」和「小程序白名单问题」。

### 8.5 日常维护命令

```bash
# 网络体检（连不上先跑这个，别急着改代码）
python agent/doctor.py

# 只扫描不写：确认配置和实体数量是否正常
python agent/sync_agent.py --once --dry-run

# 列出服务端上「本地 sync_meta 里已经不认」的孤儿实体（只预览，不动数据）
python agent/sync_agent.py --prune-orphans
python agent/sync_agent.py --prune-orphans --type subject    # 只看某一类

# 确认无误后清理（推送墓碑删除）
python agent/sync_agent.py --prune-orphans --yes
```

> **孤儿实体怎么来的？** 桌面端 `storage/db.rs` 的 `normalize_default_subjects()` 会把
> 同名重复学科停用（`enabled=0`）并**删掉它们的 `sync_meta` 行**。如果代理仍然给这些行
> 发 sync_id，就会陷入「发号 → 桌面端删号 → 下轮再发一个新号」的死循环，每一轮都在
> 服务端堆一个重复学科。代理已加 `mint_guard`（停用且无 sync_id 的行不发号）堵住源头，
> `--prune-orphans` 用来清理历史上已经堆下的。

---

## 9. 已知限制与后续演进

- **不能从手机启停学习模式**：学习模式状态机由桌面端 Rust 后台 tick 推进，代理直接改 SQLite 会破坏状态一致性。v1 只做只读展示；演进路径是在 Tauri 里加一个命令轮询接口，由代理转发控制指令。
- **`app_event` 不参与摘要校验**：它按 3 天窗口同步，服务端会留存更早的记录，纳入摘要会永远对不上。
- **桌面端页面不会瞬时刷新**：代理写回 SQLite 后，桌面端需要切换页面或等其自身轮询才显示最新值。
- **单 PC 假设**：当前按「一台电脑 + 多部手机」设计。若将来要多台电脑，需要把 `study_mode` 的主设备控制逻辑升级为跨设备锁（桌面端已有 `active_device_id` 字段可复用）。
- **凭据不进同步通道**：飞书 / WebDAV / SMTP 配置必须在每台设备上各自配置，这是刻意的安全边界。
