import { useEffect, useRef, useState } from 'react';
import { getStudyModeState } from '../services/focusApi';
import type { StudyModeState } from '../types/focus';
import './FinderBarWidgetPage.css';

const idleState: StudyModeState = {
  id: null, phase: 'idle', status: 'idle', mode: 'normal', timer_kind: 'pomodoro', subject_id: null,
  planned_seconds: 0, focus_seconds: 0, break_seconds: 0, long_break_seconds: 0, long_break_interval: 4,
  effective_break_seconds: 0, break_kind: 'short', break_reason: null, cycle_index: 0, started_at: null, phase_started_at: null,
  paused_at: null, ended_at: null, current_session: null, study_elapsed_seconds: 0, study_remaining_seconds: 0,
  phase_elapsed_seconds: 0, phase_remaining_seconds: 0, focus_enforcement_active: false, whitelist_enabled: true,
  is_paused: false,
};

export default function FinderBarWidgetPage() {
  const [state, setState] = useState<StudyModeState>(idleState);
  const syncedAt = useRef(Date.now());
  const [, tick] = useState(0);

  useEffect(() => {
    const refresh = () => void getStudyModeState().then((next) => { syncedAt.current = Date.now(); setState(next); }).catch(() => undefined);
    refresh();
    const id = window.setInterval(refresh, 5000);
    const clock = window.setInterval(() => tick((value) => value + 1), 1000);
    return () => { window.clearInterval(id); window.clearInterval(clock); };
  }, []);

  const liveDelta = state.status === 'active' && !state.is_paused ? Math.floor((Date.now() - syncedAt.current) / 1000) : 0;
  const countup = state.timer_kind === 'countup' && state.phase === 'focus';
  const seconds = countup ? state.phase_elapsed_seconds + liveDelta : Math.max(0, state.phase_remaining_seconds - liveDelta);
  const progress = state.planned_seconds > 0 ? Math.min(100, Math.round(((state.study_elapsed_seconds + liveDelta) / state.planned_seconds) * 100)) : 0;
  const label = state.status !== 'active' ? '待机' : state.is_paused ? '暂停' : state.phase === 'break' ? '休息' : '专注';

  return <div className="finder-bar-widget" title="考研专注">
    <span className={`finder-bar-widget-dot ${state.status === 'active' ? 'is-active' : ''}`} />
    <span className="finder-bar-widget-label">{label}</span>
    <strong>{format(seconds)}</strong>
    {state.planned_seconds > 0 && <span className="finder-bar-widget-progress"><i style={{ width: `${progress}%` }} /></span>}
  </div>;
}

function format(total: number) {
  const value = Math.max(0, Math.floor(total));
  const h = Math.floor(value / 3600);
  const m = Math.floor((value % 3600) / 60).toString().padStart(2, '0');
  const s = (value % 60).toString().padStart(2, '0');
  return h > 0 ? `${h}:${m}:${s}` : `${m}:${s}`;
}
