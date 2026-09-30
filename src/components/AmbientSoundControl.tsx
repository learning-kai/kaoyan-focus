import { useCallback, useEffect, useId, useLayoutEffect, useRef, useState, type CSSProperties } from 'react';
import { createPortal } from 'react-dom';
import './AmbientSoundControl.css';
import {
  AudioLines,
  AudioWaveform,
  Bird,
  Building2,
  CloudLightning,
  CloudRain,
  Coffee,
  Droplets,
  Flame,
  Headphones,
  MoonStar,
  RotateCcw,
  Sailboat,
  TrainFront,
  Waves,
  Wind,
  X,
  type LucideIcon,
} from 'lucide-react';
import { useAmbientSound } from '../hooks/useAmbientSound';
import {
  clearAmbientSoundMix,
  setAmbientSoundChannelVolume,
  setAmbientSoundEnabled,
  setAmbientSoundMasterVolume,
  toggleAmbientSoundChannel,
} from '../services/ambientSoundPlayer';
import { openExternalUrl } from '../services/systemApi';
import {
  AMBIENT_SOUND_GROUP_LABELS,
  AMBIENT_SOUND_GROUP_ORDER,
  AMBIENT_SOUND_PROCESSED_BY,
  AMBIENT_SOUNDS,
  countActiveAmbientSounds,
  isAmbientMixAudible,
} from '../utils/ambientSound';

const SOUND_ICONS: Record<string, LucideIcon> = {
  rain: CloudRain,
  storm: CloudLightning,
  waves: Waves,
  wind: Wind,
  stream: Droplets,
  birds: Bird,
  'summer-night': MoonStar,
  train: TrainFront,
  city: Building2,
  boat: Sailboat,
  'coffee-shop': Coffee,
  fireplace: Flame,
  'pink-noise': AudioWaveform,
  'white-noise': AudioLines,
};

const PANEL_GAP = 12;
const PANEL_MARGIN = 16;
const PANEL_WIDTH = 400;

function volumeStyle(volume: number): CSSProperties {
  return { '--sound-volume-percent': `${volume}%` } as CSSProperties;
}

async function openCreditLink(url: string) {
  try {
    await openExternalUrl(url);
  } catch {
    // 浏览器预览没有桌面命令，退回到新标签页。
    window.open(url, '_blank', 'noopener,noreferrer');
  }
}

/**
 * 侧边栏里的白噪音入口 + 混音面板。
 * 面板通过 portal 挂到 body，避免被侧边栏的 overflow / backdrop-filter 裁切。
 */
