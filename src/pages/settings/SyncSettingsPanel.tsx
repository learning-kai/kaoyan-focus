import { Activity, ChevronDown, Cloud, Download, RefreshCw, RotateCcw, Save, UploadCloud } from 'lucide-react';
import type { AppSettings, ObjectStorageSettings, SyncBackupEntry, SyncBackend, SyncRunSummary, WebDavSettings } from '../../types/settings';
import { Detail, formatBytes } from './SettingsPrimitives';
import type { SettingsPanelKey } from './types';

type SyncSettingsPanelProps = {
  expandedPanels: Record<SettingsPanelKey, boolean>;
  handleDownloadObjectStorage: () => Promise<void>;
  handleDownloadWebDav: () => Promise<void>;
  handlePreviewBackup: (entry: SyncBackupEntry) => Promise<void>;
  handleRestoreBackup: (entry: SyncBackupEntry) => Promise<void>;
  handleSaveObjectStorageSettings: () => Promise<void>;
  handleSaveWebDavSettings: () => Promise<void>;
  handleTestObjectStorage: () => Promise<void>;
  handleTestWebDav: () => Promise<void>;
  handleUploadObjectStorage: () => Promise<void>;
  handleUploadWebDav: () => Promise<void>;
  lastAutoSyncMessage: string | null;
  objectStorageActionDisabled: boolean;
  objectStorageBusy: boolean;
  objectStorageMessage: string | null;
  objectStorageSettings: ObjectStorageSettings;
  refreshSyncDetails: () => Promise<void>;
  settings: AppSettings;
  settingsLocked: boolean;
  syncBackups: SyncBackupEntry[];
  syncDetailMessage: string | null;
  syncRuns: SyncRunSummary[];
  togglePanel: (panel: SettingsPanelKey) => void;
  updateObjectStorageSettings: (patch: Partial<ObjectStorageSettings>) => void;
  updateSyncBackend: (syncBackend: SyncBackend) => void;
  updateWebDavSettings: (patch: Partial<WebDavSettings>) => void;
  webDavActionDisabled: boolean;
  webDavBusy: boolean;
  webDavMessage: string | null;
  webDavSettings: WebDavSettings;
};

