'use strict';

const http = require('http');
const crypto = require('crypto');
const { URL } = require('url');
const WebSocket = require('ws');

const config = require('./config');
const schema = require('./schema');
const protocol = require('./protocol');
const Store = require('./store');

const store = new Store(config);
store.load();

/** 在线客户端：{ kind, send, close, deviceId, role, lastSeenAt } */
const clients = new Set();
const rateBuckets = new Map();

const LEVELS = { debug: 10, info: 20, warn: 30, error: 40 };
const currentLevel = LEVELS[config.logging.level] || LEVELS.info;

function log(level, message, extra) {
  if ((LEVELS[level] || 20) < currentLevel) return;
  const time = new Date().toISOString();
  const suffix = extra === undefined ? '' : ` ${JSON.stringify(extra)}`;
  console.log(`${time} [${level}] ${message}${suffix}`);
}

/* ----------------------------- 鉴权 ----------------------------- */

function readToken(req, url) {
  const header = String(req.headers.authorization || '');
  const bearer = header.toLowerCase().startsWith('bearer ') ? header.slice(7).trim() : '';
  const query = url.searchParams.get('token') || '';
  return { bearer, query };
}

function ticketOk(ticket) {
  const record = store.tickets.get(ticket);
  if (!record) return false;
  if (record.expiresAt < Date.now()) {
    store.tickets.delete(ticket);
    return false;
  }
  return true;
}

function authorize(req, url) {
  const { bearer, query } = readToken(req, url);
  if (bearer && bearer === config.auth.token) return true;
  if (config.auth.allowQueryToken && query && (query === config.auth.token || ticketOk(query))) return true;
  return false;
}

function issueTicket(deviceId, role, ttlMs) {
  const ticket = `tk_${crypto.randomBytes(16).toString('hex')}`;
  const ttl = Number.isFinite(ttlMs) && ttlMs > 0 ? ttlMs : config.auth.ticketTtlMs;
  store.tickets.set(ticket, { deviceId, role, expiresAt: Date.now() + ttl });
  return { ticket, expiresAt: Date.now() + ttl };
}

/* ----------------------------- HTTP 基础工具 ----------------------------- */

function sendJson(res, status, payload) {
  const body = JSON.stringify(payload);
  res.writeHead(status, {
    'Content-Type': 'application/json; charset=utf-8',
    'Content-Length': Buffer.byteLength(body),
    'Cache-Control': 'no-store'
  });
  res.end(body);
}

function readJsonBody(req) {
  return new Promise((resolve, reject) => {
    const limit = config.server.bodyLimitBytes;
    let size = 0;
    const chunks = [];
    req.on('data', (chunk) => {
      size += chunk.length;
      if (size > limit) {
        reject(new Error('请求体过大'));
        req.destroy();
        return;
      }
      chunks.push(chunk);
    });
    req.on('end', () => {
      if (chunks.length === 0) return resolve({});
      try {
        resolve(JSON.parse(Buffer.concat(chunks).toString('utf8')));
      } catch (error) {
        reject(new Error('请求体不是合法 JSON'));
      }
    });
    req.on('error', reject);
  });
}

/** 简易限流：按设备每分钟计数，超限返回 false */
function rateOk(deviceId, cost) {
  const nowMinute = Math.floor(Date.now() / 60000);
  const bucket = rateBuckets.get(deviceId);
  if (!bucket || bucket.minute !== nowMinute) {
    rateBuckets.set(deviceId, { minute: nowMinute, count: cost });
    return true;
  }
  bucket.count += cost;
  return bucket.count <= config.sync.maxOpsPerMinutePerDevice;
}

/* ----------------------------- 同步核心 ----------------------------- */

function applyAndBroadcast(ops, context) {
  const { results, broadcast, serverRev } = store.applyOps(ops, context);
  if (broadcast.length > 0) {
    for (const entry of broadcast) {
      const message = { t: 'apply', op: entry, serverRev: store.state.serverRev };
      for (const client of clients) {
        // 不回推给发起端：发起端已经通过 ack 知道结果
        if (client.deviceId === entry.deviceId) continue;
        client.send(message);
      }
    }
    log('debug', `广播 ${broadcast.length} 条变更`, { from: context.deviceId });
  }
  return { results, serverRev, applied: broadcast.length };
}

