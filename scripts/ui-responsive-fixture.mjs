(() => {
  const query = new URLSearchParams(location.search);
  const requestedTheme = query.get('theme') || 'light';
  try {
    localStorage.setItem('kaoyan-focus-theme', requestedTheme);
  } catch {
    // Theme storage can be unavailable in hardened browser contexts.
  }
  const requestedPhase = query.get('uiPhase') || 'idle';
  const phase = ['focus', 'awaiting_break', 'break'].includes(requestedPhase) ? requestedPhase : 'idle';
  const paused = query.get('paused') === '1';
  const mode = query.get('mode') === 'strict' ? 'strict' : 'normal';
  const dockMode = ['collapsed', 'peek'].includes(query.get('dock')) ? query.get('dock') : 'floating';
  const dockEdge = query.get('edge') || (dockMode === 'floating' ? null : 'right');
  const isLongBreak = query.get('breakKind') === 'long';
  const now = Date.now();
  const iso = (offsetSeconds = 0) => new Date(now - offsetSeconds * 1000).toISOString();
  const phaseDuration = phase === 'break' ? (isLongBreak ? 900 : 300) : 1500;
  const desiredRemaining = phase === 'focus' ? 1062 : phase === 'break' ? (isLongBreak ? 708 : 201) : 134;
  const elapsed = phase === 'awaiting_break' ? 134 : Math.max(0, phaseDuration - desiredRemaining);
  const active = phase !== 'idle';

  const subjects = [
    { id: 1, name: '政治', color: '#ff9500', enabled: true, created_at: iso(), updated_at: iso() },
    { id: 2, name: '英语', color: '#34c759', enabled: true, created_at: iso(), updated_at: iso() },
    { id: 3, name: '数学强化与错题复盘', color: '#007aff', enabled: true, created_at: iso(), updated_at: iso() },
    { id: 4, name: '专业课', color: '#ff3b30', enabled: true, created_at: iso(), updated_at: iso() },
  ];

  const currentSession = active ? {
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
  } : null;

  const studyState = {
    id: active ? 42 : null,
    phase,
    status: active ? 'active' : 'idle',
    mode,
    subject_id: active ? 3 : null,
    planned_seconds: active ? 7200 : 0,
    focus_seconds: 1500,
    break_seconds: 300,
    long_break_seconds: 900,
    long_break_interval: 4,
    effective_break_seconds: isLongBreak ? 900 : 300,
    break_kind: isLongBreak ? 'long' : 'short',
    cycle_index: active ? 3 : 0,
    started_at: active ? iso(2280) : null,
    phase_started_at: active ? iso(elapsed) : null,
    paused_at: active && paused ? iso(15) : null,
    ended_at: null,
    current_session: currentSession,
    study_elapsed_seconds: active ? 2280 : 0,
    study_remaining_seconds: active ? 4920 : 0,
    phase_elapsed_seconds: active ? elapsed : 0,
    phase_remaining_seconds: active ? desiredRemaining : 0,
    focus_enforcement_active: active && phase !== 'break',
    whitelist_enabled: true,
    is_paused: active && paused,
  };

  const settings = {
    default_study_minutes: 120,
    default_focus_minutes: 25,
    break_minutes: 5,
    long_break_minutes: 15,
    long_break_interval: 4,
    default_focus_mode: 'normal',
    whitelist_mode: 'allowlist',
    ui_theme: requestedTheme,
    launch_at_startup: false,
    auto_start_break_after_focus: false,
    show_foreground_rule_toggle: query.get('showForegroundRuleToggle') !== '0',
    schedule_reminder_enabled: false,
    schedule_reminder_lead_minutes: 10,
    focus_widget_enabled: true,
    focus_widget_auto_follow: true,
    focus_widget_remember_geometry: true,
    focus_widget_always_on_top: true,
    focus_widget_x: null,
    focus_widget_y: null,
    focus_widget_width: 280,
    focus_widget_height: 144,
    sync_backend: 'webdav',
    primary_owner_device_id: 'fixture-device',
    primary_owner_updated_at: now,
    emergency_cooldown_seconds: 300,
    checklist_category_names: '{"politics":"政治","english":"英语","math":"数学","major":"专业课","general":"通用"}',
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

  const todayItems = [
    { id: 1, today_date: '2026-07-29', source_task_id: 11, subject_id: 3, title: '高等数学多元函数积分长标题压力测试', note: '整理错题并归纳方法', due_date: null, sort_order: 0, completed: false, synced_source_completion: false, created_at: iso(), updated_at: iso() },
    { id: 2, today_date: '2026-07-29', source_task_id: null, subject_id: 2, title: '英语阅读两篇', note: null, due_date: null, sort_order: 1, completed: true, synced_source_completion: false, created_at: iso(), updated_at: iso() },
  ];

  const categoryNames = { politics: '政治', english: '英语', math: '数学', major: '专业课' };
  const categories = Object.entries(categoryNames).map(([key, title], index) => ({
    key,
    title,
    highlighted: key === 'math',
    pending_tasks: [{ id: index + 10, category_key: key, subject_id: index + 1, title: `${title}核心任务与错题复盘`, note: '本周重点', due_date: null, sort_order: 0, completed: false, created_at: iso(), updated_at: iso() }],
    completed_tasks: [],
  }));

  const dayBlocks = [
    { id: 5, schedule_date: '2026-07-29', title: '数学强化与错题复盘', note: null, category_key: 'math', subject_id: 3, source_today_item_id: 1, template_id: null, start_minute: 570, end_minute: 630, status: active ? 'running' : 'planned', linked_study_mode_id: active ? 42 : null, linked_focus_session_id: active ? 99 : null, has_conflict: false, created_at: iso(), updated_at: iso() },
    { id: 6, schedule_date: '2026-07-29', title: '英语阅读与长难句', note: null, category_key: 'english', subject_id: 2, source_today_item_id: null, template_id: null, start_minute: 660, end_minute: 720, status: 'planned', linked_study_mode_id: null, linked_focus_session_id: null, has_conflict: false, created_at: iso(), updated_at: iso() },
    { id: 7, schedule_date: '2026-07-29', title: '短时背单词', note: null, category_key: 'english', subject_id: 2, source_today_item_id: null, template_id: null, start_minute: 780, end_minute: 795, status: 'planned', linked_study_mode_id: null, linked_focus_session_id: null, has_conflict: false, created_at: iso(), updated_at: iso() },
    { id: 8, schedule_date: '2026-07-29', title: '短时公式复习', note: null, category_key: 'math', subject_id: 3, source_today_item_id: null, template_id: null, start_minute: 795, end_minute: 810, status: 'planned', linked_study_mode_id: null, linked_focus_session_id: null, has_conflict: false, created_at: iso(), updated_at: iso() },
  ];

  const weekDates = ['2026-07-27', '2026-07-28', '2026-07-29', '2026-07-30', '2026-07-31', '2026-08-01', '2026-08-02'];
  const scheduleData = {
    selected_date: '2026-07-29',
    today_date: '2026-07-29',
    week_start_date: '2026-07-27',
    day_blocks: dayBlocks,
    week_days: weekDates.map((date, weekday) => ({ date, weekday, blocks: date === '2026-07-29' ? dayBlocks : [], planned_minutes: date === '2026-07-29' ? 150 : 0 })),
    today_items: todayItems,
    templates: [],
  };

  const checklistData = {
    today_date: '2026-07-29',
    active_category_key: 'math',
    highlighted_subject_id: 3,
    categories,
    today_items: todayItems,
  };

  const focusStats = {
    today_seconds: 2280,
    week_seconds: 12840,
    month_seconds: 46800,
    interruption_count: 1,
    subjects: subjects.slice(0, 3).map((subject, index) => ({ subject, total_seconds: 7200 - index * 1200, session_count: 3 - index, interruption_count: index })),
  };

  const responses = {
    get_app_settings: settings,
    get_sync_device_id: 'fixture-device',
    list_subjects: subjects,
    get_study_mode_state: studyState,
    list_focus_sessions: currentSession ? [currentSession] : [],
    get_focus_stats_summary: focusStats,
    get_checklist_page_data: checklistData,
    get_schedule_page_data: scheduleData,
    list_interruption_summary: [],
    list_alarms: [],
    get_next_alarm: null,
    trigger_due_alarms: [],
    has_active_alarm: false,
    list_whitelist_apps: [],
    list_running_processes: [],
    list_recent_blocked_apps: [],
    get_current_potplayer_media: { process_name: 'PotPlayerMini64.exe', media_path: null, media_directory: null, window_title: '', source: null },
    get_current_foreground_app: { process_id: 100, process_name: 'explorer.exe', process_path: 'C:\\Windows\\explorer.exe', window_title: '桌面' },
    get_daily_review_page_data: { review_date: '2026-07-29', review: null, summary: { study_seconds: 2280, focus_session_count: 2, interruption_count: 1, schedule_total: 2, schedule_completed: 0, today_total: 2, today_completed: 1 } },
    get_weekly_review_page_data: { week_start_date: '2026-07-27', week_end_date: '2026-08-02', review: null, summary: { study_seconds: 12840, focus_session_count: 8, interruption_count: 3 } },
    get_app_data_location: { app_data_dir: 'C:\\Users\\Fixture\\AppData\\Local\\kaoyan-focus', database_path: 'C:\\Users\\Fixture\\AppData\\Local\\kaoyan-focus\\kaoyan-focus.sqlite3', database_size_bytes: 1048576 },
    get_webdav_settings: { enabled: false, url: '', username: '', password: '', remote_path: 'kaoyan-focus/kaoyan-focus.sqlite3' },
    get_object_storage_settings: { enabled: false, endpoint: '', bucket: '', access_key_id: '', secret_access_key: '', region: '', object_key: 'study-sync.json' },
    get_email_reminder_settings: { enabled: false, smtp_host: '', smtp_port: 465, smtp_security: 'tls', username: '', password: '', from: '', to: '' },
    get_feishu_sync_settings: { enabled: false, app_id: '', app_secret: '', redirect_uri: 'http://127.0.0.1:39781/feishu/callback' },
    get_feishu_sync_status: { enabled: false, configured: false, authenticated: false, expires_at: null, tasklist_guid: null, tasklist_count: 0, tasklists: [], calendar_id: null, redirect_uri: 'http://127.0.0.1:39781/feishu/callback', pending_authorization_url: null, pending_message: null, required_scopes: '', last_run: null },
    get_caldav_settings: { enabled: false, server_url: '', username: '', password: '', selected_calendar_url: '', selected_calendar_name: '' },
    list_sync_runs: [],
    list_sync_backups: [],
    get_runtime_health: { status: 'healthy', summary: '浏览器验收夹具', checked_at: iso(), checks: [], tasks: [] },
    get_custom_reminder_sound: null,
    check_due_task_email_reminders: { status: 'skipped', message: 'UI fixture', sent_count: 0 },
    focus_widget_get_always_on_top: true,
    focus_widget_get_dock_state: { mode: dockMode, edge: dockEdge },
    focus_widget_peek_from_edge: { mode: 'peek', edge: dockEdge || 'right' },
    focus_widget_collapse_to_edge: { mode: 'collapsed', edge: dockEdge || 'right' },
    'plugin:window|is_fullscreen': false,
    'plugin:updater|check': null,
    'plugin:notification|is_permission_granted': true,
    'plugin:notification|request_permission': 'granted',
  };

  const stateCommands = new Set(['pause_study_mode', 'resume_study_mode', 'confirm_study_break', 'skip_study_break', 'update_study_mode_subject']);
  const voidCommands = new Set([
    'auto_sync_webdav_database',
    'sync_feishu_bridge',
    'sync_caldav_calendar',
    'focus_widget_return_to_main',
    'hide_focus_widget',
    'show_study_reminder',
    'plugin:notification|notify',
    'plugin:event|unlisten',
  ]);
  const callbacks = new Map();
  let callbackId = 1;
  window.__UI_FIXTURE_UNKNOWN_COMMANDS__ = [];
  window.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener() {} };
  window.__TAURI_INTERNALS__ = {
    metadata: {
      currentWindow: { label: query.get('windowLabel') === 'focus-widget' ? 'focus-widget' : 'main' },
      currentWebview: { label: query.get('windowLabel') === 'focus-widget' ? 'focus-widget' : 'main' },
    },
    transformCallback(callback) {
      const id = callbackId++;
      callbacks.set(id, callback);
      return id;
    },
    unregisterCallback(id) {
      callbacks.delete(id);
    },
    runCallback(id, data) {
      callbacks.get(id)?.(data);
    },
    callbacks,
    async invoke(command, args) {
      if (command === 'plugin:event|listen') return callbackId++;
      if (command === 'update_study_mode_whitelist') {
        if (studyState.mode !== 'strict') {
          studyState.whitelist_enabled = Boolean(args?.whitelistEnabled);
          studyState.focus_enforcement_active = studyState.whitelist_enabled && phase !== 'break';
        }
        return structuredClone(studyState);
      }
      if (stateCommands.has(command)) return structuredClone(studyState);
      if (command === 'focus_widget_toggle_always_on_top') return true;
      if (voidCommands.has(command)) return null;
      if (Object.prototype.hasOwnProperty.call(responses, command)) return structuredClone(responses[command]);
      if (command.startsWith('plugin:window|') || command.startsWith('plugin:webview|')) return null;
      window.__UI_FIXTURE_UNKNOWN_COMMANDS__.push(command);
      return null;
    },
  };
})();
