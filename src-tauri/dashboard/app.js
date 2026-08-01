const DASHBOARD_TOKEN = new URLSearchParams(window.location.search).get('token') || '';
const PROJECT_DATA_ENDPOINT = DASHBOARD_TOKEN
  ? `./api/study-data?token=${encodeURIComponent(DASHBOARD_TOKEN)}`
  : './api/study-data';
const DASHBOARD_ANALYTICS = window.DashboardAnalytics;
const SESSION_PREVIEW_LIMIT = 12;
const WEEKDAY_LABELS = ['一', '二', '三', '四', '五', '六', '日'];
const STATUS_CLASS_NAMES = ['insufficient', 'steady', 'correction', 'slowing', 'hard_push'];

const els = {
  datasetStatus: document.getElementById('dataset-status'),
  datasetMeta: document.getElementById('dataset-meta'),
  rangeLabel: document.getElementById('range-label'),
  statusLine: document.getElementById('status-line'),
  refreshProjectData: document.getElementById('refresh-project-data'),
  dataSourceStatus: document.getElementById('data-source-status'),
  dataSourcePath: document.getElementById('data-source-path'),
  dataSourceMeta: document.getElementById('data-source-meta'),
  statusPanel: document.getElementById('diagnosis-panel'),
  statusLabel: document.getElementById('status-label'),
  statusHeadline: document.getElementById('status-headline'),
  statusBody: document.getElementById('status-body'),
  statusReasons: document.getElementById('status-reasons'),
  primaryAction: document.getElementById('primary-action'),
  periodLabel: document.getElementById('period-label'),
  previousPeriod: document.getElementById('previous-period'),
  nextPeriod: document.getElementById('next-period'),
  planProgressBar: document.getElementById('plan-progress-bar'),
  planProgressValue: document.getElementById('plan-progress-value'),
  planProgressMeta: document.getElementById('plan-progress-meta'),
  planTasks: document.getElementById('plan-tasks'),
  planDays: document.getElementById('plan-days'),
  weekList: document.getElementById('week-list'),
  subjectList: document.getElementById('subject-list'),
  sessionTableBody: document.getElementById('session-table-body'),
  sessionTableCount: document.getElementById('session-table-count'),
  toggleSessionRows: document.getElementById('toggle-session-rows'),
  subjectFilter: document.getElementById('subject-filter'),
  recordFilterButtons: Array.from(document.querySelectorAll('[data-record-filter]')),
  metricHours: document.getElementById('metric-hours'),
  metricHoursNote: document.getElementById('metric-hours-note'),
  metricFocus: document.getElementById('metric-focus'),
  metricFocusNote: document.getElementById('metric-focus-note'),
  metricTaskRate: document.getElementById('metric-task-rate'),
  metricTaskNote: document.getElementById('metric-task-note'),
  metricDays: document.getElementById('metric-days'),
  metricDaysNote: document.getElementById('metric-days-note'),
  emptyState: document.getElementById('empty-state'),
};

let state = {
  records: [],
  plans: [],
  activeRange: 'week',
  subjectFilter: 'all',
  recordFilter: 'all',
  sessionRowsExpanded: false,
  source: null,
  readOnly: true,
  error: '',
  weekOffset: 0,
};
let loadingProjectData = false;
let datasetMessage = '正在读取项目数据库';

bindControls();
render();
void loadProjectData();

function bindControls() {
  els.previousPeriod?.addEventListener('click', () => {
    state.weekOffset -= 1;
    render('已切换到上一周');
  });
  els.nextPeriod?.addEventListener('click', () => {
    state.weekOffset += 1;
    render('已切换到下一周');
  });
  els.refreshProjectData?.addEventListener('click', () => {
    void loadProjectData();
  });
  els.subjectFilter?.addEventListener('change', () => {
    state.subjectFilter = els.subjectFilter.value || 'all';
    state.sessionRowsExpanded = false;
    render('已更新会话筛选');
  });
  for (const button of els.recordFilterButtons) {
    button.addEventListener('click', () => {
      state.recordFilter = button.dataset.recordFilter || 'all';
      state.sessionRowsExpanded = false;
      render('已更新会话筛选');
    });
  }
  els.toggleSessionRows?.addEventListener('click', () => {
    state.sessionRowsExpanded = !state.sessionRowsExpanded;
    render(state.sessionRowsExpanded ? '已展开全部会话' : '已收起会话');
  });
}

