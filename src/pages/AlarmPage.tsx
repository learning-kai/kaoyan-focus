import { useEffect, useLayoutEffect, useMemo, useState } from 'react';
import { AlarmClock, BellRing, CheckCircle2, Plus, Power, RefreshCw, Save, Trash2 } from 'lucide-react';
import {
  ALARM_STATE_CHANGED_EVENT,
  createAlarm,
  deleteAlarm,
  dismissAlarm,
  listAlarms,
  notifyAlarmStateChanged,
  setAlarmEnabled,
  updateAlarm,
} from '../services/alarmApi';
import { stopPersistentAlarmSound } from '../services/alertApi';
import { useConfirmDialog } from '../hooks/useConfirmDialog';
import { formatDateKey } from '../utils/date';
import type { Alarm, AlarmDraft } from '../types/alarm';

const defaultAlarmTitle = '闹钟';

type AlarmPageProps = {
  focusAlarmId?: number | null;
  onFocusAlarmHandled?: () => void;
};

function tomorrowString() {
  const date = new Date();
  date.setDate(date.getDate() + 1);
  return formatDateKey(date);
}

function localDateTime(dateString: string, timeString: string) {
  const [year, month, day] = dateString.split('-').map(Number);
  const [hour, minute] = timeString.split(':').map(Number);
  if (
    !Number.isInteger(year)
    || !Number.isInteger(month)
    || !Number.isInteger(day)
    || !Number.isInteger(hour)
    || !Number.isInteger(minute)
  ) return null;
  return new Date(year, month - 1, day, hour, minute, 0, 0);
}

function todayDateIfFuture(timeString: string) {
  const dateString = formatDateKey();
  const target = localDateTime(dateString, timeString);
  return target && target.getTime() > Date.now() ? dateString : null;
}

function nextFiveMinuteTime() {
  const date = new Date();
  date.setMinutes(Math.ceil((date.getMinutes() + 1) / 5) * 5, 0, 0);
  if (date.getHours() === 0 && date.getMinutes() === 0 && date.getDate() !== new Date().getDate()) {
    return '00:00';
  }
  return `${String(date.getHours()).padStart(2, '0')}:${String(date.getMinutes()).padStart(2, '0')}`;
}

function defaultDraft(): AlarmDraft {
  return {
    title: defaultAlarmTitle,
    note: '',
    alarmDate: formatDateKey(),
    alarmTime: nextFiveMinuteTime(),
    enabled: true,
  };
}

function alarmKey(id: number) {
  return `alarm:${id}`;
}

function formatAlarmDateTime(alarm: Alarm) {
  return `${alarm.alarm_date} ${alarm.alarm_time}`;
}

function formatRelative(alarm: Alarm) {
  if (alarm.status === 'ringing') return '正在响铃';
  if (alarm.status === 'dismissed') return alarm.dismissed_at ? '已确认' : '已结束';
  if (!alarm.enabled) return '已关闭';

  const distanceMs = new Date(alarm.alarm_at).getTime() - Date.now();
  if (distanceMs <= 0) return '等待触发';
  const minutes = Math.ceil(distanceMs / 60000);
  if (minutes < 60) return `${minutes} 分钟后`;
  const hours = Math.floor(minutes / 60);
  const restMinutes = minutes % 60;
  return restMinutes ? `${hours} 小时 ${restMinutes} 分钟后` : `${hours} 小时后`;
}

function isExpired(alarm: Alarm) {
  return alarm.status === 'scheduled' && !alarm.enabled && new Date(alarm.alarm_at).getTime() < Date.now();
}

