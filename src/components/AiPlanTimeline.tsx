import { AlertTriangle, CalendarX2, Clock } from 'lucide-react';
import type {
  AiPlanItem,
  AiPlanWarning,
  AiUnscheduledEntry,
} from '../types/aiScheduler';

/**
 * AI 草案时间轴（S3 只读版）。
 *
 * 刻意做成纯展示组件、不持有任何状态：S5 会在它之上接「拖拽调整」，
 * 届时只需把移动回调加进来，不必重写渲染逻辑。
 */

export function formatMinute(minute: number) {
  const safe = Math.max(0, Math.min(24 * 60, minute));
  const hours = Math.floor(safe / 60);
  const minutes = safe % 60;
  return `${String(hours).padStart(2, '0')}:${String(minutes).padStart(2, '0')}`;
}

/** `category_key` 的兜底显示名。宿主页面若传入真实标签（来自设置里的分类名）则优先用。 */
const FALLBACK_CATEGORY_LABELS: Record<string, string> = {
  politics: '政治',
  english: '英语',
  math: '数学',
  major: '专业课',
  general: '通用',
};

const PRIORITY_LABELS: Record<string, string> = {
  high: '高',
  medium: '中',
  low: '低',
};

function categoryLabel(key: string, labels?: Record<string, string>) {
  return labels?.[key] ?? FALLBACK_CATEGORY_LABELS[key] ?? key;
}

function durationLabel(minutes: number) {
  if (minutes <= 0) {
    return '0 分钟';
  }
  if (minutes < 60) {
    return `${minutes} 分钟`;
  }
  const hours = minutes / 60;
  return Number.isInteger(hours) ? `${hours} 小时` : `${hours.toFixed(1)} 小时`;
}

export type AiPlanTimelineProps = {
  items: AiPlanItem[];
  warnings: AiPlanWarning[];
  unscheduled: AiUnscheduledEntry[];
  /** 分类显示名，来自宿主页面的清单数据 */
  categoryLabels?: Record<string, string>;
  /** 多日规划时才显示日期分组标题 */
  showDateGroups?: boolean;
};

export default function AiPlanTimeline({
  items,
  warnings,
  unscheduled,
  categoryLabels,
  showDateGroups = false,
}: AiPlanTimelineProps) {
  const byDate = new Map<string, AiPlanItem[]>();
  for (const item of items) {
    const bucket = byDate.get(item.schedule_date);
    if (bucket) {
      bucket.push(item);
    } else {
      byDate.set(item.schedule_date, [item]);
    }
  }
  const dates = [...byDate.keys()].sort();

  // 只把「针对具体条目」的告警贴到条目上；其余（如超容量）在汇总区统一展示。
  const warningsByItem = new Map<string, AiPlanWarning[]>();
  for (const warning of warnings) {
    if (!warning.item_id) {
      continue;
    }
    const bucket = warningsByItem.get(warning.item_id);
    if (bucket) {
      bucket.push(warning);
    } else {
      warningsByItem.set(warning.item_id, [warning]);
    }
  }

  if (items.length === 0 && unscheduled.length === 0) {
    return (
      <div className="empty-state compact ai-plan-empty">
        <CalendarX2 size={24} />
        <strong>暂时排不出条目</strong>
        <p>可以在上方放宽可用时段或减少任务后重新生成。</p>
      </div>
    );
  }

  return (
    <div className="ai-plan-timeline">
      {dates.map((date) => {
        const dayItems = [...(byDate.get(date) ?? [])].sort(
          (left, right) => left.start_minute - right.start_minute,
        );
        const dayMinutes = dayItems.reduce(
          (total, item) => total + (item.end_minute - item.start_minute),
          0,
        );

        return (
          <div className="ai-plan-day" key={date}>
            {showDateGroups && (
              <div className="ai-plan-day-head">
                <span>{date}</span>
                <small>
                  {dayItems.length} 条 · {durationLabel(dayMinutes)}
                </small>
              </div>
            )}
            {dayItems.map((item) => {
              const itemWarnings = warningsByItem.get(item.id) ?? [];
              const hasConflict = item.conflict_with.length > 0;
              const minutes = item.end_minute - item.start_minute;
              return (
                <article
                  aria-label={`${item.title}，${formatMinute(item.start_minute)} 到 ${formatMinute(item.end_minute)}`}
                  className={`ai-plan-item category-${item.category_key}${item.kind === 'meal' ? ' is-meal' : ''}${hasConflict ? ' has-conflict' : ''}`}
                  key={item.id}
                >
                  <div className="ai-plan-item-time">
                    <strong>{formatMinute(item.start_minute)}</strong>
                    <span>{formatMinute(item.end_minute)}</span>
                  </div>
                  <div className="ai-plan-item-body">
                    <strong className="ai-plan-item-title">{item.title}</strong>
                    <small>
                      {item.kind === 'meal'
                        ? '生活安排 · 固定时间 · 不占学习目标'
                        : `${categoryLabel(item.category_key, categoryLabels)} · ${durationLabel(minutes)} · 优先级${PRIORITY_LABELS[item.priority] ?? item.priority}`}
                    </small>
                    {item.rationale && <p className="ai-plan-item-rationale">{item.rationale}</p>}
                    {hasConflict && (
                      <small className="ai-plan-item-warn">
                        <AlertTriangle size={11} /> 与 {item.conflict_with.length} 条已有日程时间重叠
                      </small>
                    )}
                    {itemWarnings.map((warning, index) => (
                      <small className="ai-plan-item-warn" key={`${warning.code}-${index}`}>
                        <AlertTriangle size={11} /> {warning.message}
                      </small>
                    ))}
                  </div>
                </article>
              );
            })}
          </div>
        );
      })}

      {unscheduled.length > 0 && (
        <div className="ai-plan-unscheduled">
          <div className="ai-plan-day-head">
            <span>
              <Clock size={12} /> 没排上的任务
            </span>
            <small>{unscheduled.length} 条</small>
          </div>
          {unscheduled.map((entry) => (
            <div className="ai-plan-unscheduled-item" key={entry.item_id}>
              <strong>{entry.title}</strong>
              <small>{entry.reason}</small>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