async function loadProjectData() {
  loadingProjectData = true;
  state.error = '';
  document.body.classList.add('is-loading-data');
  document.body.setAttribute('aria-busy', 'true');
  render('正在从项目数据库只读读取...');
  try {
    if (!DASHBOARD_TOKEN) throw new Error('缺少只读访问令牌');
    const response = await fetch(PROJECT_DATA_ENDPOINT, { cache: 'no-store' });
    if (!response.ok) throw new Error(`只读服务返回 ${response.status}`);
    const payload = await response.json();
    if (payload.error) throw new Error(payload.error);
    const records = dedupeRecords(Array.isArray(payload.records) ? payload.records : []);
    const plans = normalizePlans(payload.plans);
    if (!records.length && !plans.length) throw new Error('项目数据库里没有可分析的计划或专注记录');
    state.records = records;
    state.plans = plans;
    state.source = payload.source || null;
    state.readOnly = payload.readOnly !== false;
    datasetMessage = `已只读读取项目数据：${records.length} 条会话，${plans.length} 天计划`;
  } catch (error) {
    state.records = [];
    state.plans = [];
    state.source = null;
    state.readOnly = true;
    state.error = getErrorMessage(error);
    datasetMessage = `读取失败：${state.error}`;
  } finally {
    loadingProjectData = false;
    document.body.classList.remove('is-loading-data');
    document.body.removeAttribute('aria-busy');
    render(datasetMessage);
  }
}

function render(message = datasetMessage) {
  const records = sortRecords(dedupeRecords(state.records));
  const validRecords = DASHBOARD_ANALYTICS.filterFocusTimeRecords(records);
  const anchorDate = getAnchorDate(records, state.plans);
  const week = buildWeekModel(validRecords, state.plans, anchorDate, state.weekOffset);
  const subjects = buildSubjectModel(validRecords, week.plans);
  const diagnosis = DASHBOARD_ANALYTICS.buildStudyDiagnosis({
    days: week.days,
    subjects,
  });
  const tasks = week.days.reduce(
    (summary, day) => ({
      done: summary.done + day.tasksDone,
      total: summary.total + day.tasksTotal,
    }),
    { done: 0, total: 0 },
  );

  renderHeader(message, records, week);
  renderDiagnosis(diagnosis, week);
  renderMetrics(week, diagnosis, tasks, validRecords.length);
  renderPlanProgress(week, diagnosis, tasks);
  renderWeekList(week.days);
  renderSubjects(subjects);
  renderSessionTable(validRecords);
  renderSourcePanel();
  renderEmptyState(records, state.plans);

  els.statusLine.textContent = state.error
    ? `读取失败：${state.error}`
    : `${validRecords.length} 条有效专注记录 · 计划数据按日期和科目聚合`;
}

function renderHeader(message, records, week) {
  els.datasetStatus.textContent = message;
  els.datasetMeta.textContent = `${records.length} 条会话 · ${week.plans.length} 天有计划`;
  els.rangeLabel.textContent = formatWeekLabel(week.start, week.end);
  if (els.periodLabel) els.periodLabel.textContent = formatWeekLabel(week.start, week.end);
}

function renderDiagnosis(diagnosis, week) {
  const status = STATUS_CLASS_NAMES.includes(diagnosis.status) ? diagnosis.status : 'insufficient';
  els.statusPanel?.setAttribute('data-status', status);
  els.statusLabel.textContent = diagnosis.label;
  els.statusHeadline.textContent = diagnosis.headline;
  els.statusBody.textContent = buildDiagnosisBody(diagnosis, week);
  els.statusReasons.innerHTML = diagnosis.reasons.map((reason) => `<li>${escapeHtml(reason)}</li>`).join('');
  els.primaryAction.textContent = diagnosis.action;
}