export default function AmbientSoundControl() {
  const { mix, failedIds } = useAmbientSound();
  const [open, setOpen] = useState(false);
  const [panelStyle, setPanelStyle] = useState<CSSProperties>({});
  const triggerRef = useRef<HTMLButtonElement>(null);
  const panelRef = useRef<HTMLDivElement>(null);
  const panelId = useId();
  const titleId = useId();

  const activeCount = countActiveAmbientSounds(mix);
  const audible = isAmbientMixAudible(mix);
  const statusText = audible ? `播放中 · ${activeCount} 种` : activeCount > 0 ? `已暂停 · ${activeCount} 种` : '未开启';

  const closePanel = useCallback((restoreFocus: boolean) => {
    setOpen(false);
    if (restoreFocus) triggerRef.current?.focus({ preventScroll: true });
  }, []);

  const updatePosition = useCallback(() => {
    const trigger = triggerRef.current;
    if (!trigger) return;
    const rect = trigger.getBoundingClientRect();
    const width = Math.min(PANEL_WIDTH, window.innerWidth - PANEL_MARGIN * 2);
    const maxHeight = window.innerHeight - PANEL_MARGIN * 2;

    // 侧边栏在左侧时，面板贴着侧边栏右边、底部与入口对齐；
    // 窄屏下侧边栏变成顶栏时，面板放到入口下方。
    const besideSidebar = rect.right + PANEL_GAP + width <= window.innerWidth - PANEL_MARGIN && rect.top > window.innerHeight / 2;
    if (besideSidebar) {
      setPanelStyle({
        left: rect.right + PANEL_GAP,
        bottom: Math.max(PANEL_MARGIN, window.innerHeight - rect.bottom),
        width,
        maxHeight,
      });
    } else {
      const top = Math.min(rect.bottom + PANEL_GAP, window.innerHeight - PANEL_MARGIN - 240);
      setPanelStyle({
        left: Math.max(PANEL_MARGIN, Math.min(rect.left, window.innerWidth - PANEL_MARGIN - width)),
        top: Math.max(PANEL_MARGIN, top),
        width,
        maxHeight: window.innerHeight - Math.max(PANEL_MARGIN, top) - PANEL_MARGIN,
      });
    }
  }, []);

  useLayoutEffect(() => {
    if (!open) return;
    updatePosition();
    window.addEventListener('resize', updatePosition);
    return () => window.removeEventListener('resize', updatePosition);
  }, [open, updatePosition]);

  useEffect(() => {
    if (!open) return;
    panelRef.current?.focus({ preventScroll: true });

    function handlePointerDown(event: PointerEvent) {
      const target = event.target as Node | null;
      if (!target) return;
      if (panelRef.current?.contains(target) || triggerRef.current?.contains(target)) return;
      closePanel(false);
    }

    function handleKeyDown(event: KeyboardEvent) {
      if (event.key === 'Escape') {
        event.stopPropagation();
        closePanel(true);
      }
    }

    document.addEventListener('pointerdown', handlePointerDown);
    document.addEventListener('keydown', handleKeyDown);
    return () => {
      document.removeEventListener('pointerdown', handlePointerDown);
      document.removeEventListener('keydown', handleKeyDown);
    };
  }, [open, closePanel]);

  return (
    <div className="ambient-sound-entry">
      <button
        aria-controls={open ? panelId : undefined}
        aria-expanded={open}
        aria-haspopup="dialog"
        aria-label={`白噪音：${statusText}`}
        className={audible ? 'ambient-sound-trigger is-playing' : 'ambient-sound-trigger'}
        onClick={() => (open ? closePanel(false) : setOpen(true))}
        ref={triggerRef}
        title="白噪音混音"
        type="button"
      >
        <Headphones size={17} />
        <span className="ambient-sound-trigger-copy">
          <strong>白噪音</strong>
          <small>{statusText}</small>
        </span>
        {audible && <span aria-hidden="true" className="ambient-sound-live" />}
      </button>

      {open &&
        createPortal(
          <div
            aria-labelledby={titleId}
            className="ambient-sound-panel"
            id={panelId}
            ref={panelRef}
            role="dialog"
            style={panelStyle}
            tabIndex={-1}
          >
            <header className="ambient-sound-head">
              <div>
                <p className="eyebrow">专注环境音</p>
                <h3 id={titleId}>白噪音混音</h3>
              </div>
              <button aria-label="关闭白噪音面板" className="ambient-sound-icon-button" onClick={() => closePanel(true)} type="button">
                <X size={16} />
              </button>
            </header>

            <section className="ambient-sound-master" aria-label="总控制">
              <label className="capability-row focus-whitelist-toggle ambient-sound-switch">
                <span>{activeCount === 0 ? '未选择声音' : mix.enabled ? '正在播放' : '已暂停'}</span>
                <input
                  aria-label="白噪音总开关"
                  checked={mix.enabled && activeCount > 0}
                  disabled={activeCount === 0}
                  onChange={(event) => setAmbientSoundEnabled(event.target.checked)}
                  role="switch"
                  type="checkbox"
                />
              </label>
              <div className="ambient-sound-master-volume">
                <span>总音量</span>
                <input
                  aria-label="白噪音总音量"
                  aria-valuetext={`${mix.masterVolume}%`}
                  className="sound-volume-slider"
                  max={100}
                  min={0}
                  onChange={(event) => setAmbientSoundMasterVolume(Number(event.target.value))}
                  style={volumeStyle(mix.masterVolume)}
                  type="range"
                  value={mix.masterVolume}
                />
                <strong>{mix.masterVolume}%</strong>
              </div>
              <button className="ambient-sound-clear" disabled={activeCount === 0} onClick={clearAmbientSoundMix} type="button">
                <RotateCcw size={14} />
                全部取消
              </button>
            </section>

            <p className="ambient-sound-hint">
              {activeCount === 0 ? '点选下面的声音开始播放，可以同时选多种叠加。' : '点选声音开关，拖动下方滑块调各自音量。'}
            </p>

            <div className="ambient-sound-groups">
              {AMBIENT_SOUND_GROUP_ORDER.map((group) => (
                <section className="ambient-sound-group" key={group} aria-label={AMBIENT_SOUND_GROUP_LABELS[group]}>
                  <p className="ambient-sound-group-label">{AMBIENT_SOUND_GROUP_LABELS[group]}</p>
                  <div className="ambient-sound-grid">
                    {AMBIENT_SOUNDS.filter((sound) => sound.group === group).map((sound) => {
                      const channel = mix.sounds[sound.id];
                      const active = Boolean(channel?.active);
                      const volume = channel?.volume ?? 0;
                      const failed = failedIds.includes(sound.id);
                      const Icon = SOUND_ICONS[sound.id] ?? AudioLines;
                      const tileClass = ['ambient-sound-tile', active ? 'is-active' : '', active && mix.enabled ? 'is-playing' : '', failed ? 'is-failed' : '']
                        .filter(Boolean)
                        .join(' ');
                      return (
                        <div className={tileClass} key={sound.id}>
                          <button
                            aria-pressed={active}
                            className="ambient-sound-toggle"
                            onClick={() => toggleAmbientSoundChannel(sound.id)}
                            title={failed ? `${sound.label}加载失败，点击重试` : sound.label}
                            type="button"
                          >
                            <Icon size={20} />
                            <span>{sound.label}</span>
                            {failed && <small>加载失败</small>}
                          </button>
                          <input
                            aria-label={`${sound.label}音量`}
                            aria-valuetext={`${volume}%`}
                            className="sound-volume-slider ambient-sound-tile-volume"
                            max={100}
                            min={0}
                            onChange={(event) => setAmbientSoundChannelVolume(sound.id, Number(event.target.value))}
                            style={volumeStyle(volume)}
                            type="range"
                            value={volume}
                          />
                        </div>
                      );
                    })}
                  </div>
                </section>
              ))}
            </div>

            <details className="ambient-sound-credits">
              <summary>音源署名与协议</summary>
              <ul>
                {AMBIENT_SOUNDS.map((sound) => (
                  <li key={sound.id}>
                    <span>{sound.sourceTitle}</span>
                    <span>
                      <button className="ambient-sound-link" onClick={() => void openCreditLink(sound.sourceUrl)} type="button">
                        {sound.author}
                      </button>
                      {' · '}
                      <button className="ambient-sound-link" onClick={() => void openCreditLink(sound.licenseUrl)} type="button">
                        {sound.license}
                      </button>
                    </span>
                  </li>
                ))}
              </ul>
              <p>
                循环片段由{' '}
                <button className="ambient-sound-link" onClick={() => void openCreditLink(AMBIENT_SOUND_PROCESSED_BY.url)} type="button">
                  {AMBIENT_SOUND_PROCESSED_BY.name}
                </button>{' '}
                项目剪辑处理。
              </p>
            </details>
          </div>,
          document.body,
        )}
    </div>
  );
}
