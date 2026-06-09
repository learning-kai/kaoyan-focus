import type { AppUpdate } from '../services/updateApi';
import type { ForegroundApp } from '../types/monitor';
import type {
  AppSettings,
  EmailReminderSettings,
  FeishuSyncSettings,
  FeishuSyncStatus,
  ObjectStorageSettings,
  RuntimeHealth,
  WebDavSettings,
} from '../types/settings';
import type { AppDataLocation } from '../pages/settings/types';

function formatDateTime(value?: string | null) {
  if (!value) return '暂无';
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return value;
  return date.toLocaleString('zh-CN');
}

function describeOnOff(value: boolean) {
  return value ? '已启用' : '已关闭';
}

function describeConfigured(value?: boolean | null) {
  return value ? '已配置' : '未配置';
}

function describeHealthStatus(value?: string | null) {
  switch ((value ?? 'unknown').toLowerCase()) {
    case 'ok':
    case 'healthy':
    case 'synced':
      return '正常';
    case 'warning':
    case 'degraded':
    case 'not_run':
      return '需关注';
    case 'error':
    case 'failed':
      return '异常';
    case 'unavailable':
      return '不可用';
    default:
      return '未知';
  }
}

export function buildSystemDiagnosticSummary(args: {
  availableUpdate: AppUpdate | null;
  autoUpdateMessage: string | null;
  dataLocation: AppDataLocation | null;
  emailSettings: EmailReminderSettings;
  feishuSettings: FeishuSyncSettings;
  feishuStatus: FeishuSyncStatus | null;
  foregroundApp: ForegroundApp | null;
  lastAutoSyncMessage: string | null;
  objectStorageSettings: ObjectStorageSettings;
  runtimeHealth: RuntimeHealth | null;
  settings: AppSettings;
  updateMessage: string | null;
  updateProgress: number | null;
  webDavSettings: WebDavSettings;
}) {
  const {
    availableUpdate,
    autoUpdateMessage,
    dataLocation,
    emailSettings,
    feishuSettings,
    feishuStatus,
    foregroundApp,
    lastAutoSyncMessage,
    objectStorageSettings,
    runtimeHealth,
    settings,
    updateMessage,
    updateProgress,
    webDavSettings,
  } = args;

  const checks = runtimeHealth?.checks ?? [];
  const tasks = runtimeHealth?.tasks ?? [];

  return [
    '考研专注系统诊断摘要',
    `生成时间：${new Date().toLocaleString('zh-CN')}`,
    '',
    '数据目录',
    `- 数据目录：${dataLocation?.app_data_dir ?? '尚未读取'}`,
    `- SQLite 文件：${dataLocation?.database_path ?? '尚未读取'}`,
    `- 主设备：${settings.primary_owner_device_id ?? '未绑定'}`,
    '',
    '运行健康',
    `- 总体状态：${describeHealthStatus(runtimeHealth?.status)}`,
    runtimeHealth?.summary ? `- 摘要：${runtimeHealth.summary}` : null,
    runtimeHealth?.checked_at ? `- 检查时间：${formatDateTime(runtimeHealth.checked_at)}` : null,
    runtimeHealth?.protected_storage
      ? `- 受保护存储：${describeHealthStatus(runtimeHealth.protected_storage.status)}${runtimeHealth.protected_storage.message ? ` / ${runtimeHealth.protected_storage.message}` : ''}`
      : null,
    checks.length > 0
      ? `- 检查项：${checks
          .slice(0, 4)
          .map((check) => `${check.label ?? check.key ?? '检查项'}=${describeHealthStatus(check.status)}`)
          .join('； ')}`
      : null,
    tasks.length > 0
      ? `- 后台任务：${tasks
          .slice(0, 4)
          .map((task) => `${task.task}=${describeHealthStatus(task.status)}`)
          .join('； ')}`
      : null,
    '',
    '同步与集成',
    `- 同步后端：${settings.sync_backend === 'object_storage' ? '对象存储 / R2 / S3' : 'WebDAV'}`,
    `- WebDAV：${describeOnOff(webDavSettings.enabled)} / 密码${describeConfigured(webDavSettings.password_configured || Boolean(webDavSettings.password))}`,
    `- 对象存储：${describeOnOff(objectStorageSettings.enabled)} / 密钥${describeConfigured(objectStorageSettings.secret_access_key_configured || Boolean(objectStorageSettings.secret_access_key))}`,
    `- 邮件提醒：${describeOnOff(emailSettings.enabled)} / SMTP ${emailSettings.smtp_host || '未配置'} / 密码${describeConfigured(emailSettings.password_configured || Boolean(emailSettings.password))}`,
    `- 飞书：${describeOnOff(feishuSettings.enabled)} / 认证${feishuStatus?.authenticated ? '已完成' : '未完成'} / App Secret${describeConfigured(feishuSettings.app_secret_configured || Boolean(feishuSettings.app_secret))}`,
    `- 最近同步：${lastAutoSyncMessage ?? '暂无'}`,
    `- 主题：${settings.ui_theme}`,
    `- 学习模式：${settings.default_focus_mode}`,
    '',
    '前台应用',
    foregroundApp
      ? `- ${foregroundApp.process_name} (#${foregroundApp.process_id})${foregroundApp.window_title ? ` / ${foregroundApp.window_title}` : ''}`
      : '- 尚未检测到前台应用',
    foregroundApp?.process_path ? `- 路径：${foregroundApp.process_path}` : null,
    '',
    '更新',
    availableUpdate ? `- 可用版本：${availableUpdate.version}` : '- 暂无可用更新',
    updateMessage ? `- 提示：${updateMessage}` : null,
    updateProgress !== null ? `- 下载进度：${updateProgress}%` : null,
    autoUpdateMessage ? `- 自动更新：${autoUpdateMessage}` : null,
  ]
    .filter((line): line is string => Boolean(line))
    .join('\n');
}
