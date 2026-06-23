const DASHBOARD_TOKEN = new URLSearchParams(window.location.search).get('token') || '';
const PROJECT_DATA_ENDPOINT = DASHBOARD_TOKEN
  ? `./api/study-data?token=${encodeURIComponent(DASHBOARD_TOKEN)}`
  : './api/study-data';
const RANGE_LABELS = {
  7: '近 7 天',
  30: '近 30 天',
  90: '近 90 天',
  all: '全部数据',
};
const RECORD_FILTER_LABELS = {
  all: '全部',
  'low-focus': '低会话分',
  'task-debt': '有欠账',
  'long-session': '长时段',
};
const THEME_LABELS = {
  console: 'Console',
  paper: 'Paper',
  graphite: 'Graphite',
  focus: 'Focus',
};
const SESSION_PREVIEW_LIMIT = 8;
const WEEKDAY_LABELS = ['一', '二', '三', '四', '五', '六', '日'];
const WEEKDAY_FULL_LABELS = ['周一', '周二', '周三', '周四', '周五', '周六', '周日'];
const MONTH_LABELS = ['1月', '2月', '3月', '4月', '5月', '6月', '7月', '8月', '9月', '10月', '11月', '12月'];
const HEAT_LEVEL_LABELS = ['无记录', '低效', '未达标', '达标', '高效', '峰值'];
const DASHBOARD_ANALYTICS = window.DashboardAnalytics;

const SUBJECTS = ['数学', '英语', '政治', '专业课', '复盘'];

const els = {
  datasetStatus: document.getElementById('dataset-status'),
  datasetMeta: document.getElementById('dataset-meta'),
  rangeLabel: document.getElementById('range-label'),
  statusLine: document.getElementById('status-line'),
  refreshProjectData: document.getElementById('refresh-project-data'),
  dataSourceStatus: document.getElementById('data-source-status'),
  dataSourcePath: document.getElementById('data-source-path'),
  dataSourceMeta: document.getElementById('data-source-meta'),
  dailySnapshot: document.getElementById('daily-snapshot'),
  summaryHeadline: document.getElementById('summary-headline'),
  summaryBody: document.getElementById('summary-body'),
  summaryTrend: document.getElementById('summary-trend'),
  summaryRisk: document.getElementById('summary-risk'),
  summaryAction: document.getElementById('summary-action'),
  targetProgressValue: document.getElementById('target-progress-value'),
  targetProgressBar: document.getElementById('target-progress-bar'),
  targetProgressMeta: document.getElementById('target-progress-meta'),
  comparisonSummary: document.getElementById('comparison-summary'),
  comparisonList: document.getElementById('comparison-list'),
  taskFunnelSummary: document.getElementById('task-funnel-summary'),
  taskFunnelBars: document.getElementById('task-funnel-bars'),
  riskSummary: document.getElementById('risk-summary'),
  riskList: document.getElementById('risk-list'),
  metricHours: document.getElementById('metric-hours'),
  metricHoursNote: document.getElementById('metric-hours-note'),
  metricFocus: document.getElementById('metric-focus'),
  metricFocusNote: document.getElementById('metric-focus-note'),
  metricTaskRate: document.getElementById('metric-task-rate'),
  metricTaskNote: document.getElementById('metric-task-note'),
  metricStreak: document.getElementById('metric-streak'),
  metricStreakNote: document.getElementById('metric-streak-note'),
  metricWindow: document.getElementById('metric-window'),
  metricWindowNote: document.getElementById('metric-window-note'),
  metricDays: document.getElementById('metric-days'),
  metricDaysNote: document.getElementById('metric-days-note'),
  learningTrendChart: document.getElementById('learning-trend-chart'),
  learningTrendVerdict: document.getElementById('learning-trend-verdict'),
  learningTrendDelta: document.getElementById('learning-trend-delta'),
  learningTrendWindow: document.getElementById('learning-trend-window'),
  learningTrendEffective: document.getElementById('learning-trend-effective'),
  learningTrendMinutes: document.getElementById('learning-trend-minutes'),
  learningTrendFocus: document.getElementById('learning-trend-focus'),
  weekTimelineChart: document.getElementById('week-timeline-chart'),
  weekTimelineLabel: document.getElementById('week-timeline-label'),
  weekTimelineTotal: document.getElementById('week-timeline-total'),
  bestHourChart: document.getElementById('best-hour-chart'),
  bestHourLabel: document.getElementById('best-hour-label'),
  bestHourSummary: document.getElementById('best-hour-summary'),
  yearHeatmapChart: document.getElementById('year-heatmap-chart'),
  yearHeatmapLabel: document.getElementById('year-heatmap-label'),
  yearHeatmapSummary: document.getElementById('year-heatmap-summary'),
  trendChart: document.getElementById('trend-chart'),
  qualityChart: document.getElementById('quality-chart'),
  quadrantSummary: document.getElementById('quadrant-summary'),
  consistencyGrid: document.getElementById('consistency-grid'),
  subjectChart: document.getElementById('subject-chart'),
  heatmapChart: document.getElementById('heatmap-chart'),
  subjectTable: document.getElementById('subject-table'),
  sessionTableBody: document.getElementById('session-table-body'),
  sessionTableCount: document.getElementById('session-table-count'),
  toggleSessionRows: document.getElementById('toggle-session-rows'),
  insightsList: document.getElementById('insights-list'),
  prescriptionSummary: document.getElementById('prescription-summary'),
  prescriptionList: document.getElementById('prescription-list'),
  dataQualityCard: document.getElementById('data-quality-card'),
  dataQualityList: document.getElementById('data-quality-list'),
  subjectFilter: document.getElementById('subject-filter'),
  recordFilterButtons: Array.from(document.querySelectorAll('[data-record-filter]')),
  windowList: document.getElementById('window-list'),
  segmentButtons: Array.from(document.querySelectorAll('.segment-button')),
  themeButtons: Array.from(document.querySelectorAll('button[data-theme]')),
  focusPeriodButtons: Array.from(document.querySelectorAll('[data-focus-period]')),
};

let state = {
  records: [],
  activeRange: '30',
  activeTheme: 'paper',
  subjectFilter: 'all',
  recordFilter: 'all',
  sessionRowsExpanded: false,
  source: null,
  readOnly: true,
  weekOffset: 0,
  monthOffset: 0,
  yearOffset: 0,
};
let loadingProjectData = false;
let datasetMessage = '正在读取项目数据库';

applyTheme(state.activeTheme);
bindControls();
render();
void loadProjectData();

function bindControls() {
  for (const button of els.segmentButtons) {
    button.addEventListener('click', () => {
      state.activeRange = button.dataset.range || '30';
      state.sessionRowsExpanded = false;
      render(`已切换到${RANGE_LABELS[state.activeRange] || '指定范围'}`);
    });
  }

  for (const button of els.themeButtons) {
    button.addEventListener('click', () => {
      state.activeTheme = button.dataset.theme || 'console';
      applyTheme(state.activeTheme);
      syncThemeButtons();
      render(`界面风格已切换为 ${THEME_LABELS[state.activeTheme] || '自定义'}`);
    });
  }

  for (const button of els.focusPeriodButtons) {
    button.addEventListener('click', () => {
      const action = button.dataset.focusPeriod || '';
      if (action === 'week-prev') state.weekOffset -= 1;
      if (action === 'week-next') state.weekOffset += 1;
      if (action === 'month-prev') state.monthOffset -= 1;
      if (action === 'month-next') state.monthOffset += 1;
      if (action === 'year-prev') state.yearOffset -= 1;
      if (action === 'year-next') state.yearOffset += 1;
      render('已切换专注统计周期');
    });
  }

  els.refreshProjectData.addEventListener('click', () => {
    void loadProjectData();
  });

  els.subjectFilter.addEventListener('change', () => {
    state.subjectFilter = els.subjectFilter.value || 'all';
    state.sessionRowsExpanded = false;
    render('已更新明细筛选');
  });

  for (const button of els.recordFilterButtons) {
    button.addEventListener('click', () => {
      state.recordFilter = button.dataset.recordFilter || 'all';
      state.sessionRowsExpanded = false;
      render('已更新明细筛选');
    });
  }

  els.toggleSessionRows.addEventListener('click', () => {
    state.sessionRowsExpanded = !state.sessionRowsExpanded;
    render(state.sessionRowsExpanded ? '已展开最近学习记录' : '已收起最近学习记录');
  });
}

async function loadProjectData() {
  loadingProjectData = true;
  document.body.classList.add('is-loading-data');
  document.body.setAttribute('aria-busy', 'true');
  render('正在从项目数据库只读读取...');
  try {
    if (!DASHBOARD_TOKEN) {
      throw new Error('缺少只读访问令牌');
    }

    const response = await fetch(PROJECT_DATA_ENDPOINT, { cache: 'no-store' });
    if (!response.ok) {
      throw new Error(`只读服务返回 ${response.status}`);
    }
    const payload = await response.json();
    if (payload.error) {
      throw new Error(payload.error);
    }
    const records = Array.isArray(payload.records) ? dedupeRecords(payload.records) : [];
    if (!records.length) {
      throw new Error('项目数据库里没有可分析的专注记录');
    }

    state.records = records;
    state.source = payload.source || null;
    state.readOnly = payload.readOnly !== false;
    datasetMessage = `已只读读取项目数据：${records.length} 条专注记录`;
    render(datasetMessage);
  } catch (error) {
    state.records = generateSampleData();
    state.source = null;
    state.readOnly = true;
    datasetMessage = `未连接只读项目数据，正在展示示例：${getErrorMessage(error)}`;
    render(datasetMessage);
  } finally {
    loadingProjectData = false;
    document.body.classList.remove('is-loading-data');
    document.body.removeAttribute('aria-busy');
    render(datasetMessage);
  }
}

function getErrorMessage(error) {
  return error instanceof Error ? error.message : String(error);
}

function render(message = datasetMessage) {
  const records = sortRecords(state.records);
  const anchorDate = getAnchorDate(records);
  const rangeDays = resolveRangeDays(state.activeRange);
  const filtered = filterByRange(records, rangeDays, anchorDate);
  const dailySeries = buildDailySeries(filtered, anchorDate, rangeDays);
  const subjectSeries = buildSubjectSeries(filtered);
  const heatmapSeries = buildHeatmapSeries(filtered);
  const weekTimeline = buildWeekTimelineSeries(records, state.weekOffset);
  const bestHours = buildMonthlyBestHours(records, state.monthOffset);
  const yearHeatmap = buildYearHeatmapSeries(records, state.yearOffset);
  const overview = buildOverview(filtered, records, anchorDate, dailySeries);
  const comparison = buildComparison(records, anchorDate, rangeDays);
  const timeWindowSeries = buildTimeWindowSeries(filtered);
  const taskFulfillment = buildTaskFulfillment(filtered, dailySeries, subjectSeries);
  const subjectGaps = buildSubjectGapSeries(filtered, records, rangeDays, anchorDate);
  const lowEfficiencyDays = detectLowEfficiencyDays(dailySeries);
  const rhythm = buildRhythmAdvice(dailySeries, rangeDays);
  const quality = buildQualityQuadrants(dailySeries);
  const dailySnapshot = buildDailySnapshot(dailySeries);
  const target = buildTargetProgress(dailySeries);
  const learningTrend = DASHBOARD_ANALYTICS.buildLearningTrend(dailySeries);
  const risks = buildRiskItems(subjectGaps, taskFulfillment, lowEfficiencyDays, rhythm);
  const dataQuality = buildDataQuality(filtered, dailySeries, subjectSeries, taskFulfillment, rhythm);
  const prescription = buildPrescription({
    overview,
    comparison,
    subjectSeries,
    taskFulfillment,
    subjectGaps,
    lowEfficiencyDays,
    rhythm,
    timeWindowSeries,
    target,
    dataQuality,
  });
  const insights = buildInsights({
    overview,
    subjectSeries,
    heatmapSeries,
    comparison,
    rangeDays,
    taskFulfillment,
    subjectGaps,
    lowEfficiencyDays,
    rhythm,
    timeWindowSeries,
    dataQuality,
  });
  const summary = buildExecutiveSummary({
    overview,
    comparison,
    risks,
    prescription,
    dataQuality,
    filteredCount: filtered.length,
  });

  syncRangeButtons();
  renderExecutiveSummary(summary);
  renderMetrics(overview, anchorDate, filtered.length);
  renderLearningTrend(learningTrend);
  renderTargetProgress(target);
  renderComparisonPanel(comparison);
  renderTaskFunnel(taskFulfillment);
  renderRiskList(risks);
  renderWeekTimeline(weekTimeline);
  renderBestHourChart(bestHours);
  renderYearHeatmap(yearHeatmap);
  renderDailySnapshot(dailySnapshot);
  renderTrendChart(dailySeries);
  renderQualityChart(quality);
  renderQuadrantSummary(quality);
  renderConsistencyGrid(dailySeries);
  renderSubjectChart(subjectSeries);
  renderHeatmapChart(heatmapSeries);
  renderSubjectTable(subjectSeries);
  renderSubjectFilter(records);
  renderRecordFilterButtons(filtered);
  renderSessionTable(applyRecordFilters(filtered));
  renderInsights(insights);
  renderPrescription(prescription);
  renderWindowList(timeWindowSeries);
  renderDataQuality(dataQuality);

  syncThemeButtons();
  els.datasetStatus.textContent = message;
  els.datasetMeta.textContent = `${records.length} 条记录 · ${countSubjects(records)} 个科目`;
  els.rangeLabel.textContent = RANGE_LABELS[state.activeRange] || '自定义范围';
  els.statusLine.textContent = buildFooterStatus(overview, anchorDate, filtered.length, comparison);
  renderDataSourcePanel();
}

function syncRangeButtons() {
  for (const button of els.segmentButtons) {
    const active = button.dataset.range === String(state.activeRange);
    button.classList.toggle('is-active', active);
    button.setAttribute('aria-pressed', String(active));
  }
}

function applyTheme(theme) {
  const nextTheme = THEME_LABELS[theme] ? theme : 'console';
  state.activeTheme = nextTheme;
  document.body.dataset.theme = nextTheme;
}

function syncThemeButtons() {
  for (const button of els.themeButtons) {
    const active = button.dataset.theme === state.activeTheme;
    button.classList.toggle('is-active', active);
    button.setAttribute('aria-pressed', String(active));
  }
}

function renderMetrics(overview, anchorDate, filteredCount) {
  const { totalMinutes, avgFocus, taskRate, streak, bestWindow, activeDays } = overview;
  els.metricHours.textContent = formatHours(totalMinutes);
  els.metricHoursNote.textContent = `${filteredCount} 条记录，截止 ${formatDateLabel(anchorDate)}`;
  els.metricFocus.textContent = formatNumber(avgFocus, 1);
  els.metricFocusNote.textContent = '按天综合：质量、时长、任务、打断';
  els.metricTaskRate.textContent = `${formatNumber(taskRate * 100, 0)}%`;
  els.metricTaskNote.textContent = `${overview.tasksDone} / ${overview.tasksTotal}`;
  els.metricStreak.textContent = String(streak);
  els.metricStreakNote.textContent = '连续有学习记录的天数';
  els.metricWindow.textContent = bestWindow || '--';
  els.metricWindowNote.textContent = overview.bestWindowNote || '根据会话质量和学习时长综合判断';
  els.metricDays.textContent = String(activeDays);
  els.metricDaysNote.textContent = '有学习记录的日期数';
}

function renderExecutiveSummary(summary) {
  els.summaryHeadline.textContent = summary.headline;
  els.summaryBody.textContent = summary.body;
  els.summaryTrend.textContent = summary.trend;
  els.summaryRisk.textContent = summary.risk;
  els.summaryAction.textContent = summary.action;
}

function renderLearningTrend(trend) {
  const status = trend?.status || 'insufficient';
  const statusClass = trendStatusClass(status);
  const current = trend?.current || null;

  syncTrendStatusClass(els.learningTrendVerdict, statusClass);
  syncTrendStatusClass(els.learningTrendDelta, statusClass);
  els.learningTrendVerdict.textContent = trend?.label || '样本不足';
  els.learningTrendDelta.textContent =
    status === 'insufficient' ? trend?.windowLabel || '至少需要 6 天' : `指数 ${formatSignedNumber(trend.delta, 1)}`;
  els.learningTrendWindow.textContent = trend?.windowLabel || '--';
  els.learningTrendEffective.textContent = current ? `${current.effectiveDays} / ${current.totalDays} 天` : '--';
  els.learningTrendMinutes.textContent = current ? formatMinutesLabel(current.avgMinutes) : '--';
  els.learningTrendFocus.textContent = current ? formatNumber(current.avgFocus, 1) : '--';

  renderLearningTrendChart(trend);
}