function renderMetrics(week, diagnosis, tasks, recordCount) {
  const actualMinutes = week.days.reduce((total, day) => total + day.actualMinutes, 0);
  const taskRate = tasks.total > 0 ? tasks.done / tasks.total : null;
  const activeDays = week.days.filter((day) => day.actualMinutes > 0).length;
  const quality = diagnosis.quality == null ? '--' : `${Math.round(diagnosis.quality)}`;
  els.metricHours.textContent = formatHours(actualMinutes);
  els.metricHoursNote.textContent = `${recordCount} 条有效专注记录`;
  els.metricFocus.textContent = quality;
  els.metricFocusNote.textContent = '有效会话质量均值';
  els.metricTaskRate.textContent = taskRate == null ? '--' : `${Math.round(taskRate * 100)}%`;
  els.metricTaskNote.textContent = tasks.total > 0 ? `${tasks.done} / ${tasks.total} 项任务` : '本周没有计划任务';
  els.metricDays.textContent = String(activeDays);
  els.metricDaysNote.textContent = `计划 ${diagnosis.plannedDays} 天`;
}

function renderPlanProgress(week, diagnosis, tasks) {
  const rate = diagnosis.planRate == null ? 0 : Math.min(1, Math.max(0, diagnosis.planRate));
  els.planProgressBar.style.width = `${Math.round(rate * 100)}%`;
  els.planProgressValue.textContent = diagnosis.planRate == null ? '--' : `${Math.round(diagnosis.planRate * 100)}%`;
  els.planProgressMeta.textContent = `${formatHours(diagnosis.actualMinutes)} / ${formatHours(diagnosis.plannedMinutes)} · 实际 / 计划`;
  els.planTasks.textContent = tasks.total > 0 ? `${tasks.done}/${tasks.total}` : '--';
  els.planDays.textContent = `${diagnosis.activeDays}/${diagnosis.plannedDays || 0}`;
}

function renderWeekList(days) {
  els.weekList.innerHTML = days
    .map((day) => {
      const rate = day.plannedMinutes > 0 ? Math.min(1, day.actualMinutes / day.plannedMinutes) : 0;
      const stateClass = day.actualMinutes <= 0 ? 'is-empty' : rate >= 0.9 ? 'is-on-track' : 'is-behind';
      return `<li class="day-row ${stateClass}">
        <div class="day-label"><strong>周${WEEKDAY_LABELS[day.weekday]}</strong><span>${formatDateLabel(day.date)}</span></div>
        <div class="day-track"><span style="width:${Math.round(rate * 100)}%"></span></div>
        <div class="day-values"><strong>${formatHours(day.actualMinutes)}</strong><span>${day.plannedMinutes > 0 ? `计划 ${formatHours(day.plannedMinutes)}` : '无计划'}</span></div>
      </li>`;
    })
    .join('');
}

function renderSubjects(subjects) {
  if (!subjects.length) {
    els.subjectList.innerHTML = '<li class="empty-row">本周还没有科目计划或学习记录</li>';
    return;
  }
  els.subjectList.innerHTML = subjects
    .map((item) => {
      const rate = item.plannedMinutes > 0 ? item.actualMinutes / item.plannedMinutes : null;
      const tag = rate == null ? '无计划' : rate >= 0.9 ? '按计划' : `${Math.round(rate * 100)}%`;
      const tone = rate == null ? '' : rate >= 0.9 ? 'good' : rate < 0.7 ? 'risk' : 'warn';
      return `<li class="subject-row">
        <div><strong>${escapeHtml(item.subject)}</strong><span>${formatHours(item.actualMinutes)} 实际 · ${item.plannedMinutes > 0 ? `${formatHours(item.plannedMinutes)} 计划` : '无计划'}</span></div>
        <span class="subject-tag ${tone}">${tag}</span>
      </li>`;
    })
    .join('');
}

