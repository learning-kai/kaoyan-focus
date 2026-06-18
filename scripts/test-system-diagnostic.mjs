import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import ts from 'typescript';

const helperSource = await readFile(new URL('../src/utils/systemDiagnostic.ts', import.meta.url), 'utf8');
const helperModuleSource = ts.transpileModule(helperSource, {
  compilerOptions: {
    module: ts.ModuleKind.ESNext,
    target: ts.ScriptTarget.ES2020,
  },
}).outputText;
const helperUrl = `data:text/javascript;base64,${Buffer.from(helperModuleSource).toString('base64')}`;
const { buildSystemDiagnosticSummary } = await import(helperUrl);

const summary = buildSystemDiagnosticSummary({
  availableUpdate: { version: '2.0.0' },
  autoUpdateMessage: 'auto update finished',
  dataLocation: {
    app_data_dir: 'smoke/app.sqlite3',
    database_path: 'smoke/kaoyan-focus.sqlite3',
  },
  emailSettings: {
    enabled: true,
    smtp_host: 'smtp.example.com',
    smtp_port: 465,
    smtp_security: 'tls',
    username: 'study@example.com',
    password: '',
    password_configured: true,
    from: 'study@example.com',
    to: 'team@example.com',
  },
  feishuSettings: {
    enabled: false,
    app_id: 'app-id',
    app_secret: '',
    app_secret_configured: false,
    redirect_uri: 'http://localhost:1420/feishu/callback',
  },
  feishuStatus: { authenticated: true },
  foregroundApp: {
    process_name: 'explorer.exe',
    process_id: 1234,
    process_path: 'C:\\Windows\\explorer.exe',
    window_title: 'Focus window',
  },
  lastAutoSyncMessage: 'auto sync finished',
  objectStorageSettings: {
    enabled: false,
    endpoint: 'https://example.r2.cloudflarestorage.com',
    bucket: 'kaoyan-focus',
    access_key_id: 'access-key',
    secret_access_key: '',
    secret_access_key_configured: true,
    region: 'auto',
    object_key: 'study-sync.json',
  },
  runtimeHealth: {
    status: 'ok',
    summary: 'runtime healthy',
    checked_at: 'not-a-date',
    protected_storage: {
      status: 'error',
      message: 'storage unavailable',
    },
    checks: [
      { key: 'cpu', label: 'CPU', status: 'ok' },
      { key: 'disk', label: 'Disk', status: 'warning' },
      { key: 'network', label: 'Network', status: 'error' },
    ],
    tasks: [
      { task: 'sync', status: 'healthy' },
      { task: 'backup', status: 'unknown' },
    ],
  },
  settings: {
    sync_backend: 'object_storage',
    primary_owner_device_id: 'device-42',
    ui_theme: 'dawn',
    default_focus_mode: 'deep',
    whitelist_mode: 'blocklist',
  },
  updateMessage: 'update ready',
  updateProgress: 42,
  webDavSettings: {
    enabled: true,
    url: 'https://dav.example.com/remote.php/dav/files/me',
    username: 'study',
    password: '',
    password_configured: false,
    remote_path: 'kaoyan-focus/kaoyan-focus.sqlite3',
  },
});

assert.match(summary, /^考研专注系统诊断摘要\n生成时间：/);
assert.match(summary, /\n数据目录\n/);
assert.match(summary, /\n运行健康\n/);
assert.match(summary, /\n同步与集成\n/);
assert.match(summary, /\n前台应用\n/);
assert.match(summary, /\n更新\n/);

const expectedLines = [
  '- 数据目录：smoke/app.sqlite3',
  '- SQLite 文件：smoke/kaoyan-focus.sqlite3',
  '- 主设备：device-42',
  '- 总体状态：正常',
  '- 摘要：runtime healthy',
  '- 检查时间：not-a-date',
  '- 受保护存储：异常 / storage unavailable',
  '- 检查项：CPU=正常； Disk=需关注； Network=异常',
  '- 后台任务：sync=正常； backup=未知',
  '- 同步后端：对象存储 / R2 / S3',
  '- WebDAV：已启用 / 密码未配置',
  '- 对象存储：已关闭 / 密钥已配置',
  '- 邮件提醒：已启用 / SMTP smtp.example.com / 密码已配置',
  '- 飞书：已关闭 / 认证已完成 / App Secret未配置',
  '- 最近同步：auto sync finished',
  '- 主题：dawn',
  '- 学习模式：deep',
  '- 前台规则：黑名单',
  '- explorer.exe (#1234) / Focus window',
  '- 路径：C:\\Windows\\explorer.exe',
  '- 可用版本：2.0.0',
  '- 提示：update ready',
  '- 下载进度：42%',
  '- 自动更新：auto update finished',
];

for (const line of expectedLines) {
  assert.ok(summary.includes(line), `Missing diagnostic line: ${line}`);
}

console.log('system diagnostic summary probe passed');
