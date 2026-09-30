import { useEffect } from 'react';
import { EyeOff } from 'lucide-react';
import './MicroBreak.css';
import { useMicroBreak, useMicroBreakCountdown } from '../../hooks/useMicroBreak';
import { endMicroBreakEarly } from '../../services/microBreakCoordinator';

/**
 * 闭眼微休息的全屏提示。挂在 App 顶层，任何页面都能看到。
 * 不抢焦点、不做焦点陷阱：用户闭着眼，界面只是给睁眼后的一瞥看的。
 */
export default function MicroBreakOverlay() {
  const { active } = useMicroBreak();
  const secondsLeft = useMicroBreakCountdown(active?.endsAt ?? null);

  useEffect(() => {
    if (!active) return;
    function handleKeyDown(event: KeyboardEvent) {
      if (event.key === 'Escape') endMicroBreakEarly();
    }
    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [active]);

  if (!active) return null;

  const title = active.preview ? '试听：闭上眼睛' : '闭上眼睛，休息一下';
  const meta = active.preview ? '这是一次试听' : `本轮第 ${active.cueNumber} 次微休息`;

  return (
    <div className="micro-break-overlay" role="status" aria-live="polite">
      <div className="micro-break-card">
        <span className="micro-break-icon" aria-hidden="true">
          <EyeOff size={28} />
        </span>
        <h2>{title}</h2>
        <strong className="micro-break-count" aria-label={`还剩 ${secondsLeft} 秒`}>
          {secondsLeft}
        </strong>
        <p>听到上行提示音后睁眼，继续刚才的事。计时不会暂停。</p>
        <small>{meta}</small>
        <button className="micro-break-skip" onClick={endMicroBreakEarly} type="button">
          我已休息好（Esc）
        </button>
      </div>
    </div>
  );
}
