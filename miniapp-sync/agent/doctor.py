#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""同步链路体检工具。

一条命令查清「连不上同步服务」到底卡在哪一层：
    配置 → 本地库 → DNS → TCP → TLS → HTTPS → 大响应 → SSE → 代理路径对比

用法：
    python doctor.py                          # 用同目录 config.json
    python doctor.py --config config.test.json
    python doctor.py --db "C:/path/to/kaoyan-focus.sqlite3"

只做只读探测，不写服务端、不写本地库，可随时安全运行。
"""

import argparse
import json
import os
import socket
import ssl
import sys
import time
import urllib.error
import urllib.parse
import urllib.request

BASE_DIR = os.path.dirname(os.path.abspath(__file__))
PASS = "PASS"
WARN = "WARN"
FAIL = "FAIL"

results = []


def report(level, title, detail=""):
    results.append(level)
    icon = {PASS: "✓", WARN: "!", FAIL: "✗"}[level]
    print(" %s [%s] %s%s" % (icon, level, title, (" — " + detail) if detail else ""))


def section(title):
    print("\n" + "─" * 62)
    print(" " + title)
    print("─" * 62)


# ------------------------------------------------------------------ 各项检查

def check_config(config):
    section("1. 配置检查")
    server = config.get("server") or {}
    base_url = str(server.get("baseUrl", ""))
    token = str(server.get("token", ""))

    if not base_url:
        report(FAIL, "server.baseUrl 未配置")
    elif base_url.startswith("https://"):
        report(PASS, "baseUrl 使用 HTTPS", base_url)
    elif base_url.startswith("http://127.0.0.1") or base_url.startswith("http://localhost"):
        report(PASS, "baseUrl 指向本机（SSH 隧道模式）", base_url)
    elif base_url.startswith("http://"):
        report(WARN, "baseUrl 是明文 HTTP", "公网使用请改 HTTPS，否则 token 会裸奔")
    else:
        report(FAIL, "baseUrl 协议不合法", base_url)

    if not token or "CHANGE-ME" in token:
        report(FAIL, "token 还是占位符", "三端必须使用同一个 token")
    elif len(token) < 24:
        report(WARN, "token 偏短", "建议 32 位以上随机串")
    else:
        report(PASS, "token 已配置", "%d 位" % len(token))

    report(PASS, "代理模式", "proxyMode=%s%s" % (
        server.get("proxyMode", "none"),
        (" proxyUrl=" + str(server.get("proxyUrl"))) if server.get("proxyUrl") else ""))
    if server.get("forceIpv4"):
        report(PASS, "已强制 IPv4 解析")
    fallback = server.get("fallback") or {}
    if fallback.get("enabled"):
        report(PASS, "备用线路已启用", fallback.get("baseUrl", ""))
    else:
        report(WARN, "备用线路未启用", "弱网环境建议配置 SSH 隧道端点作为 fallback")
    return base_url, token


def check_local_db(db_path):
    section("2. 本地数据库")
    if not os.path.exists(db_path):
        report(FAIL, "数据库不存在", db_path)
        return
    size_mb = os.path.getsize(db_path) / 1024.0 / 1024.0
    age_min = (time.time() - os.path.getmtime(db_path)) / 60.0
    report(PASS, "数据库存在", "%s（%.1f MB，%.0f 分钟前修改）" % (db_path, size_mb, age_min))

    try:
        import sqlite3
        conn = sqlite3.connect("file:%s?mode=ro" % db_path.replace("\\", "/"), uri=True, timeout=5)
        conn.execute("PRAGMA busy_timeout = 5000")
        sync_meta = conn.execute("SELECT count(*) FROM sync_meta").fetchone()[0]
        report(PASS, "sync_meta 已分配 sync_id", "%d 条" % sync_meta)
        for table in ("checklist_tasks", "today_plan_items", "focus_sessions", "study_modes", "subjects"):
            try:
                count = conn.execute("SELECT count(*) FROM %s" % table).fetchone()[0]
                print("      %-20s %d" % (table, count))
            except sqlite3.Error:
                pass
        conn.close()
    except Exception as error:
        report(WARN, "读取数据库统计失败", str(error))


def check_dns(host):
    section("3. DNS 解析")
    if not host:
        report(FAIL, "无法从 baseUrl 解析出主机名")
        return []
    try:
        infos = socket.getaddrinfo(host, 443, proto=socket.IPPROTO_TCP)
    except socket.gaierror as error:
        report(FAIL, "解析失败", str(error))
        return []

    addresses = sorted({info[4][0] for info in infos})
    v4 = [a for a in addresses if ":" not in a]
    v6 = [a for a in addresses if ":" in a]
    report(PASS, "解析成功", "%s → %s" % (host, ", ".join(addresses)))
    if v6 and not v4:
        report(WARN, "只有 IPv6 地址", "IPv6 路由不通时会导致握手卡死，建议开启 forceIpv4")
    return addresses


def check_tcp(host, addresses):
    section("4. TCP 连接（443）")
    if not host:
        return
    for family, label in ((socket.AF_INET, "IPv4"), (socket.AF_INET6, "IPv6")):
        targets = [a for a in addresses if (":" in a) == (family == socket.AF_INET6)]
        if not targets:
            print("      %s: 无地址，跳过" % label)
            continue
        address = targets[0]
        started = time.time()
        try:
            sock = socket.socket(family, socket.SOCK_STREAM)
            sock.settimeout(8)
            sock.connect((address, 443))
            elapsed = time.time() - started
            try:
                mss = sock.getsockopt(socket.IPPROTO_TCP, socket.TCP_MAXSEG)
            except OSError:
                mss = -1
            sock.close()
            report(PASS, "%s 连接成功" % label, "%s 用时 %.2fs，MSS=%s" % (address, elapsed, mss))
        except Exception as error:
            report(FAIL, "%s 连接失败" % label, "%s: %s" % (type(error).__name__, error))


def check_tls(host):
    section("5. TLS 握手与证书")
    if not host:
        return
    context = ssl.create_default_context()
    try:
        context.set_alpn_protocols(["h2", "http/1.1"])
    except NotImplementedError:
        pass

    started = time.time()
    try:
        with socket.create_connection((host, 443), timeout=10) as raw:
            with context.wrap_socket(raw, server_hostname=host) as tls:
                elapsed = time.time() - started
                report(PASS, "TLS 握手成功", "%s / %s，用时 %.2fs" % (tls.version(), tls.cipher()[0], elapsed))
                try:
                    selected = tls.selected_alpn_protocol()
                    if selected:
                        print("      ALPN: %s" % selected)
                except Exception:
                    pass
                cert = tls.getpeercert()
                if cert:
                    subject = dict(x[0] for x in cert.get("subject", []))
                    issuer = dict(x[0] for x in cert.get("issuer", []))
                    print("      证书主体: %s" % subject.get("commonName", "?"))
                    print("      签发机构: %s" % issuer.get("organizationName", issuer.get("commonName", "?")))
                    not_after = cert.get("notAfter")
                    if not_after:
                        try:
                            expires = time.mktime(time.strptime(not_after, "%b %d %H:%M:%S %Y %Z"))
                            days = (expires - time.time()) / 86400.0
                            level = FAIL if days < 0 else (WARN if days < 14 else PASS)
                            report(level, "证书有效期", "%s（剩 %.0f 天）" % (not_after, days))
                        except ValueError:
                            report(PASS, "证书有效期", not_after)
    except ssl.SSLCertVerificationError as error:
        report(FAIL, "证书校验失败", str(error))
    except Exception as error:
        report(FAIL, "TLS 握手失败", "%s: %s" % (type(error).__name__, error))
        print("      提示：TCP 通但握手失败，通常是中间设备/代理拦截，或 MTU 黑洞")
        print("           可尝试 proxyMode=none 直连、开启 forceIpv4、或改用 SSH 隧道")


def _open(url, token, opener, timeout=15):
    request = urllib.request.Request(url, headers={"Authorization": "Bearer %s" % token})
    with opener.open(request, timeout=timeout) as response:
        return response.read()


def check_https(base_url, token):
    section("6. HTTPS 接口（三条路径对比）")
    if not base_url:
        report(FAIL, "baseUrl 缺失，跳过")
        return None

    paths = [
        ("直连（忽略所有代理）", urllib.request.build_opener(urllib.request.ProxyHandler({}))),
        ("跟随环境代理", urllib.request.build_opener()),
    ]
    env_proxy = os.environ.get("https_proxy") or os.environ.get("HTTPS_PROXY")
    if env_proxy:
        print("      检测到环境代理: %s" % env_proxy)

    direct_ok = proxy_ok = None
    for label, opener in paths:
        started = time.time()
        try:
            body = _open(base_url + "/health", token, opener)
            payload = json.loads(body)
            elapsed = time.time() - started
            report(PASS, "%s 正常" % label,
                   "%.2fs serverRev=%s entities=%s" % (elapsed, payload.get("serverRev"), payload.get("entities")))
            if label.startswith("直连"):
                direct_ok = True
            else:
                proxy_ok = True
        except Exception as error:
            detail = str(getattr(error, "reason", error))
            report(FAIL, "%s 失败" % label, "%s: %s" % (type(error).__name__, detail))
            if label.startswith("直连"):
                direct_ok = False
            else:
                proxy_ok = False

    return {"direct": direct_ok, "proxy": proxy_ok, "envProxy": env_proxy}


def check_payload(base_url, token):
    section("7. 大响应读取（弱网最容易在这里断）")
    if not base_url:
        return
    opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))
    for label, path in (("增量首批", "/delta?cursor=0&limit=200"), ("快照首页", "/snapshot?limit=200")):
        started = time.time()
        try:
            body = _open(base_url + path, token, opener, timeout=20)
            report(PASS, "%s 读取成功" % label, "%.1f KB / %.2fs" % (len(body) / 1024.0, time.time() - started))
        except Exception as error:
            report(FAIL, "%s 读取失败" % label, "%s: %s" % (type(error).__name__, error))


def check_sse(base_url, token, device_id="pc-doctor"):
    section("8. SSE 实时下行")
    if not base_url:
        return
    opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))
    params = urllib.parse.urlencode({"deviceId": device_id, "role": "pc"})
    request = urllib.request.Request(
        base_url + "/stream?" + params,
        headers={"Authorization": "Bearer %s" % token, "Accept": "text/event-stream"},
        method="GET",
    )
    started = time.time()
    try:
        response = opener.open(request, timeout=8)
        event = None
        for raw in response:
            line = raw.decode("utf-8", "replace").strip()
            if line.startswith("event:"):
                event = line[6:].strip()
            if event == "connected":
                break
        response.close()
        if event == "connected":
            report(PASS, "SSE 连接建立", "用时 %.2fs" % (time.time() - started))
        else:
            report(WARN, "SSE 已连接但没有收到 connected 事件")
    except Exception as error:
        report(FAIL, "SSE 连接失败", "%s: %s" % (type(error).__name__, error))


# ------------------------------------------------------------------ 结论

def summarize(https_result, base_url):
    section("结论与建议")
    failures = results.count(FAIL)
    warns = results.count(WARN)

    if https_result:
        direct, proxy = https_result["direct"], https_result["proxy"]
        if direct and proxy:
            print("  • 直连与环境代理都能通。建议保持 proxyMode=none（少一个故障点）。")
        elif direct and not proxy:
            print("  • 直连正常，但环境代理路径失败 —— 代理正在破坏到该域名的 TLS 连接。")
            print("    配置里保持 proxyMode=none 即可，并注意：不要把代理写进 proxyUrl。")
        elif not direct and proxy:
            print("  • 直连失败但代理可用。把 proxyMode 设为 custom，proxyUrl 填环境代理地址。")
            print("    当前环境代理: %s" % https_result.get("envProxy"))
        else:
            print("  • 两条路径都不通。按顺序尝试：")
            print("    1) forceIpv4=true（IPv6 路由黑洞很常见）")
            print("    2) 改用 SSH 隧道：ssh -N -L 8787:127.0.0.1:8787 root@<服务器>")
            print("       然后把 baseUrl 指向 http://127.0.0.1:8787，或启用 server.fallback")
            print("    3) 若在校园网/公司网，尝试换网络或用手机热点验证")

    if failures == 0:
        print("\n  总体：链路健康，同步代理可以正常工作。")
    else:
        print("\n  总体：%d 项失败，%d 项警告 —— 先修上面的 FAIL 项再启动代理。" % (failures, warns))
    print()
    return failures


def main():
    parser = argparse.ArgumentParser(description="同步链路体检")
    parser.add_argument("--config", default=os.path.join(BASE_DIR, "config.json"))
    parser.add_argument("--db", default=None, help="覆盖本地数据库路径")
    args = parser.parse_args()

    print("\n考研专注 · 同步链路体检")
    print("配置文件: %s" % args.config)

    try:
        with open(args.config, "r", encoding="utf-8") as handle:
            config = json.load(handle)
    except Exception as error:
        print("无法读取配置: %s" % error)
        return 2

    base_url, token = check_config(config)

    db_path = args.db or (config.get("local") or {}).get("dbPath", "auto")
    if db_path == "auto":
        appdata = os.environ.get("APPDATA") or os.path.expanduser("~")
        db_path = os.path.join(appdata, "com.kaoyan.focus", "kaoyan-focus.sqlite3")
    check_local_db(db_path)

    host = urllib.parse.urlparse(base_url).hostname if base_url else None
    addresses = check_dns(host)
    check_tcp(host, addresses)
    check_tls(host)
    https_result = check_https(base_url, token)
    check_payload(base_url, token)
    check_sse(base_url, token, config.get("deviceId", "pc-doctor"))

    failures = summarize(https_result, base_url)
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