export function SyncSettingsPanel({
  expandedPanels,
  handleDownloadObjectStorage,
  handleDownloadWebDav,
  handlePreviewBackup,
  handleRestoreBackup,
  handleSaveObjectStorageSettings,
  handleSaveWebDavSettings,
  handleTestObjectStorage,
  handleTestWebDav,
  handleUploadObjectStorage,
  handleUploadWebDav,
  lastAutoSyncMessage,
  objectStorageActionDisabled,
  objectStorageBusy,
  objectStorageMessage,
  objectStorageSettings,
  refreshSyncDetails,
  settings,
  settingsLocked,
  syncBackups,
  syncDetailMessage,
  syncRuns,
  togglePanel,
  updateObjectStorageSettings,
  updateSyncBackend,
  updateWebDavSettings,
  webDavActionDisabled,
  webDavBusy,
  webDavMessage,
  webDavSettings,
}: SyncSettingsPanelProps) {
  return (
    <div
      aria-labelledby="settings-tab-sync"
      className="settings-tab-panel"
      id="settings-panel-sync"
      role="tabpanel"
    >
      <section className="command-panel sync-backend-panel">
        <div className="panel-title">
          <div>
            <p className="eyebrow">Sync Backend</p>
            <h3>云同步方式</h3>
          </div>
          <Cloud size={20} />
        </div>
        <div className="setting-row mode-setting">
          <div>
            <strong>同步后端</strong>
            <p>WebDAV 保持默认；对象存储用于 Cloudflare R2、MinIO 或其他 S3 兼容服务。</p>
          </div>
          <div className="segmented-control">
            <button className={settings.sync_backend === 'webdav' ? 'active' : ''} disabled={settingsLocked} onClick={() => updateSyncBackend('webdav')} type="button">WebDAV</button>
            <button className={settings.sync_backend === 'object_storage' ? 'active' : ''} disabled={settingsLocked} onClick={() => updateSyncBackend('object_storage')} type="button">对象存储</button>
          </div>
        </div>
      </section>

      <div className="settings-grid">
        <section className="command-panel">
          <div className="panel-title">
            <div>
              <p className="eyebrow">WebDAV</p>
              <h3>数据同步</h3>
            </div>
            <Cloud size={20} />
            <button
              aria-expanded={expandedPanels.webdav}
              className="settings-collapse-button"
              onClick={() => togglePanel('webdav')}
              type="button"
            >
              <span>{webDavSettings.enabled ? '已启用' : '已关闭'}</span>
              <ChevronDown size={17} />
            </button>
          </div>
          {expandedPanels.webdav && (
            <>
              <p className="panel-copy">填写 WebDAV 地址和账号后，可以把本机 SQLite 数据库上传到云端，或从云端恢复到本机。</p>

              <label className="capability-row sync-toggle-row">
                <Cloud size={17} />
                <input
                  checked={webDavSettings.enabled}
                  disabled={settingsLocked}
                  onChange={(event) => updateWebDavSettings({ enabled: event.target.checked })}
                  type="checkbox"
                />
                <span>启用 WebDAV 同步</span>
              </label>

              <div className="form-stack">
                <label className="field-block">
                  <span>WebDAV 地址</span>
                  <input className="text-input" disabled={settingsLocked} onChange={(event) => updateWebDavSettings({ url: event.target.value })} placeholder="https://dav.example.com/remote.php/dav/files/me" value={webDavSettings.url} />
                </label>
                <div className="inline-fields">
                  <label className="field-block">
                    <span>用户名</span>
                    <input className="text-input" disabled={settingsLocked} onChange={(event) => updateWebDavSettings({ username: event.target.value })} placeholder="用户名" value={webDavSettings.username} />
                  </label>
                  <label className="field-block">
                    <span>密码</span>
                    <input className="text-input" disabled={settingsLocked} onChange={(event) => updateWebDavSettings({ password: event.target.value })} placeholder="密码或应用密钥" type="password" value={webDavSettings.password} />
                  </label>
                </div>
                <label className="field-block">
                  <span>远端文件路径</span>
                  <input className="text-input" disabled={settingsLocked} onChange={(event) => updateWebDavSettings({ remote_path: event.target.value })} placeholder="kaoyan-focus/kaoyan-focus.sqlite3" value={webDavSettings.remote_path} />
                </label>
              </div>

              {lastAutoSyncMessage && <p className="alert neutral">启动自动同步：{lastAutoSyncMessage}</p>}
              {!webDavSettings.enabled && <p className="alert neutral">WebDAV 同步已关闭，保存配置后不会参与启动自动同步。</p>}
              {webDavMessage && <p className="alert success">{webDavMessage}</p>}
              <div className="row-actions">
                <button className="secondary-action" disabled={webDavBusy || settingsLocked} onClick={() => void handleSaveWebDavSettings()} type="button"><Save size={17} />保存</button>
                <button className="secondary-action" disabled={webDavActionDisabled} onClick={() => void handleTestWebDav()} type="button"><Cloud size={17} />测试连接</button>
                <button className="primary-action" disabled={webDavActionDisabled} onClick={() => void handleUploadWebDav()} type="button"><UploadCloud size={17} />上传本机数据</button>
                <button className="secondary-action" disabled={webDavActionDisabled} onClick={() => void handleDownloadWebDav()} type="button"><Download size={17} />从云端恢复</button>
              </div>
            </>
          )}
        </section>

        <section className="command-panel">
          <div className="panel-title">
            <div>
              <p className="eyebrow">Sync Journal</p>
              <h3>最近同步详情</h3>
            </div>
            <Activity size={20} />
            <button
              aria-expanded={expandedPanels.syncJournal}
              className="settings-collapse-button"
              onClick={() => togglePanel('syncJournal')}
              type="button"
            >
              <span>{syncRuns[0]?.status ?? '暂无记录'}</span>
              <ChevronDown size={17} />
            </button>
          </div>
          {expandedPanels.syncJournal && (
            <>
              {syncDetailMessage && <p className="alert neutral">{syncDetailMessage}</p>}
              <div className="settings-list">
                {syncRuns.length === 0 ? (
                  <p className="panel-copy">暂无同步日志。完成一次对象存储同步后会显示校验、备份和接管信息。</p>
                ) : syncRuns.map((run) => (
                  <div className="details-card stacked" key={run.id}>
                    <Detail label={`${run.trigger} · ${run.status}`} value={run.finished_at} />
                    <Detail label="方向 / 耗时" value={`${run.direction ?? 'skip'} · ${run.duration_ms}ms`} />
                    <Detail label="实体 / 删除 / 字节" value={`${run.exported_count} / ${run.deleted_count} / ${formatBytes(run.bytes)}`} />
                    <Detail label="Active / Remote" value={`${run.active_snapshot_sync_id ?? 'none'} (${run.active_snapshot_phase ?? '-'}) / ${run.remote_active_snapshot_sync_id ?? 'none'} (${run.remote_active_snapshot_phase ?? '-'})`} />
                    <Detail label="UpdatedAt" value={`${run.active_snapshot_updated_at ?? '-'} / ${run.remote_snapshot_updated_at ?? '-'}`} />
                    {run.remote_exported_drift_seconds !== null && <Detail label="Clock drift" value={`${run.remote_exported_drift_seconds}s`} />}
                    {run.detail && <Detail label="Decision" value={run.detail} />}
                    {run.validation_report && <Detail label="校验" value={run.validation_report} />}
                    {run.backup_path && <Detail label="本地备份" value={run.backup_path} />}
                    {run.remote_backup_key && <Detail label="云端备份" value={run.remote_backup_key} />}
                  </div>
                ))}
              </div>
              <div className="row-actions">
                <button className="secondary-action" disabled={objectStorageBusy} onClick={() => void refreshSyncDetails()} type="button"><RefreshCw size={17} />刷新详情</button>
              </div>
            </>
          )}
        </section>

        <section className="command-panel">
          <div className="panel-title">
            <div>
              <p className="eyebrow">Restore</p>
              <h3>备份恢复</h3>
            </div>
            <RotateCcw size={20} />
            <button
              aria-expanded={expandedPanels.backups}
              className="settings-collapse-button"
              onClick={() => togglePanel('backups')}
              type="button"
            >
              <span>{syncBackups.length} 个备份</span>
              <ChevronDown size={17} />
            </button>
          </div>
          {expandedPanels.backups && (
            <>
              <p className="panel-copy">恢复前先点预检查看校验结果；真正恢复会自动备份当前本地数据库。</p>
              <div className="settings-list">
                {syncBackups.length === 0 ? (
                  <p className="panel-copy">暂无可用备份。</p>
                ) : syncBackups.slice(0, 8).map((entry) => (
                  <div className="details-card stacked" key={`${entry.source}:${entry.key}`}>
                    <Detail label={`${entry.source.toUpperCase()} · ${entry.label}`} value={entry.created_at ?? '未知时间'} />
                    <Detail label="大小" value={entry.bytes === null ? '未知' : formatBytes(entry.bytes)} />
                    <div className="row-actions">
                      <button className="secondary-action" disabled={objectStorageBusy || settingsLocked} onClick={() => void handlePreviewBackup(entry)} type="button">预检</button>
                      <button className="secondary-action" disabled={objectStorageBusy || settingsLocked} onClick={() => void handleRestoreBackup(entry)} type="button">恢复</button>
                    </div>
                  </div>
                ))}
              </div>
            </>
          )}
        </section>

        <section className="command-panel">
          <div className="panel-title">
            <div>
              <p className="eyebrow">S3 / R2</p>
              <h3>对象存储同步</h3>
            </div>
            <Cloud size={20} />
            <button
              aria-expanded={expandedPanels.objectStorage}
              className="settings-collapse-button"
              onClick={() => togglePanel('objectStorage')}
              type="button"
            >
              <span>{objectStorageSettings.enabled ? objectStorageSettings.object_key || '已启用' : '已关闭'}</span>
              <ChevronDown size={17} />
            </button>
          </div>
          {expandedPanels.objectStorage && (
            <>
              <p className="panel-copy">填写 Cloudflare R2 或 S3 兼容对象存储信息后，应用会使用共享 JSON 数据包与手机端同步。</p>

              <label className="capability-row sync-toggle-row">
                <Cloud size={17} />
                <input
                  checked={objectStorageSettings.enabled}
                  disabled={settingsLocked}
                  onChange={(event) => updateObjectStorageSettings({ enabled: event.target.checked })}
                  type="checkbox"
                />
                <span>启用对象存储同步</span>
              </label>

              <div className="form-stack">
                <label className="field-block">
                  <span>Endpoint</span>
                  <input className="text-input" disabled={settingsLocked} onChange={(event) => updateObjectStorageSettings({ endpoint: event.target.value })} placeholder="https://<account-id>.r2.cloudflarestorage.com" value={objectStorageSettings.endpoint} />
                </label>
                <div className="inline-fields">
                  <label className="field-block">
                    <span>Bucket</span>
                    <input className="text-input" disabled={settingsLocked} onChange={(event) => updateObjectStorageSettings({ bucket: event.target.value })} placeholder="kaoyan-focus" value={objectStorageSettings.bucket} />
                  </label>
                  <label className="field-block">
                    <span>Region</span>
                    <input className="text-input" disabled={settingsLocked} onChange={(event) => updateObjectStorageSettings({ region: event.target.value })} placeholder="auto" value={objectStorageSettings.region} />
                  </label>
                </div>
                <div className="inline-fields">
                  <label className="field-block">
                    <span>Access Key ID</span>
                    <input className="text-input" disabled={settingsLocked} onChange={(event) => updateObjectStorageSettings({ access_key_id: event.target.value })} placeholder="由对象存储控制台生成" value={objectStorageSettings.access_key_id} />
                  </label>
                  <label className="field-block">
                    <span>Secret Access Key</span>
                    <input className="text-input" disabled={settingsLocked} onChange={(event) => updateObjectStorageSettings({ secret_access_key: event.target.value })} placeholder="只保存在本机数据" type="password" value={objectStorageSettings.secret_access_key} />
                  </label>
                </div>
                <label className="field-block">
                  <span>Object Key</span>
                  <input className="text-input" disabled={settingsLocked} onChange={(event) => updateObjectStorageSettings({ object_key: event.target.value })} placeholder="study-sync.json" value={objectStorageSettings.object_key} />
                </label>
              </div>

              {objectStorageMessage && <p className="alert success">{objectStorageMessage}</p>}
              {!objectStorageSettings.enabled && <p className="alert neutral">对象存储同步已关闭，保存配置后不会参与启动自动同步。</p>}
              <div className="row-actions">
                <button className="secondary-action" disabled={objectStorageBusy || settingsLocked} onClick={() => void handleSaveObjectStorageSettings()} type="button"><Save size={17} />保存</button>
                <button className="secondary-action" disabled={objectStorageActionDisabled} onClick={() => void handleTestObjectStorage()} type="button"><Cloud size={17} />测试连接</button>
                <button className="primary-action" disabled={objectStorageActionDisabled} onClick={() => void handleUploadObjectStorage()} type="button"><UploadCloud size={17} />上传本机数据</button>
                <button className="secondary-action" disabled={objectStorageActionDisabled} onClick={() => void handleDownloadObjectStorage()} type="button"><Download size={17} />从云端恢复</button>
              </div>
            </>
          )}
        </section>
      </div>
    </div>
  );
}
