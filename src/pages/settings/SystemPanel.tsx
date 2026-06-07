import { Activity, BellRing, ChevronDown, Database, Download, HardDrive, MonitorDot, RefreshCw, ShieldCheck } from 'lucide-react';
import type { AppUpdate } from '../../services/updateApi';
import type { ForegroundApp } from '../../types/monitor';
import { Capability, Detail } from './SettingsPrimitives';
import type { SettingsPanelKey } from './types';

type SystemPanelProps = {
  availableUpdate: AppUpdate | null;
  checkingUpdate: boolean;
  dataLocation: string | null;
  expandedPanels: Record<SettingsPanelKey, boolean>;
  foregroundApp: ForegroundApp | null;
  handleCheckUpdate: () => Promise<void>;
  handleDetectForegroundApp: () => Promise<void>;
  handleInstallUpdate: () => Promise<void>;
  installingUpdate: boolean;
  loading: boolean;
  settingsLocked: boolean;
  togglePanel: (panel: SettingsPanelKey) => void;
  updateMessage: string | null;
  updateProgress: number | null;
};

export function SystemPanel({
  availableUpdate,
  checkingUpdate,
  dataLocation,
  expandedPanels,
  foregroundApp,
  handleCheckUpdate,
  handleDetectForegroundApp,
  handleInstallUpdate,
  installingUpdate,
  loading,
  settingsLocked,
  togglePanel,
  updateMessage,
  updateProgress,
}: SystemPanelProps) {
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

              {dataLocation && (
                <div className="details-card">
                  <span>数据文件路径</span>
                  <strong>{dataLocation}</strong>
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
              {updateMessage && <p className="alert neutral">{updateMessage}</p>}
              {updateProgress !== null && <p className="alert neutral">下载进度 {updateProgress}%</p>}
              <div className="row-actions">
                <button className="secondary-action" disabled={checkingUpdate || installingUpdate || settingsLocked} onClick={() => void handleCheckUpdate()} type="button">
                  <RefreshCw size={17} />
                  {checkingUpdate ? '检查中' : '检查更新'}
                </button>
                <button className="primary-action" disabled={availableUpdate === null || installingUpdate || settingsLocked} onClick={() => void handleInstallUpdate()} type="button">
                  <Download size={17} />
                  {installingUpdate ? '安装中' : '下载并安装'}
                </button>
              </div>
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
              <button className="secondary-action" disabled={loading} onClick={() => void handleDetectForegroundApp()} type="button">
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
    </div>
  );
}
