"""三端共用的同步协议核心（PC 端 Python 实现）。

算法必须与下面两处逐字保持一致，否则一致性校验会误报：
  - server/protocol.js
  - miniapp/services/protocol.js

用 FNV-1a-64（两个 32 位通道拼接）代替 SHA-1：小程序端没有同步 crypto 能力，
而这里只需要快速发现数据漂移，不需要抗碰撞。
"""

import json
import re

FNV_PRIME = 0x01000193
BASIS_A = 0x811C9DC5
BASIS_B = 0x84F6A1B3
UINT32 = 0xFFFFFFFF


def _fnv1a32(code_points, basis):
    h = basis & UINT32
    for cp in code_points:
        for byte in (cp & 0xFF, (cp >> 8) & 0xFF):
            h ^= byte
            h = (h * FNV_PRIME) & UINT32
    return h & UINT32


def fnv1a64_hex(value):
    code_points = [ord(ch) for ch in str(value)]
    return "%08x%08x" % (_fnv1a32(code_points, BASIS_A), _fnv1a32(code_points, BASIS_B))


def _normalize_scalar(value):
    if value is None:
        return None
    if isinstance(value, bool):
        return value
    if isinstance(value, str):
        return value
    if isinstance(value, int):
        return value
    if isinstance(value, float):
        if value != value or value in (float("inf"), float("-inf")):
            return None
        rounded = round(value, 6)
        return int(rounded) if float(rounded).is_integer() else rounded
    return None


def canonicalize(value):
    scalar = _normalize_scalar(value)
    if scalar is not None:
        return scalar
    if isinstance(value, (int, float)):
        return 0
    if isinstance(value, list):
        return [canonicalize(item) for item in value]
    if isinstance(value, dict):
        return {key: canonicalize(value[key]) for key in sorted(value.keys())}
    return None


def canonical_string(value):
    return json.dumps(canonicalize(value), ensure_ascii=False, separators=(",", ":"))


def entity_hash(entity_type, sync_id, fields, deleted_at=None):
    return fnv1a64_hex(canonical_string({
        "entityType": entity_type,
        "syncId": sync_id,
        "fields": fields or {},
        "deletedAt": deleted_at if deleted_at is not None else None,
    }))


def digest_of(entities):
    """entities: {entity_type: {sync_id: hash}}"""
    parts = []
    for entity_type in sorted(entities.keys()):
        bucket = entities.get(entity_type) or {}
        for sync_id in sorted(bucket.keys()):
            parts.append("%s|%s|%s" % (entity_type, sync_id, bucket[sync_id]))
    return fnv1a64_hex("\n".join(parts))


# ---------------------------------------------------------------- 时间处理

_FRACTION_RE = re.compile(r"(?<=\.)(\d+)")


def parse_time_ms(value):
    """把 SQLite 里的多种时间格式统一成毫秒。

    桌面端写入的是 RFC3339，且带 9 位纳秒小数（Python 的 fromisoformat 只吃 6 位），
    所以这里先截断小数位再解析。
    """
    if value is None:
        return None
    if isinstance(value, (int, float)):
        return int(value)
    text = str(value).strip()
    if not text:
        return None
    text = text.replace("Z", "+00:00")
    match = _FRACTION_RE.search(text)
    if match and len(match.group(1)) > 6:
        text = text[:match.start(1)] + match.group(1)[:6] + text[match.end(1):]
    try:
        from datetime import datetime, timezone
        parsed = datetime.fromisoformat(text)
        if parsed.tzinfo is None:
            parsed = parsed.replace(tzinfo=timezone.utc)
        return int(parsed.timestamp() * 1000)
    except ValueError:
        return None


def now_ms():
    import time
    return int(time.time() * 1000)


def now_rfc3339():
    from datetime import datetime, timezone
    return datetime.now(timezone.utc).isoformat()
