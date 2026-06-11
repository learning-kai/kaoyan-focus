import { useEffect, useRef } from 'react';
import {
  notifyPersistentAlarm,
  notifyStudyReminder,
  stopPersistentAlarmSound,
} from '../services/alertApi';
import {
  ALARM_STATE_CHANGED_EVENT,
  getNextAlarm,
  notifyAlarmStateChanged,
  triggerDueAlarms,
} from '../services/alarmApi';
import { checkDueTaskEmailReminders } from '../services/emailApi';
import {
  FEISHU_SYNC_REFRESH_EVENT,
  syncFeishuBridge,
} from '../services/feishuApi';
import { getStudyModeState } from '../services/focusApi';
import { getSchedulePageData } from '../services/scheduleApi';
import { getAppSettings, getSyncDeviceId } from '../services/settingsApi';
import { buildStudyReminder, isFinishedStudyMode, isStaleFinishedStudyReminder, markStudyReminderSeen } from '../services/studyReminder';
import { listenTauriEvent } from '../services/tauriEvents';
import { isTauriRuntime } from '../services/tauriInvoke';
import { checkForAppUpdate } from '../services/updateApi';
import { showStudyReminder } from '../services/systemApi';
import {
  autoSyncConfiguredDatabase,
  STUDY_SYNC_STATE_CHANGED_EVENT,
  syncConfiguredStateChange,
} from '../services/syncApi';
import { currentMinuteOfDay, formatDateKey } from '../utils/date';
import type { Alarm } from '../types/alarm';
import type { AppPage } from '../types/navigation';

const AUTO_SYNC_STARTUP_DELAY_MS = 5000;
const AUTO_SYNC_INTERVAL_MS = 60 * 1000;
const ACTIVE_AUTO_SYNC_INTERVAL_MS = 60 * 1000;
const AUTO_UPDATE_CHECK_STARTUP_DELAY_MS = 12 * 1000;
const AUTO_UPDATE_CHECK_INTERVAL_MS = 6 * 60 * 60 * 1000;
const AUTO_UPDATE_NOTICE_STORAGE_KEY = 'kaoyan-focus:last-auto-update-notice-version';
const FEISHU_AUTO_SYNC_INTERVAL_MS = 30 * 1000;
const SCHEDULE_REMINDER_INTERVAL_MS = 30 * 1000;
const SCHEDULE_REMINDER_LOOKBACK_MINUTES = 1;
const EMAIL_REMINDER_INTERVAL_MS = 5 * 60 * 1000;
const STUDY_COMPLETION_REMINDER_INTERVAL_MS = 3 * 1000;
const STUDY_COMPLETION_SYNC_SUPPRESS_MS = 15 * 1000;
const ALARM_CHECK_MIN_DELAY_MS = 1000;
const ALARM_CHECK_MAX_DELAY_MS = 60 * 1000;
const SILENT_AUTO_SYNC_SKIP_REASONS = new Set([
  'webdav_not_configured',
  'webdav_disabled',
  'object_storage_not_configured',
  'object_storage_disabled',
  'object_storage_sync_in_flight',
  'study_mode_active',
]);

