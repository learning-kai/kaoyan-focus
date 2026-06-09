import { useEffect, useMemo, useState } from 'react';
import { ArrowLeftToLine, EyeOff } from 'lucide-react';
import { getStudyModeState, listSubjects } from '../services/focusApi';
import { hideFocusWidget, returnFocusWidgetToMain } from '../services/focusWidgetApi';
import { STUDY_SYNC_STATE_CHANGED_EVENT } from '../services/syncApi';
import { listenTauriEvent } from '../services/tauriEvents';
import { isTauriRuntime } from '../services/tauriInvoke';
import type { StudyModePhase, StudyModeState, Subject } from '../types/focus';
import './FocusWidgetPage.css';

const REMAINING_REFRESH_MS = 1000;
const TITLE = '专注悬浮窗';

const idleState: StudyModeState = {
  id: null,
  phase: 'idle',
  status: 'idle',
  mode: 'normal',
  subject_id: null,
  planned_seconds: 0,
  focus_seconds: 0,
  break_seconds: 0,
  long_break_seconds: 0,
  long_break_interval: 4,
  effective_break_seconds: 0,
  break_kind: 'short',
  cycle_index: 0,
  started_at: null,
  phase_started_at: null,
  paused_at: null,
  ended_at: null,
  current_session: null,
  study_elapsed_seconds: 0,
  study_remaining_seconds: 0,
  phase_elapsed_seconds: 0,
  phase_remaining_seconds: 0,
  focus_enforcement_active: false,
  whitelist_enabled: true,
  is_paused: false,
};

const phaseLabel: Record<StudyModePhase, string> = {
  idle: '待开始',
  focus: '专注中',
  awaiting_break: '等待休息',
  break: '休息中',
  finished: '已完成',
  emergency_exited: '已退出',
};

export default function FocusWidgetPage() {
  const [studyState, setStudyState] = useState<StudyModeState>(idleState);
  const [subjects, setSubjects] = useState<Subject[]>([]);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    document.title = TITLE;
  }, []);

  useEffect(() => {
    if (!isTauriRuntime()) {
      return undefined;
    }

    let cancelled = false;

    void listSubjects()
      .then((nextSubjects) => {
        if (!cancelled) {
          setSubjects(nextSubjects);
        }
      })
      .catch(() => {
        // Subject names are optional for the widget.
      });

    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(() => {
    if (!isTauriRuntime()) {
      return undefined;
    }

    let cancelled = false;
    let unlisten: (() => void) | undefined;

    async function refreshStudyState() {
      try {
        const nextState = await getStudyModeState();
        if (!cancelled) {
          setStudyState(nextState);
          setError(null);
        }
      } catch (reason) {
        if (!cancelled) {
          setError(reason instanceof Error ? reason.message : String(reason));
        }
      }
    }

    void refreshStudyState();
    const timerId = window.setInterval(() => {
      void refreshStudyState();
    }, REMAINING_REFRESH_MS);

    void listenTauriEvent(STUDY_SYNC_STATE_CHANGED_EVENT, () => {
      void refreshStudyState();
    })
      .then((dispose) => {
        unlisten = dispose;
      })
      .catch(() => {
        // Older builds may not expose the sync event.
      });

    return () => {
      cancelled = true;
      window.clearInterval(timerId);
      unlisten?.();
    };
  }, []);

  const remainingSeconds = studyState.study_remaining_seconds;
  const progressPercent = studyState.planned_seconds > 0
    ? Math.max(0, Math.min(100, Math.round((studyState.study_elapsed_seconds / studyState.planned_seconds) * 100)))
    : 0;
  const subjectName = useMemo(() => {
    if (studyState.subject_id == null) {
      return '未指定科目';
    }
    return subjects.find((subject) => subject.id === studyState.subject_id)?.name ?? `科目 #${studyState.subject_id}`;
  }, [studyState.subject_id, subjects]);
  const stageLabel = studyState.is_paused ? `暂停中 · ${phaseLabel[studyState.phase]}` : phaseLabel[studyState.phase];
  const roundLabel = `第 ${Math.max(studyState.cycle_index, 0)} 轮`;
  const combinedLabel = `${subjectName} / ${roundLabel}`;
  const progressLabel = `${progressPercent}%`;
  const canInteract = isTauriRuntime();

  async function returnToMain() {
    if (!canInteract) return;

    try {
      await returnFocusWidgetToMain();
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    }
  }

  async function hideWidget() {
    if (!canInteract) return;

    try {
      await hideFocusWidget();
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    }
  }

  return (
    <section className="focus-widget-shell">
      <div className="focus-widget-panel">
        <strong className="focus-widget-time">{formatWidgetSeconds(remainingSeconds)}</strong>

        <div className="focus-widget-meta">
          <p className="focus-widget-stage">
            <span className="focus-widget-dot" />
            <span>{stageLabel}</span>
          </p>
          <p className="focus-widget-subject">{combinedLabel}</p>
        </div>

        <div className="focus-widget-progress">
          <div className="focus-widget-progress-row">
            <span>总进度</span>
            <strong>{progressLabel}</strong>
          </div>
          <div className="focus-widget-bar" aria-hidden="true">
            <div className="focus-widget-bar-fill" style={{ width: `${progressPercent}%` }} />
          </div>
        </div>

        <div className="focus-widget-actions">
          <button className="focus-widget-button" disabled={!canInteract} onClick={() => void returnToMain()} type="button">
            <ArrowLeftToLine size={15} />
            <span>返回主界面</span>
          </button>
          <button className="focus-widget-button secondary" disabled={!canInteract} onClick={() => void hideWidget()} type="button">
            <EyeOff size={15} />
            <span>隐藏</span>
          </button>
        </div>

        <p className="focus-widget-error" aria-live="polite">
          {error ?? ''}
        </p>
      </div>
    </section>
  );
}

function formatWidgetSeconds(totalSeconds: number) {
  const safeSeconds = Math.max(Math.floor(totalSeconds), 0);
  const hours = Math.floor(safeSeconds / 3600);
  const minutes = Math.floor((safeSeconds % 3600) / 60).toString().padStart(2, '0');
  const seconds = Math.floor(safeSeconds % 60).toString().padStart(2, '0');
  return hours > 0 ? `${hours}:${minutes}:${seconds}` : `${minutes}:${seconds}`;
}
