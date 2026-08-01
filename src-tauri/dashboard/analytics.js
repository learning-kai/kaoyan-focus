(function attachDashboardAnalytics(global) {
  const EFFECTIVE_DAY_MINUTES = 180;
  const EFFECTIVE_DAY_SCORE = 60;
  const LEARNING_TREND_MIN_DAYS = 6;
  const LEARNING_TREND_LONG_WINDOW = 7;
  const LEARNING_TREND_MINUTE_THRESHOLD = 30;
  const STUDY_STATUS_RULES = Object.freeze({
    minimumActiveDays: 3,
    slowdownRate: 0.7,
    correctionRate: 0.9,
    qualityRisk: 65,
    interruptionRiskPerHour: 3,
  });

  function clamp(value, min, max) {
    const number = Number(value);
    if (!Number.isFinite(number)) return min;
    return Math.min(max, Math.max(min, number));
  }

  function round(value, digits = 0) {
    const factor = 10 ** digits;
    return Math.round(value * factor) / factor;
  }

  function average(values) {
    const list = values.filter((value) => Number.isFinite(value));
    if (!list.length) return 0;
    return list.reduce((total, value) => total + value, 0) / list.length;
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

  function normalizeTrendDay(day, index) {
    const minutes = clamp(day?.minutes, 0, 24 * 60);
    const dailyFocusScore =
      day?.dailyFocusScore == null && day?.score == null ? 0 : clamp(day?.dailyFocusScore ?? day?.score, 0, 100);
    return {
      date: String(day?.date ?? index + 1),
      minutes,
      dailyFocusScore,
      volumeTargetRate: clamp((minutes / EFFECTIVE_DAY_MINUTES) * 100, 0, 100),
      effective: minutes >= EFFECTIVE_DAY_MINUTES && dailyFocusScore >= EFFECTIVE_DAY_SCORE,
      active: minutes > 0,
    };
  }

  function buildTrendWindowStats(days, startIndex, size) {
    const slice = days.slice(startIndex, startIndex + size);
    const totalDays = slice.length;
    const effectiveDays = slice.filter((day) => day.effective).length;
    const avgMinutes = round(average(slice.map((day) => day.minutes)), 1);
    const avgFocus = round(average(slice.map((day) => day.dailyFocusScore)), 1);
    const volumeTargetRate = round(average(slice.map((day) => day.volumeTargetRate)), 1);
    const effectiveDensity = totalDays > 0 ? round((effectiveDays / totalDays) * 100, 1) : 0;
    return {
      startIndex,
      endIndex: startIndex + totalDays - 1,
      startDate: slice[0]?.date ?? '',
      endDate: slice[slice.length - 1]?.date ?? '',
      totalDays,
      effectiveDays,
      avgMinutes,
      avgFocus,
      volumeTargetRate,
      effectiveDensity,
      totalMinutes: round(slice.reduce((total, day) => total + day.minutes, 0), 1),
    };
  }

  function buildLearningTrendPoints(days, currentStartIndex, currentEndIndex) {
    return days.map((day, index) => {
      const start = Math.max(0, index - LEARNING_TREND_LONG_WINDOW + 1);
      const window = days.slice(start, index + 1);
      return {
        ...day,
        index,
        rollingMinutes: round(average(window.map((item) => item.minutes)), 1),
        rollingFocus: round(average(window.map((item) => item.dailyFocusScore)), 1),
        rollingVolumeRate: round(average(window.map((item) => item.volumeTargetRate)), 1),
        inCurrentWindow: index >= currentStartIndex && index <= currentEndIndex,
      };
    });
  }

  function buildLearningTrend(dailySeries) {
    const days = (Array.isArray(dailySeries) ? dailySeries : []).map(normalizeTrendDay);
    if (days.length < LEARNING_TREND_MIN_DAYS) {
      return {
        status: 'insufficient',
        label: '样本不足',
        delta: 0,
        deltaMinutes: 0,
        deltaPercent: null,
        thresholdMinutes: LEARNING_TREND_MINUTE_THRESHOLD,
        windowSize: 0,
        windowLabel: `至少需要 ${LEARNING_TREND_MIN_DAYS} 天`,
        previous: null,
        current: null,
        points: buildLearningTrendPoints(days, -1, -1),
      };
    }

    const windowSize =
      days.length >= LEARNING_TREND_LONG_WINDOW * 2
        ? LEARNING_TREND_LONG_WINDOW
        : Math.max(3, Math.floor(days.length / 2));
    const currentStartIndex = days.length - windowSize;
    const previousStartIndex = currentStartIndex - windowSize;
    const previous = buildTrendWindowStats(days, previousStartIndex, windowSize);
    const current = buildTrendWindowStats(days, currentStartIndex, windowSize);
    const deltaMinutes = round(current.avgMinutes - previous.avgMinutes, 1);
    const deltaPercent =
      previous.avgMinutes > 0
        ? round(deltaMinutes / previous.avgMinutes, 3)
        : current.avgMinutes > 0
          ? 1
          : 0;
    const status =
      deltaMinutes >= LEARNING_TREND_MINUTE_THRESHOLD
        ? 'up'
        : deltaMinutes <= -LEARNING_TREND_MINUTE_THRESHOLD
          ? 'down'
          : 'flat';
    const labels = {
      up: '专注时间上升',
      down: '专注时间下滑',
      flat: '专注时间持平',
    };

    return {
      status,
      label: labels[status],
      delta: deltaMinutes,
      deltaMinutes,
      deltaPercent,
      thresholdMinutes: LEARNING_TREND_MINUTE_THRESHOLD,
      windowSize,
      windowLabel: `${current.totalDays} 天对比 ${previous.totalDays} 天`,
      previous,
      current,
      points: buildLearningTrendPoints(days, current.startIndex, current.endIndex),
    };
  }

  function normalizeStudyDay(day) {
    const plannedMinutes = clamp(day?.plannedMinutes ?? day?.planned_minutes, 0, 24 * 60);
    const actualMinutes = clamp(day?.actualMinutes ?? day?.actual_minutes ?? day?.minutes, 0, 24 * 60);
    const quality = day?.quality == null ? null : clamp(day.quality, 0, 100);
    return {
      date: String(day?.date ?? ''),
      plannedMinutes,
      actualMinutes,
      quality,
      interruptionCount: clamp(day?.interruptionCount ?? day?.interruption_count, 0, 1000),
      emergencyExitCount: clamp(day?.emergencyExitCount ?? day?.emergency_exit_count, 0, 1000),
      pausedSeconds: clamp(day?.pausedSeconds ?? day?.paused_seconds, 0, 7 * 24 * 3600),
    };
  }

  function buildStudyDiagnosis(input = {}) {
    const days = (Array.isArray(input.days) ? input.days : []).map(normalizeStudyDay);
    const subjects = Array.isArray(input.subjects) ? input.subjects : [];
    const plannedMinutes = round(days.reduce((total, day) => total + day.plannedMinutes, 0), 1);
    const actualMinutes = round(days.reduce((total, day) => total + day.actualMinutes, 0), 1);
    const plannedDays = days.filter((day) => day.plannedMinutes > 0).length;
    const activeDays = days.filter((day) => day.actualMinutes > 0).length;
    const qualityDays = days.filter((day) => day.quality != null && day.actualMinutes > 0);
    const quality = qualityDays.length ? round(average(qualityDays.map((day) => day.quality)), 1) : null;
    const interruptions = days.reduce((total, day) => total + day.interruptionCount, 0);
    const emergencyExits = days.reduce((total, day) => total + day.emergencyExitCount, 0);
    const actualHours = Math.max(actualMinutes / 60, 0.25);
    const interruptionRate = round(interruptions / actualHours, 2);
    const planRate = plannedMinutes > 0 ? round(actualMinutes / plannedMinutes, 3) : null;
    const subjectGaps = subjects
      .map((item) => {
        const planned = clamp(item?.plannedMinutes ?? item?.planned_minutes, 0, 24 * 60);
        const actual = clamp(item?.actualMinutes ?? item?.actual_minutes, 0, 24 * 60);
        return {
          subject: String(item?.subject ?? '未命名科目'),
          plannedMinutes: planned,
          actualMinutes: actual,
          rate: planned > 0 ? round(actual / planned, 3) : null,
        };
      })
      .filter((item) => item.plannedMinutes > 0)
      .sort((left, right) => (left.rate ?? 0) - (right.rate ?? 0));
    const subjectGap = subjectGaps.find((item) => item.rate < STUDY_STATUS_RULES.slowdownRate);
    const hasQualityRisk =
      (quality != null && quality < STUDY_STATUS_RULES.qualityRisk) ||
      emergencyExits > 0 ||
      interruptionRate >= STUDY_STATUS_RULES.interruptionRiskPerHour;

    let status = 'insufficient';
    if (plannedDays >= STUDY_STATUS_RULES.minimumActiveDays && activeDays >= STUDY_STATUS_RULES.minimumActiveDays) {
      if (planRate >= STUDY_STATUS_RULES.correctionRate && hasQualityRisk) status = 'hard_push';
      else if (planRate < STUDY_STATUS_RULES.slowdownRate) status = 'slowing';
      else if (planRate < STUDY_STATUS_RULES.correctionRate || subjectGap || hasQualityRisk) status = 'correction';
      else status = 'steady';
    }

    const labels = {
      insufficient: '数据不足',
      steady: '稳定推进',
      correction: '需要纠偏',
      slowing: '明显失速',
      hard_push: '低效硬撑',
    };
    const reasons = [];
    if (status === 'insufficient') {
      reasons.push(plannedDays < STUDY_STATUS_RULES.minimumActiveDays ? '本周可用计划不足 3 天' : '本周有效学习不足 3 天');
    } else {
      reasons.push(`已完成计划 ${Math.round((planRate || 0) * 100)}%`);
      if (quality != null) reasons.push(`专注质量 ${Math.round(quality)} 分`);
      if (subjectGap) reasons.push(`${subjectGap.subject} 只完成计划的 ${Math.round(subjectGap.rate * 100)}%`);
      if (emergencyExits > 0) reasons.push(`${emergencyExits} 次应急退出拉低稳定性`);
      else if (interruptionRate >= STUDY_STATUS_RULES.interruptionRiskPerHour) reasons.push(`每小时约 ${interruptionRate} 次打断`);
      if (reasons.length < 2) reasons.push(`${activeDays} 天有有效学习记录`);
    }

    let action = '先补齐计划缺口最大的科目';
    if (status === 'insufficient') action = '先连续记录 3 天，再判断学习节奏';
    if (status === 'steady') action = '保持当前节奏，优先守住连续性';
    if (status === 'hard_push') action = '缩短单次目标，先降低打断和暂停';
    if (status === 'correction' && !subjectGap) action = '下一个学习时段按计划完成，不再临时换科';

    return {
      status,
      label: labels[status],
      headline: `本周${labels[status]}`,
      reasons: reasons.slice(0, 3),
      action,
      plannedMinutes,
      actualMinutes,
      planRate,
      quality,
      plannedDays,
      activeDays,
      interruptionRate,
      subjectGaps,
      riskTags: [
        ...(subjectGap ? ['subject-gap'] : []),
        ...(hasQualityRisk ? ['quality-risk'] : []),
        ...(activeDays < plannedDays ? ['continuity-gap'] : []),
      ],
    };
  }

  function shouldExcludeFromFocusTimeStats(record) {
    if (!record) return false;
    const status = String(record.status ?? '').trim().toLowerCase();
    const endReason = String(record.endReason ?? record.end_reason ?? '')
      .trim()
      .toLowerCase();
    const emergencyExitCount = Number(record.emergencyExitCount ?? record.emergency_exit_count ?? 0);
    return status === 'emergency_exited' || endReason === 'emergency_exit' || emergencyExitCount > 0;
  }

  function filterFocusTimeRecords(records) {
    return (Array.isArray(records) ? records : []).filter((record) => !shouldExcludeFromFocusTimeStats(record));
  }

  function shouldExcludeFromFocusTimeline(record) {
    return shouldExcludeFromFocusTimeStats(record);
  }

  function filterFocusTimelineRecords(records) {
    return filterFocusTimeRecords(records);
  }

  global.DashboardAnalytics = {
    EFFECTIVE_DAY_MINUTES,
    EFFECTIVE_DAY_SCORE,
    buildAnnualHeatmapCalendar,
    buildEffectiveDayProgress,
    buildLearningTrend,
    buildStudyDiagnosis,
    calculateDailyFocusScore,
    filterFocusTimeRecords,
    filterFocusTimelineRecords,
    getAnnualHeatLevel,
    isEffectiveDay,
    shouldExcludeFromFocusTimeStats,
    shouldExcludeFromFocusTimeline,
    STUDY_STATUS_RULES,
  };
})(typeof window === 'undefined' ? globalThis : window);
