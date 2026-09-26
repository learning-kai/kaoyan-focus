"""PC 端本地数据访问层。

职责：
  1. 读取桌面端 SQLite（`kaoyan-focus.sqlite3`），把业务行翻译成设备无关的同步实体
  2. 复用桌面端已有的 sync_meta 里的 sync_id，缺失时才补发，保证与桌面端自带同步不打架
  3. 按字段白名单把手机端的改动写回本地库（写回越权字段会破坏桌面端状态机，必须严格限制）

设计边界：
  - 本模块只读业务表 + 只写 sync_meta 和白名单字段，绝不触碰 settings（里面有 WebDAV/飞书/SMTP 凭据）
  - 与桌面端进程并发访问同一个 SQLite 文件，靠 WAL + busy_timeout 保证不锁死
"""

import json
import os
import sqlite3
import uuid
from datetime import datetime, timedelta, timezone

from protocol import entity_hash, parse_time_ms, now_ms, now_rfc3339

# sync_meta 里的 entity_type 命名，必须与桌面端保持一致（复用既有 sync_id）
META_SUBJECT = "subject"
META_STUDY_MODE = "study_mode"
META_FOCUS_SESSION = "focus_session"
META_APP_EVENT = "app_event"
META_CHECKLIST_TASK = "checklist_task"
META_TODAY_PLAN_ITEM = "today_plan_item"
META_SCHEDULE_BLOCK = "schedule_block"
META_SCHEDULE_TEMPLATE = "schedule_template"
META_DAILY_REVIEW = "daily_review"
META_WEEKLY_REVIEW = "weekly_review"


def _bool(value):
    return bool(value) if value is not None else False


def _int(value, default=0):
    try:
        return int(value)
    except (TypeError, ValueError):
        return default


def _weekdays(value):
    if isinstance(value, list):
        return [_int(v) for v in value]
    if isinstance(value, str):
        try:
            parsed = json.loads(value)
            if isinstance(parsed, list):
                return [_int(v) for v in parsed]
        except (ValueError, TypeError):
            return []
    return []


# ------------------------------------------------------------------ 实体定义

