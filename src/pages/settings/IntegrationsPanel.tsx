import { ChevronDown, ExternalLink, Mail, RefreshCw, Save } from 'lucide-react';
import type { EmailReminderSettings, FeishuSyncSettings, FeishuSyncStatus } from '../../types/settings';
import { Detail } from './SettingsPrimitives';
import type { SettingsPanelKey } from './types';

type IntegrationsPanelProps = {
  emailActionDisabled: boolean;
  emailBusy: boolean;
  emailMessage: string | null;
  emailSettings: EmailReminderSettings;
  expandedPanels: Record<SettingsPanelKey, boolean>;
  feishuActionDisabled: boolean;
  feishuBusy: boolean;
  feishuMessage: string | null;
  feishuSettings: FeishuSyncSettings;
  feishuStatus: FeishuSyncStatus | null;
  handleLogoutFeishu: () => Promise<void>;
  handleOpenFeishuLogin: () => Promise<void>;
  handlePollFeishuLogin: () => Promise<void>;
  handleRebuildFeishuTasklists: () => Promise<void>;
  handleSaveEmailSettings: () => Promise<void>;
  handleSaveFeishuSettings: () => Promise<void>;
  handleStartFeishuLogin: () => Promise<void>;
  handleSyncFeishu: () => Promise<void>;
  handleTestEmail: () => Promise<void>;
  settingsLocked: boolean;
  togglePanel: (panel: SettingsPanelKey) => void;
  updateEmailSettings: (patch: Partial<EmailReminderSettings>) => void;
  updateFeishuSettings: (patch: Partial<FeishuSyncSettings>) => void;
};

