(function attachDashboardAnalytics(global) {
  const EFFECTIVE_DAY_MINUTES = 180;
  const EFFECTIVE_DAY_SCORE = 60;

  function clamp(value, min, max) {
    const number = Number(value);
    if (!Number.isFinite(number)) return min;
    return Math.min(max, Math.max(min, number));
  }

  function round(value, digits = 0) {
    const factor = 10 ** digits;
    return Math.round(value * factor) / factor;
  }

  function calculateDailyFocusScore(input) {
    const minutes = clamp(input?.minutes, 0, 24 * 60);
    const sessionQuality = clamp(input?.sessionQuality, 0, 100);
    const tasksTotal = clamp(input?.tasksTotal, 0, 1000);
    const tasksDone = clamp(input?.tasksDone, 0, tasksTotal || 1000);
    const taskQuality = tasksTotal > 0 ? clamp((tasksDone / tasksTotal) * 100, 0, 100) : 72;
    const volumeQuality = clamp((minutes / EFFECTIVE_DAY_MINUTES) * 100, 0, 100);
    const plannedSeconds = Math.max(60, Number(input?.plannedSeconds) || minutes * 60 || 60);
    const pausedSeconds = clamp(input?.pausedSeconds, 0, 7 * 24 * 3600);
    const interruptionCount = clamp(input?.interruptionCount, 0, 1000);
    const emergencyExitCount = clamp(input?.emergencyExitCount, 0, 1000);
    const activeHours = Math.max(0.25, minutes / 60);
    const interruptionLoad = interruptionCount / activeHours;
    const pauseRatio = pausedSeconds / plannedSeconds;
    const continuityQuality = clamp(100 - interruptionLoad * 14 - emergencyExitCount * 35 - pauseRatio * 60, 0, 100);
    const score = round(
      sessionQuality * 0.4 + volumeQuality * 0.3 + taskQuality * 0.2 + continuityQuality * 0.1,
    );

    return {
      minutes,
      dailyFocusScore: score,
      score,
      effective: minutes >= EFFECTIVE_DAY_MINUTES && score >= EFFECTIVE_DAY_SCORE,
      parts: {
        session: round(sessionQuality),
        volume: round(volumeQuality),
        task: round(taskQuality),
        continuity: round(continuityQuality),
      },
    };
  }

  function isEffectiveDay(day) {
    return Boolean(
      day &&
        day.minutes >= EFFECTIVE_DAY_MINUTES &&
        Number.isFinite(Number(day.dailyFocusScore)) &&
        Number(day.dailyFocusScore) >= EFFECTIVE_DAY_SCORE,
    );
  }

  function getAnnualHeatLevel(day) {
    if (!day || Number(day.minutes) <= 0) return 0;
    const score = Number(day.dailyFocusScore ?? day.score ?? 0);
    const minutes = Number(day.minutes) || 0;
    if (score < EFFECTIVE_DAY_SCORE) return 1;
    if (!isEffectiveDay({ minutes, dailyFocusScore: score })) return 2;
    if (score >= 90 && minutes >= 300) return 5;
    if (score >= 75 && minutes >= 210) return 4;
    return 3;
  }

  function buildEffectiveDayProgress(dailySeries) {
    const days = Array.isArray(dailySeries) ? dailySeries : [];
    const totalDays = days.length;
    const effectiveDays = days.filter(isEffectiveDay).length;
    return {
      effectiveDays,
      totalDays,
      percent: totalDays > 0 ? effectiveDays / totalDays : 0,
      label: `${effectiveDays} / ${totalDays} 天`,
      standard: `每天 ${EFFECTIVE_DAY_MINUTES / 60} 小时 + 日有效度 ${EFFECTIVE_DAY_SCORE} 分`,
    };
  }

  function toDateKey(date) {
    const year = date.getFullYear();
    const month = String(date.getMonth() + 1).padStart(2, '0');
    const day = String(date.getDate()).padStart(2, '0');
    return `${year}-${month}-${day}`;
  }

  function mondayWeekday(date) {
    return (date.getDay() + 6) % 7;
  }

  function addDays(date, delta) {
    const next = new Date(date);
    next.setDate(next.getDate() + delta);
    return next;
  }

  function daysBetween(left, right) {
    const leftUtc = Date.UTC(left.getFullYear(), left.getMonth(), left.getDate());
    const rightUtc = Date.UTC(right.getFullYear(), right.getMonth(), right.getDate());
    return Math.round((rightUtc - leftUtc) / 86400000);
  }

  function buildAnnualHeatmapCalendar(year, dailyEntries) {
    const start = new Date(year, 0, 1);
    const end = new Date(year + 1, 0, 1);
    const dayCount = daysBetween(start, end);
    const startWeekday = mondayWeekday(start);
    const source = Array.isArray(dailyEntries)
      ? dailyEntries
      : Object.entries(dailyEntries || {}).map(([date, value]) => ({ ...value, date }));
    const byDate = new Map(source.filter((item) => item?.date).map((item) => [item.date, item]));
    const days = [];

    for (let index = 0; index < dayCount; index += 1) {
      const date = addDays(start, index);
      const dateKey = toDateKey(date);
      const sourceDay = byDate.get(dateKey);
      const minutes = Number(sourceDay?.minutes) || 0;
      const dailyFocusScore = sourceDay?.dailyFocusScore ?? sourceDay?.score ?? null;
      const day = {
        ...(sourceDay || {}),
        date: dateKey,
        row: mondayWeekday(date),
        column: Math.floor((index + startWeekday) / 7),
        minutes,
        dailyFocusScore,
      };
      days.push({
        ...day,
        effective: isEffectiveDay(day),
        heatLevel: getAnnualHeatLevel(day),
      });
    }

    return {
      year,
      days,
      columns: Math.ceil((dayCount + startWeekday) / 7),
      activeDays: days.filter((day) => day.minutes > 0).length,
      effectiveDays: days.filter(isEffectiveDay).length,
      totalMinutes: days.reduce((total, day) => total + day.minutes, 0),
      maxHeatValue: 5,
    };
  }

  global.DashboardAnalytics = {
    EFFECTIVE_DAY_MINUTES,
    EFFECTIVE_DAY_SCORE,
    buildAnnualHeatmapCalendar,
    buildEffectiveDayProgress,
    calculateDailyFocusScore,
    getAnnualHeatLevel,
    isEffectiveDay,
  };
})(typeof window === 'undefined' ? globalThis : window);