function renderLearningTrendChart(trend) {
  clearSvg(els.learningTrendChart);
  const width = 960;
  const height = 320;
  const margin = { top: 24, right: 76, bottom: 50, left: 54 };
  const plotWidth = width - margin.left - margin.right;
  const bottom = height - margin.bottom;
  const right = width - margin.right;
  const points = Array.isArray(trend?.points) ? trend.points : [];
  const focusColor = cssVar('--chart-focus', '#efb35b');
  const volumeColor = cssVar('--chart-minutes', 'rgba(70, 211, 178, 0.78)');
  const windowFill = cssVar('--trend-window-fill', 'rgba(70, 211, 178, 0.08)');
  const targetColor = cssVar('--axis-line', 'rgba(255,255,255,0.18)');

  setSvgViewBox(els.learningTrendChart, width, height);
  els.learningTrendChart.setAttribute('aria-label', `学习趋势判断：${trend?.label || '样本不足'}`);

  if (!points.length) {
    addText(els.learningTrendChart, width / 2, height / 2, '暂无趋势数据', {
      fill: 'var(--muted)',
      'text-anchor': 'middle',
      'font-size': '18',
    });
    return;
  }

  const step = points.length > 1 ? plotWidth / (points.length - 1) : 0;
  const xFor = (index) => margin.left + index * step;
  const yFor = (value) => scale(value, 0, 110, bottom, margin.top);

  if (trend?.current && points.length > 1) {
    const startX = Math.max(margin.left, xFor(trend.current.startIndex) - step / 2);
    const endX = Math.min(right, xFor(trend.current.endIndex) + step / 2);
    addRect(els.learningTrendChart, startX, margin.top, Math.max(2, endX - startX), bottom - margin.top, {
      fill: windowFill,
      stroke: 'none',
    });
  }

  drawChartGrid(els.learningTrendChart, margin, width, height, 5);
  renderLearningTrendAxes(els.learningTrendChart, margin, width, height);

  addLine(els.learningTrendChart, margin.left, yFor(DASHBOARD_ANALYTICS.EFFECTIVE_DAY_SCORE), right, yFor(DASHBOARD_ANALYTICS.EFFECTIVE_DAY_SCORE), {
    stroke: targetColor,
    'stroke-width': 1.2,
    'stroke-dasharray': '7 7',
  });
  addText(els.learningTrendChart, right + 8, yFor(DASHBOARD_ANALYTICS.EFFECTIVE_DAY_SCORE) + 4, '60 分', {
    fill: 'var(--muted)',
    'font-size': '12',
  });
  addLine(els.learningTrendChart, margin.left, yFor(100), right, yFor(100), {
    stroke: targetColor,
    'stroke-width': 1.2,
    'stroke-dasharray': '3 7',
  });
  addText(els.learningTrendChart, right + 8, yFor(100) + 4, '3h', {
    fill: 'var(--muted)',
    'font-size': '12',
  });

  const focusLinePoints = points.map((point, index) => ({
    x: xFor(index),
    y: yFor(point.rollingFocus),
    item: point,
  }));
  const volumeLinePoints = points.map((point, index) => ({
    x: xFor(index),
    y: yFor(point.rollingVolumeRate),
    item: point,
  }));

  drawLineSeries(els.learningTrendChart, volumeLinePoints, {
    stroke: volumeColor,
    'stroke-width': 2.6,
    'stroke-dasharray': '8 6',
    'stroke-linecap': 'round',
    'stroke-linejoin': 'round',
  });
  drawLineSeries(els.learningTrendChart, focusLinePoints, {
    stroke: focusColor,
    'stroke-width': 3.2,
    'stroke-linecap': 'round',
    'stroke-linejoin': 'round',
  });

  for (const [index, point] of points.entries()) {
    const x = xFor(index);
    const title = `${formatDateLabel(point.date)}：${formatMinutesLabel(point.minutes)}，日有效度 ${formatNumber(point.dailyFocusScore, 1)}，${point.effective ? '达标' : '未达标'}，7日均有效度 ${formatNumber(point.rollingFocus, 1)}`;
    addCircle(els.learningTrendChart, x, yFor(point.rollingVolumeRate), 2.7, {
      fill: volumeColor,
      stroke: 'var(--panel)',
      'stroke-width': 1,
    }, title);
    addCircle(els.learningTrendChart, x, yFor(point.rollingFocus), point.inCurrentWindow ? 4 : 3, {
      fill: focusColor,
      stroke: 'var(--panel)',
      'stroke-width': 1.2,
    }, title);
  }

  drawXLabels(els.learningTrendChart, points, margin, width, height);
}

function renderLearningTrendAxes(svg, margin, width, height) {
  const bottom = height - margin.bottom;
  const right = width - margin.right;
  const axisColor = cssVar('--axis-line', 'rgba(255,255,255,0.18)');
  const mutedColor = cssVar('--muted', '#8d9a93');

  addLine(svg, margin.left, bottom, right, bottom, {
    stroke: axisColor,
    'stroke-width': 1,
  });
  addLine(svg, margin.left, margin.top, margin.left, bottom, {
    stroke: axisColor,
    'stroke-width': 1,
  });

  for (const value of [0, 50, 100]) {
    const y = scale(value, 0, 110, bottom, margin.top);
    addText(svg, margin.left - 12, y + 4, String(value), {
      fill: mutedColor,
      'text-anchor': 'end',
      'font-size': '12',
    });
  }
}

function trendStatusClass(status) {
  if (status === 'up') return 'trend-delta-up';
  if (status === 'down') return 'trend-delta-down';
  if (status === 'flat') return 'trend-delta-flat';
  return 'trend-delta-insufficient';
}

function syncTrendStatusClass(element, statusClass) {
  element.classList.remove('trend-delta-up', 'trend-delta-down', 'trend-delta-flat', 'trend-delta-insufficient');
  element.classList.add(statusClass);
}

function renderTrendChart(dailySeries) {
  clearSvg(els.trendChart);
  const width = 960;
  const height = 320;
  const margin = { top: 20, right: 68, bottom: 54, left: 52 };
  const plotWidth = width - margin.left - margin.right;
  const plotHeight = height - margin.top - margin.bottom;
  const days = dailySeries.length;
  const minutesColor = cssVar('--chart-minutes', 'rgba(70, 211, 178, 0.78)');
  const minutesStroke = cssVar('--chart-minutes-stroke', 'rgba(70, 211, 178, 0.24)');
  const focusColor = cssVar('--chart-focus', '#efb35b');
  const pointStroke = cssVar('--chart-point-stroke', 'rgba(255, 255, 255, 0.2)');

  setSvgViewBox(els.trendChart, width, height);
  if (!days) {
    addText(els.trendChart, width / 2, height / 2, '暂无数据', {
      fill: 'var(--muted)',
      'text-anchor': 'middle',
      'font-size': '18',
    });
    return;
  }

  const maxMinutes = Math.max(60, ...dailySeries.map((item) => item.minutes));
  const step = days > 1 ? plotWidth / (days - 1) : plotWidth;
  const barWidth = Math.max(12, Math.min(22, step * 0.55));

  drawChartGrid(els.trendChart, margin, width, height, 5);
  renderTrendAxes(els.trendChart, margin, width, height, maxMinutes);

  const minutesPoints = [];
  const focusPoints = [];
  for (const [index, item] of dailySeries.entries()) {
    const x = margin.left + index * step;
    const minuteY = scale(item.minutes, 0, maxMinutes, height - margin.bottom, margin.top);
    const focusY =
      item.dailyFocusScore == null ? null : scale(item.dailyFocusScore, 0, 100, height - margin.bottom, margin.top);
    minutesPoints.push({ x, y: minuteY, item });
    focusPoints.push({ x, y: focusY, item });

    const barHeight = height - margin.bottom - minuteY;
    addRect(
      els.trendChart,
      x - barWidth / 2,
      minuteY,
      barWidth,
      Math.max(0, barHeight),
      {
        rx: 6,
        fill: minutesColor,
        stroke: minutesStroke,
      },
      `${formatDateLabel(item.date)}：${item.minutes} 分钟`,
    );

    if (item.dailyFocusScore != null) {
      addCircle(
        els.trendChart,
        x,
        focusY,
        4.2,
        {
          fill: focusColor,
          stroke: pointStroke,
          'stroke-width': 1,
        },
        `${formatDateLabel(item.date)}：日有效度 ${formatNumber(item.dailyFocusScore, 1)}`,
      );
    }
  }

  drawLineSeries(
    els.trendChart,
    focusPoints.filter((point) => point.y != null),
    {
      stroke: focusColor,
      'stroke-width': 3,
    },
  );

  drawXLabels(els.trendChart, dailySeries, margin, width, height);
}

function renderSubjectChart(subjectSeries) {
  clearSvg(els.subjectChart);
  const width = 960;
  const height = 280;
  const margin = { top: 18, right: 38, bottom: 16, left: 180 };
  const rowHeight = subjectSeries.length ? (height - margin.top - margin.bottom) / subjectSeries.length : 0;
  const barWidth = width - margin.left - margin.right;
  const maxMinutes = Math.max(1, ...subjectSeries.map((item) => item.minutes));
  const trackColor = cssVar('--chart-track', 'rgba(255,255,255,0.04)');
  const barStroke = cssVar('--chart-bar-stroke', 'rgba(255,255,255,0.12)');

  setSvgViewBox(els.subjectChart, width, height);
  if (!subjectSeries.length) {
    addText(els.subjectChart, width / 2, height / 2, '暂无科目分布', {
      fill: 'var(--muted)',
      'text-anchor': 'middle',
      'font-size': '18',
    });
    return;
  }

  subjectSeries.forEach((item, index) => {
    const y = margin.top + index * rowHeight;
    const barY = y + 10;
    const barHeight = Math.max(16, rowHeight - 20);
    const widthPx = (item.minutes / maxMinutes) * barWidth;
    const color = subjectColor(index);

    addText(els.subjectChart, margin.left - 16, y + rowHeight / 2 + 5, item.subject, {
      fill: 'var(--text)',
      'text-anchor': 'end',
      'font-size': '15',
      'font-weight': '600',
    });

    addRect(els.subjectChart, margin.left, barY, barWidth, barHeight, {
      rx: 8,
      fill: trackColor,
    });

    addRect(
      els.subjectChart,
      margin.left,
      barY,
      Math.max(8, widthPx),
      barHeight,
      {
        rx: 8,
        fill: color,
        stroke: barStroke,
      },
      `${item.subject}：${item.minutes} 分钟，平均会话质量 ${formatNumber(item.avgFocus, 1)}`,
    );

    addText(
      els.subjectChart,
      width - margin.right,
      y + rowHeight / 2 + 5,
      `${item.minutes} 分钟 · ${formatNumber(item.share * 100, 0)}% · ${formatNumber(item.avgFocus, 1)} 分`,
      {
        fill: 'var(--muted)',
        'text-anchor': 'end',
        'font-size': '13',
      },
    );
  });
}

function renderHeatmapChart(heatmapSeries) {
  clearSvg(els.heatmapChart);
  const width = 960;
  const height = 240;
  const margin = { top: 34, right: 18, bottom: 20, left: 46 };
  const cellW = 32;
  const cellH = 20;
  const gap = 2;
  const startX = margin.left;
  const startY = margin.top;
  const maxHeatValue = Math.max(1, ...heatmapSeries.cells.map((cell) => heatValue(cell.minutes, cell.avgFocus)));
  const heatColors = heatPalette();
  const heatEmpty = cssVar('--heat-empty', 'rgba(255,255,255,0.05)');
  const heatStroke = cssVar('--heat-stroke', 'rgba(255,255,255,0.06)');

  setSvgViewBox(els.heatmapChart, width, height);
  if (!heatmapSeries.cells.some((cell) => cell.minutes > 0)) {
    addText(els.heatmapChart, width / 2, height / 2, '暂无热力数据', {
      fill: 'var(--muted)',
      'text-anchor': 'middle',
      'font-size': '18',
    });
    return;
  }

  heatmapSeries.hours.forEach((hour, index) => {
    if (hour % 2 !== 0) return;
    const x = startX + hour * (cellW + gap) + cellW / 2;
    addText(els.heatmapChart, x, 18, `${String(hour).padStart(2, '0')}`, {
      fill: 'var(--muted)',
      'text-anchor': 'middle',
      'font-size': '11',
    });
  });

  heatmapSeries.rows.forEach((row, rowIndex) => {
    const y = startY + rowIndex * (cellH + gap) + cellH / 2 + 4;
    addText(els.heatmapChart, 28, y, row.label, {
      fill: 'var(--muted)',
      'text-anchor': 'end',
      'font-size': '11',
    });
  });

  for (const cell of heatmapSeries.cells) {
    const x = startX + cell.hour * (cellW + gap);
    const y = startY + cell.row * (cellH + gap);
    const value = heatValue(cell.minutes, cell.avgFocus);
    const level = heatIntensityLevel(value, maxHeatValue);
    const fill = level > 0 ? heatColors[level] : heatEmpty;
    addRect(
      els.heatmapChart,
      x,
      y,
      cellW,
      cellH,
      {
        rx: 6,
        fill,
        stroke: heatStroke,
        'data-heat-level': level,
      },
      `${cell.rowLabel} ${String(cell.hour).padStart(2, '0')}:00 - ${cell.minutes} 分钟，会话质量 ${cell.avgFocus == null ? '暂无' : formatNumber(cell.avgFocus, 1)}，强度 ${HEAT_LEVEL_LABELS[level]}`,
    );
  }

  addText(els.heatmapChart, width - 14, 18, '小时', {
    fill: 'var(--muted)',
    'text-anchor': 'end',
    'font-size': '11',
  });
}