export function useAutoSync(setLastAutoSyncMessage: (message: string | null) => void) {
  useEffect(() => {
    let disposed = false;
    let syncInFlight = false;
    let feishuSyncInFlight = false;
    let lastAutoSyncAt = 0;
    let lastFeishuAutoSyncAt = 0;

    async function runFeishuSync(trigger = 'auto') {
      if (feishuSyncInFlight) {
        return;
      }

      lastFeishuAutoSyncAt = Date.now();
      feishuSyncInFlight = true;
      await syncFeishuBridge(trigger)
        .then((result) => {
          if (result.status === 'synced' && (result.pulled_count > 0 || result.deleted_count > 0)) {
            window.dispatchEvent(new CustomEvent(FEISHU_SYNC_REFRESH_EVENT, { detail: result }));
            void syncConfiguredStateChange('feishu_bridge_in').catch(() => undefined);
          }
        })
        .catch(() => {
          // Feishu bridge status is visible in Settings; automatic checks stay quiet.
        })
        .finally(() => {
          feishuSyncInFlight = false;
        });
    }

    async function runAutoSync() {
      if (syncInFlight) {
        return;
      }

      lastAutoSyncAt = Date.now();
      syncInFlight = true;
      await autoSyncConfiguredDatabase()
        .then((result) => {
          if (disposed || (result.skipped_reason && SILENT_AUTO_SYNC_SKIP_REASONS.has(result.skipped_reason))) {
            return;
          }

          setLastAutoSyncMessage(result.message);
        })
        .catch((reason) => {
          if (disposed) {
            return;
          }

          setLastAutoSyncMessage(reason instanceof Error ? reason.message : String(reason));
        })
        .finally(() => {
          syncInFlight = false;
        });

      void runFeishuSync('auto');
    }

    async function getRequiredInterval() {
      try {
        const state = await getStudyModeState();
        const active = state.status === 'active' && ['focus', 'awaiting_break', 'break'].includes(state.phase);
        return active ? ACTIVE_AUTO_SYNC_INTERVAL_MS : AUTO_SYNC_INTERVAL_MS;
      } catch {
        return AUTO_SYNC_INTERVAL_MS;
      }
    }

    const startupTimerId = window.setTimeout(() => {
      void runAutoSync();
    }, AUTO_SYNC_STARTUP_DELAY_MS);

    const intervalId = window.setInterval(() => {
      void (async () => {
        const requiredInterval = await getRequiredInterval();
        if (Date.now() - lastAutoSyncAt >= requiredInterval) {
          await runAutoSync();
        }
        if (Date.now() - lastFeishuAutoSyncAt >= FEISHU_AUTO_SYNC_INTERVAL_MS) {
          await runFeishuSync('auto_poll');
        }
      })();
    }, ACTIVE_AUTO_SYNC_INTERVAL_MS);

    return () => {
      disposed = true;
      window.clearTimeout(startupTimerId);
      window.clearInterval(intervalId);
    };
  }, [setLastAutoSyncMessage]);
}

export function useSyncTakeoverNavigation(setActivePage: (page: AppPage) => void) {
  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;

    void listenTauriEvent<{ active_state_changed?: boolean; took_over_active_mode?: boolean }>(
      STUDY_SYNC_STATE_CHANGED_EVENT,
      (event) => {
        if (event.payload?.took_over_active_mode) {
          setActivePage('focus');
        }
      },
    ).then((dispose) => {
      if (disposed) {
        dispose();
        return;
      }

      unlisten = dispose;
    }).catch(() => {
      // Browser previews and partial desktop runtimes should not surface event wiring noise.
    });

    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [setActivePage]);
}