ENTITY_SPECS = {
    "subject": {
        "meta_type": META_SUBJECT,
        "table": "subjects",
        "track_deletes": True,
        "sql": """
            SELECT s.id AS local_id, m.sync_id AS sync_id, s.name, s.color, s.enabled, s.updated_at
            FROM subjects s
            LEFT JOIN sync_meta m ON m.entity_type = 'subject' AND m.local_id = s.id
        """,
        "build": lambda r: {
            "name": r["name"],
            "color": r["color"],
            "enabled": _bool(r["enabled"]),
        },
        # 桌面端 storage/db.rs 的 normalize_default_subjects() 会把同名重复学科
        # 停用（enabled=0）并删掉它们的 sync_meta 行。如果我们仍然给这些行发号，
        # 就会陷入「发号 → 桌面端删号 → 下轮再发一个新号」的死循环，
        # 每一轮都在服务端堆一个重复学科。
        # 所以：停用且**尚无 sync_id** 的行不给发号（已有 sync_id 的照常同步，
        # 避免用户主动停用某个正在同步的学科时把它从云上弄丢）。
        "mint_guard": lambda r: _bool(r.get("enabled")),
        "write_map": {},
    },
    "checklist_task": {
        "meta_type": META_CHECKLIST_TASK,
        "table": "checklist_tasks",
        "track_deletes": True,
        "sql": """
            SELECT t.id AS local_id, m.sync_id AS sync_id, t.board_scope, t.title, t.note,
                   t.due_date, t.sort_order, t.completed, t.updated_at,
                   c.name AS column_name,
                   sm.sync_id AS subject_sync_id
            FROM checklist_tasks t
            LEFT JOIN sync_meta m ON m.entity_type = 'checklist_task' AND m.local_id = t.id
            LEFT JOIN checklist_columns c ON c.id = t.column_id
            LEFT JOIN sync_meta sm ON sm.entity_type = 'subject' AND sm.local_id = t.subject_id
        """,
        "build": lambda r: {
            "board_scope": r["board_scope"],
            "column_name": r["column_name"],
            "title": r["title"],
            "note": r["note"],
            "due_date": r["due_date"],
            "sort_order": _int(r["sort_order"]),
            "completed": _bool(r["completed"]),
            "subject_sync_id": r["subject_sync_id"],
        },
        "write_map": {
            "title": ("title", "str"),
            "note": ("note", "str"),
            "due_date": ("due_date", "str"),
            "completed": ("completed", "bool"),
            "sort_order": ("sort_order", "int"),
            "board_scope": ("board_scope", "str"),
            "subject_sync_id": ("subject_id", "subject"),
        },
    },
    "today_plan_item": {
        "meta_type": META_TODAY_PLAN_ITEM,
        "table": "today_plan_items",
        "track_deletes": True,
        "sql": """
            SELECT p.id AS local_id, m.sync_id AS sync_id, p.today_date, p.title, p.note,
                   p.due_date, p.sort_order, p.completed, p.updated_at,
                   sm.sync_id AS subject_sync_id, tm.sync_id AS source_task_sync_id
            FROM today_plan_items p
            LEFT JOIN sync_meta m ON m.entity_type = 'today_plan_item' AND m.local_id = p.id
            LEFT JOIN sync_meta sm ON sm.entity_type = 'subject' AND sm.local_id = p.subject_id
            LEFT JOIN sync_meta tm ON tm.entity_type = 'checklist_task' AND tm.local_id = p.source_task_id
        """,
        "build": lambda r: {
            "today_date": r["today_date"],
            "title": r["title"],
            "note": r["note"],
            "due_date": r["due_date"],
            "sort_order": _int(r["sort_order"]),
            "completed": _bool(r["completed"]),
            "subject_sync_id": r["subject_sync_id"],
            "source_task_sync_id": r["source_task_sync_id"],
        },
        "write_map": {
            "title": ("title", "str"),
            "note": ("note", "str"),
            "due_date": ("due_date", "str"),
            "completed": ("completed", "bool"),
            "sort_order": ("sort_order", "int"),
            "today_date": ("today_date", "str"),
            "subject_sync_id": ("subject_id", "subject"),
        },
    },
    "schedule_block": {
        "meta_type": META_SCHEDULE_BLOCK,
        "table": "schedule_blocks",
        "track_deletes": True,
        "sql": """
            SELECT b.id AS local_id, m.sync_id AS sync_id, b.schedule_date, b.title, b.note,
                   b.category_key, b.start_minute, b.end_minute, b.status, b.updated_at,
                   sm.sync_id AS subject_sync_id
            FROM schedule_blocks b
            LEFT JOIN sync_meta m ON m.entity_type = 'schedule_block' AND m.local_id = b.id
            LEFT JOIN sync_meta sm ON sm.entity_type = 'subject' AND sm.local_id = b.subject_id
        """,
        "build": lambda r: {
            "schedule_date": r["schedule_date"],
            "title": r["title"],
            "note": r["note"],
            "category_key": r["category_key"],
            "start_minute": _int(r["start_minute"]),
            "end_minute": _int(r["end_minute"]),
            "status": r["status"],
            "subject_sync_id": r["subject_sync_id"],
        },
        "write_map": {"status": ("status", "str")},
    },
    "schedule_template": {
        "meta_type": META_SCHEDULE_TEMPLATE,
        "table": "schedule_templates",
        "track_deletes": True,
        "sql": """
            SELECT t.id AS local_id, m.sync_id AS sync_id, t.title, t.note, t.category_key,
                   t.weekdays, t.start_minute, t.end_minute, t.enabled, t.updated_at,
                   sm.sync_id AS subject_sync_id
            FROM schedule_templates t
            LEFT JOIN sync_meta m ON m.entity_type = 'schedule_template' AND m.local_id = t.id
            LEFT JOIN sync_meta sm ON sm.entity_type = 'subject' AND sm.local_id = t.subject_id
        """,
        "build": lambda r: {
            "title": r["title"],
            "note": r["note"],
            "category_key": r["category_key"],
            "weekdays": _weekdays(r["weekdays"]),
            "start_minute": _int(r["start_minute"]),
            "end_minute": _int(r["end_minute"]),
            "enabled": _bool(r["enabled"]),
            "subject_sync_id": r["subject_sync_id"],
        },
        "write_map": {},
    },
    "daily_review": {
        "meta_type": META_DAILY_REVIEW,
        "table": "daily_reviews",
        "track_deletes": True,
        "sql": """
            SELECT d.id AS local_id, m.sync_id AS sync_id, d.review_date, d.summary, d.blockers,
                   d.tomorrow_focus, d.mood_score, d.updated_at
            FROM daily_reviews d
            LEFT JOIN sync_meta m ON m.entity_type = 'daily_review' AND m.local_id = d.id
        """,
        "build": lambda r: {
            "review_date": r["review_date"],
            "summary": r["summary"],
            "blockers": r["blockers"],
            "tomorrow_focus": r["tomorrow_focus"],
            "mood_score": _int(r["mood_score"], 3),
        },
        "write_map": {
            "summary": ("summary", "str"),
            "blockers": ("blockers", "str"),
            "tomorrow_focus": ("tomorrow_focus", "str"),
            "mood_score": ("mood_score", "int"),
        },
    },
    "weekly_review": {
        "meta_type": META_WEEKLY_REVIEW,
        "table": "weekly_reviews",
        "track_deletes": True,
        "sql": """
            SELECT w.id AS local_id, m.sync_id AS sync_id, w.week_start_date, w.summary,
                   w.blockers, w.next_week_focus, w.mood_score, w.updated_at
            FROM weekly_reviews w
            LEFT JOIN sync_meta m ON m.entity_type = 'weekly_review' AND m.local_id = w.id
        """,
        "build": lambda r: {
            "week_start_date": r["week_start_date"],
            "summary": r["summary"],
            "blockers": r["blockers"],
            "next_week_focus": r["next_week_focus"],
            "mood_score": _int(r["mood_score"], 3),
        },
        "write_map": {
            "summary": ("summary", "str"),
            "blockers": ("blockers", "str"),
            "next_week_focus": ("next_week_focus", "str"),
            "mood_score": ("mood_score", "int"),
        },
    },
    "focus_session": {
        "meta_type": META_FOCUS_SESSION,
        "table": "focus_sessions",
        "track_deletes": True,
        "sql": """
            SELECT f.id AS local_id, m.sync_id AS sync_id, f.mode, f.planned_seconds, f.actual_seconds,
                   f.paused_seconds, f.started_at, f.ended_at, f.status, f.end_reason,
                   f.interruption_count, f.emergency_exit_count, f.followed_by_break_type,
                   f.updated_at, sm.sync_id AS subject_sync_id
            FROM focus_sessions f
            LEFT JOIN sync_meta m ON m.entity_type = 'focus_session' AND m.local_id = f.id
            LEFT JOIN sync_meta sm ON sm.entity_type = 'subject' AND sm.local_id = f.subject_id
        """,
        "build": lambda r: {
            "mode": r["mode"],
            "subject_sync_id": r["subject_sync_id"],
            "planned_seconds": _int(r["planned_seconds"]),
            "actual_seconds": _int(r["actual_seconds"]),
            "paused_seconds": _int(r["paused_seconds"]),
            "started_at": r["started_at"],
            "ended_at": r["ended_at"],
            "status": r["status"],
            "end_reason": r["end_reason"],
            "interruption_count": _int(r["interruption_count"]),
            "emergency_exit_count": _int(r["emergency_exit_count"]),
            "followed_by_break_type": r["followed_by_break_type"],
        },
        "write_map": {},
    },
    "study_mode": {
        "meta_type": META_STUDY_MODE,
        "table": "study_modes",
        "track_deletes": True,
        "sql": """
            SELECT s.id AS local_id, m.sync_id AS sync_id, s.mode, s.timer_kind, s.planned_seconds,
                   s.focus_seconds, s.break_seconds, s.long_break_seconds, s.long_break_interval,
                   s.phase, s.cycle_index, s.started_at, s.phase_started_at, s.paused_at,
                   s.accumulated_study_seconds, s.total_paused_seconds, s.ended_at, s.status,
                   s.state_revision, s.updated_at, sm.sync_id AS subject_sync_id
            FROM study_modes s
            LEFT JOIN sync_meta m ON m.entity_type = 'study_mode' AND m.local_id = s.id
            LEFT JOIN sync_meta sm ON sm.entity_type = 'subject' AND sm.local_id = s.subject_id
        """,
        "build": lambda r: {
            "mode": r["mode"],
            "timer_kind": r["timer_kind"],
            "subject_sync_id": r["subject_sync_id"],
            "planned_seconds": _int(r["planned_seconds"]),
            "focus_seconds": _int(r["focus_seconds"]),
            "break_seconds": _int(r["break_seconds"]),
            "long_break_seconds": _int(r["long_break_seconds"]),
            "long_break_interval": _int(r["long_break_interval"]),
            "phase": r["phase"],
            "round_number": _int(r["cycle_index"]),
            "started_at": r["started_at"],
            "phase_started_at": r["phase_started_at"],
            "paused_at": r["paused_at"],
            "accumulated_study_seconds": _int(r["accumulated_study_seconds"]),
            "total_paused_seconds": _int(r["total_paused_seconds"]),
            "ended_at": r["ended_at"],
            "status": r["status"],
            "state_revision": _int(r["state_revision"]),
        },
        "write_map": {},
    },
    "app_event": {
        "meta_type": META_APP_EVENT,
        "table": "app_events",
        # 干扰事件按时间窗口同步，窗口外的旧记录会被误判成删除，所以不做删除检测
        "track_deletes": False,
        "sql": """
            SELECT e.id AS local_id, m.sync_id AS sync_id, e.process_name, e.window_title,
                   e.event_type, e.action_taken, e.created_at, fm.sync_id AS focus_session_sync_id
            FROM app_events e
            LEFT JOIN sync_meta m ON m.entity_type = 'app_event' AND m.local_id = e.id
            LEFT JOIN sync_meta fm ON fm.entity_type = 'focus_session' AND fm.local_id = e.session_id
            WHERE e.created_at >= ?
            ORDER BY e.id DESC
            LIMIT ?
        """,
        "sql_params": "app_event_window",
        "build": lambda r: {
            "package_name": r["process_name"],
            "app_name": r["window_title"],
            "event_type": r["event_type"],
            "action": r["action_taken"],
            "created_at": r["created_at"],
            "focus_session_sync_id": r["focus_session_sync_id"],
        },
        "write_map": {},
    },
}