function contextFrom(req, url, role) {
  return {
    deviceId: url.searchParams.get('deviceId') || req.headers['x-device-id'] || 'unknown',
    role: role || url.searchParams.get('role') || 'unknown',
    transport: url.searchParams.get('transport') || 'http'
  };
}

/* ----------------------------- WebSocket ----------------------------- */

function handleWebSocket(ws, url) {
  const client = {
    kind: 'ws',
    deviceId: url.searchParams.get('deviceId') || 'unknown',
    role: url.searchParams.get('role') || 'mini',
    lastSeenAt: Date.now(),
    send: (message) => {
      if (ws.readyState === WebSocket.OPEN) ws.send(JSON.stringify(message));
    },
    close: () => ws.close()
  };
  clients.add(client);
  log('info', `WS 连接建立`, { deviceId: client.deviceId, role: client.role, online: clients.size });

  const aliveTimer = setInterval(() => {
    if (Date.now() - client.lastSeenAt > config.realtime.clientTimeoutMs) {
      log('warn', 'WS 客户端超时，断开', { deviceId: client.deviceId });
      ws.terminate();
    }
  }, Math.max(5000, Math.floor(config.realtime.heartbeatMs / 2)));

  ws.on('message', (raw) => {
    let message;
    try {
      message = JSON.parse(raw.toString());
    } catch (error) {
      return client.send({ t: 'error', message: '消息不是合法 JSON' });
    }
    client.lastSeenAt = Date.now();
    handleClientMessage(client, message, (reply) => client.send(reply));
  });

  ws.on('pong', () => { client.lastSeenAt = Date.now(); });
  ws.on('close', () => {
    clearInterval(aliveTimer);
    clients.delete(client);
    log('info', 'WS 连接关闭', { deviceId: client.deviceId, online: clients.size });
  });
  ws.on('error', (error) => log('warn', `WS 错误: ${error.message}`, { deviceId: client.deviceId }));

  client.send({ t: 'connected', serverRev: store.state.serverRev, heartbeatMs: config.realtime.heartbeatMs });
}

function handleClientMessage(client, message, reply) {
  switch (message.t) {
    case 'hello': {
      const cursor = Number.isFinite(message.cursor) ? message.cursor : null;
      client.deviceId = message.deviceId || client.deviceId;
      client.role = message.role || client.role;
      // 客户端可以声明单页大小：弱网环境下分页下发，单次响应更小、更容易重试成功
      client.pageSize = Number.isFinite(message.pageSize) && message.pageSize > 0
        ? Math.min(Math.floor(message.pageSize), 2000)
        : 0;
      store.touchDevice({ deviceId: client.deviceId, role: client.role, transport: client.kind });
      const delta = store.deltaSince(cursor);
      reply({
        t: 'welcome',
        serverRev: store.state.serverRev,
        digest: store.digest().digest,
        stale: delta.stale,
        pageSize: client.pageSize
      });
      if (delta.stale || cursor === null) {
        reply({ t: 'snapshot', ...store.snapshot({ limit: client.pageSize, offset: 0 }) });
      } else if (delta.ops.length > 0) {
        reply({ t: 'delta', ops: delta.ops, serverRev: store.state.serverRev, nextCursor: delta.nextCursor, truncated: delta.truncated });
      }
      return;
    }
    case 'push': {
      const ops = Array.isArray(message.ops) ? message.ops : [];
      if (ops.length > config.sync.maxOpsPerRequest) {
        return reply({ t: 'error', message: `单次最多 ${config.sync.maxOpsPerRequest} 条 op` });
      }
      if (!rateOk(client.deviceId, ops.length)) {
        return reply({ t: 'error', message: '超出每分钟写入配额' });
      }
      for (const op of ops) { op.deviceId = client.deviceId; op.role = client.role; }
      const outcome = applyAndBroadcast(ops, { deviceId: client.deviceId, role: client.role, transport: client.kind });
      return reply({ t: 'ack', results: outcome.results, serverRev: outcome.serverRev });
    }
    case 'pull': {
      const pageSize = Number.isFinite(message.pageSize) && message.pageSize > 0
        ? Math.min(Math.floor(message.pageSize), client.pageSize || 2000)
        : client.pageSize;
      const offset = Number.isFinite(message.offset) && message.offset > 0 ? Math.floor(message.offset) : 0;
      return reply({ t: 'snapshot', ...store.snapshot({ limit: pageSize, offset: offset }) });
    }
    case 'digest': {
      const scope = Array.isArray(message.scope) && message.scope.length > 0 ? message.scope : null;
      const info = store.digest(scope);
      return reply({
        t: 'digest',
        ok: info.digest === message.digest,
        clientDigest: message.digest || null,
        serverDigest: info.digest,
        serverRev: info.serverRev,
        counts: info.counts
      });
    }
    case 'ping': {
      return reply({ t: 'pong', serverRev: store.state.serverRev, ts: Date.now() });
    }
    default:
      return reply({ t: 'error', message: `未知消息类型 ${message.t}` });
  }
}