function renderSessionTable(records) {
  const filtered = records.filter((record) => {
    if (state.subjectFilter !== 'all' && record.subject !== state.subjectFilter) return false;
    if (state.recordFilter === 'low-focus' && record.focusScore >= 65) return false;
    if (state.recordFilter === 'long-session' && record.minutes < 90) return false;
    if (state.recordFilter === 'interrupted' && record.status !== 'interrupted') return false;
    return true;
  });
  const visible = state.sessionRowsExpanded ? filtered : filtered.slice(0, SESSION_PREVIEW_LIMIT);
  els.sessionTableBody.innerHTML = visible
    .map(
      (record) => `<tr>
        <td>${formatDateLabel(record.date)}</td>
        <td>${escapeHtml(record.subject)}</td>
        <td>${formatSessionWindow(record)}</td>
        <td>${formatMinutes(record.minutes)}</td>
        <td><strong>${Math.round(record.focusScore)}</strong></td>
        <td>${formatSessionStatus(record)}</td>
      </tr>`,
    )
    .join('');
  els.sessionTableCount.textContent = `${filtered.length} 条`;
  if (els.toggleSessionRows) {
    els.toggleSessionRows.hidden = filtered.length <= SESSION_PREVIEW_LIMIT;
    els.toggleSessionRows.textContent = state.sessionRowsExpanded ? '收起' : '展开全部';
  }
  renderSubjectFilter(records);
}

function renderSubjectFilter(records) {
  if (!els.subjectFilter) return;
  const subjects = [...new Set(records.map((record) => record.subject).filter(Boolean))].sort();
  els.subjectFilter.innerHTML = ['<option value="all">全部科目</option>', ...subjects.map((subject) => `<option value="${escapeHtml(subject)}">${escapeHtml(subject)}</option>`)].join('');
  els.subjectFilter.value = subjects.includes(state.subjectFilter) ? state.subjectFilter : 'all';
}

function renderSourcePanel() {
  if (!els.dataSourceStatus) return;
  els.dataSourceStatus.textContent = state.error ? '读取失败' : state.source ? '已连接，只读' : '等待只读服务';
  els.dataSourcePath.textContent = state.source?.path || '尚未连接';
  els.dataSourceMeta.textContent = state.source
    ? `${state.source.subjectCount || 0} 个科目 · ${state.source.taskCount || 0} 条计划任务`
    : '不会修改本地数据库';
}

function renderEmptyState(records, plans) {
  const empty = Boolean(state.error || (!records.length && !plans.length));
  els.emptyState.hidden = !empty;
  if (empty) {
    els.emptyState.innerHTML = `<strong>${state.error ? '暂时无法生成看板' : '还没有学习数据'}</strong><span>${escapeHtml(state.error || '开始一次专注或创建本周计划后，这里会给出诊断。')}</span>`;
  }
}

function buildWeekModel(records, plans, anchorDate, offset) {
  const base = startOfWeek(parseDateKey(anchorDate));
  const start = addDays(base, offset * 7);
  const end = addDays(start, 6);
  const planMap = new Map(normalizePlans(plans).map((plan) => [plan.date, plan]));
  const actualMap = new Map();
  for (const record of records) {
    const current = actualMap.get(record.date) || {
      actualMinutes: 0,
      qualityWeighted: 0,
      qualityWeight: 0,
      interruptionCount: 0,
      emergencyExitCount: 0,
      pausedSeconds: 0,
    };
    current.actualMinutes += record.minutes;
    current.qualityWeighted += record.focusScore * Math.max(1, record.minutes);
    current.qualityWeight += Math.max(1, record.minutes);
    current.interruptionCount += record.interruptionCount;
    current.emergencyExitCount += record.emergencyExitCount;
    current.pausedSeconds += record.pausedSeconds;
    actualMap.set(record.date, current);
  }
  const days = Array.from({ length: 7 }, (_, index) => {
    const date = toDateKey(addDays(start, index));
    const plan = planMap.get(date) || { plannedMinutes: 0, tasksDone: 0, tasksTotal: 0, subjects: [] };
    const actual = actualMap.get(date) || {};
    return {
      date,
      weekday: index,
      plannedMinutes: plan.plannedMinutes,
      actualMinutes: actual.actualMinutes || 0,
      quality: actual.qualityWeight ? actual.qualityWeighted / actual.qualityWeight : null,
      interruptionCount: actual.interruptionCount || 0,
      emergencyExitCount: actual.emergencyExitCount || 0,
      pausedSeconds: actual.pausedSeconds || 0,
      tasksDone: plan.tasksDone,
      tasksTotal: plan.tasksTotal,
      subjects: plan.subjects,
    };
  });
  return { start: toDateKey(start), end: toDateKey(end), days, plans: days.filter((day) => day.plannedMinutes > 0 || day.tasksTotal > 0) };
}

