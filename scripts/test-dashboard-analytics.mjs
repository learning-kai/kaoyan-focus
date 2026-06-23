import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import vm from 'node:vm';

const source = await readFile(new URL('../src-tauri/dashboard/analytics.js', import.meta.url), 'utf8');
const sandbox = { window: {} };
vm.createContext(sandbox);
vm.runInContext(source, sandbox, { filename: 'analytics.js' });

const analytics = sandbox.window.DashboardAnalytics;
assert.ok(analytics, 'DashboardAnalytics should be attached to window');

const strongDay = analytics.calculateDailyFocusScore({
  minutes: 210,
  sessionQuality: 86,
  tasksDone: 5,
  tasksTotal: 5,
  interruptionCount: 0,
  emergencyExitCount: 0,
  pausedSeconds: 0,
  plannedSeconds: 210 * 60,
});

assert.equal(strongDay.score, 94);
assert.equal(strongDay.effective, true);
assert.equal(strongDay.parts.volume, 100);
assert.equal(strongDay.parts.task, 100);
assert.equal(strongDay.parts.continuity, 100);

const shortHighQualityDay = analytics.calculateDailyFocusScore({
  minutes: 80,
  sessionQuality: 92,
  tasksDone: 2,
  tasksTotal: 2,
  interruptionCount: 0,
  emergencyExitCount: 0,
  pausedSeconds: 0,
  plannedSeconds: 80 * 60,
});

assert.equal(shortHighQualityDay.effective, false);
assert.equal(analytics.getAnnualHeatLevel(shortHighQualityDay), 2);

const noisyLongDay = analytics.calculateDailyFocusScore({
  minutes: 240,
  sessionQuality: 48,
  tasksDone: 1,
  tasksTotal: 4,
  interruptionCount: 5,
  emergencyExitCount: 1,
  pausedSeconds: 35 * 60,
  plannedSeconds: 240 * 60,
});

assert.equal(noisyLongDay.effective, false);
assert.equal(analytics.getAnnualHeatLevel(noisyLongDay), 1);

const peakDay = analytics.calculateDailyFocusScore({
  minutes: 360,
  sessionQuality: 96,
  tasksDone: 6,
  tasksTotal: 6,
  interruptionCount: 0,
  emergencyExitCount: 0,
  pausedSeconds: 0,
  plannedSeconds: 360 * 60,
});

assert.equal(analytics.getAnnualHeatLevel({ minutes: 0, dailyFocusScore: null }), 0);
assert.equal(analytics.getAnnualHeatLevel(strongDay), 4);
assert.equal(analytics.getAnnualHeatLevel(peakDay), 5);

const summary = analytics.buildEffectiveDayProgress([
  { active: true, minutes: 210, dailyFocusScore: strongDay.score },
  { active: true, minutes: 80, dailyFocusScore: shortHighQualityDay.score },
  { active: false, minutes: 0, dailyFocusScore: null },
  { active: true, minutes: 360, dailyFocusScore: peakDay.score },
]);

assert.equal(summary.effectiveDays, 2);
assert.equal(summary.totalDays, 4);
assert.equal(summary.percent, 0.5);
assert.equal(summary.label, '2 / 4 天');

const leapYearCalendar = analytics.buildAnnualHeatmapCalendar(2024, [
  { date: '2024-02-29', minutes: 210, dailyFocusScore: strongDay.score },
  { date: '2024-12-31', minutes: 360, dailyFocusScore: peakDay.score },
]);
assert.equal(leapYearCalendar.days.length, 366);
assert.equal(leapYearCalendar.columns, 53);
assert.equal(leapYearCalendar.activeDays, 2);
assert.equal(leapYearCalendar.effectiveDays, 2);
assert.equal(leapYearCalendar.days.find((day) => day.date === '2024-02-29').heatLevel, 4);
assert.equal(leapYearCalendar.days.find((day) => day.date === '2024-12-31').heatLevel, 5);
assert.equal(leapYearCalendar.days.find((day) => day.date === '2024-01-01').heatLevel, 0);