function renderWeekTimeline(series) {
  clearSvg(els.weekTimelineChart);
  const width = 960;
  const height = 360;
  const margin = { top: 48, right: 22, bottom: 46, left: 62 };
  const plotWidth = width - margin.left - margin.right;
  const plotHeight = height - margin.top - margin.bottom;
  const dayWidth = plotWidth / 7;
  const timeStart = series.timeStart ?? 0;
  const timeEnd = series.timeEnd ?? 1440;
  const gridLine = cssVar('--grid-line', 'rgba(255,255,255,0.06)');
  const axisLine = cssVar('--axis-line', 'rgba(255,255,255,0.18)');
  const columnFill = cssVar('--chart-track', 'rgba(255,255,255,0.04)');
  const strongFill = cssVar('--chart-focus', '#efb35b');
  const softFill = cssVar('--chart-minutes', 'rgba(70, 211, 178, 0.78)');
  const shallowFill = cssVar('--info', '#78a7ff');
  const weakFill = 'color-mix(in srgb, var(--danger) 56%, var(--surface))';
  const exitFill = cssVar('--danger', '#f07a6d');
  const activeFill = cssVar('--success', '#a8df70');
  const mutedFill = cssVar('--muted-2', '#63766c');

  setSvgViewBox(els.weekTimelineChart, width, height);
  els.weekTimelineLabel.textContent = series.label;
  els.weekTimelineTotal.textContent = `${formatMinutesLabel(series.totalMinutes)} · ${series.segments.length} 段`;

  for (let dayIndex = 0; dayIndex < 7; dayIndex += 1) {
    const x = margin.left + dayIndex * dayWidth;
    addRect(els.weekTimelineChart, x + 3, margin.top, dayWidth - 6, plotHeight, {
      rx: 8,
      fill: columnFill,
      opacity: dayIndex % 2 === 0 ? 0.72 : 0.42,
    });
    addLine(els.weekTimelineChart, x, margin.top, x, height - margin.bottom, {
      stroke: gridLine,
      'stroke-width': 1,
    });
    addText(els.weekTimelineChart, x + dayWidth / 2, height - 18, WEEKDAY_LABELS[dayIndex], {
      class: 'timeline-day-label',
      'text-anchor': 'middle',
    });
    const daySummary = series.days?.[dayIndex];
    const summaryText = daySummary?.active
      ? `${formatCompactMinutes(daySummary.minutes)} · ${formatNumber(daySummary.dailyFocusScore, 0)} · ${
          daySummary.effective ? '达标' : '未达'
        }`
      : '无记录';
    addText(els.weekTimelineChart, x + dayWidth / 2, 24, summaryText, {
      class: daySummary?.effective ? 'timeline-summary-label is-effective' : 'timeline-summary-label',
      'text-anchor': 'middle',
    });
  }

  addLine(els.weekTimelineChart, width - margin.right, margin.top, width - margin.right, height - margin.bottom, {
    stroke: gridLine,
    'stroke-width': 1,
  });

  for (const minute of timelineTicks(timeStart, timeEnd)) {
    const y = scale(minute, timeStart, timeEnd, margin.top, height - margin.bottom);
    addLine(els.weekTimelineChart, margin.left, y, width - margin.right, y, {
      stroke: minute === timeEnd ? axisLine : gridLine,
      'stroke-width': 1,
    });
    addText(els.weekTimelineChart, margin.left - 12, y + 4, minute === 1440 ? '24:00' : `${padHour(minute / 60)}:00`, {
      class: 'timeline-time-label',
      'text-anchor': 'end',
    });
  }

  if (!series.segments.length) {
    addText(els.weekTimelineChart, width / 2, height / 2, '这一周还没有专注记录', {
      class: 'timeline-empty-label',
    });
    return;
  }

  for (const segment of series.segments) {
    const xPadding = Math.max(7, dayWidth * 0.16);
    const x = margin.left + segment.dayIndex * dayWidth + xPadding;
    const yStart = scale(clampNumber(segment.startMinute, timeStart, timeEnd, timeStart), timeStart, timeEnd, margin.top, height - margin.bottom);
    const yEnd = scale(clampNumber(segment.endMinute, timeStart, timeEnd, timeEnd), timeStart, timeEnd, margin.top, height - margin.bottom);
    const rectHeight = Math.max(3, yEnd - yStart);
    const category = getTimelineSegmentCategory(segment, {
      activeFill,
      exitFill,
      mutedFill,
      shallowFill,
      softFill,
      strongFill,
      weakFill,
    });
    addRect(
      els.weekTimelineChart,
      x,
      yStart,
      dayWidth - xPadding * 2,
      rectHeight,
      {
        rx: 6,
        fill: category.fill,
        opacity: segment.status === 'running' ? 0.72 : 0.96,
        stroke: 'rgba(255,255,255,0.16)',
        'stroke-width': 1,
      },
      `${WEEKDAY_FULL_LABELS[segment.dayIndex]} ${formatClock(segment.start)} - ${formatClock(segment.end)} · ${escapeTitle(segment.subject)} · ${category.label} · ${formatMinutesLabel(segment.minutes)} · 会话分 ${formatNumber(segment.focusScore, 0)}`,
    );

    if (rectHeight >= 24) {
      addText(els.weekTimelineChart, x + 8, yStart + Math.min(rectHeight - 6, 17), segment.subject.slice(0, 5), {
        class: 'timeline-session-label',
      });
    }
  }
}

function getTimelineSegmentCategory(segment, palette) {
  if (segment.status === 'running') {
    return { fill: palette.activeFill, label: '进行中' };
  }
  if (segment.status === 'emergency_exited') {
    return { fill: palette.exitFill, label: '应急退出' };
  }
  if (segment.status === 'interrupted') {
    return { fill: palette.mutedFill, label: '被打断' };
  }
  if (segment.focusScore >= 80) {
    return { fill: palette.strongFill, label: '深度专注' };
  }
  if (segment.focusScore >= 65) {
    return { fill: palette.softFill, label: '稳定专注' };
  }
  if (segment.focusScore >= 45) {
    return { fill: palette.shallowFill, label: '浅层专注' };
  }
  return { fill: palette.weakFill, label: '低效专注' };
}

function timelineTicks(startMinute, endMinute) {
  const startHour = Math.floor(startMinute / 60);
  const endHour = Math.ceil(endMinute / 60);
  const span = Math.max(1, endHour - startHour);
  const step = span <= 8 ? 2 : span <= 14 ? 3 : 6;
  const ticks = [];
  for (let hour = startHour; hour <= endHour; hour += step) {
    ticks.push(Math.min(1440, hour * 60));
  }
  const finalTick = Math.min(1440, endHour * 60);
  if (!ticks.includes(finalTick)) ticks.push(finalTick);
  return ticks;
}

function renderBestHourChart(series) {
  clearSvg(els.bestHourChart);
  const width = 760;
  const height = 320;
  const margin = { top: 24, right: 18, bottom: 48, left: 50 };
  const plotWidth = width - margin.left - margin.right;
  const plotHeight = height - margin.top - margin.bottom;
  const step = plotWidth / 24;
  const barWidth = Math.max(8, Math.min(18, step * 0.58));
  const maxMinutes = Math.max(60, ...series.hours.map((item) => item.minutes));
  const gridLine = cssVar('--grid-line', 'rgba(255,255,255,0.06)');
  const trackFill = cssVar('--chart-track', 'rgba(255,255,255,0.04)');
  const topFill = cssVar('--chart-focus', '#efb35b');
  const activeFill = cssVar('--chart-minutes', 'rgba(70, 211, 178, 0.78)');
  const topHours = new Set(series.topHours.map((item) => item.hour));

  setSvgViewBox(els.bestHourChart, width, height);
  els.bestHourLabel.textContent = series.label;
  els.bestHourSummary.textContent = series.best
    ? `${series.best.label} · ${formatMinutesLabel(series.best.minutes)} · ${formatNumber(series.best.avgFocus, 1)} 分`
    : '暂无数据';

  for (let tick = 0; tick <= 3; tick += 1) {
    const ratio = tick / 3;
    const y = scale(ratio, 0, 1, height - margin.bottom, margin.top);
    const value = Math.round(maxMinutes * ratio);
    addLine(els.bestHourChart, margin.left, y, width - margin.right, y, {
      stroke: gridLine,
      'stroke-width': 1,
    });
    addText(els.bestHourChart, margin.left - 10, y + 4, formatCompactMinutes(value), {
      class: 'best-hour-axis-label',
      'text-anchor': 'end',
    });
  }

  for (const item of series.hours) {
    const x = margin.left + item.hour * step + step / 2;
    const y = scale(item.minutes, 0, maxMinutes, height - margin.bottom, margin.top);
    const barHeight = height - margin.bottom - y;

    addRect(els.bestHourChart, x - barWidth / 2, margin.top, barWidth, plotHeight, {
      rx: 6,
      fill: trackFill,
    });

    if (item.minutes > 0) {
      addRect(
        els.bestHourChart,
        x - barWidth / 2,
        y,
        barWidth,
        Math.max(3, barHeight),
        {
          rx: 6,
          fill: topHours.has(item.hour) ? topFill : activeFill,
          stroke: 'rgba(255,255,255,0.14)',
        },
        `${item.label} · ${formatMinutesLabel(item.minutes)} · 会话质量 ${formatNumber(item.avgFocus, 1)} · ${item.sessionCount} 段`,
      );
    }

    if (item.hour % 4 === 0) {
      addText(els.bestHourChart, x, height - 20, `${padHour(item.hour)}:00`, {
        class: 'best-hour-axis-label',
        'text-anchor': 'middle',
      });
    }
  }

  if (!series.totalMinutes) {
    addText(els.bestHourChart, width / 2, height / 2, '这个月还没有专注记录', {
      class: 'best-hour-empty-label',
    });
  }
}

function renderYearHeatmap(series) {
  clearSvg(els.yearHeatmapChart);
  const width = 760;
  const height = 240;
  const margin = { top: 34, right: 16, bottom: 24, left: 42 };
  const gap = 3;
  const cell = Math.max(7, Math.min(12, (width - margin.left - margin.right - (series.columns - 1) * gap) / series.columns));
  const heatColors = heatPalette();
  const trackFill = cssVar('--heat-empty', 'rgba(255,255,255,0.04)');
  const heatStroke = cssVar('--heat-stroke', 'rgba(255,255,255,0.06)');
  const maxHeatValue = Math.max(1, series.maxHeatValue || 5);

  setSvgViewBox(els.yearHeatmapChart, width, height);
  els.yearHeatmapLabel.textContent = String(series.year);
  els.yearHeatmapSummary.textContent = series.activeDays
    ? `${series.effectiveDays || 0} 天达标 / ${series.activeDays} 天有记录 · ${formatMinutesLabel(series.totalMinutes)}`
    : '暂无数据';

  for (const marker of series.monthMarkers) {
    const x = margin.left + marker.column * (cell + gap);
    addText(els.yearHeatmapChart, x, 20, marker.label, {
      class: 'year-heatmap-axis-label',
    });
  }

  for (const [rowIndex, label] of ['一', '三', '五', '日'].entries()) {
    const row = rowIndex * 2;
    const y = margin.top + row * (cell + gap) + cell / 2 + 4;
    addText(els.yearHeatmapChart, margin.left - 12, y, label, {
      class: 'year-heatmap-axis-label',
      'text-anchor': 'end',
    });
  }

  for (const day of series.days) {
    const x = margin.left + day.column * (cell + gap);
    const y = margin.top + day.row * (cell + gap);
    const level = day.heatLevel ?? heatIntensityLevel(day.heatValue ?? day.minutes, maxHeatValue);
    const fill = level > 0 ? heatColors[level] : trackFill;
    addRect(
      els.yearHeatmapChart,
      x,
      y,
      cell,
      cell,
      {
        rx: 3,
        fill,
        stroke: heatStroke,
        'data-heat-level': level,
      },
      `${day.date} · ${formatMinutesLabel(day.minutes)} · ${day.sessions} 段 · 日有效度 ${day.avgFocus == null ? '暂无' : formatNumber(day.avgFocus, 1)} · ${HEAT_LEVEL_LABELS[level]}`,
    );
  }

  if (!series.activeDays) {
    addText(els.yearHeatmapChart, width / 2, height - 36, '这一年还没有专注记录', {
      class: 'year-heatmap-empty-label',
    });
  }
}

function renderSubjectTable(subjectSeries) {
  if (!subjectSeries.length) {
    els.subjectTable.innerHTML = `<tr><td colspan="4">暂无数据</td></tr>`;
    return;
  }

  els.subjectTable.innerHTML = subjectSeries
    .map(
      (item) => `
      <tr>
        <td>${escapeHtml(item.subject)}</td>
        <td>${formatNumber(item.minutes, 0)}</td>
        <td>${formatNumber(item.share * 100, 0)}%</td>
        <td>${formatNumber(item.avgFocus, 1)}</td>
      </tr>`,
    )
    .join('');
}

function renderSessionTable(records) {
  if (!records.length) {
    const emptyMessage =
      state.subjectFilter !== 'all' || state.recordFilter !== 'all'
        ? '当前筛选没有匹配记录。'
        : '暂无学习记录；只读服务连接后会自动显示项目数据。';
    syncSessionRowsControls(0, 0);
    els.sessionTableBody.innerHTML = `<tr><td colspan="6">${emptyMessage}</td></tr>`;
    return;
  }

  const sortedRecords = records.slice().sort((a, b) => compareDateKey(b.date, a.date) || b.startHour - a.startHour);
  const visibleRecords = state.sessionRowsExpanded ? sortedRecords : sortedRecords.slice(0, SESSION_PREVIEW_LIMIT);

  syncSessionRowsControls(sortedRecords.length, visibleRecords.length);

  els.sessionTableBody.innerHTML = visibleRecords
    .map((record) => {
      const completion = `${Math.min(record.tasksDone, record.tasksTotal)}/${record.tasksTotal || 0}`;
      return `
        <tr>
          <td>${formatDateLabel(record.date)}</td>
          <td>${escapeHtml(record.subject)}</td>
          <td>${formatSessionWindow(record)}</td>
          <td>${formatNumber(record.minutes, 0)}</td>
          <td>${formatNumber(record.focusScore, 0)}</td>
          <td>${completion}</td>
        </tr>`;
    })
    .join('');
}

function syncSessionRowsControls(total, visible) {
  const expandable = total > SESSION_PREVIEW_LIMIT;
  els.sessionTableCount.textContent = total ? `${visible} / ${total} 条` : '0 条';
  els.toggleSessionRows.hidden = !expandable;
  els.toggleSessionRows.textContent = state.sessionRowsExpanded ? '收起' : '展开全部';
  els.toggleSessionRows.setAttribute('aria-expanded', String(state.sessionRowsExpanded));
}

function renderTargetProgress(target) {
  els.targetProgressValue.textContent = target.label;
  els.targetProgressBar.style.width = `${formatNumber(Math.min(1, target.percent) * 100, 0)}%`;
  els.targetProgressMeta.textContent = `${formatNumber(target.percent * 100, 0)}% · ${target.standard}`;
}

function renderComparisonPanel(comparison) {
  if (!comparison) {
    els.comparisonSummary.textContent = '暂无';
    els.comparisonList.innerHTML = `<span class="delta-item">需要至少两个周期的数据</span>`;
    return;
  }

  const tone = comparison.minuteDelta >= 0 ? '向上' : '回落';
  els.comparisonSummary.textContent = `${tone} ${formatSignedPercent(comparison.minuteDelta || 0)}`;
  els.comparisonList.innerHTML = [
    ['时长', comparison.minuteDelta == null ? '--' : formatSignedPercent(comparison.minuteDelta)],
    ['日有效度', `${formatSignedNumber(comparison.focusDelta, 1)} 分`],
    ['任务', formatSignedPercent(comparison.taskRateDelta)],
  ]
    .map(
      ([label, value]) => `
      <span class="delta-item">
        <small>${label}</small>
        <strong>${value}</strong>
      </span>`,
    )
    .join('');
}

function renderTaskFunnel(taskFulfillment) {
  els.taskFunnelSummary.textContent = `${formatNumber(taskFulfillment.rate * 100, 0)}%`;
  const rows = [
    { label: '已完成', value: taskFulfillment.done, total: taskFulfillment.total, className: 'is-done' },
    { label: '欠账', value: taskFulfillment.debt, total: taskFulfillment.total, className: 'is-debt' },
    {
      label: '无计划记录',
      value: taskFulfillment.unplannedSessions,
      total: taskFulfillment.sessionCount,
      className: 'is-muted',
    },
  ];

  els.taskFunnelBars.innerHTML = rows
    .map((row) => {
      const ratio = row.total > 0 ? row.value / row.total : 0;
      return `
        <div class="funnel-row ${row.className}">
          <div class="funnel-label">
            <span>${row.label}</span>
            <strong>${formatNumber(row.value, 0)}</strong>
          </div>
          <div class="funnel-track"><span style="width: ${formatNumber(Math.min(1, ratio) * 100, 0)}%"></span></div>
        </div>`;
    })
    .join('');
}

function renderRiskList(risks) {
  els.riskSummary.textContent = risks.length ? `${risks.length} 项` : '稳定';
  if (!risks.length) {
    els.riskList.innerHTML = `<li>当前周期没有明显短板，继续保持记录密度。</li>`;
    return;
  }

  els.riskList.innerHTML = risks
    .slice(0, 4)
    .map((item) => `<li><strong>${escapeHtml(item.title)}</strong><span>${escapeHtml(item.body)}</span></li>`)
    .join('');
}

function renderDailySnapshot(snapshot) {
  if (!snapshot.length) {
    els.dailySnapshot.innerHTML = `<div class="snapshot-empty">暂无足够日数据，先积累几天记录再看快照。</div>`;
    return;
  }

  els.dailySnapshot.innerHTML = snapshot
    .map(
      (item) => `
      <article class="snapshot-card ${escapeHtml(item.tone)}">
        <div class="snapshot-head">
          <span>${escapeHtml(item.label)}</span>
          <strong>${escapeHtml(formatDateLabel(item.date))}</strong>
        </div>
        <div class="snapshot-value">
          <strong>${escapeHtml(item.value)}</strong>
          <span>${escapeHtml(item.meta)}</span>
        </div>
        <div class="snapshot-meter" aria-hidden="true"><span style="width: ${formatNumber(item.level, 0)}%"></span></div>
        <p>${escapeHtml(item.note)}</p>
      </article>`,
    )
    .join('');
}

