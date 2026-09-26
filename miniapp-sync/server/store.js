'use strict';

const fs = require('fs');
const path = require('path');

const protocol = require('./protocol');
const schema = require('./schema');

/**
 * 服务端状态存储。
 *
 * 存储分两层：
 *   - state.json ：全量快照（实体当前状态 + 历史版本 + 冲突列表 + 服务端版本号）
 *   - oplog.jsonl：追加式操作日志，供离线客户端按 cursor 增量回放
 * 数据量级很小（几千条），用文件存储换零依赖部署；写快照做去抖合并，写 oplog 用 append。
 */
class Store {
  constructor(config) {
    this.config = config;
    this.snapshotPath = path.join(config.storage.dataDir, config.storage.snapshotFile);
    this.oplogPath = path.join(config.storage.dataDir, config.storage.oplogFile);
    this.flushTimer = null;
    this.dirty = false;

    this.state = {
      serverRev: 0,
      entities: {},
      conflicts: [],
      devices: {}
    };
    this.oplog = [];
    this.opIdIndex = new Map();
    this.tickets = new Map();
  }

  load() {
    fs.mkdirSync(this.config.storage.dataDir, { recursive: true });

    if (fs.existsSync(this.snapshotPath)) {
      try {
        const parsed = JSON.parse(fs.readFileSync(this.snapshotPath, 'utf8'));
        this.state = {
          serverRev: Number(parsed.serverRev) || 0,
          entities: parsed.entities && typeof parsed.entities === 'object' ? parsed.entities : {},
          conflicts: Array.isArray(parsed.conflicts) ? parsed.conflicts : [],
          devices: parsed.devices && typeof parsed.devices === 'object' ? parsed.devices : {}
        };
      } catch (error) {
        console.error(`[store] 快照损坏，已备份并从空状态启动: ${error.message}`);
        const broken = `${this.snapshotPath}.broken-${Date.now()}`;
        fs.renameSync(this.snapshotPath, broken);
      }
    }

    if (fs.existsSync(this.oplogPath)) {
      const lines = fs.readFileSync(this.oplogPath, 'utf8').split('\n').filter(Boolean);
      const kept = lines.slice(-this.config.sync.oplogLimit);
      for (const line of kept) {
        try {
          this.oplog.push(JSON.parse(line));
        } catch (error) {
          // 单行损坏（进程被杀导致半行）时跳过，不能让整个服务起不来
        }
      }
      for (const entry of this.oplog) this.opIdIndex.set(entry.opId, entry.seq);
      if (this.oplog.length > 0) {
        this.state.serverRev = Math.max(this.state.serverRev, this.oplog[this.oplog.length - 1].seq);
      }
    }

    console.log(`[store] 载入完成: serverRev=${this.state.serverRev}, 实体类型=${Object.keys(this.state.entities).length}, oplog=${this.oplog.length}`);
  }

