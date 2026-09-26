'use strict';

/**
 * 实体规则：同步范围、权威方、小程序可写字段。
 *
 * authority:
 *   'pc'   —— PC 桌面端是权威方，小程序只能读（学习模式状态机、专注记录、干扰事件）
 *   'none' —— 双端都可写，冲突走 LWW（更新时间新者胜）
 *
 * miniWritable: 小程序允许写入的字段白名单。不在名单里的字段，
 * 即使小程序传了也会被服务端丢弃，避免手机端写出桌面端无法识别的状态。
 */
const ENTITY_RULES = {
  subject: {
    authority: 'pc',
    miniWritable: [],
    label: '科目'
  },
  checklist_task: {
    authority: 'none',
    miniWritable: ['title', 'note', 'due_date', 'completed', 'sort_order', 'board_scope', 'subject_sync_id'],
    label: '清单任务'
  },
  today_plan_item: {
    authority: 'none',
    miniWritable: ['title', 'note', 'due_date', 'completed', 'sort_order', 'subject_sync_id', 'today_date'],
    label: '今日任务'
  },
  schedule_block: {
    authority: 'pc',
    miniWritable: ['status'],
    label: '课表块'
  },
  schedule_template: {
    authority: 'pc',
    miniWritable: [],
    label: '课表模板'
  },
  daily_review: {
    authority: 'none',
    miniWritable: ['review_date', 'summary', 'blockers', 'tomorrow_focus', 'mood_score'],
    label: '每日复盘'
  },
  weekly_review: {
    authority: 'none',
    miniWritable: ['week_start_date', 'summary', 'blockers', 'next_week_focus', 'mood_score'],
    label: '周复盘'
  },
  focus_session: {
    authority: 'pc',
    miniWritable: [],
    label: '专注记录'
  },
  study_mode: {
    authority: 'pc',
    miniWritable: [],
    label: '学习模式'
  },
  app_event: {
    authority: 'pc',
    miniWritable: [],
    label: '干扰事件'
  }
};

/**
 * 明确不同步的表：settings 里有 WebDAV 密码、飞书 secret、SMTP 密码等凭据，
 * 绝对不能进同步通道。这里作为黑名单的兜底声明（PC 端 agent 也会再过滤一次）。
 */
const NEVER_SYNC_TABLES = ['settings', 'sync_runs', 'feishu_sync_runs', 'microsoft_sync_runs',
  'feishu_sync_links', 'calendar_sync_links', 'microsoft_sync_links', 'email_notification_logs', 'alarms'];

const FORBIDDEN_FIELD_PATTERN = /(password|passwd|secret|token|api[-_]?key|credential|smtp|webdav|private[-_]?key|access[-_]?token)/i;

const MAX_FIELD_KEY_LENGTH = 64;
const MAX_FIELD_VALUE_LENGTH = 8000;

function isEntityAllowed(entityType) {
  return Object.prototype.hasOwnProperty.call(ENTITY_RULES, entityType);
}

function ruleOf(entityType) {
  return ENTITY_RULES[entityType] || null;
}

/**
 * 按角色过滤字段：
 * - pc 角色：只做敏感字段过滤（PC 端自己已经在本地过滤过一次，这里是纵深防御）
 * - mini 角色：只允许 miniWritable 白名单，其余字段丢弃
 */
function sanitizeFields(entityType, role, fields) {
  const rule = ruleOf(entityType);
  const input = fields && typeof fields === 'object' && !Array.isArray(fields) ? fields : {};
  const output = {};

  for (const rawKey of Object.keys(input)) {
    if (typeof rawKey !== 'string' || rawKey.length === 0 || rawKey.length > MAX_FIELD_KEY_LENGTH) continue;
    if (FORBIDDEN_FIELD_PATTERN.test(rawKey)) continue;

    if (role !== 'pc' && rule && Array.isArray(rule.miniWritable) && rule.miniWritable.length >= 0) {
      if (!rule.miniWritable.includes(rawKey)) continue;
    }

    const value = normalizeValue(input[rawKey]);
    if (value === undefined) continue;
    const size = typeof value === 'string' ? value.length : JSON.stringify(value).length;
    if (size > MAX_FIELD_VALUE_LENGTH) continue;
    output[rawKey] = value;
  }

  return output;
}

/** 归一化值类型，保证 JSON 序列化结果在 Node / Python / 小程序三端一致 */
function normalizeValue(value) {
  if (value === null || value === undefined) return null;
  const type = typeof value;
  if (type === 'string') return value;
  if (type === 'boolean') return value;
  if (type === 'number') {
    if (!Number.isFinite(value)) return null;
    return Number.isInteger(value) ? value : Math.round(value * 1e6) / 1e6;
  }
  if (Array.isArray(value)) return value.map(normalizeValue).filter((v) => v !== null);
  if (type === 'object') {
    const out = {};
    for (const key of Object.keys(value).sort()) {
      const v = normalizeValue(value[key]);
      if (v !== undefined) out[key] = v;
    }
    return out;
  }
  return null;
}

/** 校验一条 op 的基本形状，返回错误字符串或 null */
function validateOp(op) {
  if (!op || typeof op !== 'object') return 'op 不是对象';
  if (typeof op.entityType !== 'string' || !op.entityType) return '缺少 entityType';
  if (!isEntityAllowed(op.entityType)) return `entityType 不在同步范围: ${op.entityType}`;
  if (typeof op.syncId !== 'string' || !op.syncId || op.syncId.length > 128) return 'syncId 非法';
  if (typeof op.opId !== 'string' || !op.opId || op.opId.length > 64) return 'opId 非法';
  if (typeof op.deviceId !== 'string' || !op.deviceId || op.deviceId.length > 64) return 'deviceId 非法';
  if (op.role !== 'pc' && op.role !== 'mini') return 'role 必须是 pc 或 mini';
  if (op.updatedAt !== undefined && op.updatedAt !== null && !Number.isFinite(op.updatedAt)) return 'updatedAt 非法';
  if (op.deletedAt !== undefined && op.deletedAt !== null && !Number.isFinite(op.deletedAt)) return 'deletedAt 非法';
  if (op.baseRev !== undefined && op.baseRev !== null && !Number.isInteger(op.baseRev)) return 'baseRev 非法';
  if (op.fields !== undefined && (typeof op.fields !== 'object' || Array.isArray(op.fields))) return 'fields 必须是对象';
  return null;
}

module.exports = {
  ENTITY_RULES,
  NEVER_SYNC_TABLES,
  FORBIDDEN_FIELD_PATTERN,
  isEntityAllowed,
  ruleOf,
  sanitizeFields,
  normalizeValue,
  validateOp
};
