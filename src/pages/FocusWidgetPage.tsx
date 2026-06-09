import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { ArrowLeftToLine, EyeOff, Pin, PinOff } from 'lucide-react';
import { getStudyModeState, listSubjects } from '../services/focusApi';
import {
  collapseFocusWidgetToEdge,
  defaultFocusWidgetDockState,
  getFocusWidgetAlwaysOnTop,
  getFocusWidgetDockState,
  hideFocusWidget,
  listenFocusWidgetDockState,
  peekFocusWidgetFromEdge,
  returnFocusWidgetToMain,
  toggleFocusWidgetAlwaysOnTop,
  type FocusWidgetDockEdge,
} from '../services/focusWidgetApi';
import { STUDY_SYNC_STATE_CHANGED_EVENT } from '../services/syncApi';
import { listenTauriEvent } from '../services/tauriEvents';
import { isTauriRuntime } from '../services/tauriInvoke';
import type { StudyModePhase, StudyModeState, Subject } from '../types/focus';
import './FocusWidgetPage.css';

const REMAINING_REFRESH_MS = 1000;
const TITLE = '专注悬浮窗';
const HOVER_EXPAND_DELAY_MS = 120;
const HOVER_COLLAPSE_DELAY_MS = 180;
const HOVER_COLLAPSE_RETRY_MS = 40;
const HOVER_REENTRY_LOCK_MS = 220;
const RETRACT_PREPARE_MS = 90;

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
  const [alwaysOnTop, setAlwaysOnTop] = useState(false);
  const [isAlwaysOnTopUpdating, setIsAlwaysOnTopUpdating] = useState(false);
  const [isRetracting, setIsRetracting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const shellRef = useRef<HTMLElement | null>(null);
  const expandTimerRef = useRef<number | null>(null);
  const collapseTimerRef = useRef<number | null>(null);
  const hoverLockUntilRef = useRef(0);
  const dockModeRef = useRef(dockState.mode);
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

  useEffect(() => {
    dockModeRef.current = dockState.mode;
    if (dockState.mode !== 'peek') {
      setIsRetracting(false);
    }
  }, [dockState.mode]);

  useEffect(() => {
    return () => {
      if (expandTimerRef.current !== null) {
        window.clearTimeout(expandTimerRef.current);
        expandTimerRef.current = null;
      }
      if (collapseTimerRef.current !== null) {
        window.clearTimeout(collapseTimerRef.current);
        collapseTimerRef.current = null;
      }
    };
  }, []);

  useEffect(() => {
    if (!canInteract) {
      return undefined;
    }

    let cancelled = false;

    void getFocusWidgetAlwaysOnTop()
      .then((nextAlwaysOnTop) => {
        if (!cancelled) {
          setAlwaysOnTop(nextAlwaysOnTop);
        }
      })
      .catch((reason) => {
        if (!cancelled) {
          setError(reason instanceof Error ? reason.message : String(reason));
        }
      });

    return () => {
      cancelled = true;
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
  const elapsedLabel = formatCompactDuration(studyState.study_elapsed_seconds);

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

  const toggleAlwaysOnTop = useCallback(async () => {
    if (!canInteract || isAlwaysOnTopUpdating) return;

    try {
      setIsAlwaysOnTopUpdating(true);
      setAlwaysOnTop(await toggleFocusWidgetAlwaysOnTop());
      setError(null);
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setIsAlwaysOnTopUpdating(false);
    }
  }, [canInteract, isAlwaysOnTopUpdating]);

  const peekFromEdge = useCallback(async () => {
    if (!canInteract || dockModeRef.current !== 'collapsed') return;

    if (expandTimerRef.current !== null) {
      window.clearTimeout(expandTimerRef.current);
      expandTimerRef.current = null;
    }
    if (collapseTimerRef.current !== null) {
      window.clearTimeout(collapseTimerRef.current);
      collapseTimerRef.current = null;
    }
    hoverLockUntilRef.current = Date.now() + HOVER_REENTRY_LOCK_MS;
    setIsRetracting(false);

    try {
      setDockState(await peekFocusWidgetFromEdge());
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    }
  }, [canInteract]);

  const collapseToEdge = useCallback(async () => {
    if (!canInteract || dockModeRef.current !== 'peek') return;

    if (expandTimerRef.current !== null) {
      window.clearTimeout(expandTimerRef.current);
      expandTimerRef.current = null;
    }
    if (collapseTimerRef.current !== null) {
      window.clearTimeout(collapseTimerRef.current);
      collapseTimerRef.current = null;
    }
    hoverLockUntilRef.current = Date.now() + HOVER_REENTRY_LOCK_MS;
    setIsRetracting(true);

    try {
      await waitForNextPaint();
      await waitForMilliseconds(RETRACT_PREPARE_MS);
      setDockState(await collapseFocusWidgetToEdge());
    } catch (reason) {
      setIsRetracting(false);
      setError(reason instanceof Error ? reason.message : String(reason));
    }
  }, [canInteract]);

  const handleCollapsedMouseEnter = useCallback(() => {
    if (!canInteract || dockModeRef.current !== 'collapsed') return;
    if (Date.now() < hoverLockUntilRef.current) return;

    if (collapseTimerRef.current !== null) {
      window.clearTimeout(collapseTimerRef.current);
      collapseTimerRef.current = null;
    }
    if (expandTimerRef.current !== null) {
      window.clearTimeout(expandTimerRef.current);
      expandTimerRef.current = null;
    }

    expandTimerRef.current = window.setTimeout(() => {
      expandTimerRef.current = null;
      if (dockModeRef.current !== 'collapsed') return;
      if (Date.now() < hoverLockUntilRef.current) return;
      if (!shellRef.current?.matches(':hover')) return;
      void peekFromEdge();
    }, HOVER_EXPAND_DELAY_MS);
  }, [canInteract, peekFromEdge]);

  const handleCollapsedMouseLeave = useCallback(() => {
    if (expandTimerRef.current !== null) {
      window.clearTimeout(expandTimerRef.current);
      expandTimerRef.current = null;
    }
  }, []);

  const handleExpandedMouseEnter = useCallback(() => {
    if (collapseTimerRef.current !== null) {
      window.clearTimeout(collapseTimerRef.current);
      collapseTimerRef.current = null;
    }
  }, []);

  const handleExpandedMouseLeave = useCallback(() => {
    if (!canInteract || dockModeRef.current !== 'peek') return;

    if (expandTimerRef.current !== null) {
      window.clearTimeout(expandTimerRef.current);
      expandTimerRef.current = null;
    }
    if (collapseTimerRef.current !== null) {
      window.clearTimeout(collapseTimerRef.current);
      collapseTimerRef.current = null;
    }

    const scheduleCollapseAttempt = (delayMs: number) => {
      collapseTimerRef.current = window.setTimeout(() => {
        collapseTimerRef.current = null;
        if (dockModeRef.current !== 'peek') return;

        const lockRemainingMs = hoverLockUntilRef.current - Date.now();
        if (lockRemainingMs > 0) {
          scheduleCollapseAttempt(lockRemainingMs + HOVER_COLLAPSE_RETRY_MS);
          return;
        }

        if (shellRef.current?.matches(':hover')) return;
        void collapseToEdge();
      }, delayMs);
    };

    scheduleCollapseAttempt(HOVER_COLLAPSE_DELAY_MS);
  }, [canInteract, collapseToEdge]);

  if (dockState.mode === 'collapsed') {
    const edge = dockState.edge ?? 'left';
    return (
      <section
        ref={shellRef}
        aria-label={`${stageLabel}，剩余 ${formatWidgetSeconds(remainingSeconds)}，${combinedLabel}`}
        className={`focus-widget-shell is-collapsed edge-${edge}`}
        onMouseEnter={() => void handleCollapsedMouseEnter()}
        onMouseLeave={handleCollapsedMouseLeave}
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

  const expandedEdgeClass = dockState.edge ? ` edge-${dockState.edge}` : '';

  return (
    <section
      ref={shellRef}
      className={`focus-widget-shell is-${dockState.mode}${isRetracting ? ' is-retracting' : ''}${expandedEdgeClass}`}
      onMouseEnter={handleExpandedMouseEnter}
      onMouseLeave={handleExpandedMouseLeave}
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
              aria-label={alwaysOnTop ? '取消置顶' : '置顶显示'}
              aria-pressed={alwaysOnTop}
              className={`focus-widget-icon-button${alwaysOnTop ? ' is-active' : ''}`}
              disabled={!canInteract || isAlwaysOnTopUpdating}
              onClick={() => void toggleAlwaysOnTop()}
              title={alwaysOnTop ? '取消置顶' : '置顶显示'}
              type="button"
            >
              {alwaysOnTop ? <Pin size={15} /> : <PinOff size={15} />}
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
          <div className="focus-widget-meta-row" aria-hidden="true">
            <span>已进行 {elapsedLabel}</span>
            <span>{roundLabel}</span>
          </div>
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

function formatCompactDuration(totalSeconds: number) {
  const safeSeconds = Math.max(Math.floor(totalSeconds), 0);
  const hours = Math.floor(safeSeconds / 3600);
  const minutes = Math.floor((safeSeconds % 3600) / 60);
  if (hours > 0) {
    return `${hours}h ${minutes.toString().padStart(2, '0')}m`;
  }
  return `${minutes}m`;
}

function waitForNextPaint() {
  return new Promise<void>((resolve) => {
    window.requestAnimationFrame(() => {
      window.requestAnimationFrame(() => resolve());
    });
  });
}

function waitForMilliseconds(delayMs: number) {
  return new Promise<void>((resolve) => {
    window.setTimeout(resolve, delayMs);
  });
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
