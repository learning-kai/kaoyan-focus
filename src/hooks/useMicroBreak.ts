import { useEffect, useState, useSyncExternalStore } from 'react';
import {
  getMicroBreakSnapshot,
  loadMicroBreakSettings,
  startMicroBreakListener,
  subscribeMicroBreak,
  type MicroBreakSnapshot,
} from '../services/microBreakCoordinator';
import { microBreakSecondsLeft } from '../utils/microBreak';

/** 在 App 顶层调用一次：切页、最小化到托盘都不会中断监听。 */
export function useMicroBreakListener() {
  useEffect(() => {
    void loadMicroBreakSettings();
    return startMicroBreakListener();
  }, []);
}

export function useMicroBreak(): MicroBreakSnapshot {
  return useSyncExternalStore(subscribeMicroBreak, getMicroBreakSnapshot, getMicroBreakSnapshot);
}

/** 闭眼倒计时的剩余秒数，只在有微休息时刷新。 */
export function useMicroBreakCountdown(endsAt: number | null): number {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    if (endsAt === null) return;
    setNow(Date.now());
    const timer = window.setInterval(() => setNow(Date.now()), 250);
    return () => window.clearInterval(timer);
  }, [endsAt]);
  return endsAt === null ? 0 : microBreakSecondsLeft(endsAt, now);
}