const emptyYearCalendar = analytics.buildAnnualHeatmapCalendar(2025, []);
assert.equal(emptyYearCalendar.days.length, 365);
assert.equal(emptyYearCalendar.activeDays, 0);
assert.equal(emptyYearCalendar.effectiveDays, 0);
assert.equal(emptyYearCalendar.totalMinutes, 0);
assert.ok(emptyYearCalendar.days.every((day) => day.heatLevel === 0));

function trendDay(day, minutes, dailyFocusScore) {
  return {
    date: `2026-06-${String(day).padStart(2, '0')}`,
    minutes,
    dailyFocusScore,
    effective: minutes >= 180 && dailyFocusScore >= 60,
    active: minutes > 0,
  };
}

const upwardTrend = analytics.buildLearningTrend([
  ...Array.from({ length: 7 }, (_, index) => trendDay(index + 1, 75, 48)),
  ...Array.from({ length: 7 }, (_, index) => trendDay(index + 8, 230, 82)),
]);
assert.equal(upwardTrend.status, 'up');
assert.equal(upwardTrend.windowSize, 7);
assert.ok(upwardTrend.deltaMinutes >= 30);
assert.equal(upwardTrend.current.effectiveDays, 7);

const downwardTrend = analytics.buildLearningTrend([
  ...Array.from({ length: 7 }, (_, index) => trendDay(index + 1, 240, 86)),
  ...Array.from({ length: 7 }, (_, index) => trendDay(index + 8, 70, 46)),
]);
assert.equal(downwardTrend.status, 'down');
assert.ok(downwardTrend.deltaMinutes <= -30);
assert.equal(downwardTrend.current.effectiveDays, 0);

const flatTrend = analytics.buildLearningTrend([
  ...Array.from({ length: 7 }, (_, index) => trendDay(index + 1, 170, 66)),
  ...Array.from({ length: 7 }, (_, index) => trendDay(index + 8, 178, 67)),
]);
assert.equal(flatTrend.status, 'flat');
assert.ok(Math.abs(flatTrend.deltaMinutes) < 30);

const lessTimeEvenWithBetterScoreTrend = analytics.buildLearningTrend([
  ...Array.from({ length: 7 }, (_, index) => trendDay(index + 1, 240, 52)),
  ...Array.from({ length: 7 }, (_, index) => trendDay(index + 8, 120, 94)),
]);
assert.equal(lessTimeEvenWithBetterScoreTrend.status, 'down');
assert.equal(lessTimeEvenWithBetterScoreTrend.deltaMinutes, -120);

const moreTimeEvenWithLowerScoreTrend = analytics.buildLearningTrend([
  ...Array.from({ length: 7 }, (_, index) => trendDay(index + 1, 120, 94)),
  ...Array.from({ length: 7 }, (_, index) => trendDay(index + 8, 240, 52)),
]);
assert.equal(moreTimeEvenWithLowerScoreTrend.status, 'up');
assert.equal(moreTimeEvenWithLowerScoreTrend.deltaMinutes, 120);

const shortTrend = analytics.buildLearningTrend([
  trendDay(1, 210, 80),
  trendDay(2, 190, 72),
  trendDay(3, 0, null),
  trendDay(4, 160, 64),
  trendDay(5, 220, 88),
]);
assert.equal(shortTrend.status, 'insufficient');
assert.equal(shortTrend.windowSize, 0);

const blankDaysCountAsZeroTrend = analytics.buildLearningTrend([
  ...Array.from({ length: 7 }, (_, index) => trendDay(index + 1, 210, 90)),
  ...Array.from({ length: 6 }, (_, index) => trendDay(index + 8, 210, 90)),
  trendDay(14, 0, null),
]);
assert.equal(blankDaysCountAsZeroTrend.status, 'down');
assert.equal(blankDaysCountAsZeroTrend.current.totalDays, 7);
assert.equal(blankDaysCountAsZeroTrend.current.effectiveDays, 6);
assert.ok(blankDaysCountAsZeroTrend.current.avgFocus < 80);

