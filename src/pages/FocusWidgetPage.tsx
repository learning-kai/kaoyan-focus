import { useCallback, useEffect, useMemo, useState } from 'react';
import { ArrowLeftToLine, EyeOff } from 'lucide-react';
import { getStudyModeState, listSubjects } from '../services/focusApi';
import {
  collapseFocusWidgetToEdge,
  defaultFocusWidgetDockState,
  getFocusWidgetDockState,
  hideFocusWidget,
  listenFocusWidgetDockState,
  peekFocusWidgetFromEdge,
  returnFocusWidgetToMain,
  type FocusWidgetDockEdge,
} from '../services/focusWidgetApi';
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

const collapsedPhaseLabel: Record<StudyModePhase, string> = {
  idle: '待机',
  focus: '专注',
  awaiting_break: '待休',
  break: '休息',
  finished: '完成',
  emergency_exited: '退出',
};

export default function FocusWidgetPage() {
  const [studyState, setStudyState] = useState<StudyModeState>(idleState);
  const [subjects, setSubjects] = useState<Subject[]>([]);
  const [dockState, setDockState] = useState(defaultFocusWidgetDockState);
  const [error, setError] = useState<string | null>(null);
  const canInteract = isTauriRuntime();

  useEffect(() => {
    document.title = TITLE;
    document.documentElement.classList.add('focus-widget-document');
    document.body.classList.add('focus-widget-body');
    return () => {
      document.documentElement.classList.remove('focus-widget-document');
      document.body.classList.remove('focus-widget-body');
    };
  }, []);

  useEffect(() => {
    if (!canInteract) {
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
  }, [canInteract]);

  useEffect(() => {
    if (!canInteract) {
      return undefined;
    }

    let cancelled = false;
    let unlistenStudyState: (() => void) | undefined;

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
        unlistenStudyState = dispose;
      })
      .catch(() => {
        // Older builds may not expose the sync event.
      });

    return () => {
      cancelled = true;
      window.clearInterval(timerId);
      unlistenStudyState?.();
    };
  }, [canInteract]);

  useEffect(() => {
    if (!canInteract) {
      return undefined;
    }

    let cancelled = false;
    let unlistenDockState: (() => void) | undefined;

    void getFocusWidgetDockState()
      .then((nextDockState) => {
        if (!cancelled) {
          setDockState(nextDockState);
        }
      })
      .catch(() => {
        // The widget remains usable in floating mode if dock state is unavailable.
      });

    void listenFocusWidgetDockState((nextDockState) => {
      if (!cancelled) {
        setDockState(nextDockState);
      }
    })
      .then((dispose) => {
        unlistenDockState = dispose;
      })
      .catch(() => {
        // Docking is progressive enhancement over the real window.
      });

    return () => {
      cancelled = true;
      unlistenDockState?.();
    };
  }, [canInteract]);

  const remainingSeconds = studyState.phase_remaining_seconds;
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
  const roundLabel = `第 ${Math.max(studyState.cycle_index, 1)} 轮`;
  const combinedLabel = `${subjectName} / ${roundLabel}`;
  const progressLabel = `${progressPercent}%`;

  const returnToMain = useCallback(async () => {
    if (!canInteract) return;

    try {
      await returnFocusWidgetToMain();
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    }
  }, [canInteract]);

  const hideWidget = useCallback(async () => {
    if (!canInteract) return;

    try {
      await hideFocusWidget();
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    }
  }, [canInteract]);

  const peekFromEdge = useCallback(async () => {
    if (!canInteract || dockState.mode !== 'collapsed') return;

    try {
      setDockState(await peekFocusWidgetFromEdge());
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    }
  }, [canInteract, dockState.mode]);

  const collapseToEdge = useCallback(async () => {
    if (!canInteract || dockState.mode !== 'peek') return;

    try {
      setDockState(await collapseFocusWidgetToEdge());
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    }
  }, [canInteract, dockState.mode]);

  if (dockState.mode === 'collapsed') {
    const edge = dockState.edge ?? 'left';
    return (
      <section
        aria-label={`${stageLabel}，剩余 ${formatWidgetSeconds(remainingSeconds)}，${combinedLabel}`}
        className={`focus-widget-shell is-collapsed edge-${edge}`}
        onMouseEnter={() => void peekFromEdge()}
      >
        <button
          className="focus-widget-collapsed-tab"
          disabled={!canInteract}
          onClick={() => void peekFromEdge()}
          title="展开悬浮窗"
          type="button"
        >
          <span className="focus-widget-dot" />
          <strong>{formatCollapsedSeconds(remainingSeconds, edge)}</strong>
          <span>{collapsedPhaseLabel[studyState.phase]}</span>
        </button>
      </section>
    );
  }

  return (
    <section
      className={`focus-widget-shell is-${dockState.mode}`}
      onMouseLeave={() => void collapseToEdge()}
    >
      <div className="focus-widget-panel">
        <header className="focus-widget-topbar">
          <span className="focus-widget-stage">
            <span className="focus-widget-dot" />
            <span>{stageLabel}</span>
          </span>
          <div className="focus-widget-actions">
            <button
              aria-label="返回主界面"
              className="focus-widget-icon-button"
              disabled={!canInteract}
              onClick={() => void returnToMain()}
              title="返回主界面"
              type="button"
            >
              <ArrowLeftToLine size={15} />
            </button>
            <button
              aria-label="隐藏"
              className="focus-widget-icon-button"
              disabled={!canInteract}
              onClick={() => void hideWidget()}
              title="隐藏"
              type="button"
            >
              <EyeOff size={15} />
            </button>
          </div>
        </header>

        <strong className="focus-widget-time">{formatWidgetSeconds(remainingSeconds)}</strong>

        <p className="focus-widget-subject">{combinedLabel}</p>

        <div className="focus-widget-progress" aria-label={`总进度 ${progressLabel}`}>
          <div className="focus-widget-progress-row">
            <span>总进度</span>
            <strong>{progressLabel}</strong>
          </div>
          <div className="focus-widget-bar" aria-hidden="true">
            <div className="focus-widget-bar-fill" style={{ width: `${progressPercent}%` }} />
          </div>
        </div>

        {error && (
          <p className="focus-widget-error" aria-live="polite">
            {error}
          </p>
        )}
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

function formatCollapsedSeconds(totalSeconds: number, edge: FocusWidgetDockEdge) {
  const safeSeconds = Math.max(Math.floor(totalSeconds), 0);
  if (edge === 'top' || edge === 'bottom') {
    return formatWidgetSeconds(safeSeconds);
  }

  const hours = Math.floor(safeSeconds / 3600);
  if (hours > 0) {
    const minutes = Math.floor((safeSeconds % 3600) / 60);
    return `${hours}h${minutes.toString().padStart(2, '0')}`;
  }

  return `${Math.max(1, Math.ceil(safeSeconds / 60))}m`;
}