  /** 写入 ops，返回每条 op 的裁决结果和需要广播的条目 */
  applyOps(ops, context) {
    const results = [];
    const broadcast = [];

    for (const rawOp of ops) {
      const error = schema.validateOp(rawOp);
      if (error) {
        results.push({ opId: rawOp && rawOp.opId, status: 'invalid', reason: error });
        continue;
      }

      if (this.opIdIndex.has(rawOp.opId)) {
        results.push({ opId: rawOp.opId, status: 'duplicate', reason: 'opId 已处理' });
        continue;
      }

      const rule = schema.ruleOf(rawOp.entityType);

      const bucket = this.state.entities[rawOp.entityType] || (this.state.entities[rawOp.entityType] = {});
      const current = bucket[rawOp.syncId] || null;

      // mini 角色只能写白名单字段（见 schema.sanitizeFields）。**必须按字段合并，
      // 不能整体替换**：小程序只改了个标题，如果拿这份过滤后的字段覆盖整个实体，
      // 电脑端写入的其它字段（例如 checklist_task 的 column_name）就被抹掉了。
      // 后果是服务端版本与两端都不一致——摘要校验来回抖动，小程序白白拉全量，
      // 新设备拿到的快照还缺字段。PC 角色推的是完整字段集，不需要合并。
      const sanitized = schema.sanitizeFields(rawOp.entityType, rawOp.role, rawOp.fields);
      const fields = rawOp.role === 'pc' || !current
        ? sanitized
        : { ...current.fields, ...sanitized };

      const deletedAt = rawOp.deletedAt || null;
      const updatedAt = Number.isFinite(rawOp.updatedAt)
        ? rawOp.updatedAt
        : (deletedAt || Date.now());

      // hash 由合并后的 fields 算出，保证「存下来的实体」和「记录的指纹」始终一致
      const op = { ...rawOp, fields, updatedAt, deletedAt };
      const decision = protocol.resolveConflict(current, op, rule);

      if (decision.status === 'rejected') {
        results.push({
          opId: rawOp.opId, status: 'rejected', reason: decision.reason,
          entityType: rawOp.entityType, syncId: rawOp.syncId
        });
        this.rememberOpId(rawOp.opId);
        continue;
      }

      if (decision.status === 'duplicate') {
        results.push({
          opId: rawOp.opId, status: 'duplicate', reason: decision.reason,
          entityType: rawOp.entityType, syncId: rawOp.syncId, rev: current.rev
        });
        this.rememberOpId(rawOp.opId);
        continue;
      }

      if (decision.status === 'conflict') {
        // 落败：保留服务端版本，把这次落败记录进冲突列表，供小程序端提示
        this.recordConflict(rawOp, current, decision);
        results.push({
          opId: rawOp.opId, status: 'conflict', reason: decision.reason,
          entityType: rawOp.entityType, syncId: rawOp.syncId,
          rev: current.rev, winnerFields: current.fields, winnerUpdatedAt: current.updatedAt
        });
        this.rememberOpId(rawOp.opId);
        continue;
      }

      // 接受：版本号递增、保留历史、写 oplog、准备广播
      this.state.serverRev += 1;
      const rev = this.state.serverRev;
      const history = current && Array.isArray(current.history) ? current.history.slice() : [];
      if (current) {
        history.push({
          rev: current.rev,
          fields: current.fields,
          updatedAt: current.updatedAt,
          deviceId: current.originDeviceId,
          role: current.originRole
        });
      }
      while (history.length > this.config.sync.historyPerEntity) history.shift();

      const entity = {
        rev,
        updatedAt,
        deletedAt,
        hash: decision.hash,
        fields,
        originDeviceId: rawOp.deviceId,
        originRole: rawOp.role,
        history
      };
      bucket[rawOp.syncId] = entity;

      const entry = {
        seq: rev,
        opId: rawOp.opId,
        deviceId: rawOp.deviceId,
        role: rawOp.role,
        entityType: rawOp.entityType,
        syncId: rawOp.syncId,
        fields,
        updatedAt,
        deletedAt,
        hash: decision.hash,
        rev,
        ts: Date.now()
      };
      this.oplog.push(entry);
      this.rememberOpId(rawOp.opId);
      broadcast.push(entry);
      results.push({
        opId: rawOp.opId, status: 'applied', reason: decision.reason,
        entityType: rawOp.entityType, syncId: rawOp.syncId, rev
      });
    }

    this.trimOplog();
    this.appendOplog(broadcast);
    this.touchDevice(context);
    this.scheduleFlush();
    return { results, broadcast, serverRev: this.state.serverRev };
  }

  rememberOpId(opId) {
    this.opIdIndex.set(opId, this.state.serverRev);
    // 去重表只保留最近的记录，防止无限增长
    if (this.opIdIndex.size > 20000) {
      const oldest = this.opIdIndex.keys().next().value;
      this.opIdIndex.delete(oldest);
    }
  }

