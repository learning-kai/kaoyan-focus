import { CalendarClock, ClipboardList, Play } from 'lucide-react';
import type { TodayPlanItem } from '../../types/checklist';
import type { ScheduleBlock } from '../../types/schedule';

type LearningHubProps = {
  completedTodayItems: number;
  hubPrimaryDisabled: boolean;
  hubPrimaryLabel: string;
  isStartingStudy: boolean;
  nextScheduleBlock: ScheduleBlock | null;
  nextTask: TodayPlanItem | null;
  pendingTodayCount: number;
  scheduledBlockCount: number;
  scheduleBlockMeta: string | null;
  todayTaskCount: number;
  desktopReady: boolean;
  onPrimaryAction: () => void;
  onOpenSchedule: () => void;
  onOpenTodayTasks: () => void;
};

export default function LearningHub({
  completedTodayItems,
  desktopReady,
  hubPrimaryDisabled,
  hubPrimaryLabel,
  isStartingStudy,
  nextScheduleBlock,
  nextTask,
  onOpenSchedule,
  onOpenTodayTasks,
  onPrimaryAction,
  pendingTodayCount,
  scheduledBlockCount,
  scheduleBlockMeta,
  todayTaskCount,
}: LearningHubProps) {
  const hasScheduleStart = Boolean(nextScheduleBlock);

  return (
    <div className="learning-hub" aria-label="今日学习中枢">
      <div className="learning-hub-main">
        <p className="eyebrow">今日学习中枢</p>
        <h3>{nextScheduleBlock ? nextScheduleBlock.title : nextTask ? nextTask.title : '先确定今天要推进什么'}</h3>
        <p>
          {nextScheduleBlock
            ? scheduleBlockMeta
            : nextTask
              ? `今日还有 ${pendingTodayCount} 项未完成。先排入课表，再开始专注。`
              : '添加一条今日任务，应用会把它接到课表和专注流程里。'}
        </p>
      </div>
      <div className="learning-hub-stats">
        <span><strong>{pendingTodayCount}</strong> 待完成</span>
        <span><strong>{scheduledBlockCount}</strong> 课表块</span>
        <span><strong>{completedTodayItems}</strong> 已完成</span>
      </div>
      <div className="learning-hub-actions">
        <button
          aria-busy={isStartingStudy && hasScheduleStart}
          className="primary-button"
          disabled={hubPrimaryDisabled}
          onClick={onPrimaryAction}
          type="button"
        >
          {hasScheduleStart ? <Play size={16} /> : todayTaskCount > 0 ? <CalendarClock size={16} /> : <ClipboardList size={16} />}
          {isStartingStudy && hasScheduleStart ? '正在开始' : hubPrimaryLabel}
        </button>
        <button className="ghost-button" onClick={onOpenTodayTasks} type="button">
          <ClipboardList size={16} /> 今日任务
        </button>
        <button className="ghost-button" onClick={onOpenSchedule} type="button">
          <CalendarClock size={16} /> 今日课表
        </button>
      </div>
      {nextScheduleBlock && !desktopReady && <p className="learning-hub-note">浏览器预览不能启动桌面专注；请在 Windows 桌面应用中开始。</p>}
    </div>
  );
}