/* ----------------------------- SSE（PC 端长连接） ----------------------------- */

function handleSse(req, res, url) {
  const deviceId = url.searchParams.get('deviceId') || 'pc-unknown';
  const role = url.searchParams.get('role') || 'pc';

  res.writeHead(200, {
    'Content-Type': 'text/event-stream; charset=utf-8',
    'Cache-Control': 'no-cache, no-transform',
    Connection: 'keep-alive',
    'X-Accel-Buffering': 'no'
  });
  res.write(`retry: ${config.realtime.sseKeepaliveMs}\n\n`);

  const client = {
    kind: 'sse',
    deviceId,
    role,
    lastSeenAt: Date.now(),
    send: (message) => {
      res.write(`event: ${message.t}\ndata: ${JSON.stringify(message)}\n\n`);
    },
    close: () => res.end()
  };
  clients.add(client);
  store.touchDevice({ deviceId, role, transport: 'sse' });
  client.send({ t: 'connected', serverRev: store.state.serverRev });

  const keepAlive = setInterval(() => {
    res.write(': ping\n\n');
  }, config.realtime.sseKeepaliveMs);

  req.on('close', () => {
    clearInterval(keepAlive);
    clients.delete(client);
    log('info', 'SSE 连接关闭', { deviceId, online: clients.size });
  });
  log('info', 'SSE 连接建立', { deviceId, role, online: clients.size });
}

/* ----------------------------- HTTP 路由 ----------------------------- */

