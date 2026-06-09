import { CalendarClock, ClipboardList } from 'lucide-react';
import type { TodayPlanItem } from '../../types/checklist';
import type { ScheduleBlock } from '../../types/schedule';

type LearningHubProps = {
  completedTodayItems: number;
  isSchedulingTask: boolean;
  nextScheduleBlock: ScheduleBlock | null;
  nextTask: TodayPlanItem | null;
  pendingTodayCount: number;
  quickScheduleDisabled: boolean;
  scheduledBlockCount: number;
  scheduleBlockMeta: string | null;
  todayTaskCount: number;
  desktopReady: boolean;
  onOpenSchedule: () => void;
  onOpenTodayTasks: () => void;
  onQuickScheduleTask: () => void;
};

export default function LearningHub({
  completedTodayItems,
  desktopReady,
  isSchedulingTask,
  nextScheduleBlock,
  nextTask,
  onOpenSchedule,
  onOpenTodayTasks,
  onQuickScheduleTask,
  pendingTodayCount,
  quickScheduleDisabled,
  scheduledBlockCount,
  scheduleBlockMeta,
  todayTaskCount,
}: LearningHubProps) {
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
        {nextTask && !nextScheduleBlock && (
          <button
            className="learning-hub-card learning-hub-card-wide"
            disabled={quickScheduleDisabled}
            onClick={onQuickScheduleTask}
            type="button"
          >
            <span className="learning-hub-card-icon" aria-hidden="true">
              <CalendarClock size={16} />
            </span>
            <span className="learning-hub-card-copy">
              <span>{isSchedulingTask ? '安排中' : '安排下一任务'}</span>
              <strong>接入今日课表</strong>
            </span>
          </button>
        )}
        <button className="learning-hub-card" onClick={onOpenTodayTasks} type="button">
          <span className="learning-hub-card-icon" aria-hidden="true">
            <ClipboardList size={16} />
          </span>
          <span className="learning-hub-card-copy">
            <span>今日任务</span>
            <strong>查看待办</strong>
          </span>
        </button>
        <button className="learning-hub-card" onClick={onOpenSchedule} type="button">
          <span className="learning-hub-card-icon" aria-hidden="true">
            <CalendarClock size={16} />
          </span>
          <span className="learning-hub-card-copy">
            <span>今日课表</span>
            <strong>查看时间轴</strong>
          </span>
        </button>
      </div>

      {nextScheduleBlock && !desktopReady && (
        <p className="learning-hub-note">浏览器预览不能启动桌面专注，请在 Windows 桌面应用中开始。</p>
      )}
    </div>
  );
}
