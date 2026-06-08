import { Coffee, Pause, Play } from 'lucide-react';
import type { StudyModeState } from '../../types/focus';

type ActiveFocusClockProps = {
  activeClockLabel: string;
  activeModeMessage: string;
  breakKindLabel: string;
  canTogglePause: boolean;
  phaseMessage: string;
  studyState: StudyModeState;
  timerValue: string;
  onConfirmBreak: () => void;
  onTogglePause: () => void;
};

export default function ActiveFocusClock({
  activeClockLabel,
  activeModeMessage,
  breakKindLabel,
  canTogglePause,
  onConfirmBreak,
  onTogglePause,
  phaseMessage,
  studyState,
  timerValue,
}: ActiveFocusClockProps) {
  return (
    <main className="focus-clock-zone">
      <p>{activeClockLabel}</p>
      <strong>{timerValue}</strong>
      <span>{phaseMessage}</span>
      <span>{activeModeMessage}</span>
      <div className="focus-round-controls">
        {canTogglePause && (
          <button
            aria-label={studyState.is_paused ? '继续计时' : '暂停计时'}
            className={studyState.is_paused ? 'focus-round-button primary' : 'focus-round-button'}
            onClick={onTogglePause}
            title={studyState.is_paused ? '继续计时' : '暂停'}
            type="button"
          >
            {studyState.is_paused ? <Play size={28} /> : <Pause size={28} />}
          </button>
        )}
        {studyState.phase === 'awaiting_break' && (
          <button
            aria-label={`确认开始${breakKindLabel}`}
            className="focus-round-button secondary"
            disabled={studyState.is_paused}
            onClick={onConfirmBreak}
            title={`确认开始${breakKindLabel}`}
            type="button"
          >
            <Coffee size={26} />
          </button>
        )}
      </div>
    </main>
  );
}
