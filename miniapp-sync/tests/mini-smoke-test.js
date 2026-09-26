'use strict';

/**
 * 小程序端逻辑的本地冒烟测试。
 *
 * 用 Node 模拟微信运行环境（Storage / connectSocket / showToast），
 * 直接加载 miniapp/services 下的真实代码连到同步服务端，验证：
 *   1. 握手 + 拉全量快照
 *   2. 手机端改数据 → 服务端接受
 *   3. 一致性摘要校验通过
 *   4. 越权写入被服务端拒绝
 *
 * 用法（先起服务端）：
 *   node tests/mini-smoke-test.js --url ws://127.0.0.1:8787/ws --token test-token-local
 */

const path = require('path');
const WebSocket = require(path.join(__dirname, '..', 'server', 'node_modules', 'ws'));

function parseArgs() {
  const args = process.argv.slice(2);
  const result = {};
  for (let i = 0; i < args.length; i += 2) result[args[i].replace(/^--/, '')] = args[i + 1];
  return result;
}

const args = parseArgs();
const WS_URL = args.url || 'ws://127.0.0.1:8787/ws';
const TOKEN = args.token || 'test-token-local';
const REST_URL = (args.rest || 'http://127.0.0.1:8787').replace(/\/$/, '');
// 故意用很小的分页，强制走多页快照 / 分批增量路径
const PAGE_SIZE = Number(args['page-size'] || 300);

/* ------------------------------------------------------------------ 模拟微信环境 */

const storage = new Map();

global.wx = {
  getStorageSync: (key) => (storage.has(key) ? storage.get(key) : ''),
  setStorageSync: (key, value) => storage.set(key, value),
  removeStorageSync: (key) => storage.delete(key),
  showToast: (options) => console.log('   [toast]', options.title),
  showModal: (options) => options.success && options.success({ confirm: true }),
  onNetworkStatusChange: () => {},
  connectSocket: (options) => {
    const socket = new WebSocket(options.url);
    const task = {
      onOpen: (handler) => socket.on('open', handler),
      onMessage: (handler) => socket.on('message', (data) => handler({ data: data.toString() })),
      onError: (handler) => socket.on('error', handler),
      onClose: (handler) => socket.on('close', handler),
      send: (payload) => {
        try {
          socket.send(payload.data);
        } catch (error) {
          console.error('   [send-fail]', error.message);
        }
      },
      close: () => socket.close()
    };
    return task;
  }
};

/* ------------------------------------------------------------------ 加载被测代码 */

const config = require(path.join(__dirname, '..', 'miniapp', 'config.js'));
config.server.wsUrl = WS_URL;
config.server.restUrl = REST_URL;
config.server.token = TOKEN;
config.sync.snapshotPageSize = PAGE_SIZE;

const store = require(path.join(__dirname, '..', 'miniapp', 'services', 'store.js'));
const sync = require(path.join(__dirname, '..', 'miniapp', 'services', 'sync-service.js'));

/* ------------------------------------------------------------------ 断言与流程 */

let failures = 0;
function assert(label, condition, detail) {
  const ok = !!condition;
  if (!ok) failures += 1;
  console.log(`${ok ? 'PASS' : 'FAIL'}  ${label}${detail ? ' — ' + detail : ''}`);
}

function waitFor(predicate, timeoutMs, label) {
  return new Promise((resolve, reject) => {
    const started = Date.now();
    const timer = setInterval(() => {
      if (predicate()) {
        clearInterval(timer);
        resolve();
      } else if (Date.now() - started > timeoutMs) {
        clearInterval(timer);
        reject(new Error('等待超时: ' + label));
      }
    }, 100);
  });
}

function httpGet(pathAndQuery) {
  return new Promise((resolve, reject) => {
    const http = require('http');
    const url = new URL(REST_URL + pathAndQuery);
    http.get({ hostname: url.hostname, port: url.port, path: url.pathname + url.search,
      headers: { Authorization: 'Bearer ' + TOKEN } }, (res) => {
      let body = '';
      res.on('data', (chunk) => { body += chunk; });
      res.on('end', () => {
        try {
          resolve(JSON.parse(body));
        } catch (error) {
          reject(error);
        }
      });
    }).on('error', reject);
  });
}

/**
 * 摘要不一致时，逐条比对本地缓存与服务端的实体指纹，定位到底是哪条不同。
 * 没有这个定位，失败的摘要断言只会给两个对不上的 hash，等于没给信息。
 */
