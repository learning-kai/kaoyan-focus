import {
  Activity,
  BellRing,
  ChevronDown,
  Database,
  Download,
  Copy,
  ClipboardList,
  FolderOpen,
  HardDrive,
  MonitorDot,
  RefreshCw,
  ShieldCheck,
  Zap,
} from 'lucide-react';
import type { AppUpdate } from '../../services/updateApi';
import type { ForegroundApp } from '../../types/monitor';
import type {
  AppSettings,
  EmailReminderSettings,
  FeishuSyncSettings,
  FeishuSyncStatus,
  ObjectStorageSettings,
  RuntimeHealth,
  WebDavSettings,
} from '../../types/settings';
import { Capability, Detail } from './SettingsPrimitives';
import type { AppDataLocation, SettingsPanelKey } from './types';

type SystemPanelProps = {
  availableUpdate: AppUpdate | null;
  autoUpdateMessage: string | null;
  checkingUpdate: boolean;
  dataLocation: AppDataLocation | null;
  expandedPanels: Record<SettingsPanelKey, boolean>;
  foregroundApp: ForegroundApp | null;
  handleCheckUpdate: () => Promise<void>;
  handleDetectForegroundApp: () => Promise<void>;
  handleInstallUpdate: () => Promise<void>;
  handleCopyAppDataLocation: () => Promise<void>;
  handleCopySystemDiagnosticSummary: () => Promise<void>;
  handleDownloadSystemDiagnosticSummary: () => Promise<void>;
  handleOpenAppDataLocation: () => Promise<void>;
  handleRefreshRuntimeHealth: () => Promise<void>;
  handleSaveSettings: (settings: AppSettings) => Promise<void>;
  installingUpdate: boolean;
  loading: boolean;
  emailSettings: EmailReminderSettings;
  feishuSettings: FeishuSyncSettings;
  feishuStatus: FeishuSyncStatus | null;
  objectStorageSettings: ObjectStorageSettings;
  runtimeHealth: RuntimeHealth | null;
  runtimeHealthMessage: string | null;
  settings: AppSettings;
  settingsLocked: boolean;
  systemMessage: string | null;
  togglePanel: (panel: SettingsPanelKey) => void;
  updateMessage: string | null;
  updateProgress: number | null;
  webDavSettings: WebDavSettings;
};

const taskLabels: Record<string, string> = {
  email_reminder: '邮件提醒发送记录',
  email_reminder_check: '邮件提醒后台检查',
  feishu_background_sync: '飞书本地变更同步',
  feishu_sync: '飞书同步记录',
  object_storage_background_poll: '对象存储后台轮询',
  object_storage_sync: '对象存储同步记录',
  study_runtime_tick: '学习模式后台心跳',
  sync_backup_prune: '同步备份清理',
  webdav_sync: 'WebDAV 同步记录',
  whitelist_guard: '白名单守护',
};