export function useStudyCompletionReminder() {
  const initializedRef = useRef(false);
  const syncDeviceIdRef = useRef<string | null>(null);
  const syncDeviceIdLoadedRef = useRef(false);
  const suppressFinishedReminderUntilRef = useRef(0);

  useEffect(() => {
    let disposed = false;
    let checkInFlight = false;
    let unlisten: (() => void) | undefined;

    async function resolveSyncDeviceId() {
      if (syncDeviceIdLoadedRef.current) {
        return syncDeviceIdRef.current;
      }

      try {
        syncDeviceIdRef.current = await getSyncDeviceId();
      } catch {
        syncDeviceIdRef.current = null;
      }
      syncDeviceIdLoadedRef.current = true;
      return syncDeviceIdRef.current;
    }

    async function checkStudyCompletion() {
      if (checkInFlight) {
        return;
      }

      checkInFlight = true;
      try {
        const [studyState, syncDeviceId] = await Promise.all([getStudyModeState(), resolveSyncDeviceId()]);
        if (disposed) {
          return;
        }

        const finished = isFinishedStudyMode(studyState);
        if (!initializedRef.current) {
          initializedRef.current = true;
          if (finished && isStaleFinishedStudyReminder(studyState)) {
            markStudyReminderSeen(studyState, syncDeviceId);
            return;
          }
        }

        if (!finished) {
          return;
        }

        if (isStaleFinishedStudyReminder(studyState)) {
          markStudyReminderSeen(studyState, syncDeviceId);
          return;
        }

        if (Date.now() <= suppressFinishedReminderUntilRef.current) {
          markStudyReminderSeen(studyState, syncDeviceId);
          return;
        }

        const reminder = buildStudyReminder(studyState);
        if (!reminder || !markStudyReminderSeen(studyState, syncDeviceId)) {
          return;
        }

        void notifyStudyReminder(reminder);
      } catch {
        // Completion reminders are best-effort; the Focus page still shows the final state.
      } finally {
        checkInFlight = false;
      }
    }

    void listenTauriEvent<{ took_over_active_mode?: boolean }>(STUDY_SYNC_STATE_CHANGED_EVENT, (event) => {
      if (event.payload?.took_over_active_mode) {
        suppressFinishedReminderUntilRef.current = Date.now() + STUDY_COMPLETION_SYNC_SUPPRESS_MS;
      }
      void checkStudyCompletion();
    })
      .then((dispose) => {
        if (disposed) {
          dispose();
          return;
        }

        unlisten = dispose;
      })
      .catch(() => {
        // Browser previews and partial desktop runtimes may not expose this event.
      });

    void checkStudyCompletion();
    const intervalId = window.setInterval(() => {
      void checkStudyCompletion();
    }, STUDY_COMPLETION_REMINDER_INTERVAL_MS);

    return () => {
      disposed = true;
      window.clearInterval(intervalId);
      unlisten?.();
    };
  }, []);
}

export function useAutoUpdateCheck(
  setLastAutoUpdateMessage: (message: string | null) => void,
  onUpdateAvailable?: (update: { version: string; body: string | null }) => void,
) {
  useEffect(() => {
    if (!isTauriRuntime()) {
      return;
    }

    let disposed = false;
    let updateCheckInFlight = false;

    async function runAutoUpdateCheck() {
      if (updateCheckInFlight) {
        return;
      }

      updateCheckInFlight = true;
      try {
        const update = await checkForAppUpdate();
        if (disposed) {
          return;
        }

        if (update === null) {
          setLastAutoUpdateMessage('自动检查更新：当前已经是最新版本。');
          return;
        }

        // 检查是否跳过此版本
        try {
          const settings = await getAppSettings();
          if (disposed) {
            return;
          }

          if (settings.skip_update_version === update.version) {
            setLastAutoUpdateMessage(`自动检查更新：版本 ${update.version} 已被跳过。`);
            return;
          }

          // 检查是否在稍后提醒期间
          if (settings.update_reminder_snooze_until) {
            const snoozeUntil = settings.update_reminder_snooze_until * 1000; // 转换为毫秒
            if (Date.now() < snoozeUntil) {
              setLastAutoUpdateMessage(`自动检查更新：发现新版本 ${update.version}，提醒已暂时关闭。`);
              return;
            }
          }

          const message = `自动检查更新：发现新版本 ${update.version}，可在这里下载并安装。`;
          setLastAutoUpdateMessage(message);

          // 通知应用内弹窗
          if (onUpdateAvailable) {
            onUpdateAvailable({ version: update.version, body: update.body ?? null });
          }

          // 系统通知（每个版本只通知一次）
          const lastNotifiedVersion = window.localStorage.getItem(AUTO_UPDATE_NOTICE_STORAGE_KEY);
          if (lastNotifiedVersion !== update.version) {
            window.localStorage.setItem(AUTO_UPDATE_NOTICE_STORAGE_KEY, update.version);
            void showStudyReminder(
              '发现新版本',
              `考研专注 ${update.version} 可更新，打开设置页下载并安装。`,
              'silent',
              `update:${update.version}`,
            ).catch(() => {
              // Settings page also shows the automatic check result; notification is best-effort.
            });
          }
        } catch {
          // 获取设置失败时，使用默认行为
          const message = `自动检查更新：发现新版本 ${update.version}，可在这里下载并安装。`;
          setLastAutoUpdateMessage(message);

          const lastNotifiedVersion = window.localStorage.getItem(AUTO_UPDATE_NOTICE_STORAGE_KEY);
          if (lastNotifiedVersion !== update.version) {
            window.localStorage.setItem(AUTO_UPDATE_NOTICE_STORAGE_KEY, update.version);
            void showStudyReminder(
              '发现新版本',
              `考研专注 ${update.version} 可更新，打开设置页下载并安装。`,
              'silent',
              `update:${update.version}`,
            ).catch(() => {});
          }
        }
      } catch (reason) {
        if (!disposed) {
          setLastAutoUpdateMessage(`自动检查更新失败：${reason instanceof Error ? reason.message : String(reason)}`);
        }
      } finally {
        updateCheckInFlight = false;
      }
    }

    const startupTimerId = window.setTimeout(() => {
      void runAutoUpdateCheck();
    }, AUTO_UPDATE_CHECK_STARTUP_DELAY_MS);
    const intervalId = window.setInterval(() => {
      void runAutoUpdateCheck();
    }, AUTO_UPDATE_CHECK_INTERVAL_MS);

    return () => {
      disposed = true;
      window.clearTimeout(startupTimerId);
      window.clearInterval(intervalId);
    };
  }, [onUpdateAvailable]);
}

