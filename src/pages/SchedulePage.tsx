import { useEffect, useMemo, useState } from 'react';
import {
  CalendarDays,
  ChevronLeft,
  ChevronRight,
  Clock3,
  CopyPlus,
  Play,
  Plus,
  RefreshCw,
  Trash2,
} from 'lucide-react';
import { getAppSettings } from '../services/settingsApi';
import {
  createScheduleBlock,
  createScheduleBlockFromTodayItem,
  createScheduleTemplate,
  deleteScheduleBlock,
  deleteScheduleTemplate,
  getSchedulePageData,
  startStudyModeFromScheduleBlock,
  updateScheduleBlock,
} from '../services/scheduleApi';
import { listSubjects } from '../services/focusApi';
import type { AppSettings } from '../types/settings';
import type { Subject } from '../types/focus';
import type { ScheduleBlock, ScheduleBlockDraft, SchedulePageData, ScheduleTemplateDraft } from '../types/schedule';

const categories = [
  { key: 'politics', label: '政治' },
  { key: 'english', label: '英语' },
  { key: 'math', label: '数学' },
  { key: 'major', label: '专业课' },
  { key: 'general', label: '通用' },
];

const weekdays = ['周一', '周二', '周三', '周四', '周五', '周六', '周日'];
const dayStart = 6 * 60;
const dayEnd = 24 * 60;

const emptyBlockDraft = (date: string): ScheduleBlockDraft => ({
  scheduleDate: date,
  title: '',
  note: '',
  categoryKey: 'general',
  subjectId: null,
  sourceTodayItemId: null,
  startMinute: 8 * 60,
  endMinute: 9 * 60,
});

const emptyTemplateDraft: ScheduleTemplateDraft = {
  title: '',
  note: '',
  categoryKey: 'general',
  subjectId: null,
  weekdays: [1, 2, 3, 4, 5],
  startMinute: 8 * 60,
  endMinute: 9 * 60,
  enabled: true,
};

function todayString() {
  const date = new Date();
  const year = date.getFullYear();
  const month = String(date.getMonth() + 1).padStart(2, '0');
  const day = String(date.getDate()).padStart(2, '0');
  return `${year}-${month}-${day}`;
}

function shiftDate(value: string, days: number) {
  const date = new Date(`${value}T00:00:00`);
  date.setDate(date.getDate() + days);
  return date.toISOString().slice(0, 10);
}

function formatMinute(minute: number) {
  const safe = Math.max(0, Math.min(24 * 60, minute));
  return `${String(Math.floor(safe / 60)).padStart(2, '0')}:${String(safe % 60).padStart(2, '0')}`;
}

function parseTime(value: string) {
  const [hour, minute] = value.split(':').map(Number);
  return (Number.isFinite(hour) ? hour : 0) * 60 + (Number.isFinite(minute) ? minute : 0);
}

function categoryLabel(key: string) {
  return categories.find((item) => item.key === key)?.label ?? '通用';
}

function categoryKeyForSubject(subjectId: number | null) {
  if (subjectId === 1) return 'politics';
  if (subjectId === 2) return 'english';
  if (subjectId === 3) return 'math';
  if (subjectId === 4) return 'major';
  return 'general';
}

function subjectName(subjects: Subject[], subjectId: number | null) {
  return subjectId ? subjects.find((subject) => subject.id === subjectId)?.name ?? '未知科目' : '未指定';
}

