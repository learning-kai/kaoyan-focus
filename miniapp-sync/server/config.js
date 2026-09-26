'use strict';

const path = require('path');

/**
 * 服务端集中配置。改这个文件就够了，不要把配置散落到代码里。
 * 所有环境变量都有默认值，方便 systemd / docker / pm2 覆盖。
 */
const config = {
  server: {
    host: process.env.SYNC_HOST || '127.0.0.1',
    port: Number(process.env.SYNC_PORT || 8787),
    // nginx 反代时对外暴露的路径前缀，例如 https://sync.example.com/sync/ws
    // 只影响日志里的提示与文档示例，不影响路由匹配（路由按后缀匹配）。
    publicPath: process.env.SYNC_PUBLIC_PATH || '/sync',
    bodyLimitBytes: 512 * 1024
  },

  auth: {
    // 主令牌：PC 端与小程序端共用。上线前必须改成随机长字符串。
    token: process.env.SYNC_TOKEN || 'CHANGE-ME-to-a-32chars-random-string',
    // 微信小程序 wx.connectSocket 不能自定义 Header，只能把令牌放 query，
    // 因此 WebSocket 必须允许 query 令牌；REST 仍优先用 Authorization 头。
    allowQueryToken: true,
    ticketTtlMs: 365 * 24 * 60 * 60 * 1000
  },

  storage: {
    dataDir: process.env.SYNC_DATA_DIR || path.join(__dirname, 'data'),
    snapshotFile: 'state.json',
    oplogFile: 'oplog.jsonl',
    // 快照落盘去抖：高频写入时合并成一次写，避免每次 op 都刷盘
    flushDebounceMs: 400
  },

  sync: {
    // 操作日志最多保留条数；超出后旧 cursor 客户端会被判定为需要全量同步
    oplogLimit: 5000,
    // 每个实体保留的历史版本数，供小程序端查看/回滚冲突
    historyPerEntity: 5,
    conflictsLimit: 200,
    maxOpsPerRequest: 300,
    maxOpsPerMinutePerDevice: 20000,
    maxDeltaBatch: 1000
  },

  realtime: {
    heartbeatMs: 20000,
    clientTimeoutMs: 45000,
    sseKeepaliveMs: 15000
  },

  logging: {
    level: process.env.SYNC_LOG_LEVEL || 'info'
  }
};

module.exports = config;