export function useScheduleReminders() {
  useEffect(() => {
    let disposed = false;
    const remindedKeys = new Set<string>();

    async function checkScheduleReminders() {
      try {
        const settings = await getAppSettings();
        if (!settings.schedule_reminder_enabled) {
          return;
        }

        const date = formatDateKey();
        const nowMinute = currentMinuteOfDay();
        const pageData = await getSchedulePageData(date);
        if (disposed) return;

        for (const block of pageData.day_blocks) {
          if (block.status === 'completed') continue;
          const minutesUntilStart = block.start_minute - nowMinute;
          const isInLeadWindow = minutesUntilStart >= 0
            && minutesUntilStart <= settings.schedule_reminder_lead_minutes;
          const isRecentlyStarted = minutesUntilStart < 0
            && Math.abs(minutesUntilStart) <= SCHEDULE_REMINDER_LOOKBACK_MINUTES;
          if (!isInLeadWindow && !isRecentlyStarted) continue;

          const key = `${block.id}:${block.schedule_date}:${block.start_minute}`;
          if (remindedKeys.has(key)) continue;
          remindedKeys.add(key);

          void notifyStudyReminder({
            title: '课表时间到了',
            body: `${block.title} 已到开始时间，可以回到课表一键开始专注。`,
          });
        }

        for (const key of [...remindedKeys]) {
          if (!key.includes(`:${date}:`)) {
            remindedKeys.delete(key);
          }
        }
      } catch {
        // Schedule reminders are best-effort and should never interrupt the app.
      }
    }

    void checkScheduleReminders();
    const intervalId = window.setInterval(() => {
      void checkScheduleReminders();
    }, SCHEDULE_REMINDER_INTERVAL_MS);

    return () => {
      disposed = true;
      window.clearInterval(intervalId);
    };
  }, []);
}