function renderQualityChart(quality) {
  clearSvg(els.qualityChart);
  const width = 720;
  const height = 360;
  const margin = { top: 24, right: 28, bottom: 44, left: 54 };
  const plotWidth = width - margin.left - margin.right;
  const plotHeight = height - margin.top - margin.bottom;
  const right = width - margin.right;
  const bottom = height - margin.bottom;
  const axisLine = cssVar('--axis-line', 'rgba(255,255,255,0.16)');
  const pointStroke = cssVar('--chart-point-stroke', 'rgba(255,255,255,0.18)');
  const faintText = cssVar('--chart-faint', 'rgba(255,255,255,0.28)');

  setSvgViewBox(els.qualityChart, width, height);
  drawChartGrid(els.qualityChart, margin, width, height, 4);

  addLine(
    els.qualityChart,
    scale(quality.minuteThreshold, 0, quality.maxMinutes, margin.left, right),
    margin.top,
    scale(quality.minuteThreshold, 0, quality.maxMinutes, margin.left, right),
    bottom,
    {
      stroke: axisLine,
      'stroke-dasharray': '6 6',
    },
  );
  addLine(
    els.qualityChart,
    margin.left,
    scale(quality.focusThreshold, 0, 100, bottom, margin.top),
    right,
    scale(quality.focusThreshold, 0, 100, bottom, margin.top),
    {
      stroke: axisLine,
      'stroke-dasharray': '6 6',
    },
  );

  if (!quality.points.length) {
    addText(els.qualityChart, width / 2, height / 2, '暂无质量数据', {
      fill: 'var(--muted)',
      'text-anchor': 'middle',
      'font-size': '18',
    });
    return;
  }

  for (const point of quality.points) {
    const x = scale(point.minutes, 0, quality.maxMinutes, margin.left, right);
    const y = scale(point.focus, 0, 100, bottom, margin.top);
    const radius = Math.max(4, Math.min(12, 4 + point.sessions * 1.8));
    addCircle(
      els.qualityChart,
      x,
      y,
      radius,
      {
        fill: point.color,
        stroke: pointStroke,
        'stroke-width': 1,
        opacity: 0.92,
      },
      `${formatDateLabel(point.date)}：${point.minutes} 分钟，日有效度 ${formatNumber(point.focus, 1)}，${point.label}`,
    );
  }

  addText(els.qualityChart, margin.left, height - 14, '学习分钟', {
    fill: 'var(--muted)',
    'font-size': '12',
  });
  addText(els.qualityChart, right, height - 14, `${formatNumber(quality.maxMinutes, 0)} 分钟`, {
    fill: 'var(--muted)',
    'font-size': '12',
    'text-anchor': 'end',
  });
  addText(els.qualityChart, 16, margin.top + 4, '日有效度', {
    fill: 'var(--muted)',
    'font-size': '12',
  });
  addText(els.qualityChart, margin.left + plotWidth - 4, margin.top + plotHeight - 8, '低量', {
    fill: faintText,
    'font-size': '12',
    'text-anchor': 'end',
  });
}

function renderQuadrantSummary(quality) {
  els.quadrantSummary.innerHTML = quality.quadrants
    .map(
      (item) => `
      <div class="quadrant-card">
        <span class="quadrant-dot" style="background: ${item.color}"></span>
        <strong>${item.count}</strong>
        <span>${item.label}</span>
      </div>`,
    )
    .join('');
}

function renderConsistencyGrid(dailySeries) {
  if (!dailySeries.length) {
    els.consistencyGrid.innerHTML = `<span class="empty-state">暂无连续性数据</span>`;
    return;
  }

  els.consistencyGrid.innerHTML = dailySeries
    .map((item) => {
      const activeClass = item.active ? 'is-active' : 'is-empty';
      const level = item.heatLevel ?? DASHBOARD_ANALYTICS.getAnnualHeatLevel(item);
      const qualityClass =
        level >= 5
          ? 'is-peak'
          : level >= 4
            ? 'is-high'
            : level >= 3
              ? 'is-mid'
              : level >= 2
                ? 'is-under'
                : level >= 1
                  ? 'is-low'
                  : 'is-empty';
      const taskLabel = item.tasksTotal ? `${item.tasksDone}/${item.tasksTotal}` : '无计划';
      const title = `${formatDateLabel(item.date)}：${item.minutes} 分钟，日有效度 ${
        item.dailyFocusScore == null ? '暂无' : formatNumber(item.dailyFocusScore, 1)
      }，${item.effective ? '达标' : '未达标'}，任务 ${taskLabel}`;
      return `<span class="consistency-cell ${activeClass} ${qualityClass}" data-heat-level="${level}" title="${escapeHtml(title)}" aria-label="${escapeHtml(title)}"></span>`;
    })
    .join('');
}

function renderSubjectFilter(records) {
  const subjects = [...new Set(records.map((item) => item.subject))].sort((a, b) => a.localeCompare(b, 'zh-Hans-CN'));
  if (!subjects.includes(state.subjectFilter)) {
    state.subjectFilter = 'all';
  }

  els.subjectFilter.innerHTML = [
    `<option value="all">全部科目</option>`,
    ...subjects.map(
      (subject) =>
        `<option value="${escapeHtml(subject)}"${subject === state.subjectFilter ? ' selected' : ''}>${escapeHtml(subject)}</option>`,
    ),
  ].join('');
  els.subjectFilter.value = state.subjectFilter;
}

function renderRecordFilterButtons(records) {
  const scopedRecords = records.filter(
    (record) => state.subjectFilter === 'all' || record.subject === state.subjectFilter,
  );
  const counts = {
    all: scopedRecords.length,
    'low-focus': scopedRecords.filter((record) => record.focusScore < 65).length,
    'task-debt': scopedRecords.filter((record) => record.tasksTotal > record.tasksDone).length,
    'long-session': scopedRecords.filter((record) => record.minutes >= 120).length,
  };

  for (const button of els.recordFilterButtons) {
    const key = button.dataset.recordFilter || 'all';
    const active = key === state.recordFilter;
    button.classList.toggle('is-active', active);
    button.setAttribute('aria-pressed', String(active));
    button.textContent = `${RECORD_FILTER_LABELS[key] || key} ${counts[key] ?? 0}`;
  }
}

function applyRecordFilters(records) {
  return records.filter((record) => {
    if (state.subjectFilter !== 'all' && record.subject !== state.subjectFilter) return false;
    if (state.recordFilter === 'low-focus') return record.focusScore < 65;
    if (state.recordFilter === 'task-debt') return record.tasksTotal > record.tasksDone;
    if (state.recordFilter === 'long-session') return record.minutes >= 120;
    return true;
  });
}

function renderWindowList(timeWindowSeries) {
  if (!timeWindowSeries.length) {
    els.windowList.innerHTML = `<li>暂无足够的时段数据。</li>`;
    return;
  }

  els.windowList.innerHTML = timeWindowSeries
    .slice(0, 5)
    .map(
      (item, index) => `
      <li>
        <span class="rank">${index + 1}</span>
        <div>
          <strong>${item.label}</strong>
          <span>${formatHours(item.minutes)} · 会话质量 ${formatNumber(item.avgFocus, 1)} · ${formatNumber(item.sessionCount, 0)} 次</span>
        </div>
      </li>`,
    )
    .join('');
}

function renderInsights(insights) {
  if (!insights.length) {
    els.insightsList.innerHTML = `<li><div class="insight-title">暂无洞察</div><div class="insight-body">项目数据库积累更多记录后，这里会自动生成分析结论。</div></li>`;
    return;
  }

  els.insightsList.innerHTML = insights
    .map(
      (item) => `
      <li>
        <div class="insight-title">${escapeHtml(item.title)}</div>
        <div class="insight-body">${escapeHtml(item.body)}</div>
      </li>`,
    )
    .join('');
}

function renderPrescription(prescription) {
  els.prescriptionSummary.innerHTML = `
    <strong>${escapeHtml(prescription.headline)}</strong>
    <span>${escapeHtml(prescription.meta)}</span>`;

  if (!prescription.items.length) {
    els.prescriptionList.innerHTML = `<li><strong>继续积累记录</strong><span>样本变厚后，这里会自动生成下一轮安排。</span></li>`;
    return;
  }

  els.prescriptionList.innerHTML = prescription.items
    .map(
      (item) => `
      <li class="${escapeHtml(item.tone)}">
        <strong>${escapeHtml(item.title)}</strong>
        <span>${escapeHtml(item.body)}</span>
        <small>${escapeHtml(item.meta)}</small>
      </li>`,
    )
    .join('');
}

function renderDataQuality(dataQuality) {
  els.dataQualityCard.innerHTML = `
    <div>
      <strong>${formatNumber(dataQuality.score, 0)}</strong>
      <span>${escapeHtml(dataQuality.label)}</span>
    </div>
    <div class="quality-meter" aria-hidden="true"><span style="width: ${formatNumber(dataQuality.score, 0)}%"></span></div>
    <p>${escapeHtml(dataQuality.summary)}</p>`;

  els.dataQualityList.innerHTML = dataQuality.checks
    .map(
      (item) => `
      <li class="${escapeHtml(item.status)}">
        <strong>${escapeHtml(item.title)} · ${escapeHtml(item.value)}</strong>
        <span>${escapeHtml(item.body)}</span>
      </li>`,
    )
    .join('');
}

function renderDataSourcePanel() {
  els.refreshProjectData.disabled = loadingProjectData;
  els.refreshProjectData.textContent = loadingProjectData ? '正在只读读取...' : '重新读取项目数据';

  if (loadingProjectData) {
    els.dataSourceStatus.textContent = '正在只读读取项目数据库...';
    els.dataSourcePath.textContent = state.source?.path || '正在定位 kaoyan-focus.sqlite3';
    els.dataSourceMeta.textContent = 'SQLite 以 mode=ro 打开；不会写入本地数据库';
    return;
  }

  if (!state.source) {
    els.dataSourceStatus.textContent = '未连接只读服务';
    els.dataSourcePath.textContent = '未读取到项目数据库';
    els.dataSourceMeta.textContent = '正在展示示例数据；本看板不会写入本地数据';
    return;
  }

  const source = state.source;
  els.dataSourceStatus.textContent = state.readOnly ? '只读已连接' : '连接状态异常';
  els.dataSourcePath.textContent = source.path || '项目 SQLite 数据库';

  const meta = [];
  if (source.lastModified) meta.push(`更新于 ${formatDateTime(source.lastModified)}`);
  if (source.bytes) meta.push(formatBytes(source.bytes));
  if (Number.isFinite(Number(source.subjectCount))) meta.push(`${source.subjectCount} 个科目`);
  if (Number.isFinite(Number(source.taskCount))) meta.push(`${source.taskCount} 条计划项`);
  meta.push('mode=ro');
  els.dataSourceMeta.textContent = meta.join(' · ');
}

function buildOverview(filtered, allRecords, anchorDate, dailySeries = null) {
  const totalMinutes = sum(filtered, (item) => item.minutes);
  const scopedDailySeries =
    dailySeries || buildDailySeries(filtered, anchorDate, filtered.length ? daysBetween(getEarliestDate(filtered), anchorDate) + 1 : 0);
  const avgFocus = averageDailyFocusScore(scopedDailySeries);
  const tasksDone = sum(filtered, (item) => Math.min(item.tasksDone, item.tasksTotal));
  const tasksTotal = sum(filtered, (item) => item.tasksTotal);
  const taskRate = tasksTotal > 0 ? tasksDone / tasksTotal : 0;
  const activeDays = uniqueCount(filtered, (item) => item.date);
  const streak = computeStreak(allRecords, anchorDate);
  const bestWindow = computeBestWindow(filtered);
  const bestWindowNote = bestWindow ? '综合会话质量与分钟数推算' : '暂无足够数据';

  return {
    totalMinutes,
    avgFocus,
    tasksDone,
    tasksTotal,
    taskRate,
    activeDays,
    streak,
    bestWindow,
    bestWindowNote,
  };
}

function buildComparison(records, anchorDate, rangeDays) {
  const currentDays = resolveComparisonDays(records, rangeDays);
  if (!currentDays) return null;

  const current = filterByRange(records, currentDays, anchorDate);
  const previousAnchor = addDays(anchorDate, -currentDays);
  const previous = filterByRange(records, currentDays, previousAnchor);

  if (!current.length || !previous.length) return null;

  const currentMinutes = sum(current, (item) => item.minutes);
  const previousMinutes = sum(previous, (item) => item.minutes);
  const currentDailySeries = buildDailySeries(current, anchorDate, currentDays);
  const previousDailySeries = buildDailySeries(previous, previousAnchor, currentDays);
  const currentFocus = averageDailyFocusScore(currentDailySeries);
  const previousFocus = averageDailyFocusScore(previousDailySeries);
  const currentTaskRate = buildOverview(current, records, anchorDate, currentDailySeries).taskRate;
  const previousTaskRate = buildOverview(previous, records, previousAnchor, previousDailySeries).taskRate;

  return {
    days: currentDays,
    minuteDelta: previousMinutes ? (currentMinutes - previousMinutes) / previousMinutes : null,
    focusDelta: currentFocus - previousFocus,
    taskRateDelta: currentTaskRate - previousTaskRate,
  };
}

function buildTimeWindowSeries(records) {
  const buckets = new Map();

  for (const record of records) {
    for (const segment of splitRecordByHour(record)) {
      const bucket = buckets.get(segment.hour) || {
        hour: segment.hour,
        minutes: 0,
        focusWeighted: 0,
        sessionIds: new Set(),
      };
      bucket.minutes += segment.minutes;
      bucket.focusWeighted += segment.minutes * record.focusScore;
      bucket.sessionIds.add(record.id || `${record.date}-${record.subject}-${record.startHour}-${record.minutes}`);
      buckets.set(segment.hour, bucket);
    }
  }

  return [...buckets.values()]
    .filter((bucket) => bucket.minutes > 0)
    .map((bucket) => {
      const avgFocus = bucket.focusWeighted / bucket.minutes;
      const score = avgFocus * 1.25 + Math.log(bucket.minutes + 1) * 7 + bucket.sessionIds.size * 0.8;
      const endHour = Math.min(24, bucket.hour + 2);
      return {
        hour: bucket.hour,
        label: `${padHour(bucket.hour)}:00 - ${padHour(endHour)}:00`,
        minutes: bucket.minutes,
        avgFocus,
        sessionCount: bucket.sessionIds.size,
        score,
      };
    })
    .sort((a, b) => b.score - a.score || b.minutes - a.minutes || a.hour - b.hour);
}

function buildWeekTimelineSeries(records, weekOffset) {
  const start = startOfWeek(addDays(new Date(), weekOffset * 7));
  const end = addDays(start, 7);
  const segments = [];
  const dayBuckets = Array.from({ length: 7 }, () => new Map());

  for (const record of DASHBOARD_ANALYTICS.filterFocusTimelineRecords(records)) {
    for (const segment of splitRecordByClock(record)) {
      if (segment.end <= start || segment.start >= end) continue;
      const clippedStart = maxDate(segment.start, start);
      const clippedEnd = minDate(segment.end, end);
      if (clippedEnd <= clippedStart) continue;

      const date = toDateKey(clippedStart);
      const dayIndex = daysBetween(start, parseDateKey(date));
      if (dayIndex < 0 || dayIndex > 6) continue;

      const nextMidnight = addDays(parseDateKey(date), 1);
      const endMinute = clippedEnd >= nextMidnight ? 1440 : minuteOfDay(clippedEnd);
      const minutes = Math.max(0, (clippedEnd.getTime() - clippedStart.getTime()) / 60000);
      if (!minutes) continue;
      addClippedRecordSlice(dayBuckets[dayIndex], record, date, minutes);
      segments.push({
        record,
        dayIndex,
        date,
        subject: record.subject,
        status: record.status,
        focusScore: record.focusScore,
        start: clippedStart,
        end: clippedEnd,
        startMinute: minuteOfDay(clippedStart),
        endMinute: Math.max(minuteOfDay(clippedStart) + 1, endMinute),
        minutes,
      });
    }
  }

  const days = dayBuckets.map((bucket, dayIndex) => {
    const day = buildDailyAggregate(toDateKey(addDays(start, dayIndex)), [...bucket.values()]);
    return day;
  });
  const activeSegments = segments.filter((segment) => segment.minutes > 0);
  const minMinute = activeSegments.length ? Math.min(...activeSegments.map((segment) => segment.startMinute)) : 0;
  const maxMinute = activeSegments.length ? Math.max(...activeSegments.map((segment) => segment.endMinute)) : 1440;
  const timeStart = Math.max(0, Math.floor(Math.max(0, minMinute - 60) / 60) * 60);
  const timeEnd = Math.min(1440, Math.ceil(Math.min(1440, maxMinute + 60) / 60) * 60);

  segments.sort((a, b) => a.dayIndex - b.dayIndex || a.start - b.start);
  return {
    start,
    end,
    label: formatWeekPeriodLabel(start, weekOffset),
    totalMinutes: sum(segments, (segment) => segment.minutes),
    days,
    segments,
    timeStart,
    timeEnd: timeEnd > timeStart ? timeEnd : Math.min(1440, timeStart + 360),
  };
}

