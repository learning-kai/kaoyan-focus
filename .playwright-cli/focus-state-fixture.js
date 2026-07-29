(() => {
  const query = new URLSearchParams(location.search);
  const phase = query.get('uiPhase') || 'focus';
  const paused = query.get('paused') === '1';
  const mode = query.get('mode') === 'strict' ? 'strict' : 'normal';
  const isLongBreak = query.get('breakKind') === 'long';
  const now = Date.now();
  const phaseDuration = phase === 'break' ? (isLongBreak ? 900 : 300) : 1500;
  const desiredRemaining = phase === 'focus' ? 1062 : phase === 'break' ? (isLongBreak ? 708 : 201) : 134;
  const elapsed = phase === 'awaiting_break' ? 134 : Math.max(0, phaseDuration - desiredRemaining);
  const iso = (offsetSeconds = 0) => new Date(now - offsetSeconds * 1000).toISOString();

  const state = {
    id: 42,
    phase,
    status: 'active',
    mode,
    subject_id: 3,
    planned_seconds: 7200,
    focus_seconds: 1500,
    break_seconds: 300,
    long_break_seconds: 900,
    long_break_interval: 4,
    effective_break_seconds: isLongBreak ? 900 : 300,
    break_kind: isLongBreak ? 'long' : 'short',
    cycle_index: 3,
    started_at: iso(2280),
    phase_started_at: iso(elapsed),
    paused_at: paused ? iso(15) : null,
    ended_at: null,
    current_session: {
      id: 99,
      mode,
      subject_id: 3,
      planned_seconds: 1500,
      actual_seconds: elapsed,
      started_at: iso(elapsed),
      ended_at: null,
      status: 'running',
      end_reason: null,
      interruption_count: 1,
      emergency_exit_count: 0,
      created_at: iso(2280),
      updated_at: iso(),
    },
    study_elapsed_seconds: 2280,
    study_remaining_seconds: 4920,
    phase_elapsed_seconds: elapsed,
    phase_remaining_seconds: desiredRemaining,
    focus_enforcement_active: phase !== 'break',
    whitelist_enabled: true,
    is_paused: paused,
  };

  const settings = {
    default_study_minutes: 120,
    default_focus_minutes: 25,
    break_minutes: 5,
    long_break_minutes: 15,
    long_break_interval: 4,
    default_focus_mode: 'normal',
    whitelist_mode: 'allowlist',
    ui_theme: 'light',
    launch_at_startup: false,
    auto_start_break_after_focus: false,
    schedule_reminder_enabled: false,
    schedule_reminder_lead_minutes: 10,
    focus_widget_enabled: true,
    focus_widget_auto_follow: true,
    focus_widget_remember_geometry: true,
    focus_widget_always_on_top: true,
    focus_widget_x: null,
    focus_widget_y: null,
    focus_widget_width: null,
    focus_widget_height: null,
    sync_backend: 'webdav',
    primary_owner_device_id: 'fixture-device',
    primary_owner_updated_at: now,
    emergency_cooldown_seconds: 300,
    checklist_category_names: '{}',
    reminder_sound_source: 'builtin',
    reminder_sound_id: 'classic',
    reminder_sound_file_name: null,
    reminder_sound_updated_at: null,
    reminder_sound_volume: 80,
    reminder_sound_duration_seconds: 10,
    reminder_quiet_hours_enabled: false,
    reminder_quiet_hours_start: '23:00',
    reminder_quiet_hours_end: '07:00',
    auto_download_update: false,
    skip_update_version: null,
    update_reminder_snooze_until: null,
  };

  const subjects = [
    { id: 1, name: '政治', color: '#ff453a', enabled: true, created_at: iso(), updated_at: iso() },
    { id: 2, name: '英语', color: '#30d158', enabled: true, created_at: iso(), updated_at: iso() },
    { id: 3, name: '数学强化与错题复盘', color: '#0a84ff', enabled: true, created_at: iso(), updated_at: iso() },
  ];

  const todayItems = [
    { id: 1, today_date: '2026-07-29', source_task_id: 11, subject_id: 3, title: '高等数学多元函数积分长标题压力测试', note: '整理错题并归纳方法', due_date: null, sort_order: 0, completed: false, synced_source_completion: false, created_at: iso(), updated_at: iso() },
    { id: 2, today_date: '2026-07-29', source_task_id: null, subject_id: 2, title: '英语阅读两篇', note: null, due_date: null, sort_order: 1, completed: true, synced_source_completion: false, created_at: iso(), updated_at: iso() },
  ];

  const dayBlocks = [
    { id: 5, schedule_date: '2026-07-29', title: '数学强化与错题复盘', note: null, category_key: 'math', subject_id: 3, source_today_item_id: 1, template_id: null, start_minute: 570, end_minute: 630, status: 'running', linked_study_mode_id: 42, linked_focus_session_id: 99, has_conflict: false, created_at: iso(), updated_at: iso() },
  ];

  const responses = {
    get_app_settings: settings,
    get_sync_device_id: 'fixture-device',
    list_subjects: subjects,
    get_study_mode_state: state,
    list_focus_sessions: [],
    get_focus_stats_summary: { today_seconds: 2280, week_seconds: 12840, month_seconds: 46800, interruption_count: 1, subjects: [] },
    get_checklist_page_data: { today_date: '2026-07-29', active_category_key: 'math', highlighted_subject_id: 3, categories: [], today_items: todayItems },
    get_schedule_page_data: { selected_date: '2026-07-29', today_date: '2026-07-29', week_start_date: '2026-07-27', day_blocks: dayBlocks, week_days: [], today_items: todayItems, templates: [] },
    get_next_alarm: null,
    trigger_due_alarms: [],
    list_alarms: [],
    'plugin:window|is_fullscreen': false,
    'plugin:updater|check': null,
  };

  let callbackId = 1;
  const callbacks = new Map();
  window.__TAURI_INTERNALS__ = {
    metadata: { currentWindow: { label: 'main' }, currentWebview: { label: 'main' } },
    transformCallback(callback) {
      const id = callbackId++;
      callbacks.set(id, callback);
      return id;
    },
    unregisterCallback(id) {
      callbacks.delete(id);
    },
    async invoke(command) {
      if (Object.prototype.hasOwnProperty.call(responses, command)) return structuredClone(responses[command]);
      if (command.startsWith('plugin:event|')) return 0;
      if (command.startsWith('plugin:window|')) return null;
      return null;
    },
  };
})();