assert.equal(
  analytics.shouldExcludeFromFocusTimeline({
    status: 'emergency_exited',
    endReason: 'emergency_exit',
    emergencyExitCount: 1,
  }),
  true,
);
assert.equal(
  analytics.shouldExcludeFromFocusTimeline({
    status: 'interrupted',
    endReason: 'user_marked_interrupted',
    emergencyExitCount: 0,
  }),
  false,
);
assert.equal(
  analytics.shouldExcludeFromFocusTimeline({
    status: 'finished',
    endReason: 'completed',
    emergencyExitCount: 0,
  }),
  false,
);

const timelineCandidates = analytics.filterFocusTimelineRecords([
  {
    id: 'normal-session',
    subject: '数学',
    minutes: 60,
    emergencyExitCount: 0,
    endReason: 'completed',
    status: 'finished',
  },
  {
    id: 'forgotten-overnight-emergency',
    subject: '英语',
    minutes: 720,
    emergencyExitCount: 1,
    endReason: 'emergency_exit',
    status: 'emergency_exited',
  },
  {
    id: 'paused-but-not-emergency',
    subject: '政治',
    minutes: 45,
    emergencyExitCount: 0,
    endReason: 'paused_timeout',
    status: 'interrupted',
  },
]);

assert.deepEqual(
  timelineCandidates.map((record) => record.id),
  ['normal-session', 'paused-but-not-emergency'],
);

assert.equal(typeof analytics.filterFocusTimeRecords, 'function');

const focusTimeCandidates = analytics.filterFocusTimeRecords([
  {
    id: 'valid-finished-session',
    minutes: 60,
    emergencyExitCount: 0,
    endReason: 'completed',
    status: 'finished',
  },
  {
    id: 'valid-interrupted-session',
    minutes: 45,
    emergencyExitCount: 0,
    endReason: 'user_marked_interrupted',
    status: 'interrupted',
  },
  {
    id: 'invalid-emergency-status',
    minutes: 680,
    emergencyExitCount: 0,
    endReason: 'completed',
    status: 'emergency_exited',
  },
  {
    id: 'invalid-emergency-reason',
    minutes: 720,
    emergencyExitCount: 0,
    endReason: 'emergency_exit',
    status: 'finished',
  },
  {
    id: 'invalid-emergency-count',
    minutes: 540,
    emergencyExitCount: 1,
    endReason: 'completed',
    status: 'finished',
  },
]);

assert.deepEqual(
  focusTimeCandidates.map((record) => record.id),
  ['valid-finished-session', 'valid-interrupted-session'],
);

class FakeElement {
  constructor(tag = 'div', id = '') {
    this.tag = tag;
    this.id = id;
    this.children = [];
    this.attributes = {};
    this.dataset = {};
    this.style = {};
    this.textContent = '';
    this.innerHTML = '';
    this.value = '';
    this.disabled = false;
    this.hidden = false;
    this.classList = {
      values: new Set(),
      add: (...names) => names.forEach((name) => this.classList.values.add(name)),
      remove: (...names) => names.forEach((name) => this.classList.values.delete(name)),
      toggle: (name, force) => {
        const enabled = force == null ? !this.classList.values.has(name) : Boolean(force);
        if (enabled) this.classList.values.add(name);
        else this.classList.values.delete(name);
        return enabled;
      },
    };
  }

  get firstChild() {
    return this.children[0] || null;
  }

  appendChild(child) {
    this.children.push(child);
    child.parentNode = this;
    return child;
  }

  removeChild(child) {
    const index = this.children.indexOf(child);
    if (index >= 0) this.children.splice(index, 1);
    return child;
  }

  setAttribute(name, value) {
    this.attributes[name] = String(value);
  }

  removeAttribute(name) {
    delete this.attributes[name];
  }