# 允许手机端新建实体的类型（其余类型只能改不能建，避免手机端造出桌面端无法识别的数据）
INSERTABLE = {"checklist_task", "today_plan_item", "daily_review", "weekly_review"}
DELETABLE = {"checklist_task", "today_plan_item", "daily_review", "weekly_review"}


class LocalStore:
    def __init__(self, db_path, logger, allow_write=True, allow_delete=True):
        self.db_path = db_path
        self.logger = logger
        self.allow_write = allow_write
        self.allow_delete = allow_delete
        self.read_conn = None
        self.write_conn = None
        self.subject_cache = {}

    # ---------------------------------------------------------------- 连接

    def open(self):
        self.read_conn = sqlite3.connect(
            "file:%s?mode=ro" % self.db_path.replace("\\", "/"), uri=True, timeout=10.0
        )
        self.read_conn.row_factory = sqlite3.Row
        self.read_conn.execute("PRAGMA busy_timeout = 10000")

        if self.allow_write:
            self.write_conn = sqlite3.connect(self.db_path, timeout=10.0)
            self.write_conn.row_factory = sqlite3.Row
            self.write_conn.execute("PRAGMA busy_timeout = 10000")
        return self

    def close(self):
        for conn in (self.read_conn, self.write_conn):
            if conn is not None:
                try:
                    conn.close()
                except sqlite3.Error:
                    pass

    # ---------------------------------------------------------------- 读取

    def _query_rows(self, entity_type, app_event_days=3, app_event_max_rows=500):
        spec = ENTITY_SPECS[entity_type]
        params = ()
        if spec.get("sql_params") == "app_event_window":
            cutoff = (datetime.now(timezone.utc) - timedelta(days=app_event_days)).isoformat()
            params = (cutoff, app_event_max_rows)
        cursor = self.read_conn.execute(spec["sql"], params)
        return [dict(row) for row in cursor.fetchall()]

    def _ensure_sync_ids(self, entity_type, rows, mint_guard=None):
        """本地行还没有 sync_id 时补发一个，并写进桌面端的 sync_meta（与桌面端自带同步共用一套 ID）。

        mint_guard 为假值的行会被跳过：这些行不该进同步通道（见 subject 的说明）。
        """
        missing = [row for row in rows if not row.get("sync_id")]
        if mint_guard is not None:
            missing = [row for row in missing if mint_guard(row)]
        if not missing or self.write_conn is None:
            return
        now = now_ms()
        for row in missing:
            sync_id = "%s-%s" % (entity_type, uuid.uuid4().hex[:12])
            try:
                self.write_conn.execute(
                    "INSERT OR REPLACE INTO sync_meta (entity_type, local_id, sync_id, deleted_at, created_at, updated_at)"
                    " VALUES (?, ?, ?, NULL, ?, ?)",
                    (entity_type, row["local_id"], sync_id, now, now),
                )
            except sqlite3.Error as error:
                self.logger("warn", "写入 sync_id 失败: %s" % error)
                continue
            row["sync_id"] = sync_id
        if missing:
            self.write_conn.commit()
            self.logger("info", "为 %s 补发 %d 个 sync_id" % (entity_type, len(missing)))

    def scan(self, entity_types, app_event_days=3, app_event_max_rows=500):
        """扫描本地库，返回 {entity_type: {sync_id: {hash, updatedAt, fields}}}"""
        snapshot = {}
        for entity_type in entity_types:
            spec = ENTITY_SPECS.get(entity_type)
            if spec is None:
                continue
            try:
                rows = self._query_rows(entity_type, app_event_days, app_event_max_rows)
            except sqlite3.Error as error:
                self.logger("error", "扫描 %s 失败: %s" % (entity_type, error))
                continue

            self._ensure_sync_ids(spec["meta_type"], rows, spec.get("mint_guard"))
            bucket = {}
            for row in rows:
                if not row.get("sync_id"):
                    continue
                fields = spec["build"](row)
                raw_time = row.get("updated_at")
                if raw_time is None:
                    raw_time = row.get("created_at")
                updated_at = parse_time_ms(raw_time) or now_ms()
                bucket[row["sync_id"]] = {
                    "hash": entity_hash(entity_type, row["sync_id"], fields, None),
                    "updatedAt": updated_at,
                    "fields": fields,
                }
            snapshot[entity_type] = bucket
        return snapshot

    # ---------------------------------------------------------------- 写回

    def resolve_local_id(self, meta_type, sync_id):
        row = self.read_conn.execute(
            "SELECT local_id FROM sync_meta WHERE entity_type = ? AND sync_id = ?",
            (meta_type, sync_id),
        ).fetchone()
        return row["local_id"] if row else None

    def resolve_subject_id(self, sync_id):
        if not sync_id:
            return None
        if sync_id in self.subject_cache:
            return self.subject_cache[sync_id]
        local_id = self.resolve_local_id(META_SUBJECT, sync_id)
        self.subject_cache[sync_id] = local_id
        return local_id

    def default_column_id(self, board_scope):
        row = self.read_conn.execute(
            "SELECT id FROM checklist_columns WHERE board_scope = ? ORDER BY sort_order, id LIMIT 1",
            (board_scope,),
        ).fetchone()
        if row is None:
            row = self.read_conn.execute(
                "SELECT id FROM checklist_columns ORDER BY id LIMIT 1"
            ).fetchone()
        return row["id"] if row else None

    def apply_remote(self, op):
        """把服务端广播的 op 写回本地库。返回 (结果, 说明)。"""
        entity_type = op.get("entityType")
        sync_id = op.get("syncId")
        fields = op.get("fields") or {}
        spec = ENTITY_SPECS.get(entity_type)

        if spec is None:
            return "skipped", "未知实体类型"
        if not self.allow_write or self.write_conn is None:
            return "skipped", "本地写入已关闭"

        meta_type = spec["meta_type"]
        try:
            local_id = self.resolve_local_id(meta_type, sync_id)

            if op.get("deletedAt"):
                return self._apply_delete(spec, entity_type, meta_type, local_id, sync_id)

            if local_id is None:
                if entity_type not in INSERTABLE:
                    return "skipped", "该类型不允许手机端新建"
                return self._apply_insert(spec, entity_type, meta_type, sync_id, fields)

            return self._apply_update(spec, entity_type, meta_type, local_id, sync_id, fields)
        except sqlite3.Error as error:
            return "error", "SQLite 错误: %s" % error

    def _apply_update(self, spec, entity_type, meta_type, local_id, sync_id, fields):
        write_map = spec.get("write_map") or {}
        if not write_map:
            return "skipped", "该类型本地不接受远程写入"

        assignments = []
        for field, (column, kind) in write_map.items():
            if field not in fields:
                continue
            value = fields[field]
            if kind == "bool":
                value = 1 if value else 0
            elif kind == "int":
                value = _int(value)
            elif kind == "subject":
                value = self.resolve_subject_id(value)
                if value is None:
                    continue
            assignments.append((column, value))

        if not assignments:
            return "skipped", "没有可写字段"

        columns = [column for column, _ in assignments]
        wanted = [value for _, value in assignments]
        current = self.read_conn.execute(
            "SELECT %s FROM %s WHERE id = ?" % (", ".join(columns), spec["table"]),
            (local_id,),
        ).fetchone()
        if current is not None and [current[column] for column in columns] == wanted:
            # 内容已经一致：直接返回，不碰数据库。
            # （每次拉增量都会重放全部 op，这里是最热的路径，一条 commit 都不该有）
            return "unchanged", "本地已是目标值"

        sets = ["%s = ?" % column for column in columns] + ["updated_at = ?"]
        values = wanted + [now_rfc3339(), local_id]
        self.write_conn.execute(
            "UPDATE %s SET %s WHERE id = ?" % (spec["table"], ", ".join(sets)),
            tuple(values),
        )
        self._touch_meta(meta_type, local_id)
        self.write_conn.commit()
        return "applied", "已更新 %s" % entity_type

    def _apply_delete(self, spec, entity_type, meta_type, local_id, sync_id):
        if entity_type not in DELETABLE:
            return "skipped", "该类型不允许删除"
        if local_id is None:
            return "skipped", "本地不存在该实体"
        if not self.allow_delete:
            return "skipped", "本地删除已关闭"
        self.write_conn.execute("DELETE FROM %s WHERE id = ?" % spec["table"], (local_id,))
        self.write_conn.execute(
            "UPDATE sync_meta SET deleted_at = ?, updated_at = ? WHERE entity_type = ? AND local_id = ?",
            (now_ms(), now_ms(), meta_type, local_id),
        )
        self.write_conn.commit()
        return "applied", "已删除 %s" % entity_type

    def _apply_insert(self, spec, entity_type, meta_type, sync_id, fields):
        now_text = now_rfc3339()
        now_epoch = now_ms()
        if entity_type == "checklist_task":
            title = (fields.get("title") or "").strip()
            if not title:
                return "skipped", "标题为空"
            board_scope = fields.get("board_scope") or "today"
            column_id = self.default_column_id(board_scope)
            if column_id is None:
                return "skipped", "没有可用的清单列"
            cursor = self.write_conn.execute(
                "INSERT INTO checklist_tasks (board_scope, subject_id, column_id, title, note, due_date,"
                " sort_order, completed, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                (
                    board_scope,
                    self.resolve_subject_id(fields.get("subject_sync_id")),
                    column_id,
                    title,
                    fields.get("note"),
                    fields.get("due_date"),
                    _int(fields.get("sort_order")),
                    1 if fields.get("completed") else 0,
                    now_text,
                    now_text,
                ),
            )
        elif entity_type == "today_plan_item":
            title = (fields.get("title") or "").strip()
            if not title:
                return "skipped", "标题为空"
            today_date = fields.get("today_date") or datetime.now().strftime("%Y-%m-%d")
            cursor = self.write_conn.execute(
                "INSERT INTO today_plan_items (today_date, source_task_id, subject_id, title, note, due_date,"
                " sort_order, completed, synced_source_completion, created_at, updated_at)"
                " VALUES (?, NULL, ?, ?, ?, ?, ?, ?, 0, ?, ?)",
                (
                    today_date,
                    self.resolve_subject_id(fields.get("subject_sync_id")),
                    title,
                    fields.get("note"),
                    fields.get("due_date"),
                    _int(fields.get("sort_order")),
                    1 if fields.get("completed") else 0,
                    now_text,
                    now_text,
                ),
            )
        elif entity_type == "daily_review":
            review_date = fields.get("review_date")
            if not review_date:
                return "skipped", "缺少 review_date"
            cursor = self.write_conn.execute(
                "INSERT OR IGNORE INTO daily_reviews (review_date, summary, blockers, tomorrow_focus,"
                " mood_score, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?)",
                (
                    review_date,
                    fields.get("summary"),
                    fields.get("blockers"),
                    fields.get("tomorrow_focus"),
                    _int(fields.get("mood_score"), 3),
                    now_text,
                    now_text,
                ),
            )
            if cursor.lastrowid in (0, None):
                # 同一天已有复盘，改为更新
                local_id = self.read_conn.execute(
                    "SELECT id FROM daily_reviews WHERE review_date = ?", (review_date,)
                ).fetchone()
                if local_id is None:
                    return "skipped", "复盘已存在但无法定位"
                self.write_conn.execute(
                    "UPDATE daily_reviews SET summary = ?, blockers = ?, tomorrow_focus = ?, mood_score = ?,"
                    " updated_at = ? WHERE id = ?",
                    (
                        fields.get("summary"),
                        fields.get("blockers"),
                        fields.get("tomorrow_focus"),
                        _int(fields.get("mood_score"), 3),
                        now_text,
                        local_id["id"],
                    ),
                )
                self._touch_meta(meta_type, local_id["id"])
                self._bind_sync_id(meta_type, local_id["id"], sync_id)
                self.write_conn.commit()
                return "applied", "已更新同日复盘"
        elif entity_type == "weekly_review":
            week_start_date = fields.get("week_start_date")
            if not week_start_date:
                return "skipped", "缺少 week_start_date"
            cursor = self.write_conn.execute(
                "INSERT OR IGNORE INTO weekly_reviews (week_start_date, summary, blockers, next_week_focus,"
                " mood_score, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?)",
                (
                    week_start_date,
                    fields.get("summary"),
                    fields.get("blockers"),
                    fields.get("next_week_focus"),
                    _int(fields.get("mood_score"), 3),
                    now_text,
                    now_text,
                ),
            )
        else:
            return "skipped", "该类型不支持新建"

        local_id = cursor.lastrowid
        self._bind_sync_id(meta_type, local_id, sync_id)
        self.write_conn.commit()
        return "applied", "已新建 %s" % entity_type

    def _bind_sync_id(self, meta_type, local_id, sync_id):
        self.write_conn.execute(
            "INSERT OR REPLACE INTO sync_meta (entity_type, local_id, sync_id, deleted_at, created_at, updated_at)"
            " VALUES (?, ?, ?, NULL, ?, ?)",
            (meta_type, local_id, sync_id, now_ms(), now_ms()),
        )

    def _touch_meta(self, meta_type, local_id):
        self.write_conn.execute(
            "UPDATE sync_meta SET updated_at = ? WHERE entity_type = ? AND local_id = ?",
            (now_ms(), meta_type, local_id),
        )

    def sync_id_set(self, meta_type, include_deleted=True):
        """本地 sync_meta 里登记过的全部 sync_id（含墓碑）。

        用于孤儿清理：服务端有、本地 sync_meta 里查不到的实体 = 本地已经不认识它了。
        含墓碑很重要，否则被正常删除的实体反而会被当成孤儿。
        """
        sql = "SELECT sync_id FROM sync_meta WHERE entity_type = ?"
        if not include_deleted:
            sql += " AND deleted_at IS NULL"
        return {row[0] for row in self.read_conn.execute(sql, (meta_type,)) if row[0]}


def default_db_path():
    """自动定位桌面端数据库：%APPDATA%/com.kaoyan.focus/kaoyan-focus.sqlite3"""
    appdata = os.environ.get("APPDATA") or os.path.expanduser("~")
    return os.path.join(appdata, "com.kaoyan.focus", "kaoyan-focus.sqlite3")
