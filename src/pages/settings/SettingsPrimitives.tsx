import type { LucideIcon } from 'lucide-react';

export function formatBytes(bytes: number) {
  if (bytes < 1024) {
    return `${bytes} B`;
  }

  if (bytes < 1024 * 1024) {
    return `${(bytes / 1024).toFixed(1)} KB`;
  }

  return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}

export function SettingNumber({
  disabled,
  label,
  max,
  min,
  onChange,
  step = 1,
  text,
  unit = '分钟',
  value,
}: {
  disabled: boolean;
  label: string;
  max: number;
  min: number;
  onChange: (value: number) => void;
  /** 加减按钮的单次步长，默认 1；区间很大时（如 1-720 分钟）可传更大的值。 */
  step?: number;
  text: string;
  unit?: string;
  value: number;
}) {
  function applyStep(delta: number) {
    onChange(Math.min(max, Math.max(min, value + delta * step)));
  }

  return (
    <div className="setting-row rhythm-card">
      <div>
        <strong>{label}</strong>
        <p>{text}</p>
      </div>
      <div className="stepper-control">
        <button aria-label={`${label}减少${step}`} disabled={disabled || value <= min} onClick={() => applyStep(-1)} type="button">-</button>
        <label>
          <input
            className="number-input"
            disabled={disabled}
            max={max}
            min={min}
            onChange={(event) => onChange(Math.min(max, Math.max(min, Number(event.target.value) || min)))}
            type="number"
            value={value}
          />
          <span>{unit}</span>
        </label>
        <button aria-label={`${label}增加${step}`} disabled={disabled || value >= max} onClick={() => applyStep(1)} type="button">+</button>
      </div>
    </div>
  );
}

export function Capability({ enabled, icon: Icon, text }: { enabled: boolean; icon: LucideIcon; text: string }) {
  return (
    <label className="capability-row">
      <Icon size={17} />
      <input checked={enabled} readOnly type="checkbox" />
      <span>{text}</span>
    </label>
  );
}

export function Detail({ label, value }: { label: string; value: string }) {
  return (
    <div>
      <span>{label}</span>
      <strong>{value}</strong>
    </div>
  );
}