export function useAlarmWatcher(setNextAlarm: (alarm: Alarm | null) => void) {
  const nextAlarmKeyRef = useRef<string>('');

  useEffect(() => {
    let disposed = false;
    let timeoutId: number | null = null;
    const notifiedAlarmIds = new Set<number>();

    async function refreshNextAlarm() {
      try {
        const alarm = await getNextAlarm();
        if (!disposed) {
          const nextKey = alarm ? `${alarm.id}:${alarm.alarm_date}:${alarm.alarm_time}:${alarm.title}` : 'none';
          if (nextKey !== nextAlarmKeyRef.current) {
            nextAlarmKeyRef.current = nextKey;
            setNextAlarm(alarm);
          }
        }
      } catch {
        // Alarm status is visible on the Alarm page; the global widget stays quiet.
      }
    }

    function nextAlarmDelay(alarm: Alarm | null) {
      if (!alarm) return ALARM_CHECK_MAX_DELAY_MS;
      const dueAt = new Date(alarm.alarm_at).getTime();
      if (!Number.isFinite(dueAt)) return ALARM_CHECK_MAX_DELAY_MS;
      return Math.max(ALARM_CHECK_MIN_DELAY_MS, Math.min(ALARM_CHECK_MAX_DELAY_MS, dueAt - Date.now()));
    }

    function scheduleNextCheck(alarm: Alarm | null) {
      if (disposed) return;
      if (timeoutId !== null) {
        window.clearTimeout(timeoutId);
      }
      timeoutId = window.setTimeout(() => {
        void checkAlarms();
      }, nextAlarmDelay(alarm));
    }

    async function checkAlarms() {
      let nextAlarm: Alarm | null = null;
      try {
        const ringingAlarms = await triggerDueAlarms();
        if (disposed) return;

        for (const alarm of ringingAlarms) {
          const key = `alarm:${alarm.id}`;
          if (notifiedAlarmIds.has(alarm.id)) {
            continue;
          }
          notifiedAlarmIds.add(alarm.id);
          void notifyPersistentAlarm(key, {
            title: alarm.title,
            body: alarm.note?.trim() || `闹钟时间到了：${alarm.alarm_date} ${alarm.alarm_time}`,
          });
        }

        for (const id of [...notifiedAlarmIds]) {
          if (!ringingAlarms.some((alarm) => alarm.id === id)) {
            notifiedAlarmIds.delete(id);
            stopPersistentAlarmSound(`alarm:${id}`);
          }
        }

        nextAlarm = await getNextAlarm();
        const nextKey = nextAlarm ? `${nextAlarm.id}:${nextAlarm.alarm_date}:${nextAlarm.alarm_time}:${nextAlarm.title}` : 'none';
        if (nextKey !== nextAlarmKeyRef.current) {
          nextAlarmKeyRef.current = nextKey;
          setNextAlarm(nextAlarm);
        }
        if (ringingAlarms.length > 0) {
          notifyAlarmStateChanged();
        }
      } catch {
        // Alarm checks are best-effort and should never interrupt the app.
      } finally {
        scheduleNextCheck(nextAlarm);
      }
    }

    void checkAlarms();

    const handleAlarmStateChanged = () => {
      void refreshNextAlarm().then(() => {
        void checkAlarms();
      });
    };
    window.addEventListener(ALARM_STATE_CHANGED_EVENT, handleAlarmStateChanged);

    return () => {
      disposed = true;
      if (timeoutId !== null) {
        window.clearTimeout(timeoutId);
      }
      window.removeEventListener(ALARM_STATE_CHANGED_EVENT, handleAlarmStateChanged);
      stopPersistentAlarmSound();
    };
  }, []);
}

export function useEmailReminders(setLastAutoSyncMessage: (message: string | null) => void) {
  useEffect(() => {
    let disposed = false;

    async function checkEmailReminders() {
      try {
        const result = await checkDueTaskEmailReminders();
        if (!disposed && result.status === 'sent') {
          setLastAutoSyncMessage(result.message);
        }
      } catch {
        // Email reminder failures are visible from Settings test/save; the periodic check stays quiet.
      }
    }

    void checkEmailReminders();
    const intervalId = window.setInterval(() => {
      void checkEmailReminders();
    }, EMAIL_REMINDER_INTERVAL_MS);

    return () => {
      disposed = true;
      window.clearInterval(intervalId);
    };
  }, []);
}
