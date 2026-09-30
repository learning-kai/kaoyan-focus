import { useEffect, type CSSProperties } from 'react';
import { EyeOff, Sparkles } from 'lucide-react';
import { useMicroBreak } from '../../hooks/useMicroBreak';
import { loadMicroBreakSettings, previewMicroBreak, updateMicroBreakSettings } from '../../services/microBreakCoordinator';
import {
  MICRO_BREAK_INTERVAL_PRESETS,
  MICRO_BREAK_METHOD_PRESET,
  MICRO_BREAK_REST_PRESETS,
  describeMicroBreak,
  findIntervalPreset,
} from '../../utils/microBreak';

type MicroBreakPanelProps = {
  /** 当前是番茄钟时才显示「90/20 专注法」一键预设。 */
  showMethodPreset: boolean;
  /** 一键套用 90 分钟专注 + 20 分钟休息，由专注页负责改自己的节奏参数。 */
  onApplyMethodPreset: () => void;
};

/** 专注页「本次节奏」里的随机微休息设置。 */
export default function MicroBreakPanel({ onApplyMethodPreset, showMethodPreset }: MicroBreakPanelProps) {
  const { settings, settingsStatus, settingsError, active } = useMicroBreak();

  useEffect(() => {
    void loadMicroBreakSettings();
  }, []);

  const intervalPreset = findIntervalPreset(settings);
  const disabled = !settings.enabled;
  const volumeStyle = { '--sound-volume-percent': `${settings.volume}%` } as CSSProperties;

  function applyMethodPreset() {
    updateMicroBreakSettings({ enabled: true, min_interval_seconds: 3 * 60, max_interval_seconds: 5 * 60, rest_seconds: 10, adaptive: true });
    onApplyMethodPreset();
  }

  return (
    <section className="micro-break-panel" aria-label="随机微休息">
      <label className="capability-row focus-whitelist-toggle">
        <span className="micro-break-panel-title">
          <EyeOff size={15} aria-hidden="true" />
          随机微休息
        </span>
        <input
          checked={settings.enabled}
          disabled={settingsStatus === 'loading'}
          onChange={(event) => updateMicroBreakSettings({ enabled: event.target.checked })}
          role="switch"
          type="checkbox"
        />
      </label>
      <p className="focus-primary-hint">
        {settings.enabled
          ? describeMicroBreak(settings) + '。听到提示音就闭眼，听到第二声再继续，计时不暂停。'
          : '专注中随机响起提示音，闭眼休息几秒再继续。间隔不可预测，大脑会在短暂休息里快速回放刚学的内容。'}
      </p>

      {settings.enabled && (
        <div className="micro-break-options">
          <div className="micro-break-field">
            <span>提示间隔</span>
            <div className="segmented-control micro-break-segments">
              {MICRO_BREAK_INTERVAL_PRESETS.map((preset) => (
                <button
                  aria-pressed={intervalPreset?.id === preset.id}
                  className={intervalPreset?.id === preset.id ? 'active' : ''}
                  key={preset.id}
                  onClick={() => updateMicroBreakSettings({ min_interval_seconds: preset.min_interval_seconds, max_interval_seconds: preset.max_interval_seconds })}
                  type="button"
                >
                  {preset.label}
                </button>
              ))}
            </div>
          </div>

          <div className="micro-break-field">
            <span>闭眼时长</span>
            <div className="segmented-control micro-break-segments">
              {MICRO_BREAK_REST_PRESETS.map((seconds) => (
                <button
                  aria-pressed={settings.rest_seconds === seconds}
                  className={settings.rest_seconds === seconds ? 'active' : ''}
                  key={seconds}
                  onClick={() => updateMicroBreakSettings({ rest_seconds: seconds })}
                  type="button"
                >
                  {seconds} 秒
                </button>
              ))}
            </div>
          </div>

          <label className="capability-row focus-whitelist-toggle micro-break-adaptive">
            <span>
              按专注算法调整频率
              <small>前 30 分钟密、30–60 分钟放宽、60 分钟后加密</small>
            </span>
            <input
              checked={settings.adaptive}
              disabled={disabled}
              onChange={(event) => updateMicroBreakSettings({ adaptive: event.target.checked })}
              role="switch"
              type="checkbox"
            />
          </label>

          <div className="micro-break-volume">
            <span>提示音量</span>
            <input
              aria-label="微休息提示音量"
              aria-valuetext={`${settings.volume}%`}
              className="sound-volume-slider"
              max={100}
              min={0}
              onChange={(event) => updateMicroBreakSettings({ volume: Number(event.target.value) })}
              style={volumeStyle}
              type="range"
              value={settings.volume}
            />
            <button className="micro-break-preview" disabled={Boolean(active)} onClick={() => previewMicroBreak(Math.min(settings.rest_seconds, 5), settings.volume)} type="button">
              试听
            </button>
          </div>
        </div>
      )}

      {showMethodPreset && (
        <button className="micro-break-method" onClick={applyMethodPreset} type="button">
          <Sparkles size={15} aria-hidden="true" />
          <span>
            <strong>一键套用 90/20 专注法</strong>
            <small>
              专注 {MICRO_BREAK_METHOD_PRESET.focus_minutes} 分钟 · 休息 {MICRO_BREAK_METHOD_PRESET.break_minutes} 分钟 · 3–5 分钟随机闭眼 10 秒
            </small>
          </span>
        </button>
      )}

      {settingsStatus === 'unavailable' && <p className="focus-primary-hint">浏览器预览不会保存设置，也不会在专注中触发提示。</p>}
      {settingsError && (
        <p className="alert error" role="alert">
          {settingsError}
        </p>
      )}
    </section>
  );
}