const server = http.createServer(async (req, res) => {
  const url = new URL(req.url, `http://${req.headers.host || 'localhost'}`);
  const pathname = url.pathname;

  if (pathname === '/health') {
    return sendJson(res, 200, {
      ok: true,
      serverRev: store.state.serverRev,
      online: clients.size,
      entities: Object.keys(store.state.entities).length
    });
  }

  if (!authorize(req, url)) {
    return sendJson(res, 401, { ok: false, message: '令牌无效' });
  }

  try {
    if (pathname === '/snapshot' && req.method === 'GET') {
      const limit = Number(url.searchParams.get('limit'));
      const offset = Number(url.searchParams.get('offset'));
      return sendJson(res, 200, { ok: true, ...store.snapshot({ limit: limit, offset: offset }) });
    }

    if (pathname === '/delta' && req.method === 'GET') {
      const cursor = Number(url.searchParams.get('cursor'));
      const limit = Number(url.searchParams.get('limit'));
      const delta = store.deltaSince(Number.isFinite(cursor) ? cursor : null, limit);
      return sendJson(res, 200, { ok: true, ...delta });
    }

    if (pathname === '/digest' && req.method === 'GET') {
      const scope = (url.searchParams.get('scope') || '').split(',').filter(Boolean);
      return sendJson(res, 200, { ok: true, ...store.digest(scope) });
    }

    if (pathname === '/entity' && req.method === 'GET') {
      const entityType = url.searchParams.get('type');
      const syncId = url.searchParams.get('syncId');
      const entity = store.entity(entityType, syncId);
      if (!entity) return sendJson(res, 404, { ok: false, message: '实体不存在' });
      return sendJson(res, 200, { ok: true, entity });
    }

    if (pathname === '/push' && req.method === 'POST') {
      const body = await readJsonBody(req);
      const ops = Array.isArray(body.ops) ? body.ops : [];
      if (ops.length > config.sync.maxOpsPerRequest) {
        return sendJson(res, 413, { ok: false, message: `单次最多 ${config.sync.maxOpsPerRequest} 条 op` });
      }
      const context = contextFrom(req, url, body.role);
      if (!rateOk(context.deviceId, ops.length)) {
        return sendJson(res, 429, { ok: false, message: '超出每分钟写入配额' });
      }
      for (const op of ops) {
        op.deviceId = op.deviceId || context.deviceId;
        op.role = op.role || context.role;
      }
      const outcome = applyAndBroadcast(ops, context);
      return sendJson(res, 200, { ok: true, ...outcome });
    }

    if (pathname === '/conflicts' && req.method === 'GET') {
      return sendJson(res, 200, { ok: true, conflicts: store.conflicts() });
    }

    if (pathname === '/ticket' && req.method === 'POST') {
      const body = await readJsonBody(req);
      return sendJson(res, 200, {
        ok: true,
        ...issueTicket(body.deviceId || 'mini', body.role || 'mini', body.ttlMs)
      });
    }

    if (pathname === '/stream' && req.method === 'GET') {
      return handleSse(req, res, url);
    }

    if (pathname === '/reset' && req.method === 'POST') {
      store.reset();
      log('warn', '服务端状态已清空', { by: url.searchParams.get('deviceId') });
      return sendJson(res, 200, { ok: true, message: '已清空服务端状态' });
    }

    if (pathname === '/stats' && req.method === 'GET') {
      return sendJson(res, 200, {
        ok: true,
        serverRev: store.state.serverRev,
        oplog: store.oplog.length,
        oldestSeq: store.oldestSeq(),
        devices: store.state.devices,
        online: Array.from(clients).map((c) => ({ kind: c.kind, deviceId: c.deviceId, role: c.role })),
        counts: store.digest().counts
      });
    }

    return sendJson(res, 404, { ok: false, message: '未找到接口' });
  } catch (error) {
    log('error', `处理请求失败: ${error.message}`, { pathname });
    return sendJson(res, 500, { ok: false, message: error.message });
  }
});

/* ----------------------------- WebSocket 升级 ----------------------------- */

const wss = new WebSocket.Server({ noServer: true, maxPayload: config.server.bodyLimitBytes });

server.on('upgrade', (req, socket, head) => {
  const url = new URL(req.url, `http://${req.headers.host || 'localhost'}`);
  if (!url.pathname.endsWith('/ws')) {
    socket.destroy();
    return;
  }
  if (!authorize(req, url)) {
    socket.write('HTTP/1.1 401 Unauthorized\r\n\r\n');
    socket.destroy();
    return;
  }
  wss.handleUpgrade(req, socket, head, (ws) => handleWebSocket(ws, url));
});

/* ----------------------------- 心跳与退出 ----------------------------- */

const heartbeat = setInterval(() => {
  for (const client of clients) {
    if (client.kind !== 'ws') continue;
    client.send({ t: 'ping', ts: Date.now() });
  }
}, config.realtime.heartbeatMs);

function shutdown(signal) {
  log('info', `收到 ${signal}，保存状态后退出`);
  clearInterval(heartbeat);
  store.flush();
  for (const client of clients) client.close();
  server.close(() => process.exit(0));
  setTimeout(() => process.exit(0), 3000).unref();
}

process.on('SIGINT', () => shutdown('SIGINT'));
process.on('SIGTERM', () => shutdown('SIGTERM'));

server.listen(config.server.port, config.server.host, () => {
  log('info', `同步服务已启动: http://${config.server.host}:${config.server.port}${config.server.publicPath}`);
  log('info', `对外地址示例: wss://<你的域名>${config.server.publicPath}/ws?token=...&deviceId=...&role=mini`);
});
