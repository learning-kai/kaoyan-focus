'use strict';

/**
 * 三端共用的同步协议核心：canonical JSON、实体指纹、全局摘要、冲突仲裁。
 *
 * 这个文件里的算法必须在三处保持逐字一致：
 *   - server/protocol.js（本文件）
 *   - agent/protocol.py（PC 端 Python 实现）
 *   - miniapp/services/protocol.js（小程序端实现）
 * 任一端算出的 hash / digest 不一致，一致性校验就会误报，改动时必须三处同步。
 *
 * 之所以不用 SHA-1：小程序端没有同步的 crypto 能力，而这里只需要"快速发现漂移"，
 * 不需要抗碰撞，所以用无依赖的 FNV-1a-64（两个 32 位通道拼接，避免 BigInt 兼容问题）。
 */

const FNV_PRIME = 0x01000193;
const BASIS_A = 0x811c9dc5;
const BASIS_B = 0x84f6a1b3;

function fnv1a32(codePoints, basis) {
  let h = basis >>> 0;
  for (let i = 0; i < codePoints.length; i += 1) {
    const cp = codePoints[i];
    // 每个码位喂两个字节（低 8 位 + 次低 8 位），保证非 ASCII 内容也能参与散列
    const bytes = [cp & 0xff, (cp >>> 8) & 0xff];
    for (let j = 0; j < bytes.length; j += 1) {
      h = (h ^ bytes[j]) >>> 0;
      h = Math.imul(h, FNV_PRIME) >>> 0;
    }
  }
  return h >>> 0;
}

function hex8(value) {
  return (value >>> 0).toString(16).padStart(8, '0');
}

function fnv1a64Hex(input) {
  // 按 Unicode 码位拆分（Array.from 拆出来的是字符，必须再取 codePointAt），
  // 与 Python 的 [ord(ch) for ch in s] 保持一致
  const codePoints = Array.from(String(input)).map((ch) => ch.codePointAt(0));
  return hex8(fnv1a32(codePoints, BASIS_A)) + hex8(fnv1a32(codePoints, BASIS_B));
}

/** 归一化标量：让 number / bool / null 的序列化结果三端一致 */
function normalizeScalar(value) {
  if (value === null || value === undefined) return null;
  const type = typeof value;
  if (type === 'string') return value;
  if (type === 'boolean') return value;
  if (type === 'number') {
    if (!Number.isFinite(value)) return null;
    return Number.isInteger(value) ? value : Math.round(value * 1e6) / 1e6;
  }
  return null;
}

function canonicalize(value) {
  const scalar = normalizeScalar(value);
  if (scalar !== null) return scalar;
  if (typeof value === 'number' || typeof value === 'boolean') return scalar;

  if (Array.isArray(value)) {
    return value.map(canonicalize);
  }
  if (value && typeof value === 'object') {
    const out = {};
    for (const key of Object.keys(value).sort()) {
      out[key] = canonicalize(value[key]);
    }
    return out;
  }
  return null;
}

function canonicalString(value) {
  return JSON.stringify(canonicalize(value));
}

/** 单个实体的内容指纹：只覆盖业务字段，不覆盖 updatedAt，避免同步回环 */
function entityHash(entityType, syncId, fields, deletedAt) {
  return fnv1a64Hex(canonicalString({
    entityType,
    syncId,
    fields: fields || {},
    deletedAt: deletedAt === undefined ? null : deletedAt
  }));
}

/** 全量摘要：所有实体的 type|syncId|hash 排序拼接后再散列 */
function digestOf(entities) {
  const parts = [];
  for (const entityType of Object.keys(entities || {}).sort()) {
    const bucket = entities[entityType] || {};
    for (const syncId of Object.keys(bucket).sort()) {
      parts.push(`${entityType}|${syncId}|${bucket[syncId].hash}`);
    }
  }
  return fnv1a64Hex(parts.join('\n'));
}

/**
 * 冲突仲裁。
 * @param {object|null} current 服务端当前实体状态
 * @param {object} op 传入的 op（fields 已按角色过滤）
 * @param {object|null} rule schema.js 里的实体规则
 * @returns {{status:string, reason:string, hash:string, winner?:string}}
 *   status: 'applied' 接受 | 'conflict' 落败（记入冲突列表并提示）| 'rejected' 越权拒绝 | 'duplicate' 幂等丢弃
 */
function resolveConflict(current, op, rule) {
  const hash = entityHash(op.entityType, op.syncId, op.fields, op.deletedAt || null);

  // 1) 权限优先于一切：PC 权威实体（学习模式、专注记录、干扰事件）
  //    不接受小程序写入，哪怕 baseRev 命中也不能放行，否则手机端能篡改桌面端事实数据。
  if (rule && rule.authority === 'pc' && op.role !== 'pc') {
    return { status: 'rejected', reason: 'pc-authoritative', hash, winner: 'server' };
  }

  // 2) PC 端强制全量重同步：直接胜出，不走 LWW（本地 SQLite 是事实来源）
  if (op.force === true && op.role === 'pc') {
    return { status: 'applied', reason: 'force-resync', hash };
  }

  if (!current) {
    return { status: 'applied', reason: 'create', hash };
  }
  if (current.hash === hash) {
    return { status: 'duplicate', reason: 'identical', hash };
  }
  // 3) 单端连续修改：baseRev 命中说明没有并发，直接快进
  if (Number.isInteger(op.baseRev) && op.baseRev === current.rev) {
    return { status: 'applied', reason: 'fast-forward', hash };
  }

  const opTime = Number.isFinite(op.updatedAt) ? op.updatedAt : 0;
  const curTime = Number.isFinite(current.updatedAt) ? current.updatedAt : 0;

  if (opTime > curTime) return { status: 'applied', reason: 'lww-newer', hash };
  if (opTime < curTime) return { status: 'conflict', reason: 'lww-older', hash, winner: 'server' };

  // 时间戳完全相同：用指纹做确定性 tie-break，保证三端裁决结果一致
  if (hash > current.hash) return { status: 'applied', reason: 'tie-hash', hash };
  if (hash < current.hash) return { status: 'conflict', reason: 'tie-hash', hash, winner: 'server' };
  return { status: 'conflict', reason: 'tie-identical', hash, winner: 'server' };
}

module.exports = {
  fnv1a64Hex,
  canonicalize,
  canonicalString,
  entityHash,
  digestOf,
  resolveConflict,
  normalizeScalar
};