export function IntegrationsPanel({
  emailActionDisabled,
  emailBusy,
  emailMessage,
  emailSettings,
  expandedPanels,
  feishuActionDisabled,
  feishuBusy,
  feishuMessage,
  feishuSettings,
  feishuStatus,
  handleLogoutFeishu,
  handleOpenFeishuLogin,
  handlePollFeishuLogin,
  handleRebuildFeishuTasklists,
  handleSaveEmailSettings,
  handleSaveFeishuSettings,
  handleStartFeishuLogin,
  handleSyncFeishu,
  handleTestEmail,
  settingsLocked,
  togglePanel,
  updateEmailSettings,
  updateFeishuSettings,
}: IntegrationsPanelProps) {
  return (
    <div
      aria-labelledby="settings-tab-integrations"
      className="settings-tab-panel"
      id="settings-panel-integrations"
      role="tabpanel"
    >
      <div className="settings-grid">
        <section className="command-panel">
          <div className="panel-title">
            <div>
              <p className="eyebrow">Feishu</p>
              <h3>飞书任务 / 日历桥接</h3>
            </div>
            <ExternalLink size={20} />
            <button
              aria-expanded={expandedPanels.feishu}
              className="settings-collapse-button"
              onClick={() => togglePanel('feishu')}
              type="button"
            >
              <span>{feishuStatus?.authenticated ? '已登录' : feishuSettings.enabled ? '已启用' : '已关闭'}</span>
              <ChevronDown size={17} />
            </button>
          </div>
          {expandedPanels.feishu && (
            <>
              <p className="panel-copy">电脑端连接飞书开放平台，把清单同步到飞书任务，把课表同步到飞书日历。iPhone 可直接用飞书官方 App 查看和编辑。</p>

              <label className="capability-row sync-toggle-row">
                <ExternalLink size={17} />
                <input
                  checked={feishuSettings.enabled}
                  disabled={settingsLocked}
                  onChange={(event) => updateFeishuSettings({ enabled: event.target.checked })}
                  type="checkbox"
                />
                <span>启用飞书桥接同步</span>
              </label>

              <div className="form-stack">
                <label className="field-block">
                  <span>App ID</span>
                  <input
                    className="text-input"
                    disabled={settingsLocked}
                    onChange={(event) => updateFeishuSettings({ app_id: event.target.value })}
                    placeholder="飞书应用 App ID"
                    value={feishuSettings.app_id}
                  />
                </label>
                <label className="field-block">
                  <span>App Secret</span>
                  <input
                    className="text-input"
                    disabled={settingsLocked}
                    onChange={(event) => updateFeishuSettings({ app_secret: event.target.value })}
                    placeholder="只保存在本机，不进入 R2 同步"
                    type="password"
                    value={feishuSettings.app_secret}
                  />
                </label>
                <label className="field-block">
                  <span>回调地址</span>
                  <input
                    className="text-input"
                    disabled={settingsLocked}
                    onChange={(event) => updateFeishuSettings({ redirect_uri: event.target.value })}
                    placeholder="http://127.0.0.1:39781/feishu/callback"
                    value={feishuSettings.redirect_uri}
                  />
                </label>
                <div className="details-card stacked">
                  <Detail label="登录状态" value={feishuStatus?.authenticated ? '已登录' : '未登录'} />
                  <Detail label="飞书任务清单" value={feishuStatus?.tasklist_count ? `${feishuStatus.tasklist_count}/6 个分类清单` : '未创建'} />
                  <Detail label="飞书日历" value={feishuStatus?.calendar_id ? '考研专注' : '未创建'} />
                  <Detail label="回调地址" value={feishuStatus?.redirect_uri ?? feishuSettings.redirect_uri} />
                  <Detail label="需要权限" value={feishuStatus?.required_scopes ?? '读取中'} />
                  {feishuStatus?.last_run && (
                    <Detail label="最近同步" value={`${feishuStatus.last_run.status} · ${feishuStatus.last_run.finished_at}`} />
                  )}
                </div>
                {feishuStatus?.tasklists && feishuStatus.tasklists.length > 0 && (
                  <div className="details-card stacked">
                    {feishuStatus.tasklists.map((tasklist) => (
                      <Detail
                        key={tasklist.key}
                        label={tasklist.label}
                        value={tasklist.ready ? '已创建' : '未创建'}
                      />
                    ))}
                  </div>
                )}
                {feishuStatus?.pending_authorization_url && (
                  <div className="details-card stacked">
                    <Detail label="授权页" value={feishuStatus.pending_authorization_url} />
                    {feishuStatus.pending_message && <Detail label="提示" value={feishuStatus.pending_message} />}
                  </div>
                )}
              </div>

              {feishuMessage && <p className="alert success">{feishuMessage}</p>}
              {!feishuSettings.enabled && <p className="alert neutral">飞书桥接已关闭，自动同步会静默跳过。</p>}
              <div className="row-actions">
                <button className="secondary-action" disabled={feishuBusy || settingsLocked} onClick={() => void handleSaveFeishuSettings()} type="button"><Save size={17} />保存</button>
                <button className="secondary-action" disabled={feishuBusy || settingsLocked || !feishuSettings.app_id || !feishuSettings.app_secret} onClick={() => void handleStartFeishuLogin()} type="button"><ExternalLink size={17} />浏览器授权</button>
                <button className="secondary-action" disabled={feishuBusy || !feishuStatus?.pending_authorization_url} onClick={() => void handleOpenFeishuLogin()} type="button"><ExternalLink size={17} />打开授权页</button>
                <button className="secondary-action" disabled={feishuBusy} onClick={() => void handlePollFeishuLogin()} type="button"><RefreshCw size={17} />检查登录</button>
                <button className="primary-action" disabled={feishuActionDisabled || !feishuStatus?.authenticated} onClick={() => void handleSyncFeishu()} type="button"><RefreshCw size={17} />立即同步</button>
                <button className="secondary-action danger-action" disabled={feishuActionDisabled || !feishuStatus?.authenticated} onClick={() => void handleRebuildFeishuTasklists()} type="button"><RefreshCw size={17} />重建任务清单</button>
                <button className="secondary-action" disabled={feishuBusy || settingsLocked || !feishuStatus?.authenticated} onClick={() => void handleLogoutFeishu()} type="button">退出登录</button>
              </div>
            </>
          )}
        </section>

        <section className="command-panel">
          <div className="panel-title">
            <div>
              <p className="eyebrow">SMTP</p>
              <h3>截止任务邮件提醒</h3>
            </div>
            <Mail size={20} />
            <button
              aria-expanded={expandedPanels.email}
              className="settings-collapse-button"
              onClick={() => togglePanel('email')}
              type="button"
            >
              <span>{emailSettings.enabled ? '已启用' : '已关闭'}</span>
              <ChevronDown size={17} />
            </button>
          </div>
          {expandedPanels.email && (
            <>
              <p className="panel-copy">电脑端在每天 21:00 检查明天到期且未完成的清单/今日任务，并只发送一次邮件。SMTP 密码只保存在本机。</p>

              <label className="capability-row sync-toggle-row">
                <Mail size={17} />
                <input
                  checked={emailSettings.enabled}
                  disabled={settingsLocked}
                  onChange={(event) => updateEmailSettings({ enabled: event.target.checked })}
                  type="checkbox"
                />
                <span>启用邮件提醒</span>
              </label>

              <div className="form-stack">
                <div className="inline-fields">
                  <label className="field-block">
                    <span>SMTP Host</span>
                    <input className="text-input" disabled={settingsLocked} onChange={(event) => updateEmailSettings({ smtp_host: event.target.value })} placeholder="smtp.example.com" value={emailSettings.smtp_host} />
                  </label>
                  <label className="field-block">
                    <span>Port</span>
                    <input className="text-input" disabled={settingsLocked} min={1} max={65535} onChange={(event) => updateEmailSettings({ smtp_port: Number(event.target.value) || 465 })} type="number" value={emailSettings.smtp_port} />
                  </label>
                </div>
                <div className="inline-fields">
                  <label className="field-block">
                    <span>加密方式</span>
                    <select className="text-input" disabled={settingsLocked} onChange={(event) => updateEmailSettings({ smtp_security: event.target.value as EmailReminderSettings['smtp_security'] })} value={emailSettings.smtp_security}>
                      <option value="tls">TLS / SSL</option>
                      <option value="starttls">STARTTLS</option>
                      <option value="none">不加密</option>
                    </select>
                  </label>
                  <label className="field-block">
                    <span>账号</span>
                    <input className="text-input" disabled={settingsLocked} onChange={(event) => updateEmailSettings({ username: event.target.value })} placeholder="邮箱账号 / SMTP 用户名" value={emailSettings.username} />
                  </label>
                </div>
                <label className="field-block">
                  <span>授权码 / 密码</span>
                  <input className="text-input" disabled={settingsLocked} onChange={(event) => updateEmailSettings({ password: event.target.value })} placeholder="只保存在本机设置" type="password" value={emailSettings.password} />
                </label>
                <div className="inline-fields">
                  <label className="field-block">
                    <span>发件人</span>
                    <input className="text-input" disabled={settingsLocked} onChange={(event) => updateEmailSettings({ from: event.target.value })} placeholder="me@example.com" value={emailSettings.from} />
                  </label>
                  <label className="field-block">
                    <span>收件人</span>
                    <input className="text-input" disabled={settingsLocked} onChange={(event) => updateEmailSettings({ to: event.target.value })} placeholder="target@example.com" value={emailSettings.to} />
                  </label>
                </div>
              </div>

              {emailMessage && <p className="alert success">{emailMessage}</p>}
              {!emailSettings.enabled && <p className="alert neutral">邮件提醒已关闭，后台检查会静默跳过。</p>}
              <div className="row-actions">
                <button className="secondary-action" disabled={emailBusy || settingsLocked} onClick={() => void handleSaveEmailSettings()} type="button"><Save size={17} />保存</button>
                <button className="secondary-action" disabled={emailActionDisabled} onClick={() => void handleTestEmail()} type="button"><Mail size={17} />测试发送</button>
              </div>
            </>
          )}
        </section>
      </div>
    </div>
  );
}
