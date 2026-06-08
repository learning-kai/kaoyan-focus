import type { ScheduleBlock } from '../types/schedule';

export type ScheduleRecommendationKind = 'current' | 'next' | 'missed';

export type ScheduleRecommendation = {
  block: ScheduleBlock;
  kind: ScheduleRecommendationKind;
};

export function recommendScheduleBlock(
  blocks: ScheduleBlock[],
  currentMinute: number,
): ScheduleRecommendation | null {
  const activeBlocks = blocks
    .filter((block) => block.status !== 'completed')
    .sort((left, right) =>
      left.start_minute - right.start_minute ||
      left.end_minute - right.end_minute ||
      left.id - right.id,
    );

  const current = activeBlocks.find((block) =>
    block.start_minute <= currentMinute && block.end_minute > currentMinute,
  );
  if (current) {
    return { block: current, kind: 'current' };
  }

  const next = activeBlocks.find((block) => block.start_minute > currentMinute);
  if (next) {
    return { block: next, kind: 'next' };
  }

  const missed = [...activeBlocks].reverse().find((block) => block.end_minute <= currentMinute);
  return missed ? { block: missed, kind: 'missed' } : null;
}
