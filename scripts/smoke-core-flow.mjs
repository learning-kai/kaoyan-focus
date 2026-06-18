import { spawn } from 'node:child_process';
import { once } from 'node:events';
import { mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';

const DEFAULT_URL = 'http://127.0.0.1:1420/';
const targetUrl = process.env.SMOKE_URL || DEFAULT_URL;
const edgePath =
  process.env.EDGE_PATH ||
  'C:\\Program Files (x86)\\Microsoft\\Edge\\Application\\msedge.exe';
const smokeTaskTitle = `核心路径任务 ${Date.now()}`;
const smokeScheduleTitle = `核心路径课表 ${Date.now()}`;
const smokeAlarmTitle = `核心路径闹钟 ${Date.now()}`;

function sleep(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

async function fetchOk(url) {
  try {
    const response = await fetch(url, { signal: AbortSignal.timeout(2500) });
    return response.ok;
  } catch {
    return false;
  }
}

async function waitForUrl(url, timeoutMs = 30000) {
  const startedAt = Date.now();
  while (Date.now() - startedAt < timeoutMs) {
    if (await fetchOk(url)) return;
    await sleep(500);
  }
  throw new Error(`Timed out waiting for ${url}`);
}

function startDevServer() {
  const command = process.platform === 'win32' ? 'cmd.exe' : 'npm';
  const args = process.platform === 'win32'
    ? ['/d', '/s', '/c', 'npm.cmd', 'run', 'dev', '--', '--host', '127.0.0.1']
    : ['run', 'dev', '--', '--host', '127.0.0.1'];
  const child = spawn(command, args, { shell: false, stdio: ['ignore', 'pipe', 'pipe'] });
  child.stdout.on('data', (chunk) => process.stdout.write(chunk));
  child.stderr.on('data', (chunk) => process.stderr.write(chunk));
  return child;
}

async function getJson(url) {
  const response = await fetch(url, { signal: AbortSignal.timeout(2500) });
  if (!response.ok) throw new Error(`${url} returned ${response.status}`);
  return response.json();
}

async function waitForDebugger(port, timeoutMs = 20000) {
  const startedAt = Date.now();
  const listUrl = `http://127.0.0.1:${port}/json/list`;
  while (Date.now() - startedAt < timeoutMs) {
    try {
      const pages = await getJson(listUrl);
      const page = pages.find((item) => item.type === 'page' && item.webSocketDebuggerUrl);
      if (page) return page.webSocketDebuggerUrl;
    } catch {
      // Edge may need another moment to expose the debugger endpoint.
    }
    await sleep(250);
  }
  throw new Error('Timed out waiting for Edge debugger');
}

class CdpClient {
  constructor(websocket) {
    this.websocket = websocket;
    this.nextId = 1;
    this.pending = new Map();
    this.eventHandlers = new Map();
    websocket.addEventListener('message', (event) => {
      const payload = JSON.parse(event.data);
      if (!payload.id) {
        const handlers = this.eventHandlers.get(payload.method) || [];
        for (const handler of handlers) handler(payload.params || {});
        return;
      }
      const entry = this.pending.get(payload.id);
      if (!entry) return;
      this.pending.delete(payload.id);
      if (payload.error) entry.reject(new Error(payload.error.message));
      else entry.resolve(payload.result);
    });
  }

  send(method, params = {}) {
    const id = this.nextId++;
    const message = JSON.stringify({ id, method, params });
    return new Promise((resolve, reject) => {
      this.pending.set(id, { resolve, reject });
      this.websocket.send(message);
    });
  }

  async evaluate(expression) {
    const result = await this.send('Runtime.evaluate', {
      awaitPromise: true,
      expression,
      returnByValue: true,
    });
    if (result.exceptionDetails) {
      throw new Error(result.exceptionDetails.text || 'Runtime evaluation failed');
    }
    return result.result.value;
  }

  on(method, handler) {
    const handlers = this.eventHandlers.get(method) || [];
    handlers.push(handler);
    this.eventHandlers.set(method, handlers);
  }
}

async function waitForCondition(client, label, expression, timeoutMs = 15000) {
  const startedAt = Date.now();
  while (Date.now() - startedAt < timeoutMs) {
    if (await client.evaluate(expression)) return;
    await sleep(250);
  }
  const diagnostic = await client.evaluate(`
    (() => {
      const state = window.__SMOKE_STATE;
      if (!state) return 'no smoke state';
      return JSON.stringify({
        activeElement: document.activeElement ? {
          tag: document.activeElement.tagName,
          id: document.activeElement.id || null,
          className: document.activeElement.className || null,
          text: (document.activeElement.textContent || '').slice(0, 120),
          ariaLabel: document.activeElement.getAttribute?.('aria-label') || null,
        } : null,
        studyStatus: state.studyState?.status,
        blocks: state.scheduleBlocks?.map((block) => ({
          title: block.title,
          start: block.start_minute,
          end: block.end_minute,
          source: block.source_today_item_id,
          status: block.status,
        })),
        text: document.body.innerText.slice(0, 500),
      });
    })()
  `);
  throw new Error(`Timed out waiting for ${label}: ${diagnostic}`);
}

async function waitForPageLoad(client) {
  await new Promise((resolve) => {
    const timeoutId = setTimeout(resolve, 10000);
    client.on('Page.loadEventFired', () => {
      clearTimeout(timeoutId);
      resolve();
    });
  });
}

async function clickByExpression(client, label, expression) {
  const clicked = await client.evaluate(expression);
  if (!clicked) throw new Error(`Could not click ${label}`);
}

async function readThemeState(client) {
  return client.evaluate(`
    (() => {
      const themeColor = document.querySelector('meta[name="theme-color"]')?.getAttribute('content') || null;
      return {
        activeTheme: document.documentElement.dataset.theme || null,
        activeThemeLabel: document.querySelector('.theme-toggle button[aria-pressed="true"]')?.textContent?.trim() || null,
        themeColor,
      };
    })()
  `);
}

async function dispatchAltShortcut(client, label, key, targetSelector = null) {
  const result = await client.evaluate(`
    (() => {
      const target = ${targetSelector ? `document.querySelector(${JSON.stringify(targetSelector)})` : 'window'};
      if (!target) return { dispatched: false, defaultPrevented: false };
      if (target instanceof HTMLElement) target.focus();
      const event = new KeyboardEvent('keydown', {
        key: ${JSON.stringify(key)},
        altKey: true,
        bubbles: true,
        cancelable: true
      });
      target.dispatchEvent(event);
      return { dispatched: true, defaultPrevented: event.defaultPrevented };
    })()
  `);
  if (!result?.dispatched) throw new Error(`Could not dispatch ${label}`);
  return result;
}

async function dragByExpression(client, label, expression, deltaY) {
  const point = await client.evaluate(expression);
  if (!point) throw new Error(`Could not find drag target for ${label}`);
  await client.send('Input.dispatchMouseEvent', {
    type: 'mousePressed',
    x: point.x,
    y: point.y,
    button: 'left',
    buttons: 1,
    clickCount: 1,
  });
  await sleep(80);
  await client.send('Input.dispatchMouseEvent', {
    type: 'mouseMoved',
    x: point.x,
    y: point.y + deltaY,
    button: 'left',
    buttons: 1,
  });
  await sleep(80);
  await client.send('Input.dispatchMouseEvent', {
    type: 'mouseReleased',
    x: point.x,
    y: point.y + deltaY,
    button: 'left',
    buttons: 0,
    clickCount: 1,
  });
}

function smokeRuntimeMockSource() {
  return `
(() => {
  const today = new Date().toISOString().slice(0, 10);
  const nowIso = () => new Date().toISOString();
  const clone = (value) => JSON.parse(JSON.stringify(value));
  const idleStudyState = {
    id: null, phase: 'idle', status: 'idle', mode: 'normal', subject_id: null,
    planned_seconds: 0, focus_seconds: 0, break_seconds: 0, long_break_seconds: 0,
    long_break_interval: 4, effective_break_seconds: 0, break_kind: 'short', cycle_index: 0,
    started_at: null, phase_started_at: null, paused_at: null, ended_at: null,
    current_session: null, study_elapsed_seconds: 0, study_remaining_seconds: 0,
    phase_elapsed_seconds: 0, phase_remaining_seconds: 0, focus_enforcement_active: false,
    whitelist_enabled: true, is_paused: false
  };
  const settings = {
    default_study_minutes: 120, default_focus_minutes: 25, break_minutes: 5,
    long_break_minutes: 15, long_break_interval: 4, default_focus_mode: 'normal',
    whitelist_mode: 'allowlist',
    ui_theme: 'light', launch_at_startup: false, auto_start_break_after_focus: false,
    schedule_reminder_enabled: true, schedule_reminder_lead_minutes: 10,
    sync_backend: 'webdav', primary_owner_device_id: 'smoke-device',
    primary_owner_updated_at: Date.now(), emergency_cooldown_seconds: 30,
    checklist_category_names: '政治,英语,数学,专业课,通用',
    reminder_sound_source: 'builtin', reminder_sound_id: 'classic',
    reminder_sound_file_name: null, reminder_sound_updated_at: null, reminder_sound_volume: 0.6,
    reminder_quiet_hours_enabled: false, reminder_quiet_hours_start: '22:30',
    reminder_quiet_hours_end: '07:00'
  };
  const state = {
    settings,
    webdavSettings: { enabled: false, url: 'https://dav.example.com/remote.php/dav/files/me', username: 'smoke', password: '', password_configured: false, remote_path: 'kaoyan-focus/kaoyan-focus.sqlite3' },
    objectStorageSettings: { enabled: false, endpoint: 'https://example.r2.cloudflarestorage.com', bucket: 'kaoyan-focus', access_key_id: '', secret_access_key: '', secret_access_key_configured: false, region: 'auto', object_key: 'study-sync.json' },
    emailSettings: { enabled: false, smtp_host: '', smtp_port: 465, smtp_security: 'tls', username: '', password: '', password_configured: false, from: '', to: '' },
    feishuSettings: { enabled: false, app_id: '', app_secret: '', app_secret_configured: false, redirect_uri: 'http://localhost:1420/feishu/callback' },
    todayItems: [], scheduleBlocks: [], nextTodayId: 1, nextBlockId: 1, nextStudyId: 1,
    studyState: { ...idleStudyState }, syncRuns: [], invokeLog: [],
    lastCopiedText: null, lastDownloadName: null, lastDownloadHref: null, lastDownloadBlob: null,
    lastRemovedDownloadName: null, lastRemovedDownloadHref: null, lastRevokedUrl: null,
    failNextDownloadClick: false,
    lastOpenedPath: null
  };
  const subjects = [
    { id: 1, name: '政治', color: '#8b5cf6', enabled: true, created_at: nowIso(), updated_at: nowIso() },
    { id: 2, name: '英语', color: '#2563eb', enabled: true, created_at: nowIso(), updated_at: nowIso() },
    { id: 3, name: '数学', color: '#16a34a', enabled: true, created_at: nowIso(), updated_at: nowIso() },
    { id: 4, name: '专业课', color: '#ea580c', enabled: true, created_at: nowIso(), updated_at: nowIso() }
  ];
  const checklistData = () => ({
    today_date: today, active_category_key: 'general', highlighted_subject_id: null,
    categories: [{ key: 'general', title: '通用', pending_tasks: [], completed_tasks: [], highlighted: true }],
    today_items: clone(state.todayItems)
  });
  const scheduleData = (selectedDate = today) => ({
    selected_date: selectedDate || today, today_date: today, week_start_date: today,
    day_blocks: clone(state.scheduleBlocks),
    week_days: [{ date: today, weekday: 1, blocks: clone(state.scheduleBlocks), planned_minutes: state.scheduleBlocks.reduce((total, block) => total + block.end_minute - block.start_minute, 0) }],
    today_items: clone(state.todayItems.map((item) => ({ id: item.id, title: item.title, note: item.note, due_date: item.due_date, subject_id: item.subject_id, completed: item.completed }))),
    templates: []
  });
  const focusStats = () => ({ today_seconds: 0, week_seconds: 0, month_seconds: 0, interruption_count: 0, subjects: subjects.map((subject) => ({ subject, total_seconds: 0 })) });
  const appDataLocation = { app_data_dir: 'smoke', database_path: 'smoke/kaoyan-focus.sqlite3' };
  const nextAlarmAt = new Date(Date.now() + 60 * 60 * 1000);
  const nextAlarmDate = nextAlarmAt.toLocaleDateString('en-CA');
  const nextAlarmTime = nextAlarmAt.toLocaleTimeString('en-GB', { hour: '2-digit', minute: '2-digit', hour12: false });
  const alarms = [{
    id: 1,
    title: ${JSON.stringify(smokeAlarmTitle)},
    note: 'smoke',
    alarm_date: nextAlarmDate,
    alarm_time: nextAlarmTime,
    alarm_at: nextAlarmAt.toISOString(),
    enabled: true,
    status: 'scheduled',
    fired_at: null,
    dismissed_at: null,
    created_at: nowIso(),
    updated_at: nowIso(),
  }];
  try {
    Object.defineProperty(window.navigator, 'clipboard', {
      configurable: true,
      value: {
        writeText: async (text) => {
          state.lastCopiedText = String(text);
          return undefined;
        },
      },
    });
  } catch {
    // Clipboard stubbing is best-effort for browser smoke coverage.
  }
  try {
    const originalCreateObjectURL = URL.createObjectURL?.bind(URL);
    URL.createObjectURL = (blob) => {
      state.lastDownloadBlob = blob;
      return originalCreateObjectURL ? originalCreateObjectURL(blob) : 'blob:smoke';
    };
  } catch {
    // Download capture is best-effort for browser smoke coverage.
  }
  try {
    const originalAnchorClick = HTMLAnchorElement.prototype.click;
    HTMLAnchorElement.prototype.click = function click() {
      if (this.download) {
        state.lastDownloadName = this.download;
        state.lastDownloadHref = this.href;
        if (state.failNextDownloadClick) {
          state.failNextDownloadClick = false;
          throw new Error('smoke download click failure');
        }
        return undefined;
      }
      return originalAnchorClick.call(this);
    };
  } catch {
    // Download capture is best-effort for browser smoke coverage.
  }
  try {
    const originalAnchorRemove = HTMLAnchorElement.prototype.remove;
    HTMLAnchorElement.prototype.remove = function remove() {
      if (this.download) {
        state.lastRemovedDownloadName = this.download;
        state.lastRemovedDownloadHref = this.href;
      }
      return originalAnchorRemove ? originalAnchorRemove.call(this) : undefined;
    };
  } catch {
    // Download cleanup capture is best-effort for browser smoke coverage.
  }
  try {
    const originalRevokeObjectURL = URL.revokeObjectURL?.bind(URL);
    URL.revokeObjectURL = (objectUrl) => {
      state.lastRevokedUrl = objectUrl;
      return originalRevokeObjectURL ? originalRevokeObjectURL(objectUrl) : undefined;
    };
  } catch {
    // Download cleanup capture is best-effort for browser smoke coverage.
  }
  try {
    const originalExecCommand = document.execCommand?.bind(document);
    document.execCommand = (command) => {
      if (command === 'copy') {
        const activeElement = document.activeElement;
        if (activeElement instanceof HTMLInputElement || activeElement instanceof HTMLTextAreaElement) {
          state.lastCopiedText = activeElement.value;
        }
        return true;
      }
      return originalExecCommand ? originalExecCommand(command) : false;
    };
  } catch {
    // Clipboard fallback is best-effort for browser smoke coverage.
  }
  const syncResult = (trigger = 'manual') => {
    state.syncRuns.unshift({
      id: state.syncRuns.length + 1, sync_id: 'smoke-sync-' + (state.syncRuns.length + 1),
      backend: state.settings.sync_backend, trigger, direction: 'upload', status: 'synced',
      started_at: nowIso(), finished_at: nowIso(), duration_ms: 12, bytes: 2048,
      imported_count: 0, exported_count: state.todayItems.length + state.scheduleBlocks.length,
      deleted_count: 0, conflict_count: 0, active_state_changed: false, took_over_active_mode: false,
      validation_report: 'smoke ok', backup_path: null, remote_backup_key: null,
      active_snapshot_sync_id: null, remote_active_snapshot_sync_id: null,
      active_snapshot_phase: state.studyState.phase, remote_active_snapshot_phase: null,
      active_snapshot_updated_at: Date.now(), remote_snapshot_updated_at: null,
      remote_exported_drift_seconds: null, detail: 'smoke core flow', error_message: null
    });
    return { status: 'synced', message: 'Smoke 同步已完成。', direction: 'upload', skipped_reason: null, synced_at: nowIso(), remote_url: state.webdavSettings.url, bytes: 2048, backup_path: null };
  };
  const startStudy = (args = {}) => {
    const block = state.scheduleBlocks.find((item) => item.id === args.blockId) || state.scheduleBlocks[0] || null;
    if (block) {
      block.status = 'running';
      block.linked_study_mode_id = state.nextStudyId;
      block.updated_at = nowIso();
    }
    state.studyState = {
      ...idleStudyState, id: state.nextStudyId++, phase: 'focus', status: 'active',
      mode: args.mode || 'normal', subject_id: block?.subject_id ?? args.subjectId ?? null,
      planned_seconds: args.plannedSeconds || 7200, focus_seconds: args.focusSeconds || 1500,
      break_seconds: args.breakSeconds || 300, long_break_seconds: args.longBreakSeconds || 900,
      long_break_interval: args.longBreakInterval || 4, effective_break_seconds: args.breakSeconds || 300,
      cycle_index: 1, started_at: nowIso(), phase_started_at: nowIso(),
      current_session: {
        id: state.nextStudyId * 100, mode: args.mode || 'normal',
        subject_id: block?.subject_id ?? args.subjectId ?? null, planned_seconds: args.focusSeconds || 1500,
        actual_seconds: 0, started_at: nowIso(), ended_at: null, status: 'running', end_reason: null,
        interruption_count: 0, emergency_exit_count: 0, created_at: nowIso(), updated_at: nowIso()
      },
      study_elapsed_seconds: 0, study_remaining_seconds: args.plannedSeconds || 7200,
      phase_elapsed_seconds: 0, phase_remaining_seconds: args.focusSeconds || 1500,
      focus_enforcement_active: Boolean(args.whitelistEnabled), whitelist_enabled: Boolean(args.whitelistEnabled),
      is_paused: false
    };
    syncResult('focus_state_change');
    return clone(state.studyState);
  };
  async function invoke(cmd, args = {}) {
    state.invokeLog.push({ cmd, args });
    switch (cmd) {
      case 'plugin:event|listen': return state.invokeLog.length;
      case 'plugin:event|unlisten': return null;
      case 'list_alarms': return clone(alarms);
      case 'trigger_due_alarms': return [];
      case 'get_next_alarm': {
        const nextAlarm = alarms.find(
          (alarm) => alarm.enabled && alarm.status === 'scheduled' && new Date(alarm.alarm_at).getTime() > Date.now(),
        ) ?? null;
        return clone(nextAlarm);
      }
      case 'has_active_alarm': return false;
      case 'get_app_settings': return clone(state.settings);
      case 'save_app_settings': Object.assign(state.settings, args.settings || {}); return clone(state.settings);
      case 'get_sync_device_id': return 'smoke-device';
      case 'get_app_data_location': return clone(appDataLocation);
      case 'open_app_data_location': state.lastOpenedPath = appDataLocation.app_data_dir; return clone(appDataLocation);
      case 'get_runtime_health': return { status: 'ok', summary: 'Smoke runtime ready', checked_at: nowIso(), tasks: [] };
      case 'list_subjects': return clone(subjects);
      case 'get_study_mode_state': return clone(state.studyState);
      case 'start_study_mode': return startStudy(args);
      case 'start_study_mode_from_schedule_block': return startStudy(args);
      case 'pause_study_mode': state.studyState.is_paused = true; return clone(state.studyState);
      case 'resume_study_mode': state.studyState.is_paused = false; return clone(state.studyState);
      case 'confirm_study_break': state.studyState.phase = 'break'; return clone(state.studyState);
      case 'reset_study_mode': state.studyState = { ...idleStudyState }; return clone(state.studyState);
      case 'update_study_mode_subject': state.studyState.subject_id = args.subjectId ?? null; return clone(state.studyState);
      case 'list_focus_sessions': return state.studyState.current_session ? [clone(state.studyState.current_session)] : [];
      case 'get_focus_stats_summary': return focusStats();
      case 'check_focus_foreground_app': return { allowed: true, foreground_app: { process_name: 'msedge.exe', window_title: 'Smoke', executable_path: null, browser_url: null }, matched_rule: null, checked_at: nowIso(), action_taken: 'none' };
      case 'list_interruption_summary': return [
        { process_name: 'msedge.exe', process_path: 'C:\\Program Files (x86)\\Microsoft\\Edge\\Application\\msedge.exe', window_title: 'Smoke stats', interruption_count: 2, last_interrupted_at: nowIso() },
        { process_name: 'wechat.exe', process_path: 'C:\\Program Files\\WeChat\\WeChat.exe', window_title: 'Smoke chat', interruption_count: 1, last_interrupted_at: nowIso() }
      ];
      case 'get_checklist_page_data': return checklistData();
      case 'create_today_plan_item': {
        const draft = args.draft || {};
        const item = { id: state.nextTodayId++, today_date: today, source_task_id: null, subject_id: draft.subjectId ?? null, title: draft.title || 'Smoke task', note: draft.note || null, due_date: draft.dueDate || null, sort_order: state.todayItems.length + 1, completed: false, synced_source_completion: false, created_at: nowIso(), updated_at: nowIso() };
        state.todayItems.push(item);
        return clone(item);
      }
      case 'update_today_plan_item': {
        const item = state.todayItems.find((candidate) => candidate.id === args.id);
        if (!item) throw new Error('Today item not found');
        Object.assign(item, { title: args.draft?.title ?? item.title, note: args.draft?.note ?? item.note, due_date: args.draft?.dueDate ?? item.due_date, subject_id: args.draft?.subjectId ?? item.subject_id, updated_at: nowIso() });
        return clone(item);
      }
      case 'delete_today_plan_item': state.todayItems = state.todayItems.filter((item) => item.id !== args.id); return null;
      case 'complete_today_plan_item': {
        const item = state.todayItems.find((candidate) => candidate.id === args.id);
        if (item) item.completed = Boolean(args.completed);
        return clone(item);
      }
      case 'reorder_today_plan_items': return null;
      case 'get_schedule_page_data': return scheduleData(args.selectedDate);
      case 'create_schedule_block': {
        const draft = args.draft || {};
        const block = { id: state.nextBlockId++, schedule_date: draft.scheduleDate || today, title: draft.title || 'Smoke block', note: draft.note || null, category_key: draft.categoryKey || 'general', subject_id: draft.subjectId ?? null, source_today_item_id: draft.sourceTodayItemId ?? null, template_id: null, start_minute: draft.startMinute ?? 480, end_minute: draft.endMinute ?? 540, status: 'planned', linked_study_mode_id: null, linked_focus_session_id: null, has_conflict: false, created_at: nowIso(), updated_at: nowIso() };
        state.scheduleBlocks.push(block);
        return clone(block);
      }
      case 'create_schedule_block_from_today_item': {
        const item = state.todayItems.find((candidate) => candidate.id === args.todayItemId);
        const block = { id: state.nextBlockId++, schedule_date: args.scheduleDate || today, title: item?.title || 'Smoke block', note: item?.note || null, category_key: 'general', subject_id: item?.subject_id ?? null, source_today_item_id: item?.id ?? null, template_id: null, start_minute: args.startMinute ?? 480, end_minute: args.endMinute ?? 540, status: 'planned', linked_study_mode_id: null, linked_focus_session_id: null, has_conflict: false, created_at: nowIso(), updated_at: nowIso() };
        state.scheduleBlocks.push(block);
        return clone(block);
      }
      case 'move_schedule_block': {
        const block = state.scheduleBlocks.find((candidate) => candidate.id === args.id);
        if (!block) throw new Error('Schedule block not found');
        Object.assign(block, { schedule_date: args.scheduleDate || today, start_minute: args.startMinute, end_minute: args.endMinute, updated_at: nowIso() });
        return clone(block);
      }
      case 'delete_schedule_block': state.scheduleBlocks = state.scheduleBlocks.filter((block) => block.id !== args.id); return null;
      case 'get_webdav_settings': return clone(state.webdavSettings);
      case 'save_webdav_settings': Object.assign(state.webdavSettings, args.settings || {}); return clone(state.webdavSettings);
      case 'test_webdav_connection': return { configured: true, url: state.webdavSettings.url, username: state.webdavSettings.username, remote_path: state.webdavSettings.remote_path, remote_exists: true, remote_size: 2048, last_modified: nowIso(), message: 'Smoke WebDAV connection ok' };
      case 'upload_database_to_webdav': return { success: true, message: 'Smoke upload ok', remote_url: state.webdavSettings.url, bytes: 2048, backup_path: null };
      case 'download_database_from_webdav': return { success: true, message: 'Smoke download ok', remote_url: state.webdavSettings.url, bytes: 2048, backup_path: null };
      case 'auto_sync_webdav_database': return syncResult('webdav_auto');
      case 'get_object_storage_settings': return clone(state.objectStorageSettings);
      case 'save_object_storage_settings': Object.assign(state.objectStorageSettings, args.settings || {}); return clone(state.objectStorageSettings);
      case 'test_object_storage_connection': return { configured: true, endpoint: state.objectStorageSettings.endpoint, bucket: state.objectStorageSettings.bucket, region: state.objectStorageSettings.region, object_key: state.objectStorageSettings.object_key, object_exists: true, object_size: 2048, last_modified: nowIso(), message: 'Smoke object storage connection ok' };
      case 'upload_database_to_object_storage': return { success: true, message: 'Smoke upload ok', object_url: state.objectStorageSettings.endpoint, bytes: 2048, backup_path: null };
      case 'download_database_from_object_storage': return { success: true, message: 'Smoke download ok', object_url: state.objectStorageSettings.endpoint, bytes: 2048, backup_path: null };
      case 'sync_object_storage_state_change': return syncResult(args.trigger || 'object_storage_state');
      case 'list_sync_runs': return clone(state.syncRuns.slice(0, args.limit || 10));
      case 'list_sync_backups': return [];
      case 'get_email_reminder_settings': return clone(state.emailSettings);
      case 'save_email_reminder_settings': Object.assign(state.emailSettings, args.settings || {}); return clone(state.emailSettings);
      case 'test_email_reminder': return { status: 'skipped', message: 'Smoke email skipped', sent_count: 0 };
      case 'get_feishu_sync_settings': return clone(state.feishuSettings);
      case 'save_feishu_sync_settings': Object.assign(state.feishuSettings, args.settings || {}); return clone(state.feishuSettings);
      case 'get_feishu_sync_status': return { enabled: false, configured: false, authenticated: false, expires_at: null, tasklist_guid: null, tasklist_count: 0, tasklists: [], calendar_id: null, redirect_uri: state.feishuSettings.redirect_uri, pending_authorization_url: null, pending_message: null, required_scopes: '', last_run: null };
      case 'set_study_fullscreen': return null;
      default: throw new Error('Unhandled smoke command: ' + cmd);
    }
  }
  let callbackId = 1;
  window.__SMOKE_STATE = state;
  window.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener: () => undefined };
  window.__TAURI_INTERNALS__ = {
    invoke,
    callbacks: new Map(),
    transformCallback: (callback) => {
      const id = callbackId++;
      window.__TAURI_INTERNALS__.callbacks.set(id, callback);
      return id;
    },
    unregisterCallback: (id) => window.__TAURI_INTERNALS__.callbacks.delete(id),
    runCallback: (id, payload) => window.__TAURI_INTERNALS__.callbacks.get(id)?.(payload),
    convertFileSrc: (filePath) => filePath,
    metadata: { currentWindow: { label: 'main' }, currentWebview: { label: 'main' } }
  };
})();
`;
}

async function stopProcess(child) {
  if (!child || child.killed) return;
  child.kill();
  try {
    await Promise.race([once(child, 'exit'), sleep(3000)]);
  } catch {
    // Process shutdown is best-effort; cleanup retries handle lingering file locks.
  }
}

async function removeTempDir(tempPath) {
  for (let attempt = 1; attempt <= 5; attempt += 1) {
    try {
      await rm(tempPath, { force: true, recursive: true });
      return;
    } catch (error) {
      if (attempt === 5) {
        console.warn(`Could not remove temporary smoke profile ${tempPath}: ${error.message}`);
        return;
      }
      await sleep(250 * attempt);
    }
  }
}

let serverProcess = null;
let edgeProcess = null;
let userDataDir = null;
let websocket = null;

try {
  if (!(await fetchOk(targetUrl))) {
    serverProcess = startDevServer();
    await waitForUrl(targetUrl);
  }

  userDataDir = await mkdtemp(path.join(tmpdir(), 'kaoyan-focus-smoke-'));
  const debuggerPort = 9300 + Math.floor(Math.random() * 500);
  edgeProcess = spawn(edgePath, [
    '--headless',
    '--disable-gpu',
    '--no-sandbox',
    `--remote-debugging-port=${debuggerPort}`,
    `--user-data-dir=${userDataDir}`,
    '--window-size=1280,900',
    'about:blank',
  ], { stdio: ['ignore', 'ignore', 'pipe'] });

  edgeProcess.stderr.on('data', () => {
    // Edge may print importer noise on Windows; app console errors are checked through CDP.
  });

  const wsUrl = await waitForDebugger(debuggerPort);
  websocket = new WebSocket(wsUrl);
  await new Promise((resolve, reject) => {
    websocket.addEventListener('open', resolve, { once: true });
    websocket.addEventListener('error', reject, { once: true });
  });

  const client = new CdpClient(websocket);
  const consoleErrors = [];
  client.on('Runtime.consoleAPICalled', (params) => {
    if (params.type === 'error') {
      consoleErrors.push(params.args?.map((arg) => arg.value || arg.description || '').join(' ') || 'console.error');
    }
  });
  client.on('Runtime.exceptionThrown', (params) => {
    consoleErrors.push(params.exceptionDetails?.text || 'Runtime exception');
  });
  await client.send('Runtime.enable');
  await client.send('Page.enable');
  await client.send('Page.addScriptToEvaluateOnNewDocument', { source: smokeRuntimeMockSource() });
  await client.send('Page.navigate', { url: targetUrl });
  await waitForPageLoad(client);

  await waitForCondition(client, 'focus landing content', `document.body.innerText.toLowerCase().includes('focus ritual') && document.body.innerText.includes('进入学习模式') && document.body.innerText.includes('下一轮专注')`);
  await waitForCondition(client, 'desktop runtime mock', `document.body.innerText.includes('Windows 桌面') || document.body.innerText.includes('浏览器预览')`);
  await waitForCondition(client, 'focus hash/title synced', `location.hash === '#focus' && document.title === '考研专注'`);
  await waitForCondition(client, 'focus main content focused', `document.activeElement?.id === 'main-content'`);
  await waitForCondition(client, 'next alarm pill rendered', `
    document.querySelector('.next-alarm-pill.active')?.textContent?.includes(${JSON.stringify(smokeAlarmTitle)}) &&
    document.querySelector('.next-alarm-pill')?.getAttribute('title')?.includes(${JSON.stringify(smokeAlarmTitle)})
  `);
  await clickByExpression(client, 'next alarm pill navigation', `
    (() => {
      const pill = document.querySelector('.next-alarm-pill.active');
      if (!pill) return false;
      pill.click();
      return true;
    })()
  `);
  await waitForCondition(client, 'next alarm pill opened alarm page', `
    location.hash === '#alarm' &&
    Boolean(document.querySelector('.alarm-shell')) &&
    document.body.innerText.includes(${JSON.stringify(smokeAlarmTitle)})
  `);
  await waitForCondition(client, 'target alarm row focused', `
    document.querySelector('#alarm-row-1.is-targeted')?.textContent?.includes(${JSON.stringify(smokeAlarmTitle)}) &&
    Boolean(document.activeElement?.closest?.('#alarm-row-1.is-targeted'))
  `);
  await client.evaluate(`window.history.back()`);
  await waitForCondition(client, 'returned to focus after next alarm pill', `location.hash === '#focus' && document.body.innerText.includes('进入学习模式')`);
  const initialThemeState = await readThemeState(client);
  if (initialThemeState.activeTheme !== 'light' || initialThemeState.themeColor !== '#f8fbff') {
    throw new Error(`Unexpected initial theme state: ${JSON.stringify(initialThemeState)}`);
  }

  const checklistShortcut = await dispatchAltShortcut(client, 'checklist keyboard navigation', '2');
  if (!checklistShortcut.defaultPrevented) {
    throw new Error('Checklist keyboard navigation did not consume Alt+2');
  }
  await waitForCondition(client, 'checklist page loaded', `Boolean(document.querySelector('.checklist-clean-shell .today-drawer-add')) && document.body.innerText.includes('进入任务')`);
  await waitForCondition(client, 'checklist main content focused', `document.activeElement?.id === 'main-content'`);
  await clickByExpression(client, 'open today task composer', `
    (() => {
      const button = document.querySelector('.checklist-clean-shell .today-drawer-add');
      if (!button) return false;
      button.click();
      return true;
    })()
  `);
  await waitForCondition(client, 'today task composer opened', `Boolean(document.querySelector('.checklist-clean-shell .today-composer input.text-input'))`);
  await clickByExpression(client, 'fill today task title', `
    (() => {
      const input = document.querySelector('.checklist-clean-shell .today-composer input.text-input');
      if (!input) return false;
      Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set.call(input, ${JSON.stringify(smokeTaskTitle)});
      input.dispatchEvent(new Event('input', { bubbles: true }));
      input.dispatchEvent(new Event('change', { bubbles: true }));
      return true;
    })()
  `);
  await waitForCondition(client, 'today task submit enabled', `!document.querySelector('.checklist-clean-shell .today-composer .primary-action')?.disabled`);
  await clickByExpression(client, 'submit today task', `
    (() => {
      const button = document.querySelector('.checklist-clean-shell .today-composer .primary-action');
      if (!button || button.disabled) return false;
      button.click();
      return true;
    })()
  `);
  await waitForCondition(client, 'today task created', `window.__SMOKE_STATE.todayItems.some((item) => item.title === ${JSON.stringify(smokeTaskTitle)}) && document.body.innerText.includes(${JSON.stringify(smokeTaskTitle)})`);

  const scheduleShortcut = await dispatchAltShortcut(client, 'schedule keyboard navigation', '3');
  if (!scheduleShortcut.defaultPrevented) {
    throw new Error('Schedule keyboard navigation did not consume Alt+3');
  }
  await waitForCondition(client, 'schedule page loaded', `Boolean(document.querySelector('.schedule-lane')) && document.body.innerText.includes(${JSON.stringify(smokeTaskTitle)})`);
  await waitForCondition(client, 'schedule main content focused', `document.activeElement?.id === 'main-content'`);
  await waitForCondition(client, 'schedule hash/title synced', `location.hash === '#schedule' && document.title.startsWith('课表') && document.title.endsWith('考研专注')`);
  const blockedSettingsShortcut = await dispatchAltShortcut(client, 'blocked settings keyboard navigation', '8', '.date-stepper input');
  if (blockedSettingsShortcut.defaultPrevented) {
    throw new Error('Input-scoped Alt+8 shortcut should not be consumed by global navigation');
  }
  await waitForCondition(client, 'keyboard navigation stays blocked in date input', `location.hash === '#schedule' && document.body.innerText.includes('今日课表')`);
  await clickByExpression(client, 'open schedule block composer', `
    (() => {
      const button = [...document.querySelectorAll('.schedule-actions .primary-button')].find((node) => node.textContent.includes('时间块'));
      if (!button) return false;
      button.click();
      return true;
    })()
  `);
  await waitForCondition(client, 'schedule block composer opened', `Boolean(document.querySelector('.schedule-composer input[placeholder="安排标题"]'))`);
  await clickByExpression(client, 'fill schedule block title', `
    (() => {
      const input = document.querySelector('.schedule-composer input[placeholder="安排标题"]');
      if (!input) return false;
      Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set.call(input, ${JSON.stringify(smokeScheduleTitle)});
      input.dispatchEvent(new Event('input', { bubbles: true }));
      input.dispatchEvent(new Event('change', { bubbles: true }));
      return true;
    })()
  `);
  await waitForCondition(client, 'schedule block submit enabled', `!document.querySelector('.schedule-composer .primary-button')?.disabled`);
  await clickByExpression(client, 'submit schedule block', `
    (() => {
      const button = document.querySelector('.schedule-composer .primary-button');
      if (!button || button.disabled) return false;
      button.click();
      return true;
    })()
  `);
  await waitForCondition(client, 'schedule block created', `window.__SMOKE_STATE.scheduleBlocks.some((block) => block.title === ${JSON.stringify(smokeScheduleTitle)}) && document.body.innerText.includes(${JSON.stringify(smokeScheduleTitle)})`);
  const scheduleBlockCountBeforeCancel = await client.evaluate(`window.__SMOKE_STATE.scheduleBlocks.length`);
  await clickByExpression(client, 'drag today task out cancels preview', `
    (() => {
      const row = document.querySelector('.schedule-task-row');
      const lane = document.querySelector('.schedule-lane');
      if (!row || !lane) return false;
      const rect = lane.getBoundingClientRect();
      const data = new DataTransfer();
      row.dispatchEvent(new DragEvent('dragstart', { bubbles: true, cancelable: true, dataTransfer: data, clientY: rect.top + 90 }));
      lane.dispatchEvent(new DragEvent('dragover', { bubbles: true, cancelable: true, dataTransfer: data, clientY: rect.top + 180 }));
      row.dispatchEvent(new DragEvent('dragend', { bubbles: true, cancelable: true, dataTransfer: data, clientY: rect.top - 80 }));
      return true;
    })()
  `);
  await waitForCondition(client, 'drag cancel kept schedule unchanged', `window.__SMOKE_STATE.scheduleBlocks.length === ${scheduleBlockCountBeforeCancel} && !document.querySelector('.schedule-drag-preview')`);
  await clickByExpression(client, 'drag today task into schedule lane', `
    (() => {
      const row = document.querySelector('.schedule-task-row');
      const lane = document.querySelector('.schedule-lane');
      if (!row || !lane) return false;
      const rect = lane.getBoundingClientRect();
      const data = new DataTransfer();
      row.dispatchEvent(new DragEvent('dragstart', { bubbles: true, cancelable: true, dataTransfer: data, clientY: rect.top + 90 }));
      lane.dispatchEvent(new DragEvent('dragover', { bubbles: true, cancelable: true, dataTransfer: data, clientY: rect.top + 240 }));
      lane.dispatchEvent(new DragEvent('drop', { bubbles: true, cancelable: true, dataTransfer: data, clientY: rect.top + 240 }));
      return true;
    })()
  `);
  await waitForCondition(client, 'today task drag created schedule block', `window.__SMOKE_STATE.scheduleBlocks.length === ${scheduleBlockCountBeforeCancel + 1} && window.__SMOKE_STATE.scheduleBlocks.some((block) => block.source_today_item_id !== null)`);
  const moveDeltaY = await client.evaluate(`
    (() => {
      const lane = document.querySelector('.schedule-lane');
      if (!lane) return 0;
      return lane.getBoundingClientRect().height * 60 / (24 * 60 - 6 * 60);
    })()
  `);
  await dragByExpression(client, 'move schedule block by pointer drag', `
    (() => {
      const block = [...document.querySelectorAll('.schedule-block')].find((node) => node.textContent.includes(${JSON.stringify(smokeScheduleTitle)}));
      if (!block) return null;
      const blockRect = block.getBoundingClientRect();
      return { x: blockRect.left + blockRect.width / 2, y: blockRect.top + blockRect.height / 2 };
    })()
  `, moveDeltaY);
  await waitForCondition(client, 'schedule block moved', `window.__SMOKE_STATE.scheduleBlocks.some((block) => block.title === ${JSON.stringify(smokeScheduleTitle)} && block.start_minute === 540 && block.end_minute === 600)`);
  const resizeStartDeltaY = await client.evaluate(`
    (() => {
      const lane = document.querySelector('.schedule-lane');
      if (!lane) return 0;
      return lane.getBoundingClientRect().height * 15 / (24 * 60 - 6 * 60);
    })()
  `);
  await dragByExpression(client, 'resize schedule block start', `
    (() => {
      const block = [...document.querySelectorAll('.schedule-block')].find((node) => node.textContent.includes(${JSON.stringify(smokeScheduleTitle)}));
      const handle = block?.querySelector('.schedule-resize-handle.is-start');
      if (!handle) return null;
      const rect = handle.getBoundingClientRect();
      return { x: rect.left + rect.width / 2, y: rect.top + rect.height / 2 };
    })()
  `, resizeStartDeltaY);
  await waitForCondition(client, 'schedule block start resized', `window.__SMOKE_STATE.scheduleBlocks.some((block) => block.title === ${JSON.stringify(smokeScheduleTitle)} && block.start_minute === 555 && block.end_minute === 600)`);
  const resizeEndDeltaY = await client.evaluate(`
    (() => {
      const lane = document.querySelector('.schedule-lane');
      if (!lane) return 0;
      return lane.getBoundingClientRect().height * 30 / (24 * 60 - 6 * 60);
    })()
  `);
  await dragByExpression(client, 'resize schedule block end', `
    (() => {
      const block = [...document.querySelectorAll('.schedule-block')].find((node) => node.textContent.includes(${JSON.stringify(smokeScheduleTitle)}));
      const handle = block?.querySelector('.schedule-resize-handle.is-end');
      if (!handle) return null;
      const rect = handle.getBoundingClientRect();
      return { x: rect.left + rect.width / 2, y: rect.top + rect.height / 2 };
    })()
  `, resizeEndDeltaY);
  await waitForCondition(client, 'schedule block end resized', `window.__SMOKE_STATE.scheduleBlocks.some((block) => block.title === ${JSON.stringify(smokeScheduleTitle)} && block.start_minute === 555 && block.end_minute === 630)`);
  await clickByExpression(client, 'start focus from schedule page', `
    (() => {
      const block = [...document.querySelectorAll('.schedule-block')].find((node) => node.textContent.includes(${JSON.stringify(smokeScheduleTitle)}));
      const button = block?.querySelector('.schedule-block-actions button');
      if (!button || button.disabled) return false;
      button.click();
      return true;
    })()
  `);
  await waitForCondition(client, 'active focus shell', `Boolean(document.querySelector('.focus-active-shell')) && window.__SMOKE_STATE.studyState.status === 'active'`);
  await waitForCondition(client, 'focus main content focused after schedule start', `document.activeElement?.id === 'main-content'`);

  const statsShortcut = await dispatchAltShortcut(client, 'stats keyboard navigation', '6');
  if (!statsShortcut.defaultPrevented) {
    throw new Error('Stats keyboard navigation did not consume Alt+6');
  }
  await waitForCondition(client, 'stats page loaded', `Boolean(document.querySelector('.stats-toolbar')) && document.body.innerText.includes('筛选学习记录')`);
  await waitForCondition(client, 'stats main content focused', `document.activeElement?.id === 'main-content'`);
  await waitForCondition(client, 'stats hash/title synced', `location.hash === '#stats' && document.title.startsWith('统计') && document.title.endsWith('考研专注')`);
  await client.evaluate(`
    (() => {
      const statusSelect = document.querySelectorAll('.stats-toolbar-grid select')[0];
      if (!statusSelect) return false;
      statusSelect.value = 'finished';
      statusSelect.dispatchEvent(new Event('change', { bubbles: true }));
      return true;
    })()
  `);
  await waitForCondition(client, 'stats finished filter empty', `document.body.innerText.includes('没有符合筛选条件的记录')`);
  await clickByExpression(client, 'reset stats filters', `
    (() => {
      const button = [...document.querySelectorAll('.stats-toolbar-actions button')].find((node) => node.textContent.includes('重置筛选'));
      if (!button || button.disabled) return false;
      button.click();
      return true;
    })()
  `);
  await waitForCondition(client, 'stats filters reset', `document.body.innerText.includes('筛选学习记录') && document.body.innerText.includes('显示')`);
  await clickByExpression(client, 'copy stats summary', `
    (() => {
      const button = [...document.querySelectorAll('.stats-toolbar-actions button')].find((node) => node.textContent.includes('复制摘要'));
      if (!button || button.disabled) return false;
      button.click();
      return true;
    })()
  `);
  await waitForCondition(client, 'stats summary copied', `Boolean(window.__SMOKE_STATE.lastCopiedText) && window.__SMOKE_STATE.lastCopiedText.includes('考研专注学习统计摘要')`);
  await clickByExpression(client, 'export stats csv', `
    (() => {
      const button = [...document.querySelectorAll('.stats-toolbar-actions button')].find((node) => node.textContent.includes('导出 CSV'));
      if (!button || button.disabled) return false;
      button.click();
      return true;
    })()
  `);
  await waitForCondition(client, 'stats csv exported', `Boolean(window.__SMOKE_STATE.lastDownloadName) && window.__SMOKE_STATE.lastDownloadName.startsWith('kaoyan-focus-sessions-')`);
  await waitForCondition(client, 'stats csv blob captured', `Boolean(window.__SMOKE_STATE.lastDownloadBlob)`);
  const exportedCsv = await client.evaluate(`window.__SMOKE_STATE.lastDownloadBlob ? window.__SMOKE_STATE.lastDownloadBlob.text() : null`);
  if (!exportedCsv || !String(exportedCsv).includes('记录ID') || !String(exportedCsv).includes('状态')) {
    throw new Error(`Unexpected CSV export payload: ${exportedCsv}`);
  }

  await client.evaluate(`window.history.back()`);
  await waitForCondition(client, 'returned to focus from history', `location.hash === '#focus' && document.body.innerText.includes('专注中')`);

  await clickByExpression(client, 'settings navigation', `
    (() => {
      const button = [...document.querySelectorAll('.nav-item')].find((node) => node.textContent.includes('设置'));
      if (!button) return false;
      button.click();
      return true;
    })()
  `);
  await waitForCondition(client, 'settings page', `document.body.innerText.includes('节奏与数据控制')`);
  await waitForCondition(client, 'settings main content focused', `document.activeElement?.id === 'main-content'`);
  await clickByExpression(client, 'system settings tab', `
    (() => {
      const button = [...document.querySelectorAll('.settings-section-tabs button')].find((node) => node.textContent.includes('系统'));
      if (!button) return false;
      button.click();
      return true;
    })()
  `);
  await clickByExpression(client, 'privacy data toggle', `
    (() => {
      const panel = [...document.querySelectorAll('.settings-tab-panel .command-panel')].find((node) =>
        node.textContent.includes('隐私与数据边界')
      );
      const button = panel?.querySelector('.settings-collapse-button');
      if (!button || button.disabled) return false;
      button.click();
      return true;
    })()
  `);
  await waitForCondition(client, 'system panel loaded', `
    document.body.innerText.includes('复制路径信息') &&
    document.body.innerText.includes('复制诊断摘要') &&
    document.body.innerText.includes('打开数据目录')
  `);
  await waitForCondition(client, 'system data location ready', `
    document.body.innerText.includes('smoke/kaoyan-focus.sqlite3') &&
    document.body.innerText.includes('数据目录')
  `);
  await client.evaluate(`
    (() => {
      window.__SMOKE_STATE.lastCopiedText = null;
      navigator.clipboard.writeText = async () => {
        throw new Error('clipboard denied');
      };
      return true;
    })()
  `);
  const systemDetailTopBefore = await client.evaluate(`
    (() => {
      const panel = [...document.querySelectorAll('.settings-tab-panel .command-panel')].find((node) =>
        node.textContent.includes('隐私与数据边界')
      );
      const card = [...(panel?.querySelectorAll('.details-card.stacked') ?? [])].find((node) =>
        node.textContent.includes('数据目录')
      );
      if (!panel || !card) return null;
      const panelRect = panel.getBoundingClientRect();
      const cardRect = card.getBoundingClientRect();
      return cardRect.top - panelRect.top;
    })()
  `);
  await clickByExpression(client, 'copy app data location', `
    (() => {
      const button = [...document.querySelectorAll('.settings-tab-panel .row-actions button')].find((node) => node.textContent.includes('复制路径信息'));
      if (!button || button.disabled) return false;
      button.click();
      return true;
    })()
  `);
  await waitForCondition(client, 'app data clipboard copied', `
    Boolean(window.__SMOKE_STATE.lastCopiedText) &&
    window.__SMOKE_STATE.lastCopiedText.includes('数据目录: smoke') &&
    window.__SMOKE_STATE.lastCopiedText.includes('SQLite 文件: smoke/kaoyan-focus.sqlite3')
  `);
  await waitForCondition(client, 'system toast visible after path copy', `
    document.querySelector('.settings-tab-panel .alert.success')?.style.opacity === '1'
  `);
  const systemDetailTopDuringCopy = await client.evaluate(`
    (() => {
      const panel = [...document.querySelectorAll('.settings-tab-panel .command-panel')].find((node) =>
        node.textContent.includes('隐私与数据边界')
      );
      const card = [...(panel?.querySelectorAll('.details-card.stacked') ?? [])].find((node) =>
        node.textContent.includes('数据目录')
      );
      if (!panel || !card) return null;
      const panelRect = panel.getBoundingClientRect();
      const cardRect = card.getBoundingClientRect();
      return cardRect.top - panelRect.top;
    })()
  `);
  if (
    systemDetailTopBefore !== null &&
    systemDetailTopDuringCopy !== null &&
    Math.abs(systemDetailTopDuringCopy - systemDetailTopBefore) > 4
  ) {
    throw new Error(`System panel layout shifted during toast: before=${systemDetailTopBefore}, during=${systemDetailTopDuringCopy}`);
  }
  await sleep(2800);
  await waitForCondition(client, 'system toast cleared after copy', `
    document.querySelector('.settings-tab-panel .alert.success')?.style.opacity === '0'
  `);
  await clickByExpression(client, 'copy system diagnostic summary', `
    (() => {
      const button = [...document.querySelectorAll('.settings-tab-panel .row-actions button')].find((node) => node.textContent.includes('复制诊断摘要'));
      if (!button || button.disabled) return false;
      button.click();
      return true;
    })()
  `);
  await waitForCondition(client, 'system diagnostic copied', `
    Boolean(window.__SMOKE_STATE.lastCopiedText) &&
    window.__SMOKE_STATE.lastCopiedText.includes('考研专注系统诊断摘要') &&
    window.__SMOKE_STATE.lastCopiedText.includes('数据目录')
  `);
  await clickByExpression(client, 'export system diagnostic summary', `
    (() => {
      const button = [...document.querySelectorAll('.settings-tab-panel .row-actions button')].find((node) => node.textContent.includes('导出诊断摘要'));
      if (!button || button.disabled) return false;
      button.click();
      return true;
    })()
  `);
  await waitForCondition(client, 'system diagnostic exported', `
    Boolean(window.__SMOKE_STATE.lastDownloadName) &&
    window.__SMOKE_STATE.lastDownloadName.startsWith('kaoyan-focus-diagnostic-')
  `);
  await waitForCondition(client, 'system diagnostic blob captured', `Boolean(window.__SMOKE_STATE.lastDownloadBlob)`);
  const exportedDiagnostic = await client.evaluate(`window.__SMOKE_STATE.lastDownloadBlob ? window.__SMOKE_STATE.lastDownloadBlob.text() : null`);
  if (!exportedDiagnostic || !String(exportedDiagnostic).includes('考研专注系统诊断摘要') || !String(exportedDiagnostic).includes('数据目录')) {
    throw new Error(`Unexpected diagnostic export payload: ${exportedDiagnostic}`);
  }
  await clickByExpression(client, 'open app data location', `
    (() => {
      const button = [...document.querySelectorAll('.settings-tab-panel .row-actions button')].find((node) => node.textContent.includes('打开数据目录'));
      if (!button || button.disabled) return false;
      button.click();
      return true;
    })()
  `);
  await waitForCondition(client, 'app data location opened', `
    window.__SMOKE_STATE.lastOpenedPath === 'smoke' &&
    document.querySelector('.settings-tab-panel .alert.success')?.style.opacity === '1'
  `);
  await sleep(2800);
  await waitForCondition(client, 'system toast cleared after open', `
    document.querySelector('.settings-tab-panel .alert.success')?.style.opacity === '0'
  `);
  await client.evaluate(`
    (() => {
      window.__SMOKE_STATE.lastDownloadName = null;
      window.__SMOKE_STATE.lastDownloadHref = null;
      window.__SMOKE_STATE.lastDownloadBlob = null;
      window.__SMOKE_STATE.lastRemovedDownloadName = null;
      window.__SMOKE_STATE.lastRemovedDownloadHref = null;
      window.__SMOKE_STATE.lastRevokedUrl = null;
      window.__SMOKE_STATE.failNextDownloadClick = true;
      return true;
    })()
  `);
  await clickByExpression(client, 'export system diagnostic summary with cleanup failure', `
    (() => {
      const button = [...document.querySelectorAll('.settings-tab-panel .row-actions button')].find((node) => node.textContent.includes('导出诊断摘要'));
      if (!button || button.disabled) return false;
      button.click();
      return true;
    })()
  `);
  await waitForCondition(client, 'system diagnostic failure surfaced', `
    document.querySelector('.alert.error')?.textContent?.includes('smoke download click failure')
  `);
  await waitForCondition(client, 'system diagnostic cleanup removed download link', `
    Boolean(window.__SMOKE_STATE.lastRemovedDownloadName) &&
    window.__SMOKE_STATE.lastRemovedDownloadName.startsWith('kaoyan-focus-diagnostic-') &&
    Boolean(window.__SMOKE_STATE.lastRemovedDownloadHref)
  `);
  await waitForCondition(client, 'system diagnostic cleanup revoked object url', `
    Boolean(window.__SMOKE_STATE.lastRevokedUrl) &&
    window.__SMOKE_STATE.lastRevokedUrl === window.__SMOKE_STATE.lastDownloadHref
  `);
  const systemDetailTopAfter = await client.evaluate(`
    (() => {
      const panel = [...document.querySelectorAll('.settings-tab-panel .command-panel')].find((node) =>
        node.textContent.includes('隐私与数据边界')
      );
      const card = [...(panel?.querySelectorAll('.details-card.stacked') ?? [])].find((node) =>
        node.textContent.includes('数据目录')
      );
      if (!panel || !card) return null;
      const panelRect = panel.getBoundingClientRect();
      const cardRect = card.getBoundingClientRect();
      return cardRect.top - panelRect.top;
    })()
  `);
  if (
    systemDetailTopBefore !== null &&
    systemDetailTopAfter !== null &&
    Math.abs(systemDetailTopAfter - systemDetailTopBefore) > 4
  ) {
    throw new Error(`System panel layout shifted after toast cleared: before=${systemDetailTopBefore}, after=${systemDetailTopAfter}`);
  }
  await clickByExpression(client, 'sync settings tab', `
    (() => {
      const button = [...document.querySelectorAll('.settings-section-tabs button')].find((node) => node.textContent.includes('同步'));
      if (!button) return false;
      button.click();
      return true;
    })()
  `);
  await waitForCondition(client, 'sync locked feedback', `
    document.body.innerText.includes('保存 WebDAV 配置') &&
    document.body.innerText.includes('测试连接') &&
    document.body.innerText.includes('学习模式正在运行，全部配置改动已锁定')
  `);

  await clickByExpression(client, 'dawn theme toggle', `
    (() => {
      const button = [...document.querySelectorAll('.theme-toggle button')].find((node) => node.textContent.includes('晨光'));
      if (!button) return false;
      button.click();
      return true;
    })()
  `);
  await waitForCondition(client, 'theme updated to dawn', `document.documentElement.dataset.theme === 'dawn'`);
  const updatedThemeState = await readThemeState(client);
  if (updatedThemeState.activeTheme !== 'dawn' || updatedThemeState.activeThemeLabel !== '晨光' || updatedThemeState.themeColor !== '#f7fbf8') {
    throw new Error(`Unexpected updated theme state: ${JSON.stringify(updatedThemeState)}`);
  }

  if (consoleErrors.length > 0) {
    throw new Error(`Console errors found: ${consoleErrors.join('\\n')}`);
  }

  console.log('Core flow smoke passed');
} finally {
  websocket?.close();
  await stopProcess(edgeProcess);
  await stopProcess(serverProcess);
  if (userDataDir) {
    await removeTempDir(userDataDir);
  }
}
