"""与公网同步服务端的通信层。

只用标准库：REST 走 urllib（带 Authorization 头），实时下行走 SSE（urllib 流式读）。
SSE 连接断开后由这里负责指数退避重连，主循环不需要关心网络细节。

针对「弱网 / 校园网 / 代理干扰」做了三件事：
  1. proxyMode 显式控制走不走代理。默认 none —— 自家服务端直连最稳，
     系统里常见的 http_proxy 环境变量（Clash、公司代理、沙箱出口代理）
     会拦截并破坏到自建域名的 TLS 连接，属于高频故障源。
  2. 所有请求自动重试（指数退避），并把异常分类成 dns / tcp / tls / http / timeout，
     让上层能判断该重试、该换线路、还是该改配置。
  3. forceIpv4：部分网络 IPv6 有路由但黑洞，强制走 IPv4 可以避免握手卡死。
"""

import json
import socket
import ssl
import threading
import time
import urllib.error
import urllib.parse
import urllib.request


class TransportError(Exception):
    """带分类的传输异常，便于上层决策。"""

    def __init__(self, kind, message, retryable=True, status=None):
        super().__init__(message)
        self.kind = kind          # dns | tcp | tls | timeout | http | config | unknown
        self.message = message
        self.retryable = retryable
        self.status = status

    def __str__(self):
        return "[%s] %s" % (self.kind, self.message)


def _classify(error):
    """把底层异常归类，输出人类可读的诊断信息。"""
    reason = getattr(error, "reason", error)

    if isinstance(reason, socket.gaierror):
        return TransportError("dns", "域名解析失败: %s" % reason, retryable=True)
    if isinstance(reason, ssl.SSLCertVerificationError):
        return TransportError("tls", "证书校验失败: %s" % reason, retryable=False)
    if isinstance(reason, ssl.SSLError):
        return TransportError("tls", "TLS 握手/读写出错: %s" % reason, retryable=True)
    if isinstance(reason, ConnectionResetError):
        return TransportError("tcp", "连接被重置: %s" % reason, retryable=True)
    if isinstance(reason, ConnectionRefusedError):
        return TransportError("tcp", "连接被拒绝: %s" % reason, retryable=True)
    if isinstance(reason, socket.timeout) or isinstance(error, socket.timeout):
        return TransportError("timeout", "请求超时: %s" % reason, retryable=True)

    if isinstance(error, urllib.error.HTTPError):
        status = error.code
        retryable = status >= 500 or status == 429
        return TransportError("http", "服务端返回 HTTP %s" % status, retryable=retryable, status=status)

    return TransportError("unknown", "%s: %s" % (type(reason).__name__, reason), retryable=True)