function buildMonthlyBestHours(records, monthOffset) {
  const { start, end } = monthWindow(monthOffset);
  const buckets = Array.from({ length: 24 }, (_, hour) => ({
    hour,
    label: `${padHour(hour)}:00 - ${padHour((hour + 1) % 24)}:00`,
    minutes: 0,
    focusWeighted: 0,
    sessionIds: new Set(),
  }));

  for (const record of records) {
    for (const segment of splitRecordByClock(record)) {
      if (segment.end <= start || segment.start >= end) continue;
      const clippedStart = maxDate(segment.start, start);
      const clippedEnd = minDate(segment.end, end);
      const minutes = Math.max(0, (clippedEnd.getTime() - clippedStart.getTime()) / 60000);
      if (!minutes) continue;
      const bucket = buckets[clippedStart.getHours()];
      bucket.minutes += minutes;
      bucket.focusWeighted += minutes * record.focusScore;
      bucket.sessionIds.add(record.id || `${record.date}-${record.subject}-${record.startHour}-${record.minutes}`);
    }
  }

  const hours = buckets.map((bucket) => {
    const avgFocus = bucket.minutes > 0 ? bucket.focusWeighted / bucket.minutes : 0;
    const score = avgFocus * 1.22 + Math.log(bucket.minutes + 1) * 7 + bucket.sessionIds.size * 0.9;
    return {
      hour: bucket.hour,
      label: bucket.label,
      minutes: bucket.minutes,
      avgFocus,
      sessionCount: bucket.sessionIds.size,
      score,
    };
  });
  const topHours = hours
    .filter((item) => item.minutes > 0)
    .sort((a, b) => b.score - a.score || b.minutes - a.minutes || a.hour - b.hour)
    .slice(0, 5);

  return {
    start,
    end,
    label: formatMonthPeriodLabel(start, monthOffset),
    hours,
    topHours,
    best: topHours[0] || null,
    totalMinutes: sum(hours, (item) => item.minutes),
  };
}

function buildYearHeatmapSeries(records, yearOffset) {
  const { start, end, year } = yearWindow(yearOffset);
  const buckets = buildDailySliceMap(records, start, end);

  const dailyEntries = [...buckets.entries()].map(([date, bucket]) => {
    const aggregate = buildDailyAggregate(date, [...bucket.values()]);
    return {
      date,
      minutes: aggregate.minutes,
      avgFocus: aggregate.dailyFocusScore,
      dailyFocusScore: aggregate.dailyFocusScore,
      effective: aggregate.effective,
      heatLevel: aggregate.heatLevel,
      heatValue: aggregate.heatLevel,
      sessions: bucket.size,
    };
  });
  const calendar = DASHBOARD_ANALYTICS.buildAnnualHeatmapCalendar(year, dailyEntries);
  const days = calendar.days.map((day) => ({
    ...day,
    avgFocus: day.dailyFocusScore,
    heatValue: day.heatLevel,
    sessions: day.sessions || 0,
  }));

  const monthMarkers = MONTH_LABELS.map((label, month) => {
    const monthStart = new Date(year, month, 1);
    return {
      label,
      column: Math.floor((daysBetween(start, monthStart) + mondayWeekday(start)) / 7),
    };
  });

  return {
    start,
    end,
    year,
    columns: calendar.columns,
    days,
    monthMarkers,
    activeDays: calendar.activeDays,
    effectiveDays: calendar.effectiveDays,
    totalMinutes: calendar.totalMinutes,
    maxMinutes: Math.max(0, ...days.map((day) => day.minutes)),
    maxHeatValue: calendar.maxHeatValue,
  };
}

function buildTaskFulfillment(records, dailySeries, subjectSeries) {
  const done = sum(records, (item) => Math.min(item.tasksDone, item.tasksTotal));
  const total = sum(records, (item) => item.tasksTotal);
  const debt = Math.max(0, total - done);
  const unplannedSessions = records.filter((item) => item.tasksTotal === 0).length;
  const dailyDebt = dailySeries
    .filter((item) => item.tasksTotal > item.tasksDone)
    .map((item) => ({
      date: item.date,
      debt: item.tasksTotal - item.tasksDone,
      rate: item.taskRate,
    }))
    .sort((a, b) => b.debt - a.debt || a.rate - b.rate);
  const subjectDebt = subjectSeries
    .filter((item) => item.tasksTotal > item.tasksDone)
    .map((item) => ({
      subject: item.subject,
      debt: item.tasksTotal - item.tasksDone,
      rate: item.taskRate,
    }))
    .sort((a, b) => b.debt - a.debt || a.rate - b.rate);

  return {
    done,
    total,
    debt,
    rate: total > 0 ? done / total : 1,
    unplannedSessions,
    sessionCount: records.length,
    worstDay: dailyDebt[0] || null,
    worstSubject: subjectDebt[0] || null,
  };
}

function buildSubjectGapSeries(filtered, allRecords, rangeDays, anchorDate) {
  const current = buildSubjectSeries(filtered);
  if (!current.length) return [];

  const comparisonDays = resolveComparisonDays(allRecords, rangeDays);
  const previousAnchor = comparisonDays ? addDays(anchorDate, -comparisonDays) : null;
  const previous = previousAnchor ? buildSubjectSeries(filterByRange(allRecords, comparisonDays, previousAnchor)) : [];
  const baseline = previous.length ? previous : buildSubjectSeries(allRecords);
  const baselineLabel = previous.length ? '上一周期' : '全量基线';
  const baselineMap = new Map(baseline.map((item) => [item.subject, item.share]));
  const currentMap = new Map(current.map((item) => [item.subject, item]));
  const subjects = new Set([...baselineMap.keys(), ...currentMap.keys()]);

  return [...subjects]
    .map((subject) => {
      const currentItem = currentMap.get(subject) || {
        subject,
        share: 0,
        minutes: 0,
        avgFocus: 0,
      };
      const baselineShare = baselineMap.get(subject) || 0;
      return {
        subject,
        share: currentItem.share,
        baselineShare,
        deltaShare: currentItem.share - baselineShare,
        minutes: currentItem.minutes,
        avgFocus: currentItem.avgFocus,
        baselineLabel,
      };
    })
    .sort((a, b) => a.deltaShare - b.deltaShare || a.share - b.share);
}

function detectLowEfficiencyDays(dailySeries) {
  const activeDays = dailySeries.filter((item) => item.active && item.minutes > 0 && item.avgFocus != null);
  if (activeDays.length < 3) return [];

  const medianMinutes = median(activeDays.map((item) => item.minutes));
  const medianFocus = median(activeDays.map((item) => item.avgFocus));
  const minimumMinutes = Math.max(60, medianMinutes * 0.75);
  const maximumFocus = Math.max(58, medianFocus - 7);

  return activeDays
    .filter((item) => item.minutes >= minimumMinutes && item.avgFocus <= maximumFocus)
    .map((item) => ({
      ...item,
      efficiencyScore: item.minutes ? (item.avgFocus * Math.log(item.minutes + 1)) / 10 : 0,
    }))
    .sort((a, b) => a.avgFocus - b.avgFocus || b.minutes - a.minutes);
}

function buildRhythmAdvice(dailySeries, rangeDays) {
  if (!dailySeries.length) {
    return {
      activeDays: 0,
      totalDays: 0,
      activeDensity: 0,
      longestGap: 0,
      trailingGap: 0,
      variation: 0,
      rangeDays,
    };
  }

  const activeMinutes = dailySeries.filter((item) => item.active).map((item) => item.minutes);
  const avgMinutes = average(activeMinutes);
  const variance = activeMinutes.length ? average(activeMinutes.map((minutes) => (minutes - avgMinutes) ** 2)) : 0;
  let longestGap = 0;
  let currentGap = 0;
  for (const item of dailySeries) {
    if (item.active) {
      currentGap = 0;
    } else {
      currentGap += 1;
      longestGap = Math.max(longestGap, currentGap);
    }
  }

  let trailingGap = 0;
  for (let index = dailySeries.length - 1; index >= 0; index -= 1) {
    if (dailySeries[index].active) break;
    trailingGap += 1;
  }

  return {
    activeDays: activeMinutes.length,
    totalDays: dailySeries.length,
    activeDensity: dailySeries.length ? activeMinutes.length / dailySeries.length : 0,
    longestGap,
    trailingGap,
    variation: avgMinutes > 0 ? Math.sqrt(variance) / avgMinutes : 0,
    rangeDays,
  };
}

function buildQualityQuadrants(dailySeries) {
  const activeDays = dailySeries.filter((item) => item.active && item.minutes > 0 && item.avgFocus != null);
  const maxMinutes = Math.max(60, ...activeDays.map((item) => item.minutes));
  const minuteThreshold = Math.max(60, median(activeDays.map((item) => item.minutes)));
  const focusThreshold = Math.max(68, median(activeDays.map((item) => item.avgFocus)));
  const qualityColors = qualityPalette();
  const templates = [
    { key: 'deep', label: '高效深学', color: qualityColors.deep, count: 0 },
    { key: 'grind', label: '低效硬撑', color: qualityColors.grind, count: 0 },
    { key: 'light', label: '轻量维护', color: qualityColors.light, count: 0 },
    {
      key: 'empty',
      label: '空窗风险',
      color: qualityColors.empty,
      count: Math.max(0, dailySeries.length - activeDays.length),
    },
  ];
  const quadrants = new Map(templates.map((item) => [item.key, { ...item }]));
  const points = [];

  for (const item of activeDays) {
    const highMinutes = item.minutes >= minuteThreshold;
    const highFocus = item.avgFocus >= focusThreshold;
    const key = highMinutes && highFocus ? 'deep' : highMinutes && !highFocus ? 'grind' : 'light';
    const quadrant = quadrants.get(key);
    quadrant.count += 1;
    points.push({
      date: item.date,
      minutes: item.minutes,
      focus: item.avgFocus,
      sessions: item.sessionCount,
      label: quadrant.label,
      color: quadrant.color,
    });
  }

  return {
    points,
    quadrants: [...quadrants.values()],
    maxMinutes,
    minuteThreshold,
    focusThreshold,
  };
}

function buildDailySnapshot(dailySeries) {
  const activeDays = dailySeries.filter((item) => item.active && item.minutes > 0 && item.avgFocus != null);
  if (!activeDays.length) return [];

  const minuteMedian = median(activeDays.map((item) => item.minutes));
  const focusMedian = median(activeDays.map((item) => item.avgFocus));
  const sessionMedian = median(activeDays.map((item) => item.sessionCount));
  const taskRateAverage = average(activeDays.map((item) => item.taskRate));
  const maxMinutes = Math.max(1, ...activeDays.map((item) => item.minutes));
  const maxEfficiency = Math.max(1, ...activeDays.map((item) => item.efficiencyScore));

  const strongest = [...activeDays].sort((a, b) => {
    const aScore = a.efficiencyScore + a.taskRate * 45 + a.sessionCount * 3;
    const bScore = b.efficiencyScore + b.taskRate * 45 + b.sessionCount * 3;
    return bScore - aScore || b.avgFocus - a.avgFocus || b.minutes - a.minutes;
  })[0];

  const longest = [...activeDays].sort(
    (a, b) => b.minutes - a.minutes || b.avgFocus - a.avgFocus || b.taskRate - a.taskRate,
  )[0];

  const typical = [...activeDays].sort((a, b) => {
    const aDistance =
      Math.abs(a.minutes - minuteMedian) / Math.max(minuteMedian, 1) +
      Math.abs(a.avgFocus - focusMedian) / 12 +
      Math.abs(a.taskRate - taskRateAverage) * 1.2 +
      Math.abs(a.sessionCount - sessionMedian) / 4;
    const bDistance =
      Math.abs(b.minutes - minuteMedian) / Math.max(minuteMedian, 1) +
      Math.abs(b.avgFocus - focusMedian) / 12 +
      Math.abs(b.taskRate - taskRateAverage) * 1.2 +
      Math.abs(b.sessionCount - sessionMedian) / 4;
    return aDistance - bDistance || b.minutes - a.minutes;
  })[0];

  const risky = [...activeDays].sort((a, b) => {
    const aShortfall = Math.max(0, a.tasksTotal - a.tasksDone);
    const bShortfall = Math.max(0, b.tasksTotal - b.tasksDone);
    const aScore =
      a.minutes * Math.max(0, 100 - a.avgFocus) +
      aShortfall * 900 +
      (a.tasksTotal > 0 ? Math.max(0, 0.75 - a.taskRate) * 700 : 0);
    const bScore =
      b.minutes * Math.max(0, 100 - b.avgFocus) +
      bShortfall * 900 +
      (b.tasksTotal > 0 ? Math.max(0, 0.75 - b.taskRate) * 700 : 0);
    return bScore - aScore || a.avgFocus - b.avgFocus || b.minutes - a.minutes;
  })[0];

  const riskMax = Math.max(
    1,
    ...activeDays.map((item) => {
      const shortfall = Math.max(0, item.tasksTotal - item.tasksDone);
      return (
        item.minutes * Math.max(0, 100 - item.avgFocus) +
        shortfall * 900 +
        (item.tasksTotal > 0 ? Math.max(0, 0.75 - item.taskRate) * 700 : 0)
      );
    }),
  );

  return [
    {
      tone: 'is-good',
      label: '最高效率',
      date: strongest.date,
      value: `${formatNumber(strongest.avgFocus, 1)} 分`,
      meta: `${formatHours(strongest.minutes)} · ${formatNumber(strongest.taskRate * 100, 0)}% 任务兑现`,
      note: `${strongest.sessionCount} 段学习叠在一起，属于把时间和质量一起拉高的一天。`,
      level: clampNumber((strongest.efficiencyScore / maxEfficiency) * 100, 0, 100, 0),
    },
    {
      tone: 'is-info',
      label: '最长投入',
      date: longest.date,
      value: formatHours(longest.minutes),
      meta: `日有效度 ${formatNumber(longest.avgFocus, 1)} 分 · ${formatNumber(longest.taskRate * 100, 0)}% 任务兑现`,
      note: '这一天天数里最能看出你能坐多久，适合检查硬扛还是有效投入。',
      level: clampNumber((longest.minutes / maxMinutes) * 100, 0, 100, 0),
    },
    {
      tone: 'is-warn',
      label: '最典型',
      date: typical.date,
      value: '接近中位',
      meta: `${formatHours(typical.minutes)} · 日有效度 ${formatNumber(typical.avgFocus, 1)} 分`,
      note: '这更像你这个周期的常态节奏，比峰值更适合做基线。',
      level: clampNumber(
        (1 -
          (Math.abs(typical.minutes - minuteMedian) / Math.max(minuteMedian, 1) +
            Math.abs(typical.avgFocus - focusMedian) / 12 +
            Math.abs(typical.taskRate - taskRateAverage) * 1.2) /
            3) *
          100,
        0,
        100,
        0,
      ),
    },
    {
      tone: 'is-risk',
      label: '最需注意',
      date: risky.date,
      value: `${formatNumber(risky.avgFocus, 1)} 分`,
      meta: `${formatHours(risky.minutes)} · ${Math.max(0, risky.tasksTotal - risky.tasksDone)} 个任务未兑现`,
      note: '高投入但质量塌下来的日子，最适合拆块、换题型、提前收口。',
      level: clampNumber(
        ((risky.minutes * Math.max(0, 100 - risky.avgFocus) +
          Math.max(0, risky.tasksTotal - risky.tasksDone) * 900 +
          (risky.tasksTotal > 0 ? Math.max(0, 0.75 - risky.taskRate) * 700 : 0)) /
          riskMax) *
          100,
        0,
        100,
        0,
      ),
    },
  ];
}

function buildTargetProgress(dailySeries) {
  const progress = DASHBOARD_ANALYTICS.buildEffectiveDayProgress(dailySeries);
  return {
    ...progress,
    actualMinutes: progress.effectiveDays,
    targetMinutes: progress.totalDays,
  };
}

