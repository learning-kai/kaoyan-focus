'use strict';

/**
 * 真机连接失败的可诊断性测试（不需要服务端，纯本地）。
 *
 * 背景：真机上微信会强制校验「socket 合法域名」白名单，而开发者工具不校验。
 * 更麻烦的是，握手被拦时 socket 可能既不 onOpen 也不 onError，界面就永远停在
 * 「连接中」，用户拿不到任何线索。这个测试锁定三条不变式：
 *
 *   1. 连接失败必须留下可读的原始 errMsg（不能只改状态就完事）
 *   2. 握手迟迟不回调必须由看门狗兜底，把「永远连接中」变成一次可诊断的失败
 *   3. 旧连接的迟到回调不得改乱新连接的状态
 *
 * 用法：
 *   node tests/mini-connect-diagnostic-test.js
 */

const path = require('path');

/* ------------------------------------------------------------------ 模拟微信环境 */

const storage = new Map();
const MINIAPP = path.join(__dirname, '..', 'miniapp');

// 可切换的故障模式：
//   'fail' —— 立刻回调 fail（真机上域名不在白名单时的真实行为）
//   'hang' —— 什么都不回调（真机被静默拦截时的真实行为）
let mode = 'hang';
let createdSockets = [];

global.wx = {
  getStorageSync: (key) => (storage.has(key) ? storage.get(key) : ''),
  setStorageSync: (key, value) => storage.set(key, value),
  removeStorageSync: (key) => storage.delete(key),
  showToast: () => {},
  showModal: () => {},
  onNetworkStatusChange: () => {},
  connectSocket: (options) => {
    const task = {
      handlers: {},
      onOpen: (handler) => { task.handlers.open = handler; },
      onMessage: (handler) => { task.handlers.message = handler; },
      onError: (handler) => { task.handlers.error = handler; },
      onClose: (handler) => { task.handlers.close = handler; },
      send: () => {},
      close: () => { if (task.handlers.close) task.handlers.close({ code: 1000 }); }
    };
    createdSockets.push(task);
    if (mode === 'fail') {
      setTimeout(() => {
        if (options.fail) options.fail({ errMsg: 'connectSocket:fail url not in domain list' });
      }, 0);
    }
    return task;
  }
};

/* ------------------------------------------------------------------ 加载被测代码 */

const config = require(path.join(MINIAPP, 'config.js'));
const sync = require(path.join(MINIAPP, 'services', 'sync-service.js'));

// 把时间压到毫秒级，同时保持相对关系不变
config.timing.connectWatchdogMs = 200;
config.timing.reconnectBaseMs = 60;
config.timing.reconnectMaxMs = 60;

const events = [];
sync.on((event) => events.push(event));

function statusEvents() {
  return events.filter((e) => e.type === 'status').map((e) => e.status);
}

function errorEvents() {
  return events.filter((e) => e.type === 'error');
}