async function explainDigestDiff() {
  const protocol = require(path.join(__dirname, '..', 'miniapp', 'services', 'protocol.js'));
  const full = await httpGet('/snapshot');
  const lines = [];
  for (const type of Object.keys(full.entities)) {
    for (const syncId of Object.keys(full.entities[type])) {
      const remote = full.entities[type][syncId];
      const local = store.getEntity(type, syncId);
      const remoteDeleted = !!remote.deletedAt;
      const localDeleted = !!(local && local.deletedAt);
      if (remoteDeleted && localDeleted) continue;
      if (remoteDeleted !== localDeleted) {
        lines.push(`${type}/${syncId}: 删除状态不同（服务端 ${remoteDeleted ? '墓碑' : '存活'} / 本地 ${localDeleted ? '墓碑' : '存活'}）`);
        continue;
      }
      if (!local) { lines.push(`${type}/${syncId}: 本地缺失`); continue; }
      const localHash = protocol.entityHash(type, syncId, local.fields || {}, local.deletedAt || null);
      if (localHash !== remote.hash) {
        lines.push(`${type}/${syncId}: 字段不同\n      服务端 ${JSON.stringify(remote.fields)}\n      本地   ${JSON.stringify(local.fields || {})}`);
      }
    }
  }
  return lines;
}

async function main() {
  console.log('\n=== 小程序端冒烟测试 ===\n');
  store.init();
  sync.start();

  await waitFor(() => sync.statusOf() === 'online', 8000, 'WebSocket 连接');
  assert('WebSocket 连接建立', true, 'status=' + sync.statusOf());

  await waitFor(() => store.list('subject').length > 0, 15000, '全量快照');
  const subjectCount = store.list('subject').length;
  const taskCount = store.list('checklist_task').length;
  assert('拉到全量快照', subjectCount > 0 && taskCount > 0,
    `科目 ${subjectCount} 条 / 清单 ${taskCount} 条`);

  // 1) 手机端修改一条清单任务
  const target = store.list('checklist_task')[0];
  const newTitle = '【冒烟测试 ' + Date.now() + '】';
  sync.mutate('checklist_task', target.syncId, { title: newTitle });

  await waitFor(() => {
    const entity = store.getEntity('checklist_task', target.syncId);
    return entity && entity.fields.title === newTitle;
  }, 8000, '本地乐观更新');
  assert('本地乐观更新生效', true, newTitle);

  await waitFor(() => store.outboxList().length === 0, 10000, '发件箱清空');
  const remote = await httpGet('/entity?type=checklist_task&syncId=' + encodeURIComponent(target.syncId));
  assert('服务端已接受手机端修改',
    remote.ok && remote.entity.fields.title === newTitle,
    remote.ok ? remote.entity.fields.title : remote.message);

  // 2) 一致性摘要校验
  sync.checkConsistency();
  await waitFor(() => store.getServerRev() > 0, 5000, 'serverRev');
  const digest = await httpGet('/digest?scope=' + config.sync.digestScope.join(','));
  const localDigest = store.digest(config.sync.digestScope);
  const digestOk = digest.digest === localDigest;
  assert('两端摘要一致', digestOk, `本地 ${localDigest} / 服务端 ${digest.digest}`);
  if (!digestOk) {
    const diffs = await explainDigestDiff();
    console.log(diffs.length ? '  差异明细：\n    ' + diffs.join('\n    ') : '  （逐条比对未发现差异，可能是 scope 口径不同）');
  }

  // 3) 分页快照：强制全量重拉，验证多页累积后落地完整
  const full = await httpGet('/snapshot');
  let serverTotal = 0;
  for (const type of Object.keys(full.entities)) {
    for (const syncId of Object.keys(full.entities[type])) {
      if (!full.entities[type][syncId].deletedAt) serverTotal += 1;
    }
  }
  let snapshotDone = false;
  sync.on((event) => {
    if (event.type === 'change' && event.source === 'snapshot') snapshotDone = true;
  });
  sync.forcePull();
  await waitFor(() => snapshotDone, 30000, '分页快照落地');
  const localTotal = Object.keys(store.counts()).reduce((sum, type) => sum + store.counts()[type], 0);
  assert('分页快照完整落地',
    localTotal === serverTotal,
    `服务端 ${serverTotal} 条 / 本地 ${localTotal} 条，每页 ${PAGE_SIZE}`);

  // 4) 越权写入（手机端改专注记录，应被拒绝）
  const session = store.list('focus_session')[0];
  let rejected = false;
  const offHandler = (event) => {
    if (event.type === 'rejected') rejected = true;
  };
  sync.on(offHandler);
  sync.mutate('focus_session', session.syncId, { status: 'hacked-by-phone' });
  await waitFor(() => rejected, 8000, '越权拒绝');
  assert('PC 权威实体拒绝手机端写入', rejected);

  console.log(`\n结果：${failures === 0 ? '全部通过' : failures + ' 项失败'}\n`);
  process.exit(failures === 0 ? 0 : 1);
}

main().catch((error) => {
  console.error('测试异常:', error.message);
  process.exit(1);
});