  recordConflict(op, current, decision) {
    const entry = {
      id: `${Date.now()}-${Math.random().toString(16).slice(2, 8)}`,
      entityType: op.entityType,
      syncId: op.syncId,
      resolution: 'keep-server',
      reason: decision.reason,
      loserDeviceId: op.deviceId,
      loserRole: op.role,
      loserFields: op.fields,
      loserUpdatedAt: op.updatedAt || null,
      winnerFields: current.fields,
      winnerUpdatedAt: current.updatedAt,
      createdAt: Date.now()
    };
    this.state.conflicts.push(entry);
    while (this.state.conflicts.length > this.config.sync.conflictsLimit) this.state.conflicts.shift();
  }

  touchDevice(context) {
    if (!context || !context.deviceId) return;
    this.state.devices[context.deviceId] = {
      deviceId: context.deviceId,
      role: context.role || 'unknown',
      transport: context.transport || 'unknown',
      lastSeenAt: Date.now(),
      lastServerRev: this.state.serverRev
    };
  }

  trimOplog() {
    const limit = this.config.sync.oplogLimit;
    if (this.oplog.length <= limit) return;
    const removed = this.oplog.splice(0, this.oplog.length - limit);
    for (const entry of removed) {
      if (this.opIdIndex.get(entry.opId) === entry.seq) this.opIdIndex.delete(entry.opId);
    }
    // oplog 文件同步裁剪，避免无限增长
    try {
      fs.writeFileSync(this.oplogPath, this.oplog.map((e) => JSON.stringify(e)).join('\n') + '\n');
    } catch (error) {
      console.error(`[store] 裁剪 oplog 失败: ${error.message}`);
    }
  }

  oldestSeq() {
    return this.oplog.length > 0 ? this.oplog[0].seq : this.state.serverRev;
  }

  /** 增量拉取；cursor 早于 oplog 起始说明日志已裁剪，客户端必须全量同步 */
  deltaSince(cursor, maxBatch) {
    const batch = Number.isFinite(maxBatch) && maxBatch > 0
      ? Math.min(Math.floor(maxBatch), this.config.sync.maxDeltaBatch)
      : this.config.sync.maxDeltaBatch;
    const oldest = this.oldestSeq();
    if (Number.isFinite(cursor) && cursor > this.state.serverRev) {
      return { stale: false, ops: [], serverRev: this.state.serverRev, ahead: true };
    }
    if (!Number.isFinite(cursor) || cursor < oldest - 1) {
      return { stale: true, ops: [], serverRev: this.state.serverRev, oldestSeq: oldest };
    }
    const pending = this.oplog.filter((entry) => entry.seq > cursor);
    const ops = pending.slice(0, batch);
    return {
      stale: false,
      ops: ops,
      serverRev: this.state.serverRev,
      // 告诉客户端：按最后一条 op 的 seq 继续拉，不要直接跳到 serverRev，
      // 否则中间那段 op 会被永久跳过
      nextCursor: ops.length > 0 ? ops[ops.length - 1].seq : cursor,
      pending: pending.length,
      truncated: pending.length > ops.length
    };
  }

  /** 全部实体键的稳定排序索引，用于分页下发 */
  entityIndex() {
    const index = [];
    for (const entityType of Object.keys(this.state.entities).sort()) {
      const bucket = this.state.entities[entityType];
      for (const syncId of Object.keys(bucket).sort()) {
        index.push([entityType, syncId]);
      }
    }
    return index;
  }

