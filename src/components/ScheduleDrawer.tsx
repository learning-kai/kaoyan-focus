import { CalendarClock, Play, Plus, RefreshCw, Trash2, X } from 'lucide-react';
import type { ScheduleBlock, ScheduleBlockDraft, SchedulePageData } from '../types/schedule';

type ScheduleDrawerProps = {
  isOpen: boolean;
  data: SchedulePageData | null;
  draft: ScheduleBlockDraft;
  saving: boolean;
  onClose: () => void;
  onRefresh: () => void;
  onDraftChange: (patch: Partial<ScheduleBlockDraft>) => void;
  onCreate: () => void;
  onDelete: (id: number) => void;
  onStart: (block: ScheduleBlock) => void;
  canStart?: boolean;
};

function formatMinute(minute: number) {
  const safe = Math.max(0, Math.min(24 * 60, minute));
  return `${String(Math.floor(safe / 60)).padStart(2, '0')}:${String(safe % 60).padStart(2, '0')}`;
}

function parseTime(value: string) {
  const [hour, minute] = value.split(':').map(Number);
  return (Number.isFinite(hour) ? hour : 0) * 60 + (Number.isFinite(minute) ? minute : 0);
}

function categoryLabel(key: string) {
  if (key === 'politics') return '政治';
  if (key === 'english') return '英语';
  if (key === 'math') return '数学';
  if (key === 'major') return '专业课';
  return '通用';
}

function currentTopPercent() {
  const now = new Date();
  const minute = now.getHours() * 60 + now.getMinutes();
  const dayStart = 6 * 60;
  const dayEnd = 24 * 60;
  if (minute < dayStart || minute > dayEnd) return null;
  return ((minute - dayStart) / (dayEnd - dayStart)) * 100;
}

export default function ScheduleDrawer({
  isOpen,
  data,
  draft,
  saving,
  onClose,
  onRefresh,
  onDraftChange,
  onCreate,
  onDelete,
  onStart,
  canStart = true,
}: ScheduleDrawerProps) {
  const canCreate = Boolean(draft.title.trim()) && !saving;
  const nowTop = currentTopPercent();

  return (
    <section aria-hidden={!isOpen} className={`schedule-drawer${isOpen ? ' is-open' : ''}`}>
      <div className="panel-title schedule-drawer-head">
        <div>
          <p className="eyebrow">Schedule</p>
          <h3>今日课表</h3>
        </div>
        <div className="today-drawer-tools">
          <button className="focus-hud-card today-drawer-tool-card today-drawer-tool-icon" disabled={saving} onClick={onRefresh} title="刷新课表" type="button">
            <RefreshCw size={15} />
          </button>
          <button aria-label="关闭课表" className="focus-hud-card today-drawer-tool-card today-drawer-tool-icon" onClick={onClose} title="关闭课表" type="button">
            <X size={15} />
          </button>
        </div>
      </div>

      <div className="today-plan-meta">
        <span>{data?.selected_date ?? ''}</span>
        <span>{data?.day_blocks.length ?? 0} 个时间块</span>
        <span>{data?.day_blocks.filter((block) => block.status === 'completed').length ?? 0} 已完成</span>
      </div>

      <div className="schedule-drawer-composer">
        <input
          className="text-input"
          onChange={(event) => onDraftChange({ title: event.target.value })}
          onKeyDown={(event) => {
            if (event.key === 'Enter' && !event.nativeEvent.isComposing && canCreate) onCreate();
          }}
          placeholder="新增时间块"
          value={draft.title}
        />
        <input className="text-input" type="time" value={formatMinute(draft.startMinute)} onChange={(event) => onDraftChange({ startMinute: parseTime(event.target.value) })} />
        <input className="text-input" type="time" value={formatMinute(draft.endMinute)} onChange={(event) => onDraftChange({ endMinute: parseTime(event.target.value) })} />
        <button className="primary-action" disabled={!canCreate} type="button" onClick={onCreate}>
          <Plus size={15} /> 添加
        </button>
      </div>

      <div className="schedule-drawer-timeline">
        {nowTop !== null && <div className="schedule-drawer-now" style={{ top: `${nowTop}%` }} />}
        {data?.day_blocks.length ? data.day_blocks.map((block) => (
          <article className={`schedule-drawer-block category-${block.category_key}${block.has_conflict ? ' conflict' : ''}`} key={block.id}>
            <div>
              <span>{formatMinute(block.start_minute)}-{formatMinute(block.end_minute)} · {categoryLabel(block.category_key)}</span>
              <strong>{block.title}</strong>
              {block.has_conflict && <small>时间冲突</small>}
            </div>
            <div className="schedule-drawer-actions">
              {canStart && <button aria-label="开始专注" disabled={saving} type="button" onClick={() => onStart(block)}><Play size={13} /></button>}
              <button aria-label="删除时间块" disabled={saving} type="button" onClick={() => onDelete(block.id)}><Trash2 size={13} /></button>
            </div>
          </article>
        )) : (
          <div className="empty-state compact">
            <CalendarClock size={24} />
            <strong>今天还没有课表安排</strong>
            <p>可以在这里临时补一块时间。</p>
          </div>
        )}
      </div>
    </section>
  );
}
