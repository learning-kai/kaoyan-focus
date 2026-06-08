import { CalendarClock, ClipboardList, Play } from 'lucide-react';
import type { TodayPlanItem } from '../../types/checklist';
import type { ScheduleBlock } from '../../types/schedule';

type LearningHubProps = {
  completedTodayItems: number;
  hubPrimaryDisabled: boolean;
  hubPrimaryLabel: string;
  isStartingStudy: boolean;
  isSchedulingTask: boolean;
  nextScheduleBlock: ScheduleBlock | null;
  nextTask: TodayPlanItem | null;
  pendingTodayCount: number;
  quickScheduleDisabled: boolean;
  scheduledBlockCount: number;
  scheduleBlockMeta: string | null;
  todayTaskCount: number;
  desktopReady: boolean;
  onPrimaryAction: () => void;
  onOpenSchedule: () => void;
  onOpenTodayTasks: () => void;
  onQuickScheduleTask: () => void;
};

export default function LearningHub({
  completedTodayItems,
  desktopReady,
  hubPrimaryDisabled,
  hubPrimaryLabel,
  isStartingStudy,
  isSchedulingTask,
  nextScheduleBlock,
  nextTask,
  onOpenSchedule,
  onOpenTodayTasks,
  onPrimaryAction,
  onQuickScheduleTask,
  pendingTodayCount,
  quickScheduleDisabled,
  scheduledBlockCount,
  scheduleBlockMeta,
  todayTaskCount,
}: LearningHubProps) {
  const hasScheduleStart = Boolean(nextScheduleBlock);
  const hasTodayTasks = todayTaskCount > 0;
  const PrimaryIcon = hasScheduleStart ? Play : hasTodayTasks ? CalendarClock : ClipboardList;
  const primaryLabel = isStartingStudy ? '正在开始' : hubPrimaryLabel;

  return (
    <div className="learning-hub" aria-label="今日学习中枢">
      <div className="learning-hub-main">
        <p className="eyebrow">今日学习中枢</p>
        <h3>
          {nextScheduleBlock
            ? nextScheduleBlock.title
            : nextTask
              ? nextTask.title
              : pendingTodayCount > 0
                ? '先把今天剩下的任务理顺'
                : '今天的任务已经收束'}
        </h3>
        <p>
          {nextScheduleBlock
            ? scheduleBlockMeta
            : nextTask
              ? `今天还有 ${pendingTodayCount} 项未完成。先排进课表，再开始专注。`
              : pendingTodayCount === 0 && todayTaskCount > 0
                ? '今天的任务已经完成，可以直接开始下一轮专注，或者补一个新的时间块。'
                : '添加一条今日任务，应用会把它接进课表和专注流程里。'}
        </p>
      </div>

      <div className="learning-hub-stats">
        <span>
          <strong>{pendingTodayCount}</strong>
          <small>待完成</small>
        </span>
        <span>
          <strong>{scheduledBlockCount}</strong>
          <small>课表块</small>
        </span>
        <span>
          <strong>{completedTodayItems}</strong>
          <small>已完成</small>
        </span>
      </div>

      <div className="learning-hub-actions">
        <button
          aria-busy={isStartingStudy}
          className="primary-button"
          disabled={hubPrimaryDisabled}
          onClick={onPrimaryAction}
          type="button"
        >
          <PrimaryIcon size={16} />
          {primaryLabel}
        </button>
        {nextTask && !nextScheduleBlock && (
          <button
            className="ghost-button"
            disabled={quickScheduleDisabled}
            onClick={onQuickScheduleTask}
            type="button"
          >
            <CalendarClock size={16} />
            {isSchedulingTask ? '安排中' : '安排下一任务'}
          </button>
        )}
        <button className="ghost-button" onClick={onOpenTodayTasks} type="button">
          <ClipboardList size={16} /> 今日任务
        </button>
        <button className="ghost-button" onClick={onOpenSchedule} type="button">
          <CalendarClock size={16} /> 今日课表
        </button>
      </div>

      {nextScheduleBlock && !desktopReady && (
        <p className="learning-hub-note">浏览器预览不能启动桌面专注，请在 Windows 桌面应用中开始。</p>
      )}
    </div>
  );
}