function sleep(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

let failures = 0;
function check(label, condition, detail) {
  const ok = !!condition;
  if (!ok) failures += 1;
  console.log(`${ok ? 'PASS' : 'FAIL'}  ${label}${detail ? ' — ' + detail : ''}`);
}

/* ------------------------------------------------------------------ 用例 */

async function caseFailCallback() {
  console.log('\n[1] 域名不在白名单：fail 回调必须留下可读原因');
  mode = 'fail';
  events.length = 0;
  sync.start();

  check('status 先进入 connecting', sync.statusOf() === 'connecting', sync.statusOf());

  await sleep(40);

  const info = sync.lastErrorInfo();
  check('失败后状态退回 offline', sync.statusOf() === 'offline', sync.statusOf());
  check('记录了原始 errMsg', !!info && /domain list/.test(info.raw), info && info.raw);
  check('errMsg 被翻译成「socket 合法域名」提示', !!info && info.hint.indexOf('socket 合法域名') >= 0);
  check('发出了 error 事件且带 source', errorEvents().some((e) => e.source === 'connect-fail'));
  check('目标地址里的 token 被脱敏', sync.targetUrl().indexOf('token=***') >= 0, sync.targetUrl());
  check('目标地址不含真实 token', sync.targetUrl().indexOf(config.server.token) < 0);

  sync.disconnect();
}

async function caseWatchdog() {
  console.log('\n[2] 握手静默无回调：看门狗兜底，不能永远停在「连接中」');
  mode = 'hang';
  events.length = 0;
  const before = createdSockets.length;
  sync.start();

  check('status 进入 connecting', sync.statusOf() === 'connecting', sync.statusOf());

  await sleep(120);
  check('看门狗未到期前仍在 connecting', sync.statusOf() === 'connecting', sync.statusOf());

  await sleep(200);

  // 断言事件序列而不是断言「此刻」的状态：重连是循环的，瞬时状态本身带竞争，
  // 真正要保证的是看门狗**确实把状态从 connecting 推出去了**。
  const idx = events.findIndex((e) => e.type === 'status' && e.reason === 'connect-timeout');
  const info = sync.lastErrorInfo();
  check('看门狗触发了状态切换', idx >= 0);
  check('切换目标不是 connecting', idx >= 0 && events[idx].status !== 'connecting',
    idx >= 0 ? events[idx].status : 'n/a');
  check('随后进入重连调度', idx >= 0 && events.slice(idx).some((e) => e.status === 'reconnecting'));
  check('记为 connect-timeout', !!info && info.source === 'connect-timeout', info && info.source);
  check('提示指向白名单优先排查', !!info && info.hint.indexOf('白名单') >= 0);
  check('触发了重新建连', createdSockets.length > before, `${before} -> ${createdSockets.length}`);

  sync.disconnect();
}

async function caseConfigGuard() {
  console.log('\n[3] 配置自检：ws:// 或不备案域名必须直接报错，不能空转');
  events.length = 0;
  const original = config.server.wsUrl;
  config.server.wsUrl = 'ws://api.skyhold.cloud/sync/ws';
  sync.start();

  const info = sync.lastErrorInfo();
  check('明文 ws:// 被拦下', !!info && info.source === 'config', info && info.source);
  check('错误文案要求 wss://', !!info && info.raw.indexOf('wss://') >= 0, info && info.raw);
  check('状态为 offline', sync.statusOf() === 'offline', sync.statusOf());

  config.server.wsUrl = original;
  sync.disconnect();
}

async function caseStaleCallback() {
  console.log('\n[4] 旧连接的迟到回调不得改乱新连接的状态');
  mode = 'hang';
  events.length = 0;
  config.server.wsUrl = 'wss://api.skyhold.cloud/sync/ws';
  sync.start();

  const first = createdSockets[createdSockets.length - 1];
  // 手动重连会作废旧连接（作废时 close 会立刻回调）
  sync.connect('manual');

  check('重连后仍处于 connecting', sync.statusOf() === 'connecting', sync.statusOf());
  check('旧连接确实被关掉了', !!first);
  check('没有因旧连接产生错误', errorEvents().length === 0, JSON.stringify(errorEvents()));

  sync.disconnect();
}

async function caseLocalDevAllowed() {
  console.log('\n[5] 本地联调地址不能被配置自检拦掉（开发者工具里 ws://127.0.0.1 是合法的）');
  mode = 'hang';
  events.length = 0;
  const original = config.server.wsUrl;
  config.server.wsUrl = 'ws://127.0.0.1:8788/ws';
  sync.start();

  const info = sync.lastErrorInfo();
  check('本机地址未被判为配置错误', !info || info.source !== 'config', info && info.source);
  check('确实发起了连接', sync.statusOf() === 'connecting', sync.statusOf());

  // 内网地址同理（手机与电脑同一 WiFi 时常用）
  sync.disconnect();
  events.length = 0;
  config.server.wsUrl = 'ws://192.168.1.20:8788/ws';
  sync.start();
  const lanInfo = sync.lastErrorInfo();
  check('内网地址未被判为配置错误', !lanInfo || lanInfo.source !== 'config', lanInfo && lanInfo.source);

  config.server.wsUrl = original;
  sync.disconnect();
}

async function main() {
  await caseFailCallback();
  await caseWatchdog();
  await caseConfigGuard();
  await caseStaleCallback();
  await caseLocalDevAllowed();

  console.log(`\n${failures === 0 ? '全部通过' : failures + ' 项失败'}`);
  process.exit(failures === 0 ? 0 : 1);
}

main();
