# -*- coding: utf-8 -*-
"""
真机连接的旁证工具：模拟微信小程序 wx.connectSocket 发出的握手请求，
直连生产 WSS 端点，把 HTTP 状态行/响应头原样打出来。

小程序真机上「一直连接中」只有三种物理可能：
  1. 域名没进 socket 合法域名白名单 —— 握手请求根本发不出去（本脚本测不出来，但能排除服务端）
  2. TCP/TLS 建不起来 —— 本脚本会报 errno
  3. WS 升级被拒（401 / 404 / 非 101）—— 本脚本会打出状态行

用法：
  python tests/ws_handshake_probe.py
"""
import base64
import os
import socket
import ssl
import sys

HOST = os.environ.get("PROBE_HOST", "api.skyhold.cloud")
TOKEN = os.environ.get("PROBE_TOKEN", "7db26416525c0ba6d188653140b5500221707a8dee229d0db60e7733837c39d1")
PATHS = [
    "/sync/ws",   # 小程序 config.js 里的 wsUrl 路径
    "/ws",        # Caddy 剥掉 /sync 之后 Node 看到的路径
]


def probe(path, host=HOST, port=443, timeout=15):
    url = "wss://%s%s?token=%s&deviceId=probe&role=mini" % (host, path, TOKEN)
    print("=" * 70)
    print("[probe] %s" % url)

    key = base64.b64encode(os.urandom(16)).decode()

    # 微信小程序真机的 UA 大致长这样（用于确认服务端不按 UA 过滤）
    request = (
        "GET %s?token=%s&deviceId=probe&role=mini HTTP/1.1\r\n"
        "Host: %s\r\n"
        "Upgrade: websocket\r\n"
        "Connection: Upgrade\r\n"
        "Sec-WebSocket-Key: %s\r\n"
        "Sec-WebSocket-Version: 13\r\n"
        "Origin: https://servicewechat.com\r\n"
        "User-Agent: Mozilla/5.0 (iPhone; CPU iPhone OS 16_0 like Mac OS X) "
        "AppleWebKit/605.1.15 MicroMessenger/8.0.40 miniProgram\r\n"
        "\r\n"
    ) % (path, TOKEN, host, key)

    raw = None
    try:
        ctx = ssl.create_default_context()
        with socket.create_connection((host, port), timeout=timeout) as sock:
            with ctx.wrap_socket(sock, server_hostname=host) as tls:
                print("[ok] TLS 建立成功: %s / %s" % (tls.version(), tls.cipher()[0]))
                cert = tls.getpeercert()
                print("[ok] 证书 subject: %s" % (cert.get("subject")))
                print("[ok] 证书 SAN: %s" % ([v for k, v in cert.get("subjectAltName", [])],))
                if cert.get("notAfter"):
                    print("[ok] 证书有效期至: %s" % cert["notAfter"])
                tls.sendall(request.encode())
                tls.settimeout(timeout)
                raw = tls.recv(4096)
    except socket.gaierror as error:
        print("[FAIL] DNS 解析失败: %s" % error)
        return False
    except ssl.SSLError as error:
        print("[FAIL] TLS 握手失败: %s" % error)
        return False
    except (socket.timeout, TimeoutError):
        print("[FAIL] 超时：TCP/TLS 或首字节迟迟不到（真机表现就是『一直连接中』）")
        return False
    except OSError as error:
        print("[FAIL] 网络错误: %s" % error)
        return False

    if not raw:
        print("[FAIL] 连接被对端直接关闭，未返回任何字节")
        return False

    text = raw.decode("latin-1")
    status_line = text.split("\r\n")[0]
    print("[resp] %s" % status_line)
    for line in text.split("\r\n")[1:]:
        if not line.strip():
            break
        print("       %s" % line)

    ok = " 101 " in status_line
    print("[%s] %s" % ("PASS" if ok else "FAIL", "升级成功" if ok else "未完成 WS 升级"))
    return ok


if __name__ == "__main__":
    results = [probe(p) for p in PATHS]
    print("=" * 70)
    print("结论: %s" % ("服务端 WS 握手正常，问题在小程序白名单/客户端" if any(results) else "服务端侧就有问题"))
    sys.exit(0 if any(results) else 1)