function buildSubjectModel(records, plans) {
  const subjectMap = new Map();
  for (const record of records) {
    const current = subjectMap.get(record.subject) || { subject: record.subject, plannedMinutes: 0, actualMinutes: 0 };
    current.actualMinutes += record.minutes;
    subjectMap.set(record.subject, current);
  }
  for (const plan of plans) {
    for (const item of plan.subjects || []) {
      const current = subjectMap.get(item.subject) || { subject: item.subject, plannedMinutes: 0, actualMinutes: 0 };
      current.plannedMinutes += item.plannedMinutes;
      subjectMap.set(item.subject, current);
    }
  }
  return [...subjectMap.values()].sort((left, right) => {
    const leftRate = left.plannedMinutes > 0 ? left.actualMinutes / left.plannedMinutes : 2;
    const rightRate = right.plannedMinutes > 0 ? right.actualMinutes / right.plannedMinutes : 2;
    return leftRate - rightRate || right.actualMinutes - left.actualMinutes;
  });
}

function normalizePlans(plans) {
  return (Array.isArray(plans) ? plans : [])
    .map((plan) => ({
      date: normalizeDateString(plan?.date),
      plannedMinutes: clampNumber(plan?.plannedMinutes ?? plan?.planned_minutes, 0, 24 * 60, 0),
      tasksDone: clampNumber(plan?.tasksDone ?? plan?.tasks_done, 0, 1000, 0),
      tasksTotal: clampNumber(plan?.tasksTotal ?? plan?.tasks_total, 0, 1000, 0),
      subjects: (Array.isArray(plan?.subjects) ? plan.subjects : []).map((item) => ({
        subject: String(item?.subject || '未命名科目'),
        plannedMinutes: clampNumber(item?.plannedMinutes ?? item?.planned_minutes, 0, 24 * 60, 0),
        actualMinutes: 0,
        tasksDone: clampNumber(item?.tasksDone ?? item?.tasks_done, 0, 1000, 0),
        tasksTotal: clampNumber(item?.tasksTotal ?? item?.tasks_total, 0, 1000, 0),
      })),
    }))
    .filter((plan) => plan.date);
}

function normalizeRecord(raw) {
  const minutes = clampNumber(raw?.minutes, 0, 24 * 60, 0);
  return {
    id: String(raw?.id || ''),
    date: normalizeDateString(raw?.date),
    subject: String(raw?.subject || '未命名科目'),
    minutes,
    focusScore: clampNumber(raw?.focusScore ?? raw?.focus_score, 0, 100, 0),
    startHour: clampNumber(raw?.startHour ?? raw?.start_hour, 0, 23, 0),
    actualSeconds: clampNumber(raw?.actualSeconds ?? raw?.actual_seconds, 0, 7 * 24 * 3600, minutes * 60),
    plannedSeconds: clampNumber(raw?.plannedSeconds ?? raw?.planned_seconds, 0, 7 * 24 * 3600, minutes * 60),
    pausedSeconds: clampNumber(raw?.pausedSeconds ?? raw?.paused_seconds, 0, 7 * 24 * 3600, 0),
    interruptionCount: clampNumber(raw?.interruptionCount ?? raw?.interruption_count, 0, 1000, 0),
    emergencyExitCount: clampNumber(raw?.emergencyExitCount ?? raw?.emergency_exit_count, 0, 1000, 0),
    startedAt: raw?.startedAt || raw?.started_at || '',
    endedAt: raw?.endedAt || raw?.ended_at || '',
    endReason: String(raw?.endReason ?? raw?.end_reason ?? ''),
    status: String(raw?.status || ''),
  };
}

