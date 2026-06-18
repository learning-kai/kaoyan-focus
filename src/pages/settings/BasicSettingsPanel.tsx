import type { ChangeEvent, CSSProperties } from 'react';
import { useState } from 'react';
import { BellRing, Coffee, MonitorUp, Music2, Play, Power, RotateCcw, Save, Settings2, UploadCloud, VolumeX } from 'lucide-react';
import type { AppSettings, AppTheme, ReminderSoundId, ReminderSoundSource } from '../../types/settings';
import { APP_THEME_OPTIONS } from '../../theme';
import { SettingNumber } from './SettingsPrimitives';

type SettingsTab = 'rhythm' | 'automation' | 'widget' | 'sound';

type ReminderSoundSourceOption = {
  value: ReminderSoundSource;
  label: string;
  description: string;
};

type ReminderSoundOption = {
  id: ReminderSoundId;
  label: string;
  description: string;
};

type BasicSettingsPanelProps = {
  currentReminderSoundOption: ReminderSoundOption;
  currentReminderSoundSourceOption: ReminderSoundSourceOption;
  customReminderSoundFile: File | null;
  customReminderSoundInputKey: number;
  handlePreviewReminderSound: () => Promise<void>;
  handleReminderSoundFileChange: (event: ChangeEvent<HTMLInputElement>) => void;
  handleReminderSoundSourceChange: (source: ReminderSoundSource) => void;
  handleResetReminderSound: () => Promise<void>;
  handleSaveSettings: () => Promise<void>;
  handleUploadReminderSound: () => Promise<void>;
  onThemeChange: (theme: AppTheme) => void;
  reminderSoundActionDisabled: boolean;
  reminderSoundBusy: boolean;
  reminderSoundMessage: string | null;
  reminderSoundOptions: ReminderSoundOption[];
  reminderSoundSourceOptions: ReminderSoundSourceOption[];
  reminderSoundVolumeStyle: CSSProperties;
  saving: boolean;
  settings: AppSettings;
  settingsLocked: boolean;
  theme: AppTheme;
  updateReminderSoundSettings: (patch: Partial<Pick<AppSettings, 'reminder_sound_source' | 'reminder_sound_id' | 'reminder_sound_volume' | 'reminder_sound_duration_seconds'>>) => void;
  updateSettings: (patch: Partial<AppSettings>) => void;
};