export default function AlarmPage({ focusAlarmId = null, onFocusAlarmHandled }: AlarmPageProps = {}) {
  const { confirm, confirmDialog } = useConfirmDialog();
  const [alarms, setAlarms] = useState<Alarm[]>([]);
  const [draft, setDraft] = useState<AlarmDraft>(() => defaultDraft());
  const [editingId, setEditingId] = useState<number | null>(null);
  const [editingDraft, setEditingDraft] = useState<AlarmDraft | null>(null);
  const [reschedulingAlarm, setReschedulingAlarm] = useState<Alarm | null>(null);
  const [rescheduleDate, setRescheduleDate] = useState(() => tomorrowString());
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [targetedAlarmId, setTargetedAlarmId] = useState<number | null>(null);

  const ringingAlarms = useMemo(() => alarms.filter((alarm) => alarm.status === 'ringing'), [alarms]);
  const upcomingAlarms = useMemo(
    () => alarms.filter((alarm) => alarm.status === 'scheduled' && alarm.enabled),
    [alarms],
  );
  const doneAlarms = useMemo(
    () => alarms.filter((alarm) => alarm.status !== 'ringing' && !(alarm.status === 'scheduled' && alarm.enabled)),
    [alarms],
  );
  const nextAlarm = upcomingAlarms[0] ?? null;

  useEffect(() => {
    void refresh();
  }, []);

  useEffect(() => {
    if (focusAlarmId == null) {
      return;
    }

    setTargetedAlarmId(focusAlarmId);
  }, [focusAlarmId]);

  useLayoutEffect(() => {
    if (focusAlarmId == null) {
      return;
    }

    if (loading || alarms.length === 0) {
      return;
    }

    const target = document.getElementById(`alarm-row-${focusAlarmId}`);
    if (!(target instanceof HTMLElement)) {
      return;
    }

    target.scrollIntoView({ block: 'center', behavior: 'auto' });
    target.focus({ preventScroll: true });
    onFocusAlarmHandled?.();
  }, [alarms, focusAlarmId, loading, onFocusAlarmHandled]);

  useEffect(() => {
    const refreshFromGlobal = () => {
      void refresh(false);
    };
    window.addEventListener(ALARM_STATE_CHANGED_EVENT, refreshFromGlobal);
    return () => window.removeEventListener(ALARM_STATE_CHANGED_EVENT, refreshFromGlobal);
  }, []);

  async function refresh(showLoading = true) {
    try {
      if (showLoading) setLoading(true);
      setError(null);
      setAlarms(await listAlarms());
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      if (showLoading) setLoading(false);
    }
  }

  async function withSave(action: () => Promise<void>, successMessage: string) {
    try {
      setSaving(true);
      setError(null);
      setMessage(null);
      await action();
      await refresh(false);
      notifyAlarmStateChanged();
      setMessage(successMessage);
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setSaving(false);
    }
  }

  async function handleCreate() {
    if (!draft.title.trim()) return;
    await withSave(async () => {
      await createAlarm(draft);
      setDraft(defaultDraft());
    }, '闹钟已创建。');
  }

  function beginEdit(alarm: Alarm) {
    setReschedulingAlarm(null);
    setEditingId(alarm.id);
    setEditingDraft({
      title: alarm.title,
      note: alarm.note ?? '',
      alarmDate: alarm.alarm_date,
      alarmTime: alarm.alarm_time,
      enabled: alarm.enabled,
    });
  }

  async function handleUpdate() {
    if (!editingId || !editingDraft?.title.trim()) return;
    await withSave(async () => {
      await updateAlarm(editingId, editingDraft);
      setEditingId(null);
      setEditingDraft(null);
    }, '闹钟已更新。');
  }

  async function handleDismiss(alarm: Alarm) {
    await withSave(async () => {
      await dismissAlarm(alarm.id);
      stopPersistentAlarmSound(alarmKey(alarm.id));
    }, '闹钟已停止。');
  }

  async function handleToggle(alarm: Alarm) {
    if (!alarm.enabled) {
      const futureToday = todayDateIfFuture(alarm.alarm_time);
      if (futureToday) {
        await rescheduleAlarm(alarm, futureToday);
        return;
      }
      setEditingId(null);
      setEditingDraft(null);
      setReschedulingAlarm(alarm);
      setRescheduleDate(tomorrowString());
      setMessage(null);
      setError(null);
      return;
    }

    await withSave(async () => {
      await setAlarmEnabled(alarm.id, false);
      if (alarm.status === 'ringing') {
        stopPersistentAlarmSound(alarmKey(alarm.id));
      }
    }, '闹钟已关闭。');
  }

  async function rescheduleAlarm(alarm: Alarm, alarmDate: string) {
    await withSave(async () => {
      await updateAlarm(alarm.id, {
        title: alarm.title,
        note: alarm.note ?? '',
        alarmDate,
        alarmTime: alarm.alarm_time,
        enabled: true,
      });
      setReschedulingAlarm(null);
    }, `闹钟已开启：${alarmDate} ${alarm.alarm_time}`);
  }

  async function handleConfirmReschedule() {
    if (!reschedulingAlarm) return;
    await rescheduleAlarm(reschedulingAlarm, rescheduleDate);
  }

  async function handleDelete(alarm: Alarm) {
    const confirmed = await confirm({
      confirmLabel: '删除闹钟',
      message: `删除后不会再触发「${alarm.title}」，正在响铃的声音也会立即停止。`,
      title: '删除闹钟？',
      tone: 'danger',
    });
    if (!confirmed) return;

    await withSave(async () => {
      await deleteAlarm(alarm.id);
      stopPersistentAlarmSound(alarmKey(alarm.id));
      if (editingId === alarm.id) {
        setEditingId(null);
        setEditingDraft(null);
      }
      if (reschedulingAlarm?.id === alarm.id) {
        setReschedulingAlarm(null);
      }
    }, '闹钟已删除。');
  }

  function renderAlarmRow(alarm: Alarm) {
    const isEditing = editingId === alarm.id && editingDraft;
    const isRescheduling = reschedulingAlarm?.id === alarm.id;
    const isTargeted = targetedAlarmId === alarm.id;
    const rowClass = [
      'alarm-row',
      alarm.status === 'ringing' ? 'is-ringing' : '',
      alarm.enabled && alarm.status === 'scheduled' ? 'is-upcoming' : '',
      isTargeted ? 'is-targeted' : '',
      isExpired(alarm) ? 'is-expired' : '',
    ].filter(Boolean).join(' ');

    return (
      <article className={rowClass} id={`alarm-row-${alarm.id}`} key={alarm.id} tabIndex={isTargeted ? 0 : -1}>
        {isEditing ? (
          <div className="alarm-inline-editor">
            <input
              className="text-input"
              value={editingDraft.title}
              onChange={(event) => setEditingDraft({ ...editingDraft, title: event.target.value })}
              onKeyDown={(event) => {
                if (event.key === 'Enter' && !event.nativeEvent.isComposing) void handleUpdate();
                if (event.key === 'Escape') {
                  setEditingId(null);
                  setEditingDraft(null);
                }
              }}
            />
            <input
              className="text-input"
              type="date"
              value={editingDraft.alarmDate}
              onChange={(event) => setEditingDraft({ ...editingDraft, alarmDate: event.target.value })}
            />
            <input
              className="text-input"
              type="time"
              value={editingDraft.alarmTime}
              onChange={(event) => setEditingDraft({ ...editingDraft, alarmTime: event.target.value })}
            />
            <input
              className="text-input"
              placeholder="备注"
              value={editingDraft.note ?? ''}
              onChange={(event) => setEditingDraft({ ...editingDraft, note: event.target.value })}
            />
            <button className="small-action enabled" disabled={saving || !editingDraft.title.trim()} type="button" onClick={() => void handleUpdate()}>
              保存
            </button>
            <button className="small-action" type="button" onClick={() => { setEditingId(null); setEditingDraft(null); }}>
              取消
            </button>
          </div>
        ) : (
          <>
            <div className="alarm-time-card">
              <strong>{alarm.alarm_time}</strong>
              <span>{alarm.alarm_date}</span>
            </div>
            <div className="alarm-row-main">
              <div>
                <strong>{alarm.title}</strong>
                <span>{alarm.note?.trim() || '无备注'} · {formatRelative(alarm)}</span>
              </div>
              <small>{formatAlarmDateTime(alarm)}</small>
            </div>
            <div className="alarm-row-actions">
              {alarm.status === 'ringing' ? (
                <button className="small-action enabled" disabled={saving} type="button" onClick={() => void handleDismiss(alarm)}>
                  <CheckCircle2 size={14} /> 停止
                </button>
              ) : (
                <button className={alarm.enabled ? 'small-action enabled' : 'small-action'} disabled={saving} type="button" onClick={() => void handleToggle(alarm)}>
                  <Power size={14} /> {alarm.enabled ? '关闭' : '开启'}
                </button>
              )}
              <button className="small-action" disabled={saving || alarm.status === 'ringing'} type="button" onClick={() => beginEdit(alarm)}>
                编辑
              </button>
              <button className="small-action danger" disabled={saving} type="button" onClick={() => void handleDelete(alarm)}>
                <Trash2 size={14} /> 删除
              </button>
            </div>
            {isRescheduling && (
              <div className="alarm-reschedule-editor">
                <div>
                  <strong>选择下次响铃日期</strong>
                  <span>沿用 {alarm.alarm_time}，确认后这个闹钟会重新开启。</span>
                </div>
                <input
                  className="text-input"
                  min={tomorrowString()}
                  type="date"
                  value={rescheduleDate}
                  onChange={(event) => setRescheduleDate(event.target.value)}
                />
                <button className="small-action enabled" disabled={saving || !rescheduleDate} type="button" onClick={() => void handleConfirmReschedule()}>
                  开启
                </button>
                <button className="small-action" disabled={saving} type="button" onClick={() => setReschedulingAlarm(null)}>
                  取消
                </button>
              </div>
            )}
          </>
        )}
      </article>
    );
  }

  return (
    <div className="page-shell alarm-shell">
      <section className="page-header">
        <div>
          <p className="eyebrow">Alarm</p>
          <h2>全局闹钟</h2>
          <p>专注中和平常都能使用。设置一次性闹钟，到点持续响铃，确认后停止。</p>
        </div>
        <div className="header-metrics">
          <article>
            <span>下一个闹钟</span>
            <strong>{nextAlarm ? `${nextAlarm.alarm_time}` : '暂无'}</strong>
          </article>
          <article>
            <span>待响铃</span>
            <strong>{upcomingAlarms.length}</strong>
          </article>
          <article>
            <span>正在响</span>
            <strong>{ringingAlarms.length}</strong>
          </article>
        </div>
      </section>

      {(error || message) && (
        <div
          aria-live={error ? undefined : 'polite'}
          className={error ? 'alert error' : 'alert success'}
          role={error ? 'alert' : 'status'}
        >
          {error ?? message}
        </div>
      )}
      {confirmDialog}

      <section className="command-panel alarm-composer">
        <div className="panel-title">
          <div>
            <p className="eyebrow">Create</p>
            <h3>新建闹钟</h3>
          </div>
          <button className="ghost-action" disabled={loading} type="button" onClick={() => void refresh()}>
            <RefreshCw size={15} /> 刷新
          </button>
        </div>
        <div className="alarm-form-grid">
          <label className="field-block">
            <span>标题</span>
            <input
              className="text-input"
              value={draft.title}
              onChange={(event) => setDraft({ ...draft, title: event.target.value })}
              onKeyDown={(event) => {
                if (event.key === 'Enter' && !event.nativeEvent.isComposing) void handleCreate();
              }}
            />
          </label>
          <label className="field-block">
            <span>日期</span>
            <input className="text-input" type="date" value={draft.alarmDate} onChange={(event) => setDraft({ ...draft, alarmDate: event.target.value })} />
          </label>
          <label className="field-block">
            <span>时间</span>
            <input className="text-input" type="time" value={draft.alarmTime} onChange={(event) => setDraft({ ...draft, alarmTime: event.target.value })} />
          </label>
          <label className="field-block">
            <span>备注</span>
            <input className="text-input" placeholder="可选" value={draft.note ?? ''} onChange={(event) => setDraft({ ...draft, note: event.target.value })} />
          </label>
          <button className="primary-action" disabled={saving || !draft.title.trim()} type="button" onClick={() => void handleCreate()}>
            <Plus size={16} /> 添加闹钟
          </button>
        </div>
      </section>

      {ringingAlarms.length > 0 && (
        <section className="command-panel alarm-section is-ringing">
          <div className="panel-title">
            <div>
              <p className="eyebrow">Ringing</p>
              <h3>正在响铃</h3>
            </div>
            <BellRing size={18} />
          </div>
          <div className="alarm-list">{ringingAlarms.map(renderAlarmRow)}</div>
        </section>
      )}

      <section className="command-panel alarm-section">
        <div className="panel-title">
          <div>
            <p className="eyebrow">Upcoming</p>
            <h3>即将响铃</h3>
          </div>
          <AlarmClock size={18} />
        </div>
        {upcomingAlarms.length ? <div className="alarm-list">{upcomingAlarms.map(renderAlarmRow)}</div> : <div className="empty-state compact">暂无即将响铃的闹钟。</div>}
      </section>

      <section className="command-panel alarm-section">
        <div className="panel-title">
          <div>
            <p className="eyebrow">历史记录</p>
            <h3>已确认 / 已关闭</h3>
          </div>
          <Save size={18} />
        </div>
        {doneAlarms.length ? <div className="alarm-list">{doneAlarms.map(renderAlarmRow)}</div> : <div className="empty-state compact">还没有历史闹钟。</div>}
      </section>
    </div>
  );
}