  /**
   * 全量快照，支持分页。
   *
   * 分页存在的意义：弱网/校园网环境里超大的单次响应更容易被中途掐断，
   * 而分页后每次只用传几百 KB，断了也只需重试当前页。
   * limit 为 0 或缺省时返回完整快照（兼容小程序旧行为）。
   */
  snapshot(options) {
    const opts = options || {};
    const limit = Number.isFinite(opts.limit) && opts.limit > 0 ? Math.floor(opts.limit) : 0;
    const offset = Number.isFinite(opts.offset) && opts.offset > 0 ? Math.floor(opts.offset) : 0;

    if (limit === 0) {
      const total = this.entityIndex().length;
      return {
        serverRev: this.state.serverRev,
        entities: this.state.entities,
        total: total,
        offset: 0,
        limit: 0,
        hasMore: false,
        generatedAt: Date.now()
      };
    }

    const index = this.entityIndex();
    const slice = index.slice(offset, offset + limit);
    const entities = {};
    for (const [entityType, syncId] of slice) {
      if (!entities[entityType]) entities[entityType] = {};
      entities[entityType][syncId] = this.state.entities[entityType][syncId];
    }
    return {
      serverRev: this.state.serverRev,
      entities: entities,
      total: index.length,
      offset: offset,
      limit: limit,
      hasMore: offset + slice.length < index.length,
      generatedAt: Date.now()
    };
  }

  /**
   * 全量摘要（一致性校验用）。scope 为空表示全部实体类型；
   * 传 scope 时只统计指定类型——PC 端按时间窗口同步干扰事件，
   * 若把 app_event 也算进摘要会永远对不上，所以三端约定按同一 scope 比对。
   *
   * **只统计存活实体**：墓碑两端各自记的是各自的删除时刻，天然不可能相同，
   * 把它们纳入摘要会让三端永远对不上，从而每 5 分钟误触发一次无意义的强制全量重推。
   * 一致性校验真正关心的是「活着的实体是否一致」；某条记录一方删了另一方没删，
   * 表现就是存活集合多一条，摘要照样能发现。
   */
  digest(scope) {
    const types = Array.isArray(scope) && scope.length > 0
      ? scope.filter((type) => schema.isEntityAllowed(type))
      : Object.keys(this.state.entities);

    const target = {};
    const counts = {};
    for (const type of types) {
      const bucket = this.state.entities[type];
      if (!bucket) continue;
      const alive = {};
      for (const syncId of Object.keys(bucket)) {
        if (bucket[syncId].deletedAt) continue;
        alive[syncId] = bucket[syncId];
      }
      target[type] = alive;
      counts[type] = Object.keys(alive).length;
    }
    return { digest: protocol.digestOf(target), serverRev: this.state.serverRev, counts, scope: types };
  }

  entity(entityType, syncId) {
    const bucket = this.state.entities[entityType];
    if (!bucket || !bucket[syncId]) return null;
    return bucket[syncId];
  }

  conflicts() {
    return this.state.conflicts.slice(-50);
  }

  reset() {
    this.state = { serverRev: 0, entities: {}, conflicts: [], devices: {} };
    this.oplog = [];
    this.opIdIndex.clear();
    try {
      fs.writeFileSync(this.oplogPath, '');
    } catch (error) {
      console.error(`[store] 清空 oplog 失败: ${error.message}`);
    }
    this.scheduleFlush();
  }

  scheduleFlush() {
    this.dirty = true;
    if (this.flushTimer) return;
    const wait = this.config.storage.flushDebounceMs;
    this.flushTimer = setTimeout(() => {
      this.flushTimer = null;
      if (!this.dirty) return;
      this.flush();
    }, wait);
    if (typeof this.flushTimer.unref === 'function') this.flushTimer.unref();
  }

  flush() {
    try {
      const tmp = `${this.snapshotPath}.tmp`;
      fs.writeFileSync(tmp, JSON.stringify(this.state));
      fs.renameSync(tmp, this.snapshotPath);
      this.dirty = false;
    } catch (error) {
      console.error(`[store] 落盘失败: ${error.message}`);
    }
  }

  /** oplog 追加写：立即 append，保证重启后增量日志不丢 */
  appendOplog(entries) {
    if (entries.length === 0) return;
    const payload = entries.map((entry) => `${JSON.stringify(entry)}\n`).join('');
    try {
      fs.appendFileSync(this.oplogPath, payload);
    } catch (error) {
      console.error(`[store] 追加 oplog 失败: ${error.message}`);
    }
  }
}

module.exports = Store;