export function BasicSettingsPanel({
  currentReminderSoundOption,
  currentReminderSoundSourceOption,
  customReminderSoundFile,
  customReminderSoundInputKey,
  handlePreviewReminderSound,
  handleReminderSoundFileChange,
  handleReminderSoundSourceChange,
  handleResetReminderSound,
  handleSaveSettings,
  handleUploadReminderSound,
  onThemeChange,
  reminderSoundActionDisabled,
  reminderSoundBusy,
  reminderSoundMessage,
  reminderSoundOptions,
  reminderSoundSourceOptions,
  reminderSoundVolumeStyle,
  saving,
  settings,
  settingsLocked,
  theme,
  updateReminderSoundSettings,
  updateSettings,
}: BasicSettingsPanelProps) {
  const [activeSettingsTab, setActiveSettingsTab] = useState<SettingsTab>('rhythm');

  return (
    <div
      aria-labelledby="settings-tab-basic"
      className="settings-tab-panel"
      id="settings-panel-basic"
      role="tabpanel"
    >
      <div className="settings-tabs">
        <button className={`settings-tab ${activeSettingsTab === 'rhythm' ? 'active' : ''}`} onClick={() => setActiveSettingsTab('rhythm')} type="button">学习节奏</button>
        <button className={`settings-tab ${activeSettingsTab === 'automation' ? 'active' : ''}`} onClick={() => setActiveSettingsTab('automation')} type="button">自动化</button>
        <button className={`settings-tab ${activeSettingsTab === 'widget' ? 'active' : ''}`} onClick={() => setActiveSettingsTab('widget')} type="button">悬浮窗</button>
        <button className={`settings-tab ${activeSettingsTab === 'sound' ? 'active' : ''}`} onClick={() => setActiveSettingsTab('sound')} type="button">提醒音效</button>
      </div>

      <section className="command-panel rhythm-section">
        {activeSettingsTab === 'rhythm' && (
          <>
            <div className="panel-title">
              <div>
                <p className="eyebrow">Rhythm</p>
                <h3>学习节奏</h3>
              </div>
              <Settings2 size={20} />
            </div>

            <div className="rhythm-grid">
              <SettingNumber label="学习模式时长" max={720} min={1} disabled={settingsLocked} onChange={(value) => updateSettings({ default_study_minutes: value })} text="进入学习模式后的总约束时间。" value={settings.default_study_minutes} />
              <SettingNumber label="番茄专注时长" max={120} min={1} disabled={settingsLocked} onChange={(value) => updateSettings({ default_focus_minutes: value })} text="学习模式内每轮番茄钟的专注分钟数。" value={settings.default_focus_minutes} />
              <SettingNumber label="短休" max={60} min={1} disabled={settingsLocked} onChange={(value) => updateSettings({ break_minutes: value })} text="普通番茄轮次结束后的休息分钟数。" value={settings.break_minutes} />
              <SettingNumber label="长休" max={120} min={1} disabled={settingsLocked} onChange={(value) => updateSettings({ long_break_minutes: value })} text="达到长休息轮次后的休息分钟数。" value={settings.long_break_minutes} />
              <SettingNumber label="长休间隔" max={12} min={1} disabled={settingsLocked} onChange={(value) => updateSettings({ long_break_interval: value })} text="每几个番茄钟进入一次长休息。" value={settings.long_break_interval} unit="轮" />

              <div className="setting-row mode-setting">
                <div>
                  <strong>默认专注模式</strong>
                  <p>普通模式更轻量，强制模式会保持更严格的学习约束。</p>
                </div>
                <div className="segmented-control">
                  <button className={settings.default_focus_mode === 'normal' ? 'active' : ''} disabled={settingsLocked} onClick={() => updateSettings({ default_focus_mode: 'normal' })} type="button">普通</button>
                  <button className={settings.default_focus_mode === 'strict' ? 'active' : ''} disabled={settingsLocked} onClick={() => updateSettings({ default_focus_mode: 'strict' })} type="button">强制</button>
                </div>
              </div>

              <div className="setting-row mode-setting">
                <div>
                  <strong>前台规则模式</strong>
                  <p>{settings.whitelist_mode === 'blocklist' ? '黑名单模式：默认放行，命中规则才拦截。' : '白名单模式：只放行命中规则的应用、网站或视频。'}</p>
                </div>
                <div className="segmented-control">
                  <button className={settings.whitelist_mode === 'allowlist' ? 'active' : ''} disabled={settingsLocked} onClick={() => updateSettings({ whitelist_mode: 'allowlist' })} type="button">白名单</button>
                  <button className={settings.whitelist_mode === 'blocklist' ? 'active' : ''} disabled={settingsLocked} onClick={() => updateSettings({ whitelist_mode: 'blocklist' })} type="button">黑名单</button>
                </div>
              </div>

              <div className="setting-row mode-setting theme-setting-row">
                <div>
                  <strong>界面配色</strong>
                  <p>选择适合当前学习状态的界面风格，顶部也可以快速切换。</p>
                </div>
                <div className="theme-choice-grid" role="radiogroup" aria-label="界面配色">
                  {APP_THEME_OPTIONS.map((option) => (
                    <button
                      aria-checked={theme === option.id}
                      className={theme === option.id ? 'active' : ''}
                      disabled={settingsLocked}
                      key={option.id}
                      onClick={() => { onThemeChange(option.id); updateSettings({ ui_theme: option.id }); }}
                      role="radio"
                      type="button"
                    >
                      <span
                        aria-hidden="true"
                        className="theme-swatch"
                        style={{ background: `linear-gradient(135deg, ${option.swatch[0]}, ${option.swatch[1]} 58%, ${option.swatch[2]})` }}
                      />
                      <span>
                        <strong>{option.label}</strong>
                        <small>{option.description}</small>
                      </span>
                    </button>
                  ))}
                </div>
              </div>

              <div className="setting-row mode-setting">
                <div>
                  <strong>开机自启</strong>
                  <p>随 Windows 登录自动启动，方便后台提醒和同步。</p>
                </div>
                <div className="segmented-control">
                  <button className={settings.launch_at_startup ? 'active' : ''} disabled={settingsLocked} onClick={() => updateSettings({ launch_at_startup: true })} type="button"><Power size={15} />开启</button>
                  <button className={!settings.launch_at_startup ? 'active' : ''} disabled={settingsLocked} onClick={() => updateSettings({ launch_at_startup: false })} type="button">关闭</button>
                </div>
              </div>
            </div>
          </>
        )}

        {activeSettingsTab === 'automation' && (
          <div className="automation-settings-panel">
            <div className="reminder-sound-heading">
              <div>
                <span>Automation</span>
                <h4>自动化与提醒策略</h4>
                <p className="panel-copy">把高频动作交给应用处理：休息确认、课表提前提醒、夜间静音都可以在这里统一控制。</p>
              </div>
              <BellRing size={20} />
            </div>

            <div className="automation-settings-grid">
              <div className="setting-row mode-setting">
                <div>
                  <strong>番茄结束后自动休息</strong>
                  <p>番茄钟到点后直接进入短休或长休，不再停在等待休息确认。</p>
                </div>
                <div className="segmented-control">
                  <button className={settings.auto_start_break_after_focus ? 'active' : ''} disabled={settingsLocked} onClick={() => updateSettings({ auto_start_break_after_focus: true })} type="button"><Coffee size={15} />开启</button>
                  <button className={!settings.auto_start_break_after_focus ? 'active' : ''} disabled={settingsLocked} onClick={() => updateSettings({ auto_start_break_after_focus: false })} type="button">手动确认</button>
                </div>
              </div>

              <div className="setting-row mode-setting">
                <div>
                  <strong>课表提前提醒</strong>
                  <p>今日课表开始前提前通知，关闭后只保留闹钟和专注阶段提醒。</p>
                </div>
                <div className="segmented-control">
                  <button className={settings.schedule_reminder_enabled ? 'active' : ''} disabled={settingsLocked} onClick={() => updateSettings({ schedule_reminder_enabled: true })} type="button"><BellRing size={15} />开启</button>
                  <button className={!settings.schedule_reminder_enabled ? 'active' : ''} disabled={settingsLocked} onClick={() => updateSettings({ schedule_reminder_enabled: false })} type="button">关闭</button>
                </div>
              </div>

              <SettingNumber
                disabled={settingsLocked || !settings.schedule_reminder_enabled}
                label="课表提醒提前量"
                max={60}
                min={0}
                onChange={(value) => updateSettings({ schedule_reminder_lead_minutes: value })}
                text="0 表示到点提醒；建议 5-10 分钟，足够收尾并切换状态。"
                value={settings.schedule_reminder_lead_minutes}
              />

              <div className="setting-row mode-setting">
                <div>
                  <strong>免打扰时段</strong>
                  <p>该时段内通知仍显示，但提醒声音会静音，适合夜间复盘或宿舍场景。</p>
                </div>
                <div className="segmented-control">
                  <button className={settings.reminder_quiet_hours_enabled ? 'active' : ''} disabled={settingsLocked} onClick={() => updateSettings({ reminder_quiet_hours_enabled: true })} type="button"><VolumeX size={15} />开启</button>
                  <button className={!settings.reminder_quiet_hours_enabled ? 'active' : ''} disabled={settingsLocked} onClick={() => updateSettings({ reminder_quiet_hours_enabled: false })} type="button">关闭</button>
                </div>
              </div>

              <div className="setting-row quiet-hours-row">
                <div>
                  <strong>静音时间</strong>
                  <p>支持跨天时段，例如 22:30 到 07:00。</p>
                </div>
                <div className="quiet-hours-inputs">
                  <label>
                    <span>开始</span>
                    <input className="time-input" disabled={settingsLocked || !settings.reminder_quiet_hours_enabled} onChange={(event) => updateSettings({ reminder_quiet_hours_start: event.target.value })} type="time" value={settings.reminder_quiet_hours_start} />
                  </label>
                  <label>
                    <span>结束</span>
                    <input className="time-input" disabled={settingsLocked || !settings.reminder_quiet_hours_enabled} onChange={(event) => updateSettings({ reminder_quiet_hours_end: event.target.value })} type="time" value={settings.reminder_quiet_hours_end} />
                  </label>
                </div>
              </div>
            </div>
          </div>
        )}

        {activeSettingsTab === 'widget' && (
          <>
            <div className="panel-title">
              <div>
                <p className="eyebrow">Widget</p>
                <h3>悬浮窗</h3>
              </div>
              <MonitorUp size={20} />
            </div>

            <div className="rhythm-grid">
              <div className="setting-row mode-setting">
                <div>
                  <strong>悬浮窗倒计时</strong>
                  <p>开启后可保留显示独立置顶小窗；暂停、结束、退出或空闲时不会被强制隐藏。</p>
                </div>
                <div className="segmented-control">
                  <button className={settings.focus_widget_enabled ? 'active' : ''} disabled={settingsLocked} onClick={() => updateSettings({ focus_widget_enabled: true })} type="button">
                    <MonitorUp size={15} />
                    开启
                  </button>
                  <button className={!settings.focus_widget_enabled ? 'active' : ''} disabled={settingsLocked} onClick={() => updateSettings({ focus_widget_enabled: false })} type="button">
                    关闭
                  </button>
                </div>
              </div>

              <div className="setting-row mode-setting">
                <div>
                  <strong>自动跟随学习模式</strong>
                  <p>开始学习或暂停时自动显示悬浮窗；手动隐藏后，本次学习不会再次自动弹出。</p>
                </div>
                <div className="segmented-control">
                  <button className={settings.focus_widget_auto_follow ? 'active' : ''} disabled={settingsLocked || !settings.focus_widget_enabled} onClick={() => updateSettings({ focus_widget_auto_follow: true })} type="button">
                    开启
                  </button>
                  <button className={!settings.focus_widget_auto_follow ? 'active' : ''} disabled={settingsLocked || !settings.focus_widget_enabled} onClick={() => updateSettings({ focus_widget_auto_follow: false })} type="button">
                    关闭
                  </button>
                </div>
              </div>

              <div className="setting-row mode-setting">
                <div>
                  <strong>记住悬浮窗位置</strong>
                  <p>拖动或调整悬浮窗后保存位置和尺寸，下次显示时恢复到同一位置。</p>
                </div>
                <div className="segmented-control">
                  <button className={settings.focus_widget_remember_geometry ? 'active' : ''} disabled={settingsLocked || !settings.focus_widget_enabled} onClick={() => updateSettings({ focus_widget_remember_geometry: true })} type="button">
                    开启
                  </button>
                  <button className={!settings.focus_widget_remember_geometry ? 'active' : ''} disabled={settingsLocked || !settings.focus_widget_enabled} onClick={() => updateSettings({ focus_widget_remember_geometry: false })} type="button">
                    关闭
                  </button>
                </div>
              </div>

              <div className="setting-row mode-setting">
                <div>
                  <strong>悬浮窗总在最前</strong>
                  <p>开启后悬浮窗保持在其他窗口上方；关闭后按普通窗口层级显示。</p>
                </div>
                <div className="segmented-control">
                  <button className={settings.focus_widget_always_on_top ? 'active' : ''} disabled={settingsLocked || !settings.focus_widget_enabled} onClick={() => updateSettings({ focus_widget_always_on_top: true })} type="button">
                    置顶
                  </button>
                  <button className={!settings.focus_widget_always_on_top ? 'active' : ''} disabled={settingsLocked || !settings.focus_widget_enabled} onClick={() => updateSettings({ focus_widget_always_on_top: false })} type="button">
                    普通层级
                  </button>
                </div>
              </div>
            </div>
          </>
        )}

        {activeSettingsTab === 'sound' && (
          <div className="reminder-sound-panel">
            <div className="reminder-sound-heading">
              <div>
                <span>Reminder Sound</span>
                <h4>提醒音效</h4>
                <p className="panel-copy">当前：{currentReminderSoundSourceOption.label} · {currentReminderSoundOption.label}。可试听内置音色，也可上传自己的提醒音频。</p>
              </div>
              <button className="secondary-action" disabled={reminderSoundActionDisabled} onClick={() => void handlePreviewReminderSound()} type="button">
                <Play size={17} />
                {reminderSoundBusy ? '处理中' : '试听'}
              </button>
            </div>

            <div className="segmented-control secondary-segmented">
              {reminderSoundSourceOptions.map((option) => (
                <button
                  className={settings.reminder_sound_source === option.value ? 'active' : ''}
                  disabled={settingsLocked}
                  key={option.value}
                  onClick={() => handleReminderSoundSourceChange(option.value)}
                  type="button"
                >
                  <Music2 size={15} />
                  {option.label}
                </button>
              ))}
            </div>

            {settings.reminder_sound_source === 'builtin' ? (
              <div className="sound-option-grid">
                {reminderSoundOptions.map((option) => (
                  <button
                    className={settings.reminder_sound_id === option.id ? 'sound-option-card active' : 'sound-option-card'}
                    disabled={settingsLocked}
                    key={option.id}
                    onClick={() => updateReminderSoundSettings({ reminder_sound_id: option.id })}
                    type="button"
                  >
                    <strong>{option.label}</strong>
                    <span>{option.description}</span>
                  </button>
                ))}
              </div>
            ) : (
              <div className="custom-sound-row">
                <label className={settingsLocked ? 'custom-sound-picker disabled' : 'custom-sound-picker'}>
                  <span className="custom-sound-picker-icon"><UploadCloud size={20} /></span>
                  <span className="custom-sound-picker-copy">
                    <strong>{customReminderSoundFile?.name ?? '选择音频文件'}</strong>
                    <small>{settings.reminder_sound_file_name ? `当前已保存：${settings.reminder_sound_file_name}` : '支持系统可播放的本地音频文件。'}</small>
                  </span>
                  <input
                    className="custom-sound-input"
                    disabled={settingsLocked}
                    key={customReminderSoundInputKey}
                    onChange={handleReminderSoundFileChange}
                    type="file"
                  />
                </label>
                {settings.reminder_sound_file_name && (
                  <div className="custom-sound-file">
                    <span>当前自定义音频</span>
                    <strong>{settings.reminder_sound_file_name}</strong>
                    {settings.reminder_sound_updated_at && <small>{settings.reminder_sound_updated_at}</small>}
                  </div>
                )}
              </div>
            )}

            <div className="sound-volume-row">
              <div>
                <span>音量</span>
                <strong>{settings.reminder_sound_volume}%</strong>
                <small>试听和保存后的提醒都会使用这个音量。</small>
              </div>
              <input
                className="sound-volume-slider"
                disabled={settingsLocked}
                max={100}
                min={0}
                onChange={(event) => updateReminderSoundSettings({ reminder_sound_volume: Number(event.target.value) })}
                style={reminderSoundVolumeStyle}
                type="range"
                value={settings.reminder_sound_volume}
              />
            </div>

            <SettingNumber
              disabled={settingsLocked}
              label="持续时间"
              max={300}
              min={5}
              onChange={(value) => updateReminderSoundSettings({ reminder_sound_duration_seconds: value })}
              text="提醒音效播放多久后自动停止。"
              unit="秒"
              value={settings.reminder_sound_duration_seconds}
            />

            {reminderSoundMessage && <p className="alert success">{reminderSoundMessage}</p>}
            <div className="row-actions">
              <button className="custom-sound-confirm" disabled={reminderSoundActionDisabled || !customReminderSoundFile} onClick={() => void handleUploadReminderSound()} type="button"><UploadCloud size={17} />上传并启用</button>
              <button className="secondary-action" disabled={reminderSoundActionDisabled || (!settings.reminder_sound_file_name && !customReminderSoundFile)} onClick={() => void handleResetReminderSound()} type="button"><RotateCcw size={17} />恢复默认</button>
            </div>
          </div>
        )}
      </section>

      <div className="settings-save-row">
        <div className="settings-save-copy">
          <span>当前默认节奏</span>
          <strong>
            <b>{settings.default_focus_minutes}</b>
            <small>专注</small>
            <b>{settings.break_minutes}</b>
            <small>短休</small>
            <b>{settings.long_break_minutes}</b>
            <small>长休</small>
            <b>{settings.long_break_interval}</b>
            <small>轮一次</small>
          </strong>
        </div>
        <button className="primary-action" disabled={saving || settingsLocked} onClick={() => void handleSaveSettings()} type="button">
          <Save size={18} />
          {saving ? '保存中' : '保存设置'}
        </button>
      </div>
    </div>
  );
}