export default function SchedulePage() {
  const [data, setData] = useState<SchedulePageData | null>(null);
  const [subjects, setSubjects] = useState<Subject[]>([]);
  const [settings, setSettings] = useState<AppSettings | null>(null);
  const [selectedDate, setSelectedDate] = useState(todayString());
  const [view, setView] = useState<'day' | 'week'>('day');
  const [blockDraft, setBlockDraft] = useState<ScheduleBlockDraft>(() => emptyBlockDraft(todayString()));
  const [templateDraft, setTemplateDraft] = useState<ScheduleTemplateDraft>(emptyTemplateDraft);
  const [showBlockComposer, setShowBlockComposer] = useState(false);
  const [showTemplateComposer, setShowTemplateComposer] = useState(false);
  const [editingBlockId, setEditingBlockId] = useState<number | null>(null);
  const [editingBlockDraft, setEditingBlockDraft] = useState<ScheduleBlockDraft | null>(null);
  const [saving, setSaving] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    void initialize();
  }, []);

  useEffect(() => {
    void refresh(selectedDate);
    setBlockDraft((draft) => ({ ...draft, scheduleDate: selectedDate }));
  }, [selectedDate]);

  const scheduledTodayItemIds = useMemo(() => {
    const ids = new Set<number>();
    for (const block of data?.day_blocks ?? []) {
      if (typeof block.source_today_item_id === 'number') ids.add(block.source_today_item_id);
    }
    return ids;
  }, [data]);

  const currentMinute = useMemo(() => {
    if (selectedDate !== todayString()) return null;
    const now = new Date();
    return now.getHours() * 60 + now.getMinutes();
  }, [selectedDate]);

  async function initialize() {
    try {
      const [pageData, subjectData, appSettings] = await Promise.all([
        getSchedulePageData(selectedDate),
        listSubjects(),
        getAppSettings(),
      ]);
      setData(pageData);
      setSubjects(subjectData);
      setSettings(appSettings);
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    }
  }

  async function refresh(date = selectedDate) {
    try {
      setData(await getSchedulePageData(date));
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    }
  }

  async function withSave(action: () => Promise<void>, done: string) {
    try {
      setSaving(true);
      setError(null);
      await action();
      await refresh();
      setMessage(done);
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setSaving(false);
    }
  }

  function applySubjectToDraft(subjectId: number | null) {
    setBlockDraft((draft) => ({
      ...draft,
      subjectId,
      categoryKey: categoryKeyForSubject(subjectId),
    }));
  }

  function applyTemplateSubject(subjectId: number | null) {
    setTemplateDraft((draft) => ({
      ...draft,
      subjectId,
      categoryKey: categoryKeyForSubject(subjectId),
    }));
  }

  async function handleCreateBlock() {
    if (!blockDraft.title.trim()) return;
    await withSave(async () => {
      await createScheduleBlock(blockDraft);
      setBlockDraft(emptyBlockDraft(selectedDate));
      setShowBlockComposer(false);
    }, '课表块已添加。');
  }

  async function handleAddTodayItem(itemId: number) {
    await withSave(async () => {
      await createScheduleBlockFromTodayItem(itemId, selectedDate, blockDraft.startMinute, blockDraft.endMinute);
    }, '今日任务已安排到课表。');
  }

  async function handleCreateTemplate() {
    if (!templateDraft.title.trim()) return;
    await withSave(async () => {
      await createScheduleTemplate(templateDraft);
      setTemplateDraft(emptyTemplateDraft);
      setShowTemplateComposer(false);
    }, '周模板已保存。');
  }

  function beginEditBlock(block: ScheduleBlock) {
    setEditingBlockId(block.id);
    setEditingBlockDraft({
      scheduleDate: block.schedule_date,
      title: block.title,
      note: block.note ?? '',
      categoryKey: block.category_key,
      subjectId: block.subject_id,
      sourceTodayItemId: block.source_today_item_id,
      startMinute: block.start_minute,
      endMinute: block.end_minute,
    });
  }

  async function handleUpdateBlock() {
    if (!editingBlockId || !editingBlockDraft?.title.trim()) return;
    await withSave(async () => {
      await updateScheduleBlock(editingBlockId, editingBlockDraft);
      setEditingBlockId(null);
      setEditingBlockDraft(null);
    }, '课表块已更新。');
  }

  async function handleStart(block: ScheduleBlock) {
    const appSettings = settings ?? await getAppSettings();
    await withSave(async () => {
      await startStudyModeFromScheduleBlock(
        block.id,
        appSettings.default_study_minutes * 60,
        appSettings.default_focus_minutes * 60,
        appSettings.break_minutes * 60,
        appSettings.long_break_minutes * 60,
        appSettings.long_break_interval,
        appSettings.default_focus_mode,
      );
    }, '已从课表开始专注。');
  }

  return (
    <div className="schedule-page">
      <section className="schedule-hero">
        <div>
          <p className="eyebrow">Schedule</p>
          <h2>今日课表</h2>
          <p>把今日任务和手动安排放进一天的时间轴，到点提醒，但不自动开始专注。</p>
        </div>
        <div className="schedule-actions">
          <button className="ghost-button" type="button" onClick={() => void refresh()}>
            <RefreshCw size={16} /> 刷新
          </button>
          <button className="primary-button" type="button" onClick={() => setShowBlockComposer((value) => !value)}>
            <Plus size={16} /> 时间块
          </button>
        </div>
      </section>

      {(error || message) && <div className={error ? 'alert danger' : 'alert success'}>{error ?? message}</div>}

      <section className="schedule-toolbar soft-panel">
        <div className="segmented-control">
          <button className={view === 'day' ? 'active' : ''} type="button" onClick={() => setView('day')}>今日课表</button>
          <button className={view === 'week' ? 'active' : ''} type="button" onClick={() => setView('week')}>本周视图</button>
        </div>
        <div className="date-stepper">
          <button type="button" aria-label="前一天" onClick={() => setSelectedDate(shiftDate(selectedDate, -1))}>
            <ChevronLeft size={16} />
          </button>
          <input type="date" value={selectedDate} onChange={(event) => setSelectedDate(event.target.value)} />
          <button type="button" aria-label="后一天" onClick={() => setSelectedDate(shiftDate(selectedDate, 1))}>
            <ChevronRight size={16} />
          </button>
        </div>
        <button className="ghost-button" type="button" onClick={() => setShowTemplateComposer((value) => !value)}>
          <CopyPlus size={16} /> 周模板
        </button>
      </section>

      {showBlockComposer && (
        <section className="schedule-composer soft-panel">
          <input
            placeholder="安排标题"
            value={blockDraft.title}
            onChange={(event) => setBlockDraft({ ...blockDraft, title: event.target.value })}
            onKeyDown={(event) => {
              if (event.key === 'Enter' && !event.nativeEvent.isComposing) void handleCreateBlock();
            }}
          />
          <select value={blockDraft.subjectId ?? ''} onChange={(event) => applySubjectToDraft(event.target.value ? Number(event.target.value) : null)}>
            <option value="">未指定科目</option>
            {subjects.map((subject) => <option key={subject.id} value={subject.id}>{subject.name}</option>)}
          </select>
          <input type="time" value={formatMinute(blockDraft.startMinute)} onChange={(event) => setBlockDraft({ ...blockDraft, startMinute: parseTime(event.target.value) })} />
          <input type="time" value={formatMinute(blockDraft.endMinute)} onChange={(event) => setBlockDraft({ ...blockDraft, endMinute: parseTime(event.target.value) })} />
          <input placeholder="备注" value={blockDraft.note ?? ''} onChange={(event) => setBlockDraft({ ...blockDraft, note: event.target.value })} />
          <button className="primary-button" disabled={saving} type="button" onClick={() => void handleCreateBlock()}>保存</button>
        </section>
      )}

      {showTemplateComposer && (
        <section className="schedule-composer template-composer soft-panel">
          <input
            placeholder="模板标题"
            value={templateDraft.title}
            onChange={(event) => setTemplateDraft({ ...templateDraft, title: event.target.value })}
            onKeyDown={(event) => {
              if (event.key === 'Enter' && !event.nativeEvent.isComposing) void handleCreateTemplate();
            }}
          />
          <select value={templateDraft.subjectId ?? ''} onChange={(event) => applyTemplateSubject(event.target.value ? Number(event.target.value) : null)}>
            <option value="">未指定科目</option>
            {subjects.map((subject) => <option key={subject.id} value={subject.id}>{subject.name}</option>)}
          </select>
          <input type="time" value={formatMinute(templateDraft.startMinute)} onChange={(event) => setTemplateDraft({ ...templateDraft, startMinute: parseTime(event.target.value) })} />
          <input type="time" value={formatMinute(templateDraft.endMinute)} onChange={(event) => setTemplateDraft({ ...templateDraft, endMinute: parseTime(event.target.value) })} />
          <div className="weekday-pills">
            {weekdays.map((label, index) => {
              const day = index + 1;
              const active = templateDraft.weekdays.includes(day);
              return (
                <button
                  className={active ? 'active' : ''}
                  key={label}
                  type="button"
                  onClick={() => setTemplateDraft((draft) => ({
                    ...draft,
                    weekdays: active ? draft.weekdays.filter((item) => item !== day) : [...draft.weekdays, day],
                  }))}
                >
                  {label}
                </button>
              );
            })}
          </div>
          <button className="primary-button" disabled={saving} type="button" onClick={() => void handleCreateTemplate()}>保存模板</button>
        </section>
      )}

      <div className="schedule-grid-shell">
        <aside className="today-task-rail soft-panel">
          <div className="panel-title compact-title">
            <div>
              <p className="eyebrow">Today</p>
              <h3>今日任务</h3>
            </div>
          </div>
          {data?.today_items.length ? data.today_items.map((item) => {
            const already = scheduledTodayItemIds.has(item.id);
            return (
              <article className={already ? 'schedule-task-row muted' : 'schedule-task-row'} key={item.id}>
                <div>
                  <strong>{item.title}</strong>
                  <span>{subjectName(subjects, item.subject_id)}{item.due_date ? ` / ${item.due_date}` : ''}</span>
                </div>
                <button disabled={already || saving} type="button" onClick={() => void handleAddTodayItem(item.id)}>
                  {already ? '已安排' : '安排'}
                </button>
              </article>
            );
          }) : <div className="empty-state compact">今日任务为空。</div>}
        </aside>

        {view === 'day' ? (
          <section className="schedule-timeline soft-panel">
            <div className="schedule-time-column">
              {Array.from({ length: (dayEnd - dayStart) / 60 + 1 }, (_, index) => (
                <span key={index}>{formatMinute(dayStart + index * 60)}</span>
              ))}
            </div>
            <div className="schedule-lane">
              {currentMinute !== null && currentMinute >= dayStart && currentMinute <= dayEnd && (
                <div className="schedule-now-line" style={{ top: `${((currentMinute - dayStart) / (dayEnd - dayStart)) * 100}%` }} />
              )}
              {data?.day_blocks.map((block) => (
                <article
                  className={`schedule-block category-${block.category_key}${block.has_conflict ? ' conflict' : ''}`}
                  key={block.id}
                  style={{
                    top: `${((block.start_minute - dayStart) / (dayEnd - dayStart)) * 100}%`,
                    height: `${Math.max(5, ((block.end_minute - block.start_minute) / (dayEnd - dayStart)) * 100)}%`,
                  }}
                >
                  {editingBlockId === block.id && editingBlockDraft ? (
                    <div className="schedule-block-editor">
                      <input
                        value={editingBlockDraft.title}
                        onChange={(event) => setEditingBlockDraft({ ...editingBlockDraft, title: event.target.value })}
                        onKeyDown={(event) => {
                          if (event.key === 'Enter' && !event.nativeEvent.isComposing) void handleUpdateBlock();
                          if (event.key === 'Escape') { setEditingBlockId(null); setEditingBlockDraft(null); }
                        }}
                      />
                      <input type="time" value={formatMinute(editingBlockDraft.startMinute)} onChange={(event) => setEditingBlockDraft({ ...editingBlockDraft, startMinute: parseTime(event.target.value) })} />
                      <input type="time" value={formatMinute(editingBlockDraft.endMinute)} onChange={(event) => setEditingBlockDraft({ ...editingBlockDraft, endMinute: parseTime(event.target.value) })} />
                      <button disabled={saving} type="button" onClick={() => void handleUpdateBlock()}>保存</button>
                      <button type="button" onClick={() => { setEditingBlockId(null); setEditingBlockDraft(null); }}>取消</button>
                    </div>
                  ) : (
                    <>
                      <div onDoubleClick={() => beginEditBlock(block)}>
                        <span>{formatMinute(block.start_minute)}-{formatMinute(block.end_minute)} · {categoryLabel(block.category_key)}</span>
                        <strong>{block.title}</strong>
                        <small>{subjectName(subjects, block.subject_id)}{block.has_conflict ? ' · 时间冲突' : ''}</small>
                      </div>
                      <div className="schedule-block-actions">
                        <button aria-label="开始专注" type="button" onClick={() => void handleStart(block)}><Play size={14} /></button>
                        <button aria-label="编辑" type="button" onClick={() => beginEditBlock(block)}>改</button>
                        <button aria-label="删除" type="button" onClick={() => void withSave(() => deleteScheduleBlock(block.id), '课表块已删除。')}><Trash2 size={14} /></button>
                      </div>
                    </>
                  )}
                </article>
              ))}
              {!data?.day_blocks.length && <div className="schedule-empty"><CalendarDays size={28} />今天还没有安排。</div>}
            </div>
          </section>
        ) : (
          <section className="week-board soft-panel">
            {data?.week_days.map((day, index) => (
              <article className="week-day" key={day.date}>
                <header><strong>{weekdays[index]}</strong><span>{day.date.slice(5)} · {Math.round(day.planned_minutes / 60 * 10) / 10}h</span></header>
                <div className="week-blocks">
                  {day.blocks.map((block) => (
                    <button key={block.id} className={`week-block category-${block.category_key}`} type="button" onClick={() => { setSelectedDate(day.date); setView('day'); }}>
                      <span>{formatMinute(block.start_minute)}</span>{block.title}
                    </button>
                  ))}
                </div>
              </article>
            ))}
          </section>
        )}

        <aside className="template-rail soft-panel">
          <div className="panel-title compact-title">
            <div>
              <p className="eyebrow">Template</p>
              <h3>周模板</h3>
            </div>
            <Clock3 size={18} />
          </div>
          {data?.templates.length ? data.templates.map((template) => (
            <article className="template-row" key={template.id}>
              <div>
                <strong>{template.title}</strong>
                <span>{template.weekdays.map((day) => weekdays[day - 1]).join('、')} · {formatMinute(template.start_minute)}-{formatMinute(template.end_minute)}</span>
              </div>
              <button aria-label="删除模板" type="button" onClick={() => void withSave(() => deleteScheduleTemplate(template.id), '模板已删除。')}><Trash2 size={14} /></button>
            </article>
          )) : <div className="empty-state compact">还没有周模板。</div>}
        </aside>
      </div>
    </div>
  );
}
