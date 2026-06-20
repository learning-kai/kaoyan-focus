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

console.log('dashboard analytics probe passed');
