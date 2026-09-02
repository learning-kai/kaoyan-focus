import { useEffect, useId, useRef, useState } from 'react';
import { RotateCcw, Timer } from 'lucide-react';
import {
  FOCUS_MINUTES_MAX,
  FOCUS_MINUTES_MIN,
  FOCUS_PRESET_MINUTES,
  formatFocusDurationLabel,
  isFocusPresetMinutes,
  parseFocusMinutes,
  validateFocusMinutes,
} from '../../utils/focusDuration';

type FocusDurationPickerProps = {
  /** 当前生效的番茄专注时长（分钟），一定是合法值。 */
  value: number;
  /** 选中新的合法时长时触发，由调用方决定是否持久化。 */
  onChange: (minutes: number) => void;
  /** 是否把本次选择保存为下次默认时长。 */
  rememberDefault: boolean;
  onRememberDefaultChange: (remember: boolean) => void;
  disabled?: boolean;
};

/**
 * 开始专注前的番茄时长选择区：
 * 预设 chip 覆盖常见节奏，自定义输入框支持 1-120 分钟之间的任意整数。
 *
 * 输入框只保留合法输入：一旦解析失败就展示内联提示并保持上一个生效值不变，
 * 失焦时自动回填，避免把 NaN 或越界数字带进开始专注的请求里。
 */
export default function FocusDurationPicker({
  disabled = false,
  onChange,
  onRememberDefaultChange,
  rememberDefault,
  value,
}: FocusDurationPickerProps) {
  const inputId = useId();
  const inputRef = useRef<HTMLInputElement>(null);
  const [draft, setDraft] = useState(() => String(value));
  const [error, setError] = useState<string | null>(null);

  // 外部值变化（读取设置、点击预设、日程带入）时把输入框同步回生效值。
  useEffect(() => {
    setDraft(String(value));
    setError(null);
  }, [value]);

  function handlePresetClick(minutes: number) {
    if (disabled) return;
    setDraft(String(minutes));
    setError(null);
    onChange(minutes);
  }

  function handleDraftChange(next: string) {
    setDraft(next);
    const parsed = parseFocusMinutes(next);
    if (parsed !== null) {
      setError(null);
      onChange(parsed);
      return;
    }
    setError(validateFocusMinutes(next).message);
  }

  // 失焦或回车：非法输入不生效，直接回填当前生效值。
  function handleCommit() {
    if (parseFocusMinutes(draft) === null) {
      setDraft(String(value));
      setError(null);
    }
  }

  return (
    <div className="focus-duration-picker">
      <div className="focus-duration-head">
        <div>
          <span>
            <Timer size={14} />
            番茄专注时长
          </span>
          <p>选择预设或自定义本轮番茄钟的时长，范围 {FOCUS_MINUTES_MIN}-{FOCUS_MINUTES_MAX} 分钟。</p>
        </div>
        <strong>{formatFocusDurationLabel(value)}</strong>
      </div>

      <div aria-label="番茄专注时长预设" className="focus-duration-chips" role="group">
        {FOCUS_PRESET_MINUTES.map((minutes) => (
          <button
            aria-pressed={value === minutes}
            className={value === minutes ? 'focus-duration-chip active' : 'focus-duration-chip'}
            disabled={disabled}
            key={minutes}
            onClick={() => handlePresetClick(minutes)}
            type="button"
          >
            <b>{minutes}</b>
            <small>分钟</small>
          </button>
        ))}
        <button
          aria-pressed={!isFocusPresetMinutes(value)}
          className={!isFocusPresetMinutes(value) ? 'focus-duration-chip custom active' : 'focus-duration-chip custom'}
          disabled={disabled}
          onClick={() => inputRef.current?.focus()}
          type="button"
        >
          <b>自定义</b>
          <small>1-{FOCUS_MINUTES_MAX}</small>
        </button>
      </div>

      <div className="focus-duration-input-row">
        <label htmlFor={inputId}>自定义分钟数</label>
        <div className="focus-duration-input">
          <input
            aria-describedby={error ? `${inputId}-error` : undefined}
            aria-invalid={error ? true : undefined}
            autoComplete="off"
            className="text-input"
            disabled={disabled}
            id={inputId}
            inputMode="numeric"
            max={FOCUS_MINUTES_MAX}
            maxLength={3}
            min={FOCUS_MINUTES_MIN}
            onBlur={handleCommit}
            onChange={(event) => handleDraftChange(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === 'Enter') {
                event.preventDefault();
                handleCommit();
              }
            }}
            placeholder={String(value)}
            ref={inputRef}
            type="text"
            value={draft}
          />
          <span>分钟</span>
        </div>
      </div>

      {error && (
        <p className="focus-duration-error" id={`${inputId}-error`} role="alert">
          {error}
        </p>
      )}

      <label className="settings-switch focus-whitelist-toggle focus-duration-remember">
        <input
          aria-label="记住番茄专注时长"
          checked={rememberDefault}
          disabled={disabled}
          onChange={(event) => onRememberDefaultChange(event.target.checked)}
          role="switch"
          type="checkbox"
        />
        <span>{rememberDefault ? '记住为默认时长' : '仅本次生效'}</span>
      </label>

      {!rememberDefault && (
        <p className="focus-duration-hint">
          <RotateCcw size={13} />
          本次选择只用于这一个番茄钟，下次回到设置里的默认时长。
        </p>
      )}
    </div>
  );
}