  addEventListener() {}
}

function createFakeDocument() {
  const elements = new Map();
  const getElementById = (id) => {
    if (!elements.has(id)) elements.set(id, new FakeElement(id.includes('chart') ? 'svg' : 'div', id));
    return elements.get(id);
  };
  const segmentButtons = ['7', '30', '90', 'all'].map((range) => {
    const button = new FakeElement('button');
    button.dataset.range = range;
    return button;
  });
  const themeButtons = ['console', 'paper', 'graphite', 'focus'].map((theme) => {
    const button = new FakeElement('button');
    button.dataset.theme = theme;
    return button;
  });
  const recordFilterButtons = ['all', 'low-focus', 'task-debt', 'long-session'].map((filter) => {
    const button = new FakeElement('button');
    button.dataset.recordFilter = filter;
    return button;
  });
  const focusPeriodButtons = ['week-prev', 'week-next', 'month-prev', 'month-next', 'year-prev', 'year-next'].map(
    (period) => {
      const button = new FakeElement('button');
      button.dataset.focusPeriod = period;
      return button;
    },
  );

  return {
    body: new FakeElement('body'),
    createElementNS: (_namespace, tag) => new FakeElement(tag),
    getElementById,
    querySelectorAll: (selector) => {
      if (selector === '.segment-button') return segmentButtons;
      if (selector === 'button[data-theme]') return themeButtons;
      if (selector === '[data-record-filter]') return recordFilterButtons;
      if (selector === '[data-focus-period]') return focusPeriodButtons;
      return [];
    },
  };
}

const appSource = await readFile(new URL('../src-tauri/dashboard/app.js', import.meta.url), 'utf8');
const appDocument = createFakeDocument();
const appSandbox = {
  console,
  document: appDocument,
  fetch: async () => ({ ok: false, status: 401 }),
  getComputedStyle: () => ({ getPropertyValue: () => '' }),
  setTimeout,
  clearTimeout,
  URLSearchParams,
  window: {
    location: { search: '?token=test' },
    DashboardAnalytics: analytics,
  },
};
vm.createContext(appSandbox);
vm.runInContext(
  `${appSource.replace('void loadProjectData();', '')}
globalThis.__dashboardTest = { state, render, els, dedupeRecords };`,
  appSandbox,
  { filename: 'app.js' },
);

const dashboard = appSandbox.__dashboardTest;
dashboard.state.activeRange = 'all';
dashboard.state.records = dashboard.dedupeRecords([
  {
    id: 'normal-60',
    date: '2026-06-20',
    subject: '数学',
    minutes: 60,
    focusScore: 88,
    tasksDone: 2,
    tasksTotal: 2,
    startHour: 9,
    status: 'finished',
    endReason: 'completed',
    emergencyExitCount: 0,
  },
  {
    id: 'forgotten-overnight',
    date: '2026-06-21',
    subject: '英语',
    minutes: 720,
    focusScore: 12,
    tasksDone: 0,
    tasksTotal: 3,
    startHour: 22,
    status: 'emergency_exited',
    endReason: 'emergency_exit',
    emergencyExitCount: 1,
  },
  {
    id: 'normal-30',
    date: '2026-06-22',
    subject: '政治',
    minutes: 30,
    focusScore: 80,
    tasksDone: 1,
    tasksTotal: 1,
    startHour: 20,
    status: 'interrupted',
    endReason: 'user_marked_interrupted',
    emergencyExitCount: 0,
  },
]);
dashboard.render('test');

assert.equal(dashboard.els.metricHours.textContent, '1.5h');
assert.match(dashboard.els.metricHoursNote.textContent, /2 条有效专注记录/);
assert.equal(dashboard.els.metricDays.textContent, '2');
assert.match(dashboard.els.statusLine.textContent, /2 条有效专注记录/);
assert.doesNotMatch(dashboard.els.metricHours.textContent, /13\.5h/);

console.log('dashboard analytics probe passed');