function statusLabel(status?: string | null) {
  switch ((status ?? 'unknown').toLowerCase()) {
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

function formatDateTime(value?: string | null) {
  if (!value) return '暂无';
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return value;
  return date.toLocaleString('zh-CN');
}

function configuredLabel(configured?: boolean | null) {
  return configured ? '已配置' : '未配置';
}

function enabledLabel(enabled: boolean) {
  return enabled ? '启用' : '关闭';
}

function syncBackendLabel(value: AppSettings['sync_backend']) {
  return value === 'object_storage' ? '对象存储 / R2 / S3' : 'WebDAV';
}

function taskLabel(task: string) {
  return taskLabels[task] ?? task;
}

function shortError(value: string | null) {
  if (!value) return null;
  if (value.length > 120) return '错误已记录，内容较长，建议查看运行日志。';
  return value;
}

export function SystemPanel({
  availableUpdate,
  autoUpdateMessage,
  checkingUpdate,
  dataLocation,
  expandedPanels,
  foregroundApp,
  handleCheckUpdate,
  handleDetectForegroundApp,
  handleInstallUpdate,
  handleCopyAppDataLocation,
  handleCopySystemDiagnosticSummary,
  handleDownloadSystemDiagnosticSummary,
  handleOpenAppDataLocation,
  handleRefreshRuntimeHealth,
  handleSaveSettings,
  installingUpdate,
  loading,
  emailSettings,
  feishuSettings,
  feishuStatus,
  objectStorageSettings,
  runtimeHealth,
  runtimeHealthMessage,
  settings,
  settingsLocked,
  systemMessage,
  togglePanel,
  updateMessage,
  updateProgress,
  webDavSettings,
}: SystemPanelProps) {
  const healthChecks = runtimeHealth?.checks ?? [];
  const runtimeTasks = runtimeHealth?.tasks ?? [];
  const protectedStorage = runtimeHealth?.protected_storage ?? null;
  const activeSyncEnabled =
    settings.sync_backend === 'object_storage' ? objectStorageSettings.enabled : webDavSettings.enabled;
  const runtimeStatus = runtimeHealth?.status ?? 'unknown';

  return (
    <div
      aria-labelledby="settings-tab-system"
      className="settings-tab-panel"
      id="settings-panel-system"
      role="tabpanel"
    >
      <div className="settings-grid">
        <section className="command-panel">
          <div className="panel-title">
            <div>
              <p className="eyebrow">Rules</p>
              <h3>强制规则</h3>
            </div>
            <HardDrive size={20} />
            <button
              aria-expanded={expandedPanels.rules}
              className="settings-collapse-button"
              onClick={() => togglePanel('rules')}
              type="button"
            >
              <span>{settingsLocked ? '运行中' : '查看'}</span>
              <ChevronDown size={17} />
            </button>
          </div>
          {expandedPanels.rules && (
            <>
              <div className="settings-list">
                <Capability enabled={settingsLocked} icon={BellRing} text="学习期间关闭窗口时最小化到托盘" />
                <Capability enabled icon={ShieldCheck} text="记录非白名单应用干扰事件" />
                <Capability enabled={Boolean(dataLocation)} icon={Database} text="SQLite 本地数据目录可用" />
              </div>
            </>
          )}
        </section>
      </div>

      <div className="settings-grid lower">
        <section className="command-panel">
          <div className="panel-title">
            <div>
              <p className="eyebrow">Update</p>
              <h3>在线更新</h3>
            </div>
            <Download size={20} />
            <button
              aria-expanded={expandedPanels.update}
              className="settings-collapse-button"
              onClick={() => togglePanel('update')}
              type="button"
            >
              <span>{availableUpdate ? availableUpdate.version : '手动检查'}</span>
              <ChevronDown size={17} />
            </button>
          </div>
          {expandedPanels.update && (
            <>
              <p className="panel-copy">检查发布服务器上的新版本，下载完成后会自动重启应用。</p>
              {autoUpdateMessage && <p className="alert neutral">自动检查：{autoUpdateMessage}</p>}
              {updateMessage && <p className="alert neutral">{updateMessage}</p>}
              {updateProgress !== null && <p className="alert neutral">下载进度 {updateProgress}%</p>}
              {availableUpdate?.body && (
                <div className="update-release-notes-preview">
                  <p className="update-notes-label">更新内容：</p>
                  <div className="update-notes-content">
                    {availableUpdate.body.split('\n').slice(0, 5).map((line, index) => (
                      <p key={index}>{line}</p>
                    ))}
                    {availableUpdate.body.split('\n').length > 5 && (
                      <p className="update-notes-more">...</p>
                    )}
                  </div>
                </div>
              )}
              <label className="capability-row">
                <Zap size={17} />
                <input
                  checked={settings.auto_download_update}
                  disabled={settingsLocked}
                  onChange={() => void handleSaveSettings({
                    ...settings,
                    auto_download_update: !settings.auto_download_update,
                  })}
                  type="checkbox"
                />
                <span>自动下载更新</span>
              </label>
              <p className="capability-hint">发现新版本后自动下载，下载完成后提示安装</p>
              <div className="row-actions">
                <button
                  className="secondary-action"
                  disabled={checkingUpdate || installingUpdate || settingsLocked}
                  onClick={() => void handleCheckUpdate()}
                  type="button"
                >
                  <RefreshCw size={17} />
                  {checkingUpdate ? '检查中' : '检查更新'}
                </button>
                <button
                  className="primary-action"
                  disabled={availableUpdate === null || installingUpdate || settingsLocked}
                  onClick={() => void handleInstallUpdate()}
                  type="button"
                >
                  <Download size={17} />
                  {installingUpdate ? '安装中' : '下载并安装'}
                </button>
              </div>
              {(settings.skip_update_version || settings.update_reminder_snooze_until) && (
                <div className="update-snooze-status">
                  {settings.skip_update_version && (
                    <p className="alert neutral">
                      已跳过版本 {settings.skip_update_version}
                    </p>
                  )}
                  {settings.update_reminder_snooze_until && Date.now() < settings.update_reminder_snooze_until * 1000 && (
                    <p className="alert neutral">
                      提醒已暂时关闭，将在 {new Date(settings.update_reminder_snooze_until * 1000).toLocaleString('zh-CN')} 后恢复
                    </p>
                  )}
                </div>
              )}
            </>
          )}
        </section>

        <section className="command-panel">
          <div className="panel-title">
            <div>
              <p className="eyebrow">Foreground</p>
              <h3>前台应用检测</h3>
            </div>
            <MonitorDot size={20} />
            <button
              aria-expanded={expandedPanels.foreground}
              className="settings-collapse-button"
              onClick={() => togglePanel('foreground')}
              type="button"
            >
              <span>{foregroundApp?.process_name ?? '诊断'}</span>
              <ChevronDown size={17} />
            </button>
          </div>
          {expandedPanels.foreground && (
            <>
              <p className="panel-copy">用于验证 Windows API 能否识别当前正在使用的窗口和进程。</p>
              <button
                className="secondary-action"
                disabled={loading}
                onClick={() => void handleDetectForegroundApp()}
                type="button"
              >
                <Activity size={17} />
                {loading ? '检测中' : '检测当前应用'}
              </button>

              {foregroundApp && (
                <div className="details-card stacked">
                  <Detail label="进程名" value={foregroundApp.process_name} />
                  <Detail label="进程 ID" value={String(foregroundApp.process_id)} />
                  <Detail label="窗口标题" value={foregroundApp.window_title || '无标题'} />
                  <Detail label="进程路径" value={foregroundApp.process_path || '无法读取'} />
                </div>
              )}
            </>
          )}
        </section>
      </div>

      <div className="settings-grid lower">
        <section className="command-panel">
          <div className="panel-title">
            <div>
              <p className="eyebrow">Health</p>
              <h3>运行健康</h3>
            </div>
            <Activity size={20} />
            <button
              aria-expanded={expandedPanels.runtimeHealth}
              className="settings-collapse-button"
              onClick={() => togglePanel('runtimeHealth')}
              type="button"
            >
              <span>{statusLabel(runtimeStatus)}</span>
              <ChevronDown size={17} />
            </button>
          </div>
          {expandedPanels.runtimeHealth && (
            <>
              <p className="panel-copy">
                集中查看后台任务、受保护存储和本机运行环境状态，用于同步、提醒和更新异常排查。
              </p>
              {runtimeHealthMessage && <p className="alert neutral">{runtimeHealthMessage}</p>}
              <div className="details-card stacked">
                <Detail label="总体状态" value={statusLabel(runtimeStatus)} />
                <Detail
                  label="检查时间"
                  value={formatDateTime(runtimeHealth?.checked_at ?? runtimeHealth?.generated_at)}
                />
                <Detail label="摘要" value={runtimeHealth?.summary ?? '暂无运行诊断摘要'} />
              </div>

              <div className="settings-list">
                <Capability
                  enabled={runtimeStatus === 'ok' || runtimeStatus === 'healthy'}
                  icon={ShieldCheck}
                  text={`整体健康：${statusLabel(runtimeStatus)}`}
                />
                <Capability
                  enabled={protectedStorage?.status === 'ok' || protectedStorage?.status === 'healthy'}
                  icon={Database}
                  text={`受保护存储：${statusLabel(protectedStorage?.status)}`}
                />
                <Capability
                  enabled={activeSyncEnabled}
                  icon={RefreshCw}
                  text={`当前同步通道：${syncBackendLabel(settings.sync_backend)} · ${enabledLabel(activeSyncEnabled)}`}
                />
              </div>

              {protectedStorage && (
                <div className="details-card stacked">
                  <Detail label={protectedStorage.label ?? '受保护存储'} value={statusLabel(protectedStorage.status)} />
                  <Detail
                    label="说明"
                    value={protectedStorage.message ?? protectedStorage.detail ?? '未返回额外说明'}
                  />
                  <Detail label="检查时间" value={formatDateTime(protectedStorage.checked_at)} />
                </div>
              )}

              {healthChecks.length > 0 && (
                <div className="settings-list">
                  {healthChecks.map((check) => (
                    <div className="details-card stacked" key={check.key ?? check.label ?? check.status}>
                      <Detail label={check.label ?? check.key ?? '检查项'} value={statusLabel(check.status)} />
                      <Detail label="说明" value={check.message ?? check.detail ?? '未返回额外说明'} />
                      <Detail label="检查时间" value={formatDateTime(check.checked_at)} />
                    </div>
                  ))}
                </div>
              )}

              {runtimeTasks.length > 0 && (
                <div className="settings-list">
                  {runtimeTasks.map((task) => (
                    <div className="details-card stacked" key={task.task}>
                      <Detail label="后台任务" value={taskLabel(task.task)} />
                      <Detail label="状态" value={statusLabel(task.status)} />
                      <Detail label="最近成功" value={formatDateTime(task.last_success_at)} />
                      <Detail label="下次重试" value={formatDateTime(task.next_retry_at)} />
                      {shortError(task.last_error) && (
                        <Detail label="最近错误" value={shortError(task.last_error) ?? ''} />
                      )}
                    </div>
                  ))}
                </div>
              )}

              <button className="secondary-action" onClick={() => void handleRefreshRuntimeHealth()} type="button">
                <RefreshCw size={17} />
                刷新诊断
              </button>
            </>
          )}
        </section>

        <section className="command-panel">
          <div className="panel-title">
            <div>
              <p className="eyebrow">Privacy</p>
              <h3>隐私与数据边界</h3>
            </div>
            <ShieldCheck size={20} />
            <button
              aria-expanded={expandedPanels.privacyData}
              className="settings-collapse-button"
              onClick={() => togglePanel('privacyData')}
              type="button"
            >
              <span>{dataLocation ? '已定位' : '查看'}</span>
              <ChevronDown size={17} />
            </button>
          </div>
          {expandedPanels.privacyData && (
            <>
              <p className="panel-copy">
                敏感凭据只做状态展示，不在界面回显原文；本页用于确认哪些能力会访问本地数据或外部服务。
              </p>
              <div className="settings-list">
                <Capability
                  enabled={Boolean(dataLocation)}
                  icon={Database}
                  text="学习记录、清单和统计保存在本机 SQLite 数据库"
                />
                <Capability
                  enabled={activeSyncEnabled}
                  icon={RefreshCw}
                  text={`${syncBackendLabel(settings.sync_backend)} 同步：${enabledLabel(activeSyncEnabled)}`}
                />
                <Capability
                  enabled={emailSettings.enabled}
                  icon={BellRing}
                  text={`邮件提醒：${enabledLabel(emailSettings.enabled)} · 密码${configuredLabel(emailSettings.password_configured || Boolean(emailSettings.password))}`}
                />
                <Capability
                  enabled={feishuSettings.enabled}
                  icon={ShieldCheck}
                  text={`飞书桥接：${enabledLabel(feishuSettings.enabled)} · ${feishuStatus?.authenticated ? '已认证' : configuredLabel(feishuSettings.app_secret_configured || Boolean(feishuSettings.app_secret))}`}
                />
              </div>

              <div style={{ minHeight: '48px', position: 'relative' }}>
                <p
                  aria-atomic="true"
                  aria-live="polite"
                  className="alert success"
                  role="status"
                  style={{
                    inset: 0,
                    margin: 0,
                    opacity: systemMessage ? 1 : 0,
                    overflow: 'hidden',
                    pointerEvents: 'none',
                    position: 'absolute',
                    textOverflow: 'ellipsis',
                    whiteSpace: 'nowrap',
                  }}
                >
                  {systemMessage ?? '\u00A0'}
                </p>
              </div>

              <div className="details-card stacked">
                <Detail label="数据目录" value={dataLocation?.app_data_dir ?? '尚未读取到本机数据目录'} />
                <Detail label="SQLite 文件" value={dataLocation?.database_path ?? '尚未读取到本机数据库文件'} />
                <Detail
                  label="主设备标识"
                  value={settings.primary_owner_device_id ? '已绑定当前同步主设备' : '未绑定主设备'}
                />
              </div>

              <div className="row-actions">
                <button className="secondary-action" onClick={() => void handleCopyAppDataLocation()} type="button">
                  <Copy size={17} />
                  复制路径信息
                </button>
                <button
                  className="secondary-action"
                  onClick={() => void handleCopySystemDiagnosticSummary()}
                  type="button"
                >
                  <ClipboardList size={17} />
                  复制诊断摘要
                </button>
                <button
                  className="secondary-action"
                  onClick={() => void handleDownloadSystemDiagnosticSummary()}
                  type="button"
                >
                  <Download size={17} />
                  导出诊断摘要
                </button>
                <button className="secondary-action" onClick={() => void handleOpenAppDataLocation()} type="button">
                  <FolderOpen size={17} />
                  打开数据目录
                </button>
              </div>

              <div className="details-card stacked">
                <Detail
                  label="WebDAV"
                  value={`${enabledLabel(webDavSettings.enabled)} · 密码${configuredLabel(webDavSettings.password_configured || Boolean(webDavSettings.password))}`}
                />
                <Detail
                  label="对象存储"
                  value={`${enabledLabel(objectStorageSettings.enabled)} · 密钥${configuredLabel(objectStorageSettings.secret_access_key_configured || Boolean(objectStorageSettings.secret_access_key))}`}
                />
                <Detail
                  label="飞书授权"
                  value={
                    feishuStatus?.authenticated
                      ? `已认证，过期时间 ${formatDateTime(feishuStatus.expires_at)}`
                      : '未认证或等待授权'
                  }
                />
                <Detail
                  label="提醒邮件"
                  value={`${enabledLabel(emailSettings.enabled)} · SMTP ${emailSettings.smtp_host || '未配置'}`}
                />
              </div>
            </>
          )}
        </section>
      </div>
    </div>
  );
}