function buildRiskItems(subjectGaps, taskFulfillment, lowEfficiencyDays, rhythm) {
  const risks = [];
  const gap = subjectGaps.find((item) => item.deltaShare <= -0.08);
  if (gap) {
    risks.push({
      title: gap.subject,
      body: `相对${gap.baselineLabel}少 ${formatNumber(Math.abs(gap.deltaShare) * 100, 0)} 个百分点`,
      severity: Math.abs(gap.deltaShare) * 100,
    });
  }

  if (taskFulfillment.total > 0 && taskFulfillment.rate < 0.8) {
    risks.push({
      title: '任务欠账',
      body: `${formatNumber(taskFulfillment.debt, 0)} 个任务未兑现`,
      severity: 80 - taskFulfillment.rate * 60,
    });
  }

  if (lowEfficiencyDays.length) {
    const day = lowEfficiencyDays[0];
    risks.push({
      title: '低效日',
      body: `${formatDateLabel(day.date)} 日有效度 ${formatNumber(day.avgFocus, 1)} 分`,
      severity: 72 - day.avgFocus * 0.4,
    });
  }

  if (rhythm.longestGap >= 3 || rhythm.trailingGap >= 2) {
    risks.push({
      title: '节奏断点',
      body: `最长空窗 ${rhythm.longestGap} 天，最近空窗 ${rhythm.trailingGap} 天`,
      severity: rhythm.longestGap * 12 + rhythm.trailingGap * 10,
    });
  }

  return risks.sort((a, b) => b.severity - a.severity).map(({ title, body }) => ({ title, body }));
}

function buildDataQuality(records, dailySeries, subjectSeries, taskFulfillment, rhythm) {
  const activeRecords = records.filter((item) => item.minutes > 0);
  const taskPlannedRecords = records.filter((item) => item.tasksTotal > 0);
  const focusRecords = records.filter((item) => item.focusScore > 0);
  const longestSubjectName = subjectSeries.reduce((longest, item) => Math.max(longest, item.subject.length), 0);
  const checks = [
    {
      key: 'sample',
      score: Math.min(100, records.length * 3.4),
      title: '样本厚度',
      value: `${records.length} 条`,
      body: records.length >= 24 ? '足够支撑周期判断。' : '样本偏薄，先把记录数量养起来。',
    },
    {
      key: 'coverage',
      score: rhythm.activeDensity * 100,
      title: '活跃覆盖',
      value: `${formatNumber(rhythm.activeDensity * 100, 0)}%`,
      body: rhythm.activeDensity >= 0.65 ? '当前周期覆盖稳定。' : '空窗偏多，趋势判断会更受偶然值影响。',
    },
    {
      key: 'tasks',
      score: records.length ? (taskPlannedRecords.length / records.length) * 100 : 0,
      title: '任务字段',
      value: `${formatNumber(records.length ? (taskPlannedRecords.length / records.length) * 100 : 0, 0)}%`,
      body:
        taskPlannedRecords.length === records.length
          ? '任务完成率可信。'
          : '部分记录没有计划任务，任务兑现指标会偏保守。',
    },
    {
      key: 'focus',
      score: records.length ? (focusRecords.length / records.length) * 100 : 0,
      title: '专注评分',
      value: `${formatNumber(records.length ? (focusRecords.length / records.length) * 100 : 0, 0)}%`,
      body: focusRecords.length === records.length ? '专注质量分析完整。' : '存在 0 分或缺评分记录，质量象限会受影响。',
    },
    {
      key: 'subjects',
      score: Math.min(100, subjectSeries.length * 22),
      title: '科目覆盖',
      value: `${subjectSeries.length} 科`,
      body: subjectSeries.length >= 4 ? '科目结构判断比较完整。' : '科目覆盖偏少，缺口判断会更像单科视角。',
    },
  ];

  const score = clampNumber(average(checks.map((item) => item.score)), 0, 100, 0);
  const label = score >= 78 ? '可信度高' : score >= 55 ? '可参考' : '样本偏薄';
  const issues = checks.filter((item) => item.score < 65).length;
  const summary = issues
    ? `${issues} 个维度需要补记录；当前结论适合做方向判断，不宜当成绝对排名。`
    : '当前样本比较完整，可以放心用来看周期趋势和结构短板。';

  return {
    score,
    label,
    summary,
    checks: checks.map((item) => ({
      ...item,
      status: item.score >= 75 ? 'is-good' : item.score >= 50 ? 'is-warn' : 'is-risk',
    })),
    longestSubjectName,
    activeRecords: activeRecords.length,
  };
}

function buildPrescription(context) {
  const {
    overview,
    comparison,
    subjectSeries,
    taskFulfillment,
    subjectGaps,
    lowEfficiencyDays,
    rhythm,
    timeWindowSeries,
    target,
    dataQuality,
  } = context;
  const items = [];
  const bestWindow = timeWindowSeries[0];
  const fallbackWindow = bestWindow ? bestWindow.label : '你的固定学习窗口';
  const weakSubject =
    subjectGaps.find((item) => item.deltaShare <= -0.08) ||
    subjectSeries.slice().sort((a, b) => a.avgFocus - b.avgFocus || a.minutes - b.minutes)[0];
  const focusSubject = weakSubject?.subject || subjectSeries[0]?.subject || '重点科目';
  const recommendedMinutes =
    Math.round(Math.max(45, Math.min(120, overview.totalMinutes / Math.max(1, overview.activeDays) || 75)) / 15) * 15;

  if (weakSubject) {
    const gapLabel =
      weakSubject.deltaShare < 0
        ? `相对${weakSubject.baselineLabel}少 ${formatNumber(Math.abs(weakSubject.deltaShare) * 100, 0)} 个百分点`
        : `当前会话质量 ${formatNumber(weakSubject.avgFocus, 1)} 分`;
    items.push({
      tone: 'is-priority',
      title: `补位 ${focusSubject}`,
      body: `把 ${fallbackWindow} 留给 ${focusSubject}，先做 ${recommendedMinutes} 分钟不可挪用学习块。`,
      meta: gapLabel,
      priority: 96,
    });
  }

  if (taskFulfillment.debt > 0) {
    const subject = taskFulfillment.worstSubject?.subject || focusSubject;
    items.push({
      tone: 'is-warn',
      title: '清理任务欠账',
      body: `下一轮先把 ${subject} 的计划量压到可兑现范围，优先补 ${Math.min(3, Math.ceil(taskFulfillment.debt))} 个欠账任务。`,
      meta: `当前欠账 ${formatNumber(taskFulfillment.debt, 0)} 个，完成率 ${formatNumber(taskFulfillment.rate * 100, 0)}%`,
      priority: 90,
    });
  }

  if (lowEfficiencyDays.length) {
    const day = lowEfficiencyDays[0];
    items.push({
      tone: 'is-risk',
      title: '拆掉低效硬撑',
      body: `遇到 ${formatNumber(day.minutes, 0)} 分钟以上但专注低的日子，改成 2 个 45 分钟块，中间换题型。`,
      meta: `${formatDateLabel(day.date)} 日有效度 ${formatNumber(day.avgFocus, 1)} 分`,
      priority: 82,
    });
  }

  if (rhythm.trailingGap >= 2 || rhythm.longestGap >= 3) {
    items.push({
      tone: 'is-risk',
      title: '补连续性底座',
      body: '设置一个最低保底块：哪怕当天状态差，也完成 35 分钟复盘或错题整理。',
      meta: `最长空窗 ${rhythm.longestGap} 天，最近空窗 ${rhythm.trailingGap} 天`,
      priority: 78,
    });
  }

  if (comparison && comparison.minuteDelta < -0.08) {
    items.push({
      tone: 'is-warn',
      title: '先恢复总量',
      body: `不要立刻加难度，先连续 3 天把学习时长恢复到 ${formatHours(Math.max(90, overview.totalMinutes / Math.max(1, overview.activeDays)))} / 天附近。`,
      meta: `较上一周期 ${formatSignedPercent(comparison.minuteDelta)}`,
      priority: 74,
    });
  }

  if (bestWindow) {
    items.push({
      tone: 'is-good',
      title: '保护黄金窗口',
      body: `${bestWindow.label} 不安排碎事，专门放阻力最大的题型或背诵任务。`,
      meta: `会话质量 ${formatNumber(bestWindow.avgFocus, 1)} · ${formatHours(bestWindow.minutes)}`,
      priority: 54,
    });
  }

  if (!items.length) {
    items.push({
      tone: 'is-good',
      title: '保持当前节奏',
      body: '当前没有明显短板，下一轮重点保持记录密度，并观察科目占比是否漂移。',
      meta: `记录可信度 ${formatNumber(dataQuality.score, 0)}`,
      priority: 1,
    });
  }

  const headline = items[0]?.title || '继续积累记录';
  const meta = `${formatNumber(dataQuality.score, 0)} 分可信度 · ${items.length} 条建议`;

  return {
    headline,
    meta,
    items: items.sort((a, b) => b.priority - a.priority).slice(0, 4),
  };
}

function buildInsights(context) {
  const {
    overview,
    subjectSeries,
    heatmapSeries,
    comparison,
    taskFulfillment,
    subjectGaps,
    lowEfficiencyDays,
    rhythm,
    timeWindowSeries,
    dataQuality,
  } = context;
  const items = [];

  if (comparison && comparison.minuteDelta != null) {
    const isDrop = comparison.minuteDelta < -0.08;
    const isRise = comparison.minuteDelta > 0.08;
    items.push({
      priority: isDrop ? 94 : isRise ? 48 : 36,
      title: isDrop ? '先稳住总量' : '学习总量变化',
      body: `当前周期学习时长较上一周期 ${formatSignedPercent(comparison.minuteDelta)}，日有效度 ${formatSignedNumber(comparison.focusDelta, 1)}。${isDrop ? '下一轮先把固定学习块补回来。' : '继续同时观察时长和有效度是否同向变化。'}`,
    });
  }

  const gap = subjectGaps.find((item) => item.deltaShare <= -0.08);
  if (gap) {
    items.push({
      priority: 90 + Math.min(8, Math.abs(gap.deltaShare) * 50),
      title: '科目投入缺口',
      body: `${gap.subject} 相比${gap.baselineLabel}少了 ${formatNumber(Math.abs(gap.deltaShare) * 100, 0)} 个百分点，本轮可以安排一个不可挪用的补位时段。`,
    });
  }

  if (taskFulfillment.total > 0 && taskFulfillment.rate < 0.8) {
    items.push({
      priority: 88,
      title: '计划兑现偏低',
      body: `还有 ${formatNumber(taskFulfillment.debt, 0)} 个任务欠账，完成率 ${formatNumber(taskFulfillment.rate * 100, 0)}%。下轮计划量可以先按真实完成能力缩一档。`,
    });
  }

  if (lowEfficiencyDays.length) {
    const day = lowEfficiencyDays[0];
    items.push({
      priority: 82,
      title: '低效硬撑日',
      body: `${formatDateLabel(day.date)} 学了 ${formatNumber(day.minutes, 0)} 分钟，但日有效度只有 ${formatNumber(day.avgFocus, 1)}。这类日子更适合拆成短块并提前换题型。`,
    });
  }

  if (rhythm.longestGap >= 3 || rhythm.trailingGap >= 2) {
    items.push({
      priority: 78,
      title: '连续性断点',
      body: `当前范围最长空窗 ${rhythm.longestGap} 天，最近空窗 ${rhythm.trailingGap} 天。先把每天最小学习块固定下来，比继续加量更关键。`,
    });
  } else if (rhythm.variation >= 0.75 && rhythm.activeDays >= 4) {
    items.push({
      priority: 70,
      title: '节奏波动偏大',
      body: `日学习量波动系数 ${formatNumber(rhythm.variation, 2)}，说明强弱日差距明显。建议把高负荷日拆一点给低负荷日。`,
    });
  }

  const topSubject = subjectSeries[0];
  if (topSubject && topSubject.share >= 0.42) {
    items.push({
      priority: 62,
      title: '科目结构偏重',
      body: `${topSubject.subject} 占了 ${formatNumber(topSubject.share * 100, 0)}% 的学习时间，别让主战场挤掉弱项复盘。`,
    });
  }

  const bestWindow = timeWindowSeries[0];
  if (bestWindow) {
    items.push({
      priority: 54,
      title: '高效窗口',
      body: `${bestWindow.label} 的加权会话质量最高（${formatNumber(bestWindow.avgFocus, 1)}），适合放数学大题、专业课背诵这类高阻力任务。`,
    });
  }

  const evening = averageWindowFocus(heatmapSeries, 18, 22);
  const morning = averageWindowFocus(heatmapSeries, 6, 10);
  if (evening != null && morning != null && Math.abs(evening - morning) >= 5) {
    const direction = evening > morning ? '晚间' : '早间';
    items.push({
      priority: 42,
      title: '日内节奏',
      body: `${direction}会话质量更高（${formatNumber(Math.max(evening, morning), 1)} vs ${formatNumber(Math.min(evening, morning), 1)}），难题优先放到这个窗口。`,
    });
  }

  if (overview.streak >= 5) {
    items.push({
      priority: 34,
      title: '连续性正在形成',
      body: `当前连续记录 ${overview.streak} 天，这个稳定性值得保留；下一步再看科目配比和任务兑现。`,
    });
  }

  if (dataQuality && dataQuality.score < 55) {
    items.push({
      priority: 86,
      title: '样本可信度偏低',
      body: `当前记录质量只有 ${formatNumber(dataQuality.score, 0)} 分，先补齐任务字段和连续记录，再看更细的科目排名。`,
    });
  }

  if (!items.length) {
    items.push({
      priority: 1,
      title: '数据还不够厚',
      body: '再累积一些记录，系统就能给出更明确的结构分析和趋势判断。',
    });
  }

  return items
    .sort((a, b) => b.priority - a.priority)
    .slice(0, 5)
    .map(({ title, body }) => ({ title, body }));
}

function buildExecutiveSummary({ overview, comparison, risks, prescription, dataQuality, filteredCount }) {
  if (!filteredCount) {
    return {
      headline: '还没有可分析的学习记录',
      body: '先积累几次带科目和任务的专注记录，看板会自动汇总周期趋势、风险和下一轮动作。',
      trend: '无样本',
      risk: '待判断',
      action: '先记录',
    };
  }

  const topRisk = risks[0];
  const topAction = prescription.items[0];
  const trend =
    comparison && comparison.minuteDelta != null
      ? `${comparison.minuteDelta >= 0 ? '提升' : '回落'} ${formatNumber(Math.abs(comparison.minuteDelta) * 100, 0)}%`
      : `${formatHours(overview.totalMinutes)} 累计`;
  const qualityLabel = overview.avgFocus >= 78 ? '质量稳定' : overview.avgFocus >= 65 ? '质量可用' : '质量偏低';
  const headline = topRisk ? `当前优先处理：${topRisk.title}` : `${qualityLabel}，可以继续加厚记录`;
  const body = topAction
    ? `${formatHours(overview.totalMinutes)} 学习量、${formatNumber(overview.avgFocus, 1)} 日有效度、${formatNumber(overview.taskRate * 100, 0)}% 任务兑现。下一步：${topAction.body}`
    : `${formatHours(overview.totalMinutes)} 学习量、${formatNumber(overview.avgFocus, 1)} 日有效度；先保持记录连续性。`;

  return {
    headline,
    body,
    trend,
    risk: topRisk ? topRisk.title : '低风险',
    action: topAction ? topAction.title : '保持节奏',
    quality: dataQuality.label,
  };
}

function buildFooterStatus(overview, anchorDate, filteredCount, comparison) {
  const dateLabel = formatDateLabel(anchorDate);
  const comparisonPart =
    comparison && comparison.minuteDelta != null
      ? `，较上一周期 ${comparison.minuteDelta >= 0 ? '提升' : '下降'} ${formatNumber(Math.abs(comparison.minuteDelta) * 100, 0)}%`
      : '';
  return `当前范围 ${filteredCount} 条记录，平均日有效度 ${formatNumber(overview.avgFocus, 1)} 分，任务完成率 ${formatNumber(overview.taskRate * 100, 0)}%，截至 ${dateLabel}${comparisonPart}。`;
}

function buildDailySeries(records, anchorDate, rangeDays) {
  if (!records.length) return [];

  const days = [];
  const count = rangeDays === 'all' ? Math.max(1, daysBetween(getEarliestDate(records), anchorDate) + 1) : rangeDays;
  const startDate = rangeDays === 'all' ? getEarliestDate(records) : addDays(anchorDate, -(count - 1));
  const dailySlices = buildDailySliceMap(records, startDate, addDays(anchorDate, 1));

  for (let index = 0; index < count; index += 1) {
    const date = addDays(startDate, index);
    const key = toDateKey(date);
    const bucket = dailySlices.get(key);
    days.push(buildDailyAggregate(key, bucket ? [...bucket.values()] : []));
  }

  return days;
}