class Transport:
    def __init__(self, server_config, logger, device_id, role="pc"):
        self.base_url = str(server_config.get("baseUrl", "")).rstrip("/")
        self.token = server_config.get("token", "")
        self.timeout = float(server_config.get("timeoutSeconds", 10))
        self.device_id = device_id
        self.role = role
        self.logger = logger

        self.proxy_mode = str(server_config.get("proxyMode", "none")).lower()
        self.proxy_url = server_config.get("proxyUrl", "") or ""
        self.force_ipv4 = bool(server_config.get("forceIpv4", False))
        self.retry_attempts = max(1, int(server_config.get("retryAttempts", 3)))
        self.retry_base_ms = int(server_config.get("retryBaseMs", 800))
        self.consecutive_failures = 0

        self.ssl_context = None
        if not server_config.get("verifyTls", True):
            self.ssl_context = ssl.create_default_context()
            self.ssl_context.check_hostname = False
            self.ssl_context.verify_mode = ssl.CERT_NONE

        if self.force_ipv4:
            self._install_ipv4_only_resolver()

        self.opener = self._build_opener()
        self._lock = threading.Lock()

    # ---------------------------------------------------------------- 连接器构造

    def _build_opener(self):
        handlers = []
        host = urllib.parse.urlparse(self.base_url).hostname or ""
        # 本机地址（SSH 隧道场景）永远直连
        is_local = host in ("127.0.0.1", "localhost", "::1")

        if self.proxy_mode == "custom" and self.proxy_url:
            handlers.append(urllib.request.ProxyHandler({
                "http": self.proxy_url,
                "https": self.proxy_url
            }))
        elif self.proxy_mode == "auto" and not is_local:
            pass  # 交给 urllib 读取 http_proxy / https_proxy 环境变量与系统设置
        else:
            handlers.append(urllib.request.ProxyHandler({}))

        if self.ssl_context is not None:
            handlers.append(urllib.request.HTTPSHandler(context=self.ssl_context))

        return urllib.request.build_opener(*handlers)

    def _install_ipv4_only_resolver(self):
        original = socket.getaddrinfo

        def ipv4_only(host, port, family=0, type=0, proto=0, flags=0):
            try:
                return original(host, port, socket.AF_INET, type, proto, flags)
            except socket.gaierror:
                return original(host, port, family, type, proto, flags)

        socket.getaddrinfo = ipv4_only
        self.logger("info", "已启用 IPv4-only 解析")

    def set_base_url(self, base_url):
        """切换服务端地址（用于直连失败后回退到 SSH 隧道端点）。"""
        with self._lock:
            self.base_url = str(base_url).rstrip("/")
            self.opener = self._build_opener()
        self.logger("warn", "通信地址已切换为 %s" % self.base_url)

    def describe(self):
        return "baseUrl=%s proxyMode=%s proxyUrl=%s ipv4Only=%s" % (
            self.base_url, self.proxy_mode, self.proxy_url or "-", self.force_ipv4
        )

    # ---------------------------------------------------------------- 基础请求

    def _call_once(self, path, params=None, body=None, method="GET"):
        if not self.base_url:
            raise TransportError("config", "未配置 server.baseUrl", retryable=False)
        url = self.base_url + path
        if params:
            cleaned = {k: v for k, v in params.items() if v is not None}
            if cleaned:
                url += "?" + urllib.parse.urlencode(cleaned)

        data = None
        headers = {"Authorization": "Bearer %s" % self.token}
        if body is not None:
            data = json.dumps(body, ensure_ascii=False).encode("utf-8")
            headers["Content-Type"] = "application/json; charset=utf-8"

        request = urllib.request.Request(url, data=data, headers=headers, method=method)
        try:
            with self.opener.open(request, timeout=self.timeout) as response:
                raw = response.read().decode("utf-8")
        except Exception as error:
            raise _classify(error)
        return json.loads(raw) if raw else {}

    def _call(self, path, params=None, body=None, method="GET", attempts=None):
        total = attempts if attempts else self.retry_attempts
        last = None
        for index in range(total):
            try:
                result = self._call_once(path, params, body, method)
                self.consecutive_failures = 0
                return result
            except TransportError as error:
                last = error
                self.consecutive_failures += 1
                if not error.retryable or index == total - 1:
                    raise
                wait = self.retry_base_ms * (2 ** index) / 1000.0
                self.logger("warn", "%s%s 失败(%d/%d): %s，%.1fs 后重试"
                            % (method, path, index + 1, total, error, wait))
                time.sleep(wait)
        raise last

    def push(self, ops):
        return self._call("/push", body={"ops": ops, "role": self.role}, method="POST")

    def snapshot(self, limit=None, offset=None):
        return self._call("/snapshot", params={
            "limit": limit if limit and limit > 0 else None,
            "offset": offset if offset and offset > 0 else None
        })

    def delta(self, cursor, limit=None):
        return self._call("/delta", params={
            "cursor": cursor,
            "limit": limit if limit and limit > 0 else None
        })

    def digest(self, scope=None):
        params = {}
        if scope:
            params["scope"] = ",".join(scope)
        return self._call("/digest", params=params)

    def entity(self, entity_type, sync_id):
        return self._call("/entity", params={"type": entity_type, "syncId": sync_id})

    def health(self, timeout=None):
        """轻量探活，用于切换线路前的判定。"""
        previous = self.timeout
        if timeout:
            self.timeout = float(timeout)
        try:
            return self._call("/health", attempts=1)
        finally:
            self.timeout = previous

    # ---------------------------------------------------------------- SSE 长连接

    def sse_loop(self, on_message, stop_event, timing):
        """在独立线程里维持 SSE 连接，断线自动指数退避重连。"""
        base_delay = float(timing.get("reconnectBaseMs", 1000)) / 1000.0
        max_delay = float(timing.get("reconnectMaxMs", 30000)) / 1000.0
        delay = base_delay

        while not stop_event.is_set():
            try:
                params = urllib.parse.urlencode({"deviceId": self.device_id, "role": self.role})
                url = self.base_url + "/stream?" + params
                request = urllib.request.Request(
                    url,
                    headers={
                        "Authorization": "Bearer %s" % self.token,
                        "Accept": "text/event-stream",
                        "Cache-Control": "no-cache",
                    },
                    method="GET",
                )
                # SSE 是长连接，不能设整体超时
                response = self.opener.open(request, timeout=None)
                self.logger("info", "SSE 已连接 (%s)" % self.base_url)
                delay = base_delay
                on_message({"t": "__open__"})

                event_name = None
                data_buffer = []
                for raw_line in response:
                    if stop_event.is_set():
                        break
                    line = raw_line.decode("utf-8", errors="replace").rstrip("\n").rstrip("\r")
                    if line.startswith(":"):
                        continue
                    if line == "":
                        if data_buffer:
                            payload = "\n".join(data_buffer)
                            data_buffer = []
                            try:
                                message = json.loads(payload)
                            except ValueError:
                                continue
                            if isinstance(message, dict) and "t" not in message:
                                message["t"] = event_name
                            on_message(message)
                        event_name = None
                        continue
                    if line.startswith("event:"):
                        event_name = line[len("event:"):].strip()
                    elif line.startswith("data:"):
                        data_buffer.append(line[len("data:"):].strip())

                response.close()
                on_message({"t": "__error__", "message": "服务端关闭了流"})
            except Exception as error:
                classified = error if isinstance(error, TransportError) else _classify(error)
                on_message({"t": "__error__", "message": str(classified)})
                self.logger("warn", "SSE 断开: %s" % classified)

            jitter = 1.0 + (time.time() % 1) * 0.3
            slept = 0.0
            wait = min(delay * jitter, max_delay)
            while slept < wait and not stop_event.is_set():
                time.sleep(min(0.5, wait - slept))
                slept += 0.5
            delay = min(delay * 2, max_delay)

        self.logger("info", "SSE 监听线程退出")


class StoppableThread(threading.Thread):
    def __init__(self, target, name=None):
        super().__init__(target=target, name=name, daemon=True)
        self._stop_event = threading.Event()

    def stop(self):
        self._stop_event.set()

    def should_stop(self):
        return self._stop_event.is_set()
