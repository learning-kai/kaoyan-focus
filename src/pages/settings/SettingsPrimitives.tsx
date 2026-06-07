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
  text,
  unit = '分钟',
  value,
}: {
  disabled: boolean;
  label: string;
  max: number;
  min: number;
  onChange: (value: number) => void;
  text: string;
  unit?: string;
  value: number;
}) {
  function step(delta: number) {
    onChange(Math.min(max, Math.max(min, value + delta)));
  }

  return (
    <div className="setting-row rhythm-card">
      <div>
        <strong>{label}</strong>
        <p>{text}</p>
      </div>
      <div className="stepper-control">
        <button aria-label={`${label}减少`} disabled={disabled || value <= min} onClick={() => step(-1)} type="button">-</button>
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
        <button aria-label={`${label}增加`} disabled={disabled || value >= max} onClick={() => step(1)} type="button">+</button>
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
