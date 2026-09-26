#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""PC 端同步代理：把桌面端 SQLite 与公网同步服务保持准实时双向一致。

运行方式：
    python sync_agent.py                 # 常驻运行
    python sync_agent.py --once          # 只跑一轮（排查用）
    python sync_agent.py --dry-run       # 只扫描不写远端、不写本地（体检用）
    python sync_agent.py --resync        # 启动时强制全量重推（本地为权威，覆盖服务端）
    python sync_agent.py --prune-orphans # 预览服务端上的孤儿实体
    python sync_agent.py --prune-orphans --yes   # 清理孤儿实体（推送墓碑）

数据流：
    本地 SQLite --(扫描 diff)--> 服务端 --(广播)--> 手机端
    手机端     --(WebSocket)--> 服务端 --(SSE)--> 本地 SQLite（白名单字段写回）
"""

import argparse
import json
import os
import queue
import signal
import sys
import threading
import time
import uuid

from local_store import ENTITY_SPECS, LocalStore, default_db_path
from protocol import digest_of, now_ms
from transport import Transport, TransportError

BASE_DIR = os.path.dirname(os.path.abspath(__file__))
DEFAULT_CONFIG = os.path.join(BASE_DIR, "config.json")


class SyncAgent:
    def __init__(self, config_path, dry_run=False, once=False, resync=False):
        self.config_path = os.path.abspath(config_path)
        # 配置里的相对路径统一相对「配置文件所在目录」解析，
        # 这样从任何工作目录启动结果都一致（之前 dbPath 相对 cwd 会踩坑）
        config_dir = os.path.dirname(self.config_path)
        self.config = self._load_config(self.config_path)
        self.dry_run = dry_run or bool(self.config.get("safety", {}).get("dryRun"))
        self.once = once
        self.resync = resync

        self.device_id = self.config.get("deviceId") or "pc-%s" % uuid.uuid4().hex[:8]
        self.role = self.config.get("role") or "pc"
        self.timing = self.config.get("timing") or {}
        self.sync_cfg = self.config.get("sync") or {}
        self.entity_types = self.sync_cfg.get("entityTypes") or list(ENTITY_SPECS.keys())
        self.digest_scope = self.sync_cfg.get("digestScope") or self.entity_types

        local_cfg = self.config.get("local") or {}
        self.log_path = self._resolve(local_cfg.get("logPath") or "./sync-agent.log", config_dir)
        self.state_path = self._resolve(local_cfg.get("statePath") or "./agent-state.json", config_dir)

        db_path = local_cfg.get("dbPath") or "auto"
        self.db_path = default_db_path() if db_path == "auto" else self._resolve(db_path, config_dir)

        self.state = self._load_state()
        self.remote_queue = queue.Queue()
        self.stop_event = threading.Event()
        self.need_delta = True
        # 首轮先做全量上行，摘要校验推迟一个周期，避免刚推一半就误判不一致
        self.last_digest_at = time.time()
        self.cycles = 0
        self.stats = {"pushed": 0, "appliedRemote": 0, "conflicts": 0, "errors": 0}

        # 线路回退：直连失败若干次后自动切到备用地址（例如 SSH 隧道端点）
        self.primary_base_url = (self.config.get("server") or {}).get("baseUrl", "")
        self.using_fallback = False
        self.last_probe_at = 0.0

        allow_write = bool(self.config.get("safety", {}).get("allowLocalWrite", True)) and not self.dry_run
        allow_delete = bool(self.config.get("safety", {}).get("allowLocalDelete", True)) and not self.dry_run
        self.store = LocalStore(self.db_path, self.log, allow_write=allow_write, allow_delete=allow_delete)
        self.transport = Transport(self.config.get("server") or {}, self.log, self.device_id, self.role)

    # ---------------------------------------------------------------- 基础设施

    def _load_config(self, path):
        with open(path, "r", encoding="utf-8") as handle:
            return json.load(handle)

    @staticmethod
    def _resolve(path, config_dir):
        return path if os.path.isabs(path) else os.path.abspath(os.path.join(config_dir, path))

    def _load_state(self):
        if os.path.exists(self.state_path):
            try:
                with open(self.state_path, "r", encoding="utf-8") as handle:
                    state = json.load(handle)
                state.setdefault("entities", {})
                state.setdefault("revs", {})
                state.setdefault("cursor", 0)
                return state
            except (ValueError, OSError) as error:
                self.log("warn", "状态文件损坏，重新开始: %s" % error)
        return {"deviceId": self.device_id, "cursor": 0, "entities": {}, "revs": {}}

    def save_state(self):
        self.state["deviceId"] = self.device_id
        os.makedirs(os.path.dirname(self.state_path), exist_ok=True)
        tmp = self.state_path + ".tmp"
        with open(tmp, "w", encoding="utf-8") as handle:
            json.dump(self.state, handle, ensure_ascii=False)
        os.replace(tmp, self.state_path)

    def log(self, level, message):
        line = "%s [%s] %s" % (time.strftime("%Y-%m-%d %H:%M:%S"), level.upper(), message)
        print(line, flush=True)
        try:
            os.makedirs(os.path.dirname(self.log_path), exist_ok=True)
            with open(self.log_path, "a", encoding="utf-8") as handle:
                handle.write(line + "\n")
        except OSError:
            pass

    # ---------------------------------------------------------------- 下行：服务端 -> 本地

    def on_remote_message(self, message):
        kind = message.get("t")
        if kind == "apply":
            self.remote_queue.put(message.get("op"))
        elif kind == "__open__":
            # SSE 重新连上：按 cursor 补齐离线期间的变更
            self.need_delta = True
        elif kind == "__error__":
            # 主动退出导致的断开不算故障，否则每轮都会多出一个假 error
            if not self.stop_event.is_set():
                self.stats["errors"] += 1

    def drain_remote(self):
        applied = 0
        while True:
            try:
                op = self.remote_queue.get_nowait()
            except queue.Empty:
                break
            if not op:
                continue
            if op.get("deviceId") == self.device_id:
                continue
            result, detail = self.store.apply_remote(op)
            if result == "applied":
                applied += 1
                self.stats["appliedRemote"] += 1
                self.log("info", "写回本地 %s/%s: %s" % (op.get("entityType"), op.get("syncId"), detail))
            elif result == "error":
                self.stats["errors"] += 1
                self.log("error", "写回失败 %s/%s: %s" % (op.get("entityType"), op.get("syncId"), detail))
            else:
                self.log("debug", "跳过 %s/%s: %s" % (op.get("entityType"), op.get("syncId"), detail))
        return applied

    def _enqueue_remote(self, entity_type, sync_id, fields, deleted_at, device_id=None):
        if not entity_type or not sync_id:
            return
        self.remote_queue.put({
            "entityType": entity_type,
            "syncId": sync_id,
            "fields": fields or {},
            "deletedAt": deleted_at,
            "deviceId": device_id,
        })

    def pull_delta(self):
        """按 cursor 增量补齐。

        关键点：服务端分页返回时必须用 nextCursor（最后一条 op 的 seq）推进游标，
        绝不能直接跳到 serverRev——那样中间那段 op 会被永久跳过，属于静默丢数据。
        """
        total = 0
        batch = int(self.sync_cfg.get("deltaBatchSize", 500))
        for _ in range(500):  # 上限保护，避免异常情况下死循环
            try:
                response = self.transport.delta(self.state.get("cursor", 0), batch)
            except TransportError as error:
                self.log("warn", "拉取增量失败: %s" % error)
                return total

            if response.get("stale"):
                self.log("warn", "oplog 已裁剪，改为拉取全量快照")
                return self.pull_snapshot()

            ops = response.get("ops") or []
            for entry in ops:
                self._enqueue_remote(entry.get("entityType"), entry.get("syncId"),
                                     entry.get("fields"), entry.get("deletedAt"),
                                     entry.get("deviceId"))
            total += len(ops)

            next_cursor = response.get("nextCursor")
            if isinstance(next_cursor, int) and next_cursor > self.state.get("cursor", 0):
                self.state["cursor"] = next_cursor
            elif not ops:
                self.state["cursor"] = response.get("serverRev", self.state.get("cursor", 0))

            if not response.get("truncated"):
                break
            self.log("info", "增量还有 %s 条未拉取，继续" % response.get("pending"))
        return total

    def pull_snapshot(self):
        """分页拉取全量快照：单页小响应在弱网下更容易成功。"""
        page_size = int(self.sync_cfg.get("snapshotPageSize", 500))
        offset = 0
        pages = 0
        while pages < 500:
            try:
                response = self.transport.snapshot(limit=page_size, offset=offset)
            except TransportError as error:
                self.log("error", "拉取快照失败(%d 页后): %s" % (pages, error))
                return pages

            entities = response.get("entities") or {}
            count = 0
            for entity_type, bucket in entities.items():
                for sync_id, entity in bucket.items():
                    self._enqueue_remote(entity_type, sync_id,
                                         entity.get("fields"), entity.get("deletedAt"))
                    count += 1
            pages += 1
            self.state["cursor"] = response.get("serverRev", self.state.get("cursor", 0))

            if not response.get("hasMore") or count == 0:
                break
            offset += count
            self.log("info", "快照第 %d 页完成(%d 条)，继续拉取" % (pages, count))

        self.log("info", "全量快照拉取完成，共 %d 页" % pages)
        return pages

    # ---------------------------------------------------------------- 线路回退

    def maybe_switch_endpoint(self):
        """主线路连续失败时切到备用线路（典型场景：直连被网络环境干扰，改用 SSH 隧道端点）。

        切到备用线路后仍会定期探活主线路，一旦恢复就自动切回，避免长期依赖隧道。
        """
        fallback = (self.config.get("server") or {}).get("fallback") or {}
        if not fallback.get("enabled") or not fallback.get("baseUrl"):
            return

        now = time.time()
        probe_interval = float(fallback.get("probeIntervalMs", 300000)) / 1000.0

        if not self.using_fallback:
            threshold = int(fallback.get("afterFailures", 3))
            if (self.transport.consecutive_failures >= threshold
                    and now - self.last_probe_at >= min(probe_interval, 60.0)):
                self.log("warn", "主线路连续失败 %d 次，切换到备用线路 %s"
                         % (self.transport.consecutive_failures, fallback["baseUrl"]))
                self.transport.set_base_url(fallback["baseUrl"])
                self.using_fallback = True
                self.last_probe_at = now
                self.transport.consecutive_failures = 0
                self.need_delta = True
            return

        # 备用线路自己也在连续失败时，不必干等到下一个探测周期，立刻回头试主线路
        fallback_struggling = self.transport.consecutive_failures >= int(fallback.get("afterFailures", 3))
        if now - self.last_probe_at < probe_interval and not fallback_struggling:
            return
        self.last_probe_at = now
        if self._probe_primary():
            self.transport.consecutive_failures = 0
            self.using_fallback = False
            self.need_delta = True
            self.log("info", "主线路已恢复，切回直连")
        elif fallback_struggling:
            # 两条线路都不通：清空计数，避免每次循环都做一次无谓探测
            self.transport.consecutive_failures = 0

    def _probe_primary(self):
        fallback_url = self.transport.base_url
        try:
            self.transport.set_base_url(self.primary_base_url)
            self.transport.health(timeout=6)
            return True
        except Exception as error:
            self.log("info", "主线路探活仍失败: %s" % error)
            self.transport.set_base_url(fallback_url)
            return False

    # ---------------------------------------------------------------- 上行：本地 -> 服务端

    def build_ops(self, snapshot, include_unchanged=False, force=False):
        ops = []
        known = self.state.setdefault("entities", {})
        for entity_type, bucket in snapshot.items():
            previous = known.setdefault(entity_type, {})
            for sync_id, item in bucket.items():
                old = previous.get(sync_id)
                changed = old is None or old.get("hash") != item["hash"]
                if not changed and not include_unchanged:
                    continue
                ops.append({
                    "opId": uuid.uuid4().hex,
                    "deviceId": self.device_id,
                    "role": self.role,
                    "entityType": entity_type,
                    "syncId": sync_id,
                    "fields": item["fields"],
                    "updatedAt": item["updatedAt"],
                    "deletedAt": None,
                    "baseRev": self.state.get("revs", {}).get("%s|%s" % (entity_type, sync_id)),
                    "force": bool(force),
                })
            # 删除检测：本地已消失但之前同步过的实体，补一条墓碑
            spec = ENTITY_SPECS.get(entity_type)
            if spec and spec.get("track_deletes"):
                for sync_id, old in list(previous.items()):
                    if sync_id in bucket or old.get("deleted"):
                        continue
                    ops.append({
                        "opId": uuid.uuid4().hex,
                        "deviceId": self.device_id,
                        "role": self.role,
                        "entityType": entity_type,
                        "syncId": sync_id,
                        "fields": {},
                        "updatedAt": now_ms(),
                        "deletedAt": now_ms(),
                        "baseRev": self.state.get("revs", {}).get("%s|%s" % (entity_type, sync_id)),
                        "force": bool(force),
                    })
        return ops

    def push_ops(self, ops):
        pushed = 0
        batch_size = int(self.sync_cfg.get("pushBatchSize", 200))
        for start in range(0, len(ops), batch_size):
            batch = ops[start:start + batch_size]
            if self.dry_run:
                self.log("info", "[dry-run] 将推送 %d 条 op，例如 %s/%s"
                         % (len(batch), batch[0].get("entityType"), batch[0].get("syncId")))
                continue
            response = None
            for attempt in range(3):
                try:
                    response = self.transport.push(batch)
                    break
                except Exception as error:
                    wait = 2 ** attempt
                    self.log("warn", "推送失败(%d/3): %s，%ss 后重试" % (attempt + 1, error, wait))
                    self._sleep(wait)
            if response is None:
                self.stats["errors"] += 1
                self.log("error", "批次推送失败，本轮放弃；本地状态未推进，下一轮自动重推")
                return pushed

            for result in response.get("results") or []:
                self.handle_push_result(result, batch)
            self.state["cursor"] = response.get("serverRev", self.state.get("cursor", 0))
            pushed += len(batch)
            self.stats["pushed"] += len(batch)
        return pushed

    def handle_push_result(self, result, batch):
        status = result.get("status")
        entity_type = result.get("entityType")
        sync_id = result.get("syncId")
        if not entity_type or not sync_id:
            return
        sent = next((op for op in batch if op.get("opId") == result.get("opId")), None)

        if status in ("applied", "duplicate"):
            if sent:
                bucket = self.state.setdefault("entities", {}).setdefault(entity_type, {})
                bucket[sync_id] = {
                    "hash": result.get("hash") or _hash_of(sent),
                    "updatedAt": sent.get("updatedAt"),
                    "deleted": bool(sent.get("deletedAt")),
                }
            if result.get("rev"):
                self.state.setdefault("revs", {})["%s|%s" % (entity_type, sync_id)] = result["rev"]
        elif status == "conflict":
            self.stats["conflicts"] += 1
            self.log("warn", "冲突落败，采用服务端版本: %s/%s" % (entity_type, sync_id))
            self.adopt_server_version(entity_type, sync_id, result)
        elif status == "rejected":
            self.log("warn", "被服务端拒绝: %s/%s (%s)" % (entity_type, sync_id, result.get("reason")))

    def adopt_server_version(self, entity_type, sync_id, result):
        """冲突落败时：拉服务端当前版本写回本地，避免下一轮又推出去形成拉锯。"""
        try:
            response = self.transport.entity(entity_type, sync_id)
        except Exception as error:
            self.log("error", "拉取服务端实体失败: %s" % error)
            return
        entity = response.get("entity")
        if not entity:
            return
        self.store.apply_remote({
            "entityType": entity_type,
            "syncId": sync_id,
            "fields": entity.get("fields") or {},
            "deletedAt": entity.get("deletedAt"),
        })
        bucket = self.state.setdefault("entities", {}).setdefault(entity_type, {})
        bucket[sync_id] = {
            "hash": entity.get("hash"),
            "updatedAt": entity.get("updatedAt"),
            "deleted": bool(entity.get("deletedAt")),
        }
        if entity.get("rev"):
            self.state.setdefault("revs", {})["%s|%s" % (entity_type, sync_id)] = entity["rev"]

    # ---------------------------------------------------------------- 一致性校验

    def _local_digest(self):
        """扫描本地并按 scope 算摘要，同时返回快照供后续复用。"""
        snapshot = self.store.scan(
            self.digest_scope,
            int(self.sync_cfg.get("appEventDays", 3)),
            int(self.sync_cfg.get("appEventMaxRows", 500)),
        )
        digest = digest_of({
            entity_type: {sync_id: item["hash"] for sync_id, item in bucket.items()}
            for entity_type, bucket in snapshot.items()
        })
        return digest, snapshot

    def _remote_digest(self):
        try:
            return self.transport.digest(self.digest_scope)
        except Exception as error:
            self.log("warn", "摘要校验失败: %s" % error)
            return None

    def _digest_pair(self):
        """返回 (本地摘要, 本地快照, 远端摘要)；远端不可达时远端摘要为 None。"""
        local_digest, snapshot = self._local_digest()
        return local_digest, snapshot, self._remote_digest()

    def check_digest(self):
        """定期一致性校验。

        **不能只看一次比对就下结论。** 桌面端为进行中的专注记录每秒都在写
        `actual_seconds`，手机端的写入也会由服务端先行落地，而本地扫描和服务端查询
        之间必然存在几百毫秒的时间差——所以单次不一致里绝大多数只是「两边读的时机
        错开了」，并非真的漂移。

        直接升级为「强制全量重推」有两个坏处：白白重推几千条实体；
        更糟的是 `force: true` 会绕过 LWW，可能把手机端刚提交、SSE 还没回传到
        本地的修改顶掉。所以这里先给一个复检窗口：

          1. 比对一次，一致就结束；
          2. 不一致 → 重新扫描（上一份快照可能已比服务端旧），按常规 diff 推送本地
             变更，隔一秒再比对，最多重复几轮；
          3. 连续多轮仍不一致 → 才是状态指纹看不出来的真实漂移，这时才动用
             「本地为权威，强制全量重推」这个重手段。
        """
        if not self.digest_scope:
            return

        local_digest, snapshot, remote = self._digest_pair()
        if remote is None:
            return
        if remote.get("digest") == local_digest:
            self.log("info", "一致性校验通过 (serverRev=%s)" % remote.get("serverRev"))
            return

        for attempt in range(3):
            self._sleep(1.0)
            local_digest, snapshot, remote = self._digest_pair()
            if remote is None:
                return
            if remote.get("digest") == local_digest:
                self.log("info", "复检后一致，确认只是时机错位 (serverRev=%s)"
                         % remote.get("serverRev"))
                return
            ops = self.build_ops(snapshot)
            if ops:
                self.log("info", "摘要暂不一致，先推送 %d 条本地变更（第 %d 次复检）"
                         % (len(ops), attempt + 1))
                self.push_ops(ops)

        self.log("warn", "摘要连续 4 次不一致，本地为权威，强制全量重推")
        _local, snapshot, _remote = self._digest_pair()
        ops = self.build_ops(snapshot, include_unchanged=True, force=True)
        self.log("warn", "强制重推 %d 条实体" % len(ops))
        self.push_ops(ops)

    # ---------------------------------------------------------------- 主循环

    def run(self):
        self.log("info", "启动同步代理: deviceId=%s db=%s" % (self.device_id, self.db_path))
        self.log("info", "通信参数: %s" % self.transport.describe())
        if not os.path.exists(self.db_path):
            self.log("error", "找不到本地数据库: %s（检查 config.json 的 local.dbPath）" % self.db_path)
            return 2
        if self.dry_run:
            self.log("warn", "dry-run 模式：不会写入远端，也不会写回本地")

        self.store.open()
        sse_thread = None
        if (self.config.get("server") or {}).get("sseEnabled", True) and not self.dry_run:
            sse_thread = threading.Thread(
                target=self.transport.sse_loop,
                args=(self.on_remote_message, self.stop_event, self.timing),
                name="sse-listener",
                daemon=True,
            )
            sse_thread.start()

        try:
            while not self.stop_event.is_set():
                self.cycles += 1
                try:
                    self.maybe_switch_endpoint()
                    if self.need_delta:
                        self.need_delta = False
                        self.pull_delta()
                    self.drain_remote()

                    snapshot = self.store.scan(
                        self.entity_types,
                        int(self.sync_cfg.get("appEventDays", 3)),
                        int(self.sync_cfg.get("appEventMaxRows", 500)),
                    )
                    if self.resync:
                        self.resync = False
                        ops = self.build_ops(snapshot, include_unchanged=True, force=True)
                        self.log("info", "强制全量重推 %d 条实体" % len(ops))
                    else:
                        ops = self.build_ops(snapshot)
                    if ops:
                        self.log("info", "本地变更 %d 条，推送中" % len(ops))
                        self.push_ops(ops)
                    self.save_state()

                    interval = int(self.timing.get("digestIntervalMs", 300000)) / 1000.0
                    if interval > 0 and time.time() - self.last_digest_at > interval:
                        self.last_digest_at = time.time()
                        if not self.dry_run:
                            self.check_digest()
                except Exception as error:
                    self.stats["errors"] += 1
                    self.log("error", "主循环异常: %s" % error)

                if self.once:
                    break
                self._sleep(int(self.timing.get("scanIntervalMs", 2000)) / 1000.0)
        finally:
            self.stop_event.set()
            self.save_state()
            self.store.close()
            self.log("info", "同步代理已停止: %s" % json.dumps(self.stats, ensure_ascii=False))
        return 0

    def _sleep(self, seconds):
        slept = 0.0
        while slept < seconds and not self.stop_event.is_set():
            time.sleep(min(0.2, seconds - slept))
            slept += 0.2

    # ---------- 孤儿清理 ----------

    def orphan_report(self, entity_types=None):
        """找出「服务端有、本地 sync_meta 里没有」的实体。

        这类实体是历史遗留：本地那行已经不存在或从未登记过（例如被桌面端去重删掉
        同步身份的重复学科），但服务端还留着。不清掉的话它们会永久占位。
        """
        targets = [t for t in (entity_types or self.entity_types) if t in ENTITY_SPECS]
        remote = self.transport.snapshot()
        buckets = remote.get("entities") or {}
        report = {}
        for entity_type in targets:
            known = self.store.sync_id_set(ENTITY_SPECS[entity_type]["meta_type"])
            orphans = []
            for sync_id, item in (buckets.get(entity_type) or {}).items():
                if item.get("deletedAt"):
                    continue          # 已经是墓碑，不用再清
                if sync_id in known:
                    continue
                orphans.append((sync_id, item.get("fields") or {}))
            if orphans:
                report[entity_type] = orphans
        return report

    def prune_orphans(self, entity_types=None, confirm=False):
        """清理孤儿实体。confirm=False 时只打印，不动服务端。"""
        try:
            report = self.orphan_report(entity_types)
        except Exception as error:
            self.log("error", "孤儿扫描失败: %s" % error)
            return 1

        total = sum(len(items) for items in report.values())
        if total == 0:
            self.log("info", "没有发现孤儿实体，服务端与本地一致")
            return 0

        self.log("info", "发现 %d 个孤儿实体（服务端有、本地 sync_meta 无）:" % total)
        for entity_type, items in report.items():
            for sync_id, fields in items:
                hint = fields.get("name") or fields.get("title") or "-"
                self.log("info", "  %s/%s  %s" % (entity_type, sync_id, hint))

        if not confirm:
            self.log("warn", "以上仅为预览。确认无误后加 --yes 才会真正推送墓碑删除")
            return 0

        now = now_ms()
        ops = []
        for entity_type, items in report.items():
            for sync_id, _fields in items:
                ops.append({
                    "opId": uuid.uuid4().hex,
                    "deviceId": self.device_id,
                    "role": self.role,
                    "entityType": entity_type,
                    "syncId": sync_id,
                    "fields": {},
                    "updatedAt": now,
                    "deletedAt": now,
                    "baseRev": self.state.get("revs", {}).get("%s|%s" % (entity_type, sync_id)),
                    # 本地已经不认识这些实体了，让它们直接覆盖服务端旧版本
                    "force": True,
                })
        pushed = self.push_ops(ops)
        self.save_state()
        self.log("info", "孤儿清理完成：推送墓碑 %d 条" % pushed)
        return 0


    def run_prune(self, entity_types=None, confirm=False):
        """--prune-orphans 入口：只做一致性清理，不跑常驻循环。"""
        self.log("info", "孤儿清理: deviceId=%s db=%s" % (self.device_id, self.db_path))
        self.log("info", "通信参数: %s" % self.transport.describe())
        if not os.path.exists(self.db_path):
            self.log("error", "找不到本地数据库: %s" % self.db_path)
            return 2
        self.store.open()
        try:
            code = self.prune_orphans(entity_types, confirm=confirm)
        finally:
            self.store.close()
            self.log("info", "同步代理已停止: %s" % json.dumps(self.stats, ensure_ascii=False))
        return code


def _hash_of(op):
    from protocol import entity_hash
    return entity_hash(op.get("entityType"), op.get("syncId"), op.get("fields") or {}, op.get("deletedAt"))


def main():
    parser = argparse.ArgumentParser(description="考研专注 PC 端同步代理")
    parser.add_argument("--config", default=DEFAULT_CONFIG, help="配置文件路径")
    parser.add_argument("--once", action="store_true", help="只执行一轮")
    parser.add_argument("--dry-run", action="store_true", help="不写远端、不写本地")
    parser.add_argument("--resync", action="store_true", help="强制全量重推")
    parser.add_argument("--prune-orphans", action="store_true",
                        help="列出服务端有、本地 sync_meta 没有的实体（默认只预览）")
    parser.add_argument("--type", action="append", default=None,
                        help="配合 --prune-orphans 限定实体类型，可重复")
    parser.add_argument("--yes", action="store_true",
                        help="配合 --prune-orphans 真正推送墓碑删除")
    args = parser.parse_args()

    agent = SyncAgent(args.config, dry_run=args.dry_run, once=args.once, resync=args.resync)

    def handle_signal(signum, frame):
        agent.stop_event.set()

    signal.signal(signal.SIGINT, handle_signal)
    signal.signal(signal.SIGTERM, handle_signal)

    if args.prune_orphans:
        return agent.run_prune(entity_types=args.type, confirm=args.yes)
    return agent.run()


if __name__ == "__main__":
    sys.exit(main())