function buildDailySliceMap(records, start, end) {
  const buckets = new Map();

  for (const record of records) {
    for (const segment of splitRecordByClock(record)) {
      if (segment.end <= start || segment.start >= end) continue;
      const clippedStart = maxDate(segment.start, start);
      const clippedEnd = minDate(segment.end, end);
      const minutes = Math.max(0, (clippedEnd.getTime() - clippedStart.getTime()) / 60000);
      if (!minutes) continue;
      const date = toDateKey(clippedStart);
      const bucket = buckets.get(date) || new Map();
      addClippedRecordSlice(bucket, record, date, minutes);
      buckets.set(date, bucket);
    }
  }

  return buckets;
}

function addClippedRecordSlice(bucket, record, date, minutes) {
  const key = record.id || `${record.date}-${record.subject}-${record.startedAt || record.startHour}`;
  const existing = bucket.get(key) || buildClippedRecord(record, date, 0);
  const nextMinutes = existing.minutes + minutes;
  const sourceMinutes = Math.max(1, Number(record.minutes) || nextMinutes || 1);
  const ratio = nextMinutes / sourceMinutes;
  existing.date = date;
  existing.minutes = nextMinutes;
  existing.actualSeconds = Math.round(nextMinutes * 60);
  existing.plannedSeconds = Math.max(
    existing.actualSeconds,
    Math.round((record.plannedSeconds || record.actualSeconds || sourceMinutes * 60) * ratio),
  );
  existing.pausedSeconds = Math.round((record.pausedSeconds || 0) * ratio);
  existing.interruptionCount = Math.round((record.interruptionCount || 0) * ratio);
  existing.emergencyExitCount = Math.round((record.emergencyExitCount || 0) * ratio);
  existing.tasksDone = date === record.date ? record.tasksDone : 0;
  existing.tasksTotal = date === record.date ? record.tasksTotal : 0;
  bucket.set(key, existing);
}

function buildClippedRecord(record, date, minutes) {
  const sourceMinutes = Math.max(1, Number(record.minutes) || minutes || 1);
  const ratio = Math.max(0, minutes) / sourceMinutes;
  const actualSeconds = Math.round(Math.max(0, minutes) * 60);
  const plannedSeconds = Math.max(
    actualSeconds,
    Math.round((record.plannedSeconds || record.actualSeconds || sourceMinutes * 60) * ratio),
  );

  return {
    ...record,
    date,
    minutes,
    actualSeconds,
    plannedSeconds,
    pausedSeconds: Math.round((record.pausedSeconds || 0) * ratio),
    interruptionCount: Math.round((record.interruptionCount || 0) * ratio),
    emergencyExitCount: Math.round((record.emergencyExitCount || 0) * ratio),
    tasksDone: date === record.date ? record.tasksDone : 0,
    tasksTotal: date === record.date ? record.tasksTotal : 0,
  };
}

function buildDailyAggregate(date, bucket) {
  const minutes = sum(bucket, (item) => item.minutes);
  const sessionQuality = bucket.length ? weightedAverage(bucket, (item) => item.focusScore, (item) => Math.max(1, item.minutes)) : null;
  const tasksDone = sum(bucket, (item) => Math.min(item.tasksDone, item.tasksTotal));
  const tasksTotal = sum(bucket, (item) => item.tasksTotal);
  const score = DASHBOARD_ANALYTICS.calculateDailyFocusScore({
    minutes,
    sessionQuality: sessionQuality ?? 0,
    tasksDone,
    tasksTotal,
    interruptionCount: sum(bucket, (item) => item.interruptionCount),
    emergencyExitCount: sum(bucket, (item) => item.emergencyExitCount),
    pausedSeconds: sum(bucket, (item) => item.pausedSeconds),
    plannedSeconds: sum(bucket, (item) => item.plannedSeconds || item.actualSeconds || item.minutes * 60),
  });
  const dailyFocusScore = bucket.length ? score.score : null;
  const taskRate = tasksTotal > 0 ? tasksDone / tasksTotal : 0;

  return {
    date,
    minutes,
    avgFocus: dailyFocusScore,
    sessionQuality,
    dailyFocusScore,
    effective: score.effective,
    heatLevel: DASHBOARD_ANALYTICS.getAnnualHeatLevel({ minutes, dailyFocusScore }),
    scoreParts: score.parts,
    tasksDone,
    tasksTotal,
    taskRate,
    sessionCount: bucket.length,
    subjectCount: uniqueCount(bucket, (item) => item.subject),
    efficiencyScore: dailyFocusScore == null ? 0 : dailyFocusScore * Math.log(minutes + 1),
    active: bucket.length > 0,
  };
}

function averageDailyFocusScore(dailySeries) {
  return average(
    dailySeries
      .filter((item) => item.active && item.dailyFocusScore != null)
      .map((item) => item.dailyFocusScore),
  );
}

function buildSubjectSeries(records) {
  const grouped = groupBy(records, (item) => item.subject);
  const totals = [...grouped.entries()]
    .map(([subject, bucket]) => ({
      subject,
      minutes: sum(bucket, (item) => item.minutes),
      avgFocus: average(bucket.map((item) => item.focusScore)),
      tasksDone: sum(bucket, (item) => Math.min(item.tasksDone, item.tasksTotal)),
      tasksTotal: sum(bucket, (item) => item.tasksTotal),
      activeDays: uniqueCount(bucket, (item) => item.date),
      sessions: bucket.length,
    }))
    .sort((a, b) => b.minutes - a.minutes);
  const totalMinutes = totals.reduce((acc, item) => acc + item.minutes, 0);
  return totals.map((item) => ({
    ...item,
    share: totalMinutes > 0 ? item.minutes / totalMinutes : 0,
    taskRate: item.tasksTotal > 0 ? item.tasksDone / item.tasksTotal : 0,
    avgMinutesPerActiveDay: item.activeDays > 0 ? item.minutes / item.activeDays : 0,
  }));
}

function buildHeatmapSeries(records) {
  const rows = [
    { label: '周一', row: 0 },
    { label: '周二', row: 1 },
    { label: '周三', row: 2 },
    { label: '周四', row: 3 },
    { label: '周五', row: 4 },
    { label: '周六', row: 5 },
    { label: '周日', row: 6 },
  ];
  const cells = [];
  const map = new Map();

  for (const record of records) {
    for (const segment of splitRecordByHour(record)) {
      const date = parseDateKey(segment.date || record.date);
      const dayRow = (date.getDay() + 6) % 7;
      const key = `${dayRow}:${segment.hour}`;
      const bucket = map.get(key) || { minutes: 0, focusWeighted: 0 };
      bucket.minutes += segment.minutes;
      bucket.focusWeighted += segment.minutes * record.focusScore;
      map.set(key, bucket);
    }
  }

  for (const row of rows) {
    for (let hour = 0; hour < 24; hour += 1) {
      const bucket = map.get(`${row.row}:${hour}`) || { minutes: 0, focusWeighted: 0 };
      cells.push({
        row: row.row,
        rowLabel: row.label,
        hour,
        minutes: bucket.minutes,
        avgFocus: bucket.minutes > 0 ? bucket.focusWeighted / bucket.minutes : null,
      });
    }
  }

  return { rows, hours: Array.from({ length: 24 }, (_, index) => index), cells };
}

function renderTrendAxes(svg, margin, width, height, maxMinutes) {
  const tickCount = 4;
  const bottom = height - margin.bottom;
  const left = margin.left;
  const right = width - margin.right;
  const gridLine = cssVar('--grid-line', 'rgba(255,255,255,0.06)');
  for (let index = 0; index <= tickCount; index += 1) {
    const ratio = index / tickCount;
    const y = scale(ratio, 0, 1, bottom, margin.top);
    addLine(svg, left, y, right, y, {
      stroke: gridLine,
      'stroke-width': 1,
    });
    const minuteValue = Math.round(maxMinutes * ratio);
    addText(svg, left - 10, y + 4, `${minuteValue}`, {
      fill: 'var(--muted)',
      'text-anchor': 'end',
      'font-size': '11',
    });
    const focusValue = Math.round(100 * ratio);
    addText(svg, right + 10, y + 4, `${focusValue}`, {
      fill: 'var(--muted)',
      'font-size': '11',
    });
  }

  addText(svg, left - 10, margin.top - 4, '分钟', {
    fill: 'var(--muted)',
    'text-anchor': 'end',
    'font-size': '11',
  });
  addText(svg, right + 10, margin.top - 4, '日有效度', {
    fill: 'var(--muted)',
    'font-size': '11',
  });
}

function heatPalette() {
  return [
    cssVar('--heat-empty', 'color-mix(in srgb, var(--text) 8%, transparent)'),
    cssVar('--heat-low', '#d85f45'),
    cssVar('--heat-under', '#d7a33e'),
    cssVar('--heat-met', '#9fca58'),
    cssVar('--heat-high', '#3f9f63'),
    cssVar('--heat-peak', '#0d6b4f'),
  ];
}

function heatIntensityLevel(value, maxValue) {
  const numeric = Number(value) || 0;
  if (numeric <= 0) return 0;
  if (maxValue === 5 && numeric >= 1 && numeric <= 5) return Math.round(numeric);
  const ratio = numeric / Math.max(1, Number(maxValue) || 1);
  if (ratio >= 0.84) return 5;
  if (ratio >= 0.64) return 4;
  if (ratio >= 0.42) return 3;
  if (ratio >= 0.22) return 2;
  return 1;
}

function heatValue(minutes, focusScore) {
  if (!minutes) return 0;
  const focusFactor = focusScore == null ? 0.72 : clampNumber(focusScore, 0, 100, 0) / 100;
  return minutes * (0.45 + focusFactor * 0.75);
}

function drawChartGrid(svg, margin, width, height, lines) {
  const left = margin.left;
  const right = width - margin.right;
  const bottom = height - margin.bottom;
  const gridLine = cssVar('--grid-line', 'rgba(255,255,255,0.05)');
  const axisLine = cssVar('--axis-line', 'rgba(255,255,255,0.14)');
  const axisSoft = cssVar('--axis-soft', 'rgba(255,255,255,0.08)');
  for (let index = 0; index <= lines; index += 1) {
    const ratio = index / lines;
    const y = scale(ratio, 0, 1, bottom, margin.top);
    addLine(svg, left, y, right, y, {
      stroke: gridLine,
      'stroke-width': 1,
    });
  }
  addLine(svg, left, margin.top, left, bottom, {
    stroke: axisSoft,
    'stroke-width': 1,
  });
  addLine(svg, right, margin.top, right, bottom, {
    stroke: axisSoft,
    'stroke-width': 1,
  });
  addLine(svg, left, bottom, right, bottom, {
    stroke: axisLine,
    'stroke-width': 1,
  });
}

function drawXLabels(svg, dailySeries, margin, width, height) {
  const step = dailySeries.length > 1 ? (width - margin.left - margin.right) / (dailySeries.length - 1) : 0;
  const every = Math.max(1, Math.ceil(dailySeries.length / 8));
  dailySeries.forEach((item, index) => {
    if (index % every !== 0 && index !== dailySeries.length - 1) return;
    const x = margin.left + index * step;
    const [month, day] = item.date
      .split('-')
      .slice(1)
      .map((value) => Number(value));
    addText(svg, x, height - 20, `${month}/${day}`, {
      fill: 'var(--muted)',
      'text-anchor': 'middle',
      'font-size': '11',
    });
  });
}

function drawLineSeries(svg, points, attrs) {
  if (!points.length) return;
  const path = points.map((point, index) => `${index === 0 ? 'M' : 'L'} ${point.x} ${point.y}`).join(' ');
  addPath(svg, path, attrs);
}

function addRect(svg, x, y, width, height, attrs = {}, titleText) {
  const rect = createSvgEl('rect', { x, y, width, height, ...attrs });
  svg.appendChild(rect);
  if (titleText) {
    const title = createSvgEl('title');
    title.textContent = titleText;
    rect.appendChild(title);
  }
  return rect;
}

function addCircle(svg, cx, cy, r, attrs = {}, titleText) {
  const circle = createSvgEl('circle', { cx, cy, r, ...attrs });
  svg.appendChild(circle);
  if (titleText) {
    const title = createSvgEl('title');
    title.textContent = titleText;
    circle.appendChild(title);
  }
  return circle;
}

function addLine(svg, x1, y1, x2, y2, attrs = {}) {
  return svg.appendChild(createSvgEl('line', { x1, y1, x2, y2, ...attrs }));
}

function addPath(svg, d, attrs = {}) {
  return svg.appendChild(createSvgEl('path', { d, fill: 'none', ...attrs }));
}

function addText(svg, x, y, text, attrs = {}) {
  const node = createSvgEl('text', { x, y, ...attrs });
  node.textContent = text;
  svg.appendChild(node);
  return node;
}

function createSvgEl(tag, attrs = {}) {
  const el = document.createElementNS('http://www.w3.org/2000/svg', tag);
  for (const [key, value] of Object.entries(attrs)) {
    if (value == null) continue;
    el.setAttribute(key, String(value));
  }
  return el;
}

function setSvgViewBox(svg, width, height) {
  svg.setAttribute('viewBox', `0 0 ${width} ${height}`);
}

function clearSvg(svg) {
  while (svg.firstChild) {
    svg.removeChild(svg.firstChild);
  }
}

function normalizeRange(range) {
  return ['7', '30', '90', 'all'].includes(String(range)) ? String(range) : '30';
}

function resolveRangeDays(range) {
  const normalized = normalizeRange(range);
  if (normalized === 'all') return 'all';
  return Number(normalized);
}

function resolveComparisonDays(records, rangeDays) {
  if (rangeDays === 'all') {
    const span = daysBetween(getEarliestDate(records), getAnchorDate(records)) + 1;
    if (span < 14) return null;
    return Math.min(30, Math.max(7, Math.floor(span / 2)));
  }
  return Number(rangeDays);
}

function normalizeRecord(raw) {
  if (!raw || typeof raw !== 'object') return null;

  const startedAt = normalizeDateString(raw.startedAt ?? raw.started_at);
  const endedAt = normalizeDateString(raw.endedAt ?? raw.ended_at);
  let date = String(raw.date ?? raw.day ?? '').trim();
  if (!/^\d{4}-\d{2}-\d{2}$/.test(date) && startedAt) {
    date = toDateKey(new Date(startedAt));
  }
  if (!/^\d{4}-\d{2}-\d{2}$/.test(date)) return null;
  if (toDateKey(parseDateKey(date)) !== date) return null;

  const subject = String(raw.subject ?? raw.course ?? '未命名科目').trim() || '未命名科目';
  const minutes = clampNumber(raw.minutes ?? raw.studyMinutes ?? raw.duration, 0, 24 * 60, 0);
  const actualSeconds = clampNumber(raw.actualSeconds ?? raw.actual_seconds ?? minutes * 60, 0, 7 * 24 * 3600, minutes * 60);
  const plannedSeconds = clampNumber(raw.plannedSeconds ?? raw.planned_seconds ?? actualSeconds, 0, 7 * 24 * 3600, actualSeconds);
  const pausedSeconds = clampNumber(raw.pausedSeconds ?? raw.paused_seconds ?? 0, 0, 7 * 24 * 3600, 0);
  const interruptionCount = clampNumber(raw.interruptionCount ?? raw.interruption_count ?? 0, 0, 1000, 0);
  const emergencyExitCount = clampNumber(raw.emergencyExitCount ?? raw.emergency_exit_count ?? 0, 0, 1000, 0);
  const focusScore = clampNumber(raw.focusScore ?? raw.focus ?? raw.score, 0, 100, 0);
  const tasksTotal = clampNumber(raw.tasksTotal ?? raw.totalTasks ?? raw.tasks ?? 0, 0, 1000, 0);
  const tasksDone = clampNumber(raw.tasksDone ?? raw.doneTasks ?? 0, 0, Math.max(tasksTotal, 0), 0);
  const startHour = clampNumber(raw.startHour ?? raw.hour ?? (startedAt ? new Date(startedAt).getHours() : 19), 0, 23, 19);
  const status = String(raw.status ?? 'finished').trim() || 'finished';
  const endReason = raw.endReason ?? raw.end_reason ?? '';

  return {
    id: raw.id != null || raw.uuid != null ? String(raw.id ?? raw.uuid) : '',
    date,
    subject,
    minutes,
    actualSeconds,
    plannedSeconds,
    pausedSeconds,
    interruptionCount,
    emergencyExitCount,
    focusScore,
    tasksDone,
    tasksTotal,
    startHour,
    startedAt,
    endedAt,
    endReason: endReason == null ? '' : String(endReason),
    status,
  };
}