function dedupeRecords(records) {
  const unique = new Map();
  for (const raw of Array.isArray(records) ? records : []) {
    const record = normalizeRecord(raw);
    if (!record.date || record.minutes <= 0) continue;
    const key = record.id || `${record.date}|${record.subject}|${record.startHour}|${record.minutes}|${record.startedAt}`;
    if (!unique.has(key)) unique.set(key, record);
  }
  return [...unique.values()];
}

function sortRecords(records) {
  return [...records].sort((left, right) => `${right.date}|${right.startedAt}`.localeCompare(`${left.date}|${left.startedAt}`));
}

function getAnchorDate(records, plans) {
  const dates = [...records.map((record) => record.date), ...plans.map((plan) => plan.date)].filter(Boolean).sort();
  return dates.at(-1) || toDateKey(new Date());
}

function buildDiagnosisBody(diagnosis, week) {
  if (diagnosis.status === 'insufficient') return `本周 ${formatWeekLabel(week.start, week.end)} 的样本还不够，先积累连续记录再下结论。`;
  return `本周实际 ${formatHours(diagnosis.actualMinutes)}，计划 ${formatHours(diagnosis.plannedMinutes)}。看板把计划兑现和专注质量分开判断。`;
}

function formatSessionStatus(record) {
  if (record.status === 'interrupted') return '中断';
  if (record.status === 'running') return '进行中';
  return record.endReason === 'completed' || record.status === 'finished' ? '完成' : '已结束';
}

function formatSessionWindow(record) {
  const start = record.startedAt ? new Date(record.startedAt) : null;
  return start && !Number.isNaN(start.getTime()) ? `${String(start.getHours()).padStart(2, '0')}:${String(start.getMinutes()).padStart(2, '0')}` : `${String(record.startHour).padStart(2, '0')}:00`;
}

function formatHours(minutes) {
  const value = Number(minutes) || 0;
  if (value >= 60) return `${(value / 60).toFixed(value % 60 === 0 ? 0 : 1)}h`;
  return `${Math.round(value)}m`;
}

function formatMinutes(minutes) {
  return `${Math.round(Number(minutes) || 0)}m`;
}

function formatWeekLabel(start, end) {
  return `${formatDateLabel(start)} - ${formatDateLabel(end)}`;
}

function formatDateLabel(value) {
  const match = String(value || '').match(/^(\d{4})-(\d{2})-(\d{2})$/);
  return match ? `${Number(match[2])}月${Number(match[3])}日` : String(value || '--');
}

function escapeHtml(value) {
  return String(value ?? '')
    .replaceAll('&', '&amp;')
    .replaceAll('<', '&lt;')
    .replaceAll('>', '&gt;')
    .replaceAll('"', '&quot;')
    .replaceAll("'", '&#039;');
}

function getErrorMessage(error) {
  return error instanceof Error ? error.message : String(error);
}

function clampNumber(value, min, max, fallback) {
  const number = Number(value);
  return Number.isFinite(number) ? Math.min(max, Math.max(min, number)) : fallback;
}

function normalizeDateString(value) {
  const match = String(value || '').match(/^(\d{4}-\d{2}-\d{2})/);
  return match ? match[1] : '';
}

function parseDateKey(value) {
  const [year, month, day] = String(value).split('-').map(Number);
  return new Date(year, month - 1, day);
}

function toDateKey(date) {
  return `${date.getFullYear()}-${String(date.getMonth() + 1).padStart(2, '0')}-${String(date.getDate()).padStart(2, '0')}`;
}

function startOfWeek(date) {
  const next = new Date(date);
  const offset = (next.getDay() + 6) % 7;
  next.setDate(next.getDate() - offset);
  return next;
}

function addDays(date, count) {
  const next = new Date(date);
  next.setDate(next.getDate() + count);
  return next;
}
