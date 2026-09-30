import { AlertTriangle, CalendarX2, Clock, Info, X } from 'lucide-react';
import type { AiPlanItem, AiPlanWarning, AiUnscheduledEntry } from '../types/aiScheduler';
import {
  adjustedItemIds,
  dateHeading,
  formatDuration,
  formatMinute,
  segmentLabels,
} from '../utils/aiPlanDrawer';

/**
 * AI 草案时间轴。
 *
 * 纯展示组件、不持有状态：删除由宿主通过 `onRemoveItem` 处理；S5 的拖拽调整
 * 同样以回调接进来，不必重写渲染逻辑。
 */

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

function groupByDate(items: AiPlanItem[]): Map<string, AiPlanItem[]> {
  const byDate = new Map<string, AiPlanItem[]>();
  for (const item of items) {
    const bucket = byDate.get(item.schedule_date);
    if (bucket) {
      bucket.push(item);
    } else {
      byDate.set(item.schedule_date, [item]);
    }
  }
  return byDate;
}

function groupWarningsByItem(warnings: AiPlanWarning[]): Map<string, AiPlanWarning[]> {
  const byItem = new Map<string, AiPlanWarning[]>();
  for (const warning of warnings) {
    if (!warning.item_id) {
      continue;
    }
    const bucket = byItem.get(warning.item_id);
    if (bucket) {
      bucket.push(warning);
    } else {
      byItem.set(warning.item_id, [warning]);
    }
  }
  return byItem;
}

export type AiPlanTimelineProps = {
  items: AiPlanItem[];
  warnings: AiPlanWarning[];
  unscheduled: AiUnscheduledEntry[];
  /** 分类显示名，来自宿主页面的清单数据 */
  categoryLabels?: Record<string, string>;
  /** 本机今天（YYYY-MM-DD），日期标题据此标「今天 / 明天」 */
  today?: string;
  /** 传入时每条右上角出现删除按钮 */
  onRemoveItem?: (item: AiPlanItem) => void;
  removeDisabled?: boolean;
};

export default function AiPlanTimeline({
  items,
  warnings,
  unscheduled,
  categoryLabels,
  today,
  onRemoveItem,
  removeDisabled = false,
}: AiPlanTimelineProps) {
  const byDate = groupByDate(items);
  const dates = [...byDate.keys()].sort();
  const warningsByItem = groupWarningsByItem(warnings);
  const adjusted = adjustedItemIds(warnings);
  const segments = segmentLabels(items);

  if (items.length === 0 && unscheduled.length === 0) {
    return (
      <div className="empty-state compact ai-plan-empty">
        <CalendarX2 size={24} />
        <strong>暂时排不出条目</strong>
        <p>可以放宽可用时段或减少任务后重新生成。</p>
      </div>
    );
  }

  return (
    <div className="ai-plan-timeline">
      {dates.map((date) => {
        const dayItems = [...(byDate.get(date) ?? [])].sort(
          (left, right) => left.start_minute - right.start_minute,
        );
        const studyItems = dayItems.filter((item) => item.kind !== 'meal');
        const studyMinutes = studyItems.reduce(
          (total, item) => total + (item.end_minute - item.start_minute),
          0,
        );
        // 拆段的条目只算一条。
        const studyCount = new Set(
          studyItems.map((item) => item.source_today_item_id ?? item.id),
        ).size;

        return (
          <div className="ai-plan-day" key={date}>
            <div className="ai-plan-day-head">
              <span>{dateHeading(date, today)}</span>
              <small>
                {studyCount} 条学习 · {formatDuration(studyMinutes)}
              </small>
            </div>
            {dayItems.map((item) => {
              const itemWarnings = warningsByItem.get(item.id) ?? [];
              const hasConflict = item.conflict_with.length > 0;
              const isMeal = item.kind === 'meal';
              const isAdjusted = adjusted.has(item.id);
              const segment = segments.get(item.id);
              const minutes = item.end_minute - item.start_minute;
              return (
                <article
                  aria-label={`${item.title}，${formatMinute(item.start_minute)} 到 ${formatMinute(item.end_minute)}${isAdjusted ? '，已自动调整' : ''}`}
                  className={`ai-plan-item category-${item.category_key}${isMeal ? ' is-meal' : ''}${hasConflict ? ' has-conflict' : ''}${isAdjusted ? ' is-adjusted' : ''}`}
                  key={item.id}
                >
                  <div className="ai-plan-item-time">
                    <strong>{formatMinute(item.start_minute)}</strong>
                    <span>{formatMinute(item.end_minute)}</span>
                  </div>
                  <div className="ai-plan-item-body">
                    <div className="ai-plan-item-head">
                      <strong className="ai-plan-item-title" title={item.title}>
                        {item.title}
                      </strong>
                      {segment && <span className="ai-plan-badge">{segment}</span>}
                      {isAdjusted && <span className="ai-plan-badge is-adjusted">已自动调整</span>}
                      {onRemoveItem && (
                        <button
                          aria-label={`从草案中删除「${item.title}」`}
                          className="ai-plan-item-remove"
                          disabled={removeDisabled}
                          onClick={() => onRemoveItem(item)}
                          title={segment ? '从草案中删除（各段一起删）' : '从草案中删除'}
                          type="button"
                        >
                          <X size={13} />
                        </button>
                      )}
                    </div>
                    <small>
                      {isMeal
                        ? '生活安排 · 固定时间 · 不占学习目标'
                        : `${categoryLabel(item.category_key, categoryLabels)} · ${formatDuration(minutes)} · 优先级${PRIORITY_LABELS[item.priority] ?? item.priority}`}
                    </small>
                    {item.rationale && <p className="ai-plan-item-rationale">{item.rationale}</p>}
                    {hasConflict && (
                      <small className="ai-plan-item-warn">
                        <AlertTriangle size={11} /> 与 {item.conflict_with.length} 条已有日程时间重叠
                      </small>
                    )}
                    {itemWarnings.map((warning, index) =>
                      warning.code === 'adjusted' ? (
                        <small className="ai-plan-item-note" key={`${warning.code}-${index}`}>
                          <Info size={11} /> {warning.message}
                        </small>
                      ) : (
                        <small className="ai-plan-item-warn" key={`${warning.code}-${index}`}>
                          <AlertTriangle size={11} /> {warning.message}
                        </small>
                      ),
                    )}
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