function dedupeRecords(records) {
  const byKey = new Map();

  for (const raw of records) {
    const record = normalizeRecord(raw);
    if (!record) continue;
    const key = record.id
      ? `id:${record.id}`
      : `fingerprint:${[record.date, record.subject, record.startHour, record.startedAt, record.actualSeconds, record.minutes, record.focusScore, record.tasksDone, record.tasksTotal].join('|')}`;
    byKey.set(key, record);
  }

  return sortRecords([...byKey.values()]);
}

function sortRecords(records) {
  return [...records].sort(
    (a, b) =>
      compareDateKey(a.date, b.date) || a.startHour - b.startHour || a.subject.localeCompare(b.subject, 'zh-Hans-CN'),
  );
}

function filterByRange(records, rangeDays, anchorDate) {
  if (!records.length) return [];
  if (rangeDays === 'all') return sortRecords(records);
  return records.filter((record) => {
    const diff = daysBetween(parseDateKey(record.date), anchorDate);
    return diff >= 0 && diff < Number(rangeDays);
  });
}

function computeStreak(records, anchorDate) {
  const dateSet = new Set(records.map((item) => item.date));
  let streak = 0;
  let current = anchorDate;

  while (dateSet.has(toDateKey(current))) {
    streak += 1;
    current = addDays(current, -1);
  }

  return streak;
}

function computeBestWindow(records) {
  if (!records.length) return '';

  const buckets = new Map();
  for (const record of records) {
    for (const segment of splitRecordByHour(record)) {
      const bucket = buckets.get(segment.hour) || { minutes: 0, focusWeighted: 0 };
      bucket.minutes += segment.minutes;
      bucket.focusWeighted += segment.minutes * record.focusScore;
      buckets.set(segment.hour, bucket);
    }
  }

  const scored = [...buckets.entries()].map(([hour, bucket]) => {
    const minutes = bucket.minutes;
    const avgFocus = minutes > 0 ? bucket.focusWeighted / minutes : 0;
    const score = avgFocus * 1.2 + Math.log(minutes + 1) * 6;
    return { hour, bucket, avgFocus, minutes, score };
  });

  scored.sort((a, b) => b.score - a.score || b.minutes - a.minutes || a.hour - b.hour);
  const best = scored[0];
  if (!best) return '';

  const end = Math.min(23, best.hour + 2);
  return `${padHour(best.hour)}:00 - ${padHour(end)}:00`;
}

function averageWindowFocus(heatmapSeries, startHour, endHour) {
  const cells = heatmapSeries.cells.filter(
    (cell) => cell.hour >= startHour && cell.hour <= endHour && cell.avgFocus != null,
  );
  if (!cells.length) return null;
  return average(cells.map((cell) => cell.avgFocus));
}

function countSubjects(records) {
  return new Set(records.map((item) => item.subject)).size;
}

function getAnchorDate(records) {
  if (!records.length) return new Date();
  return parseDateKey(records[records.length - 1].date);
}

function getEarliestDate(records) {
  if (!records.length) return new Date();
  return parseDateKey(records[0].date);
}

function buildSampleRecord(date, subject, minutes, focusScore, tasksDone, tasksTotal, startHour) {
  const start = parseDateKey(date);
  start.setHours(startHour, (startHour % 2) * 10, 0, 0);
  const end = new Date(start.getTime() + minutes * 60000);
  return {
    id: `${date}-${subject}-${startHour}-${minutes}-${focusScore}`,
    date,
    subject,
    minutes,
    actualSeconds: minutes * 60,
    plannedSeconds: minutes * 60,
    pausedSeconds: 0,
    interruptionCount: focusScore < 65 ? 2 : 0,
    emergencyExitCount: 0,
    focusScore,
    tasksDone,
    tasksTotal,
    startHour,
    startedAt: start.toISOString(),
    endedAt: end.toISOString(),
    endReason: 'completed',
    status: 'finished',
  };
}

function generateSampleData() {
  const today = new Date();
  const start = addDays(today, -83);
  const rows = [];
  for (let dayIndex = 0; dayIndex < 84; dayIndex += 1) {
    const date = toDateKey(addDays(start, dayIndex));
    const sessions = 1 + ((dayIndex * 7) % 3 === 0 ? 1 : 0);
    for (let sessionIndex = 0; sessionIndex < sessions; sessionIndex += 1) {
      const subject = SUBJECTS[(dayIndex + sessionIndex) % SUBJECTS.length];
      const base = 95 + ((dayIndex * 13 + sessionIndex * 11) % 70);
      const minutes = base + (sessionIndex % 2) * 35;
      const focusScore = clampNumber(
        74 + ((dayIndex * 5 + sessionIndex * 9) % 18) - (sessionIndex === 1 ? 4 : 0),
        0,
        100,
        78,
      );
      const tasksTotal = 3 + ((dayIndex + sessionIndex) % 4);
      const tasksDone = Math.min(tasksTotal, 1 + ((dayIndex * 3 + sessionIndex) % tasksTotal));
      const startHour = [7, 9, 13, 15, 19, 21][(dayIndex + sessionIndex) % 6];
      rows.push(buildSampleRecord(date, subject, minutes, focusScore, tasksDone, tasksTotal, startHour));
    }
  }
  return sortRecords(rows);
}

function formatHours(minutes) {
  const hours = minutes / 60;
  return `${formatNumber(hours, 1)}h`;
}

function formatSessionWindow(record) {
  const start = getRecordStartDate(record);
  const end = getRecordEndDate(record, start);
  if (start && end) {
    const sameDay = toDateKey(start) === toDateKey(end);
    return sameDay
      ? `${formatClock(start)} - ${formatClock(end)}`
      : `${formatDateTime(start.toISOString())} - ${formatDateTime(end.toISOString())}`;
  }

  const durationHours = Math.max(1, Math.round(record.minutes / 60));
  const endHour = Math.min(23, record.startHour + durationHours);
  return `${padHour(record.startHour)}:00 - ${padHour(endHour)}:00`;
}

function formatDateLabel(dateKey) {
  const date = dateKey instanceof Date ? dateKey : parseDateKey(dateKey);
  return new Intl.DateTimeFormat('zh-CN', { month: 'numeric', day: 'numeric' }).format(date);
}

function formatDateTime(value) {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return '未知时间';
  return new Intl.DateTimeFormat('zh-CN', {
    month: 'numeric',
    day: 'numeric',
    hour: '2-digit',
    minute: '2-digit',
  }).format(date);
}

function formatClock(value) {
  const date = value instanceof Date ? value : new Date(value);
  if (Number.isNaN(date.getTime())) return '--:--';
  return `${padHour(date.getHours())}:${padHour(date.getMinutes())}`;
}

function formatMinutesLabel(minutes) {
  const value = Math.max(0, Math.round(Number(minutes) || 0));
  if (value < 60) return `${value} 分钟`;
  const hours = Math.floor(value / 60);
  const rest = value % 60;
  return rest ? `${hours}小时${rest}分钟` : `${hours} 小时`;
}

function formatCompactMinutes(minutes) {
  const value = Math.max(0, Math.round(Number(minutes) || 0));
  return value >= 60 ? `${formatNumber(value / 60, value >= 600 ? 0 : 1)}h` : `${value}m`;
}

function formatBytes(bytes) {
  const value = Number(bytes);
  if (!Number.isFinite(value) || value <= 0) return '0 B';
  const units = ['B', 'KB', 'MB', 'GB'];
  let size = value;
  let unitIndex = 0;
  while (size >= 1024 && unitIndex < units.length - 1) {
    size /= 1024;
    unitIndex += 1;
  }
  return `${formatNumber(size, unitIndex === 0 ? 0 : 1)} ${units[unitIndex]}`;
}

function padHour(hour) {
  return String(hour).padStart(2, '0');
}

function formatNumber(value, digits = 0) {
  if (!Number.isFinite(value)) return '0';
  return Number(value).toFixed(digits);
}

function formatSignedNumber(value, digits = 1) {
  const sign = value >= 0 ? '+' : '−';
  return `${sign}${formatNumber(Math.abs(value), digits)}`;
}

function formatSignedPercent(value) {
  return `${value >= 0 ? '+' : '−'}${formatNumber(Math.abs(value) * 100, 0)}%`;
}

function clampNumber(value, min, max, fallback) {
  const parsed = Number(value);
  if (!Number.isFinite(parsed)) return fallback;
  return Math.min(max, Math.max(min, parsed));
}

function average(values) {
  const list = values.filter((value) => Number.isFinite(value));
  if (!list.length) return 0;
  return list.reduce((sum, value) => sum + value, 0) / list.length;
}

function weightedAverage(values, valueMapper, weightMapper) {
  let weightedSum = 0;
  let totalWeight = 0;
  for (const item of values) {
    const value = Number(valueMapper(item));
    const weight = Number(weightMapper(item));
    if (!Number.isFinite(value) || !Number.isFinite(weight) || weight <= 0) continue;
    weightedSum += value * weight;
    totalWeight += weight;
  }
  return totalWeight > 0 ? weightedSum / totalWeight : 0;
}

function median(values) {
  const list = values.filter((value) => Number.isFinite(value)).sort((a, b) => a - b);
  if (!list.length) return 0;
  const middle = Math.floor(list.length / 2);
  return list.length % 2 === 0 ? (list[middle - 1] + list[middle]) / 2 : list[middle];
}

function sum(values, mapper) {
  return values.reduce((total, item, index) => total + Number(mapper(item, index) || 0), 0);
}

function uniqueCount(values, mapper) {
  return new Set(values.map(mapper)).size;
}

function groupBy(values, mapper) {
  const map = new Map();
  for (const item of values) {
    const key = mapper(item);
    const bucket = map.get(key) || [];
    bucket.push(item);
    map.set(key, bucket);
  }
  return map;
}

function scale(value, domainMin, domainMax, rangeMin, rangeMax) {
  if (domainMax === domainMin) return (rangeMin + rangeMax) / 2;
  const ratio = (value - domainMin) / (domainMax - domainMin);
  return rangeMin + (rangeMax - rangeMin) * ratio;
}

function compareDateKey(left, right) {
  return compareDates(parseDateKey(left), parseDateKey(right));
}

function compareDates(left, right) {
  return left.getTime() - right.getTime();
}

function daysBetween(left, right) {
  const leftUtc = Date.UTC(left.getFullYear(), left.getMonth(), left.getDate());
  const rightUtc = Date.UTC(right.getFullYear(), right.getMonth(), right.getDate());
  return Math.round((rightUtc - leftUtc) / 86400000);
}

function addDays(date, delta) {
  const next = new Date(date);
  next.setDate(next.getDate() + delta);
  return next;
}

function startOfWeek(date) {
  const next = new Date(date);
  next.setHours(0, 0, 0, 0);
  next.setDate(next.getDate() - mondayWeekday(next));
  return next;
}

function monthWindow(offset) {
  const now = new Date();
  const start = new Date(now.getFullYear(), now.getMonth() + offset, 1);
  const end = new Date(start.getFullYear(), start.getMonth() + 1, 1);
  return { start, end };
}

function yearWindow(offset) {
  const year = new Date().getFullYear() + offset;
  return {
    year,
    start: new Date(year, 0, 1),
    end: new Date(year + 1, 0, 1),
  };
}

function mondayWeekday(date) {
  return (date.getDay() + 6) % 7;
}

function minuteOfDay(date) {
  return date.getHours() * 60 + date.getMinutes() + date.getSeconds() / 60;
}

function minDate(left, right) {
  return left <= right ? left : right;
}

function maxDate(left, right) {
  return left >= right ? left : right;
}

function formatWeekPeriodLabel(start, offset) {
  if (offset === 0) return '本周';
  return `${formatDateLabel(start)} - ${formatDateLabel(addDays(start, 6))}`;
}

function formatMonthPeriodLabel(start, offset) {
  if (offset === 0) return `${start.getMonth() + 1}月`;
  const now = new Date();
  const monthLabel = `${start.getMonth() + 1}月`;
  return start.getFullYear() === now.getFullYear() ? monthLabel : `${start.getFullYear()}年${monthLabel}`;
}

function parseDateKey(dateKey) {
  if (dateKey instanceof Date) return new Date(dateKey);
  const [year, month, day] = dateKey.split('-').map(Number);
  return new Date(year, month - 1, day);
}

function toDateKey(date) {
  const year = date.getFullYear();
  const month = String(date.getMonth() + 1).padStart(2, '0');
  const day = String(date.getDate()).padStart(2, '0');
  return `${year}-${month}-${day}`;
}

function escapeHtml(text) {
  return String(text)
    .replaceAll('&', '&amp;')
    .replaceAll('<', '&lt;')
    .replaceAll('>', '&gt;')
    .replaceAll('"', '&quot;')
    .replaceAll("'", '&#39;');
}

function escapeTitle(text) {
  return String(text).replace(/\s+/g, ' ').trim();
}

function normalizeDateString(value) {
  if (value == null || value === '') return '';
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? '' : String(value);
}

function getRecordStartDate(record) {
  if (record.startedAt) {
    const started = new Date(record.startedAt);
    if (!Number.isNaN(started.getTime())) return started;
  }

  const fallback = parseDateKey(record.date);
  fallback.setHours(record.startHour || 0, 0, 0, 0);
  return fallback;
}

function getRecordEndDate(record, started = getRecordStartDate(record)) {
  if (record.endedAt) {
    const ended = new Date(record.endedAt);
    if (!Number.isNaN(ended.getTime()) && ended > started) return ended;
  }

  const durationSeconds = Number(record.actualSeconds) > 0 ? Number(record.actualSeconds) : Number(record.minutes || 0) * 60;
  return new Date(started.getTime() + Math.max(60, durationSeconds) * 1000);
}

function splitRecordByClock(record) {
  const start = getRecordStartDate(record);
  let end = getRecordEndDate(record, start);
  if (Number.isNaN(start.getTime())) return [];
  if (Number.isNaN(end.getTime()) || end <= start) {
    end = new Date(start.getTime() + Math.max(1, record.minutes || 0) * 60000);
  }

  const segments = [];
  let cursor = new Date(start);
  let guard = 0;
  while (cursor < end && guard < 512) {
    const nextHour = new Date(cursor);
    nextHour.setMinutes(0, 0, 0);
    nextHour.setHours(nextHour.getHours() + 1);
    const segmentEnd = minDate(nextHour > cursor ? nextHour : new Date(cursor.getTime() + 3600000), end);
    const minutes = Math.max(0, (segmentEnd.getTime() - cursor.getTime()) / 60000);
    if (minutes > 0) {
      segments.push({
        date: toDateKey(cursor),
        hour: cursor.getHours(),
        minutes,
        start: new Date(cursor),
        end: new Date(segmentEnd),
      });
    }
    cursor = new Date(segmentEnd);
    guard += 1;
  }

  if (!segments.length) {
    segments.push({
      date: record.date,
      hour: record.startHour,
      minutes: 0,
      start,
      end,
    });
  }

  return segments;
}

function cssVar(name, fallback = '') {
  const value = getComputedStyle(document.body).getPropertyValue(name).trim();
  return value || fallback;
}

function qualityPalette() {
  return {
    deep: cssVar('--quality-deep', 'rgba(70, 211, 178, 0.92)'),
    grind: cssVar('--quality-grind', 'rgba(255, 139, 125, 0.88)'),
    light: cssVar('--quality-light', 'rgba(116, 167, 255, 0.86)'),
    empty: cssVar('--quality-empty', 'rgba(147, 163, 172, 0.52)'),
  };
}

function subjectColor(index) {
  const colors = [
    cssVar('--subject-1', 'rgba(70, 211, 178, 0.86)'),
    cssVar('--subject-2', 'rgba(116, 167, 255, 0.86)'),
    cssVar('--subject-3', 'rgba(239, 179, 91, 0.86)'),
    cssVar('--subject-4', 'rgba(255, 139, 125, 0.86)'),
    cssVar('--subject-5', 'rgba(166, 223, 113, 0.86)'),
  ];
  return colors[index % colors.length];
}

function splitRecordByHour(record) {
  return splitRecordByClock(record);
}
