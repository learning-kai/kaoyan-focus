import { useEffect, useMemo, useState } from 'react';
import {
  BarChart3,
  CalendarDays,
  Clock3,
  Copy,
  ExternalLink,
  Download,
  Pencil,
  RefreshCw,
  RotateCcw,
  Search,
  ShieldAlert,
  TimerReset,
  Trash2,
} from 'lucide-react';
import {
  deleteFocusSession,
  getFocusStatsSummary,
  listFocusSessions,
  listSubjects,
  updateFocusSessionSubject,
} from '../services/focusApi';
import { listInterruptionSummary } from '../services/monitorApi';
import { openStudyDashboard } from '../services/systemApi';
import { useConfirmDialog } from '../hooks/useConfirmDialog';
import type { FocusSession, FocusStatsSummary, FocusStatus, Subject } from '../types/focus';
import type { InterruptionSummary } from '../types/monitor';
import { copyTextToClipboard } from '../utils/clipboard';
import { downloadTextFile } from '../utils/fileDownload';
import { formatDateKey } from '../utils/date';

const RECENT_SESSION_LIMIT = 100;
type SessionStatusFilter = 'all' | FocusStatus;
type SessionSortMode = 'recent' | 'longest' | 'shortest';
type SessionSubjectFilter = 'all' | 'none' | string;

function formatStudyTime(seconds: number) {
  if (seconds < 3600) {
    return `${Math.round(seconds / 60)} 分钟`;
  }

  const hours = seconds / 3600;
  return `${Number.isInteger(hours) ? hours.toFixed(0) : hours.toFixed(1)} 小时`;
}

function formatDateTime(value: string | null) {
  if (!value) return '未记录';
  return new Date(value).toLocaleString();
}

function formatTimeOnly(value: string | null) {
  if (!value) return '未记录';
  return new Date(value).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
}

function formatSessionTimeRange(session: FocusSession) {
  const start = new Date(session.started_at);
  if (!session.ended_at) {
    return `${formatDateTime(session.started_at)} - 未记录结束`;
  }

  const end = new Date(session.ended_at);
  return start.toDateString() === end.toDateString()
    ? `${formatDateTime(session.started_at)} - ${formatTimeOnly(session.ended_at)}`
    : `${formatDateTime(session.started_at)} - ${formatDateTime(session.ended_at)}`;
}

function getSessionStatusLabel(status: string) {
  const labels: Record<string, string> = {
    idle: '待开始',
    running: '进行中',
    finished: '已完成',
    interrupted: '已中断',
    emergency_exited: '已退出',
  };

  return labels[status] ?? status;
}

function getSessionDuration(session: FocusSession) {
  return session.actual_seconds || session.planned_seconds;
}

function compareSessions(left: FocusSession, right: FocusSession, sortMode: SessionSortMode) {
  const leftDuration = getSessionDuration(left);
  const rightDuration = getSessionDuration(right);
  const leftStart = new Date(left.started_at).getTime();
  const rightStart = new Date(right.started_at).getTime();

  if (sortMode === 'longest') {
    return rightDuration - leftDuration || rightStart - leftStart || right.id - left.id;
  }

  if (sortMode === 'shortest') {
    return leftDuration - rightDuration || rightStart - leftStart || right.id - left.id;
  }

  return rightStart - leftStart || rightDuration - leftDuration || right.id - left.id;
}

function normalizeSearch(value: string) {
  return value.trim().toLowerCase();
}

function buildSessionSearchTarget(session: FocusSession, subjectName: string) {
  return [
    subjectName,
    getSessionStatusLabel(session.status),
    session.end_reason ?? '',
    session.started_at,
    session.ended_at ?? '',
    formatSessionTimeRange(session),
    formatStudyTime(getSessionDuration(session)),
  ]
    .join(' ')
    .toLowerCase();
}

function getFocusModeLabel(mode: FocusSession['mode']) {
  return mode === 'strict' ? '强制模式' : '普通模式';
}

function escapeCsvCell(value: string) {
  const normalized = value.replace(/\r?\n/g, ' ');
  return /[",\n]/.test(normalized) ? `"${normalized.replace(/"/g, '""')}"` : normalized;
}

function buildSessionsCsv(sessions: FocusSession[], subjectNameMap: Map<number, string>) {
  const headers = [
    '记录ID',
    '状态',
    '模式',
    '科目',
    '计划秒数',
    '实际秒数',
    '开始时间',
    '结束时间',
    '时间段',
    '结束原因',
    '干扰次数',
    '紧急退出次数',
  ];

  const rows = sessions.map((session) => {
    const subjectName = session.subject_id ? subjectNameMap.get(session.subject_id) ?? '未指定科目' : '未指定科目';
    return [
      String(session.id),
      getSessionStatusLabel(session.status),
      getFocusModeLabel(session.mode),
      subjectName,
      String(session.planned_seconds),
      String(session.actual_seconds),
      session.started_at,
      session.ended_at ?? '',
      formatSessionTimeRange(session),
      session.end_reason ?? '',
      String(session.interruption_count),
      String(session.emergency_exit_count),
    ];
  });

  return '\ufeff' + [headers, ...rows].map((row) => row.map(escapeCsvCell).join(',')).join('\r\n');
}

function buildStatsSummaryText(args: {
  stats: FocusStatsSummary | null;
  interruptions: InterruptionSummary[];
  filteredSessions: FocusSession[];
  sessionSearch: string;
  sessionStatusFilter: SessionStatusFilter;
  sessionSubjectFilter: SessionSubjectFilter;
  sessionSortMode: SessionSortMode;
  subjectNameMap: Map<number, string>;
  totalSessions: number;
  filteredSessionSeconds: number;
}) {
  const {
    stats,
    interruptions,
    filteredSessions,
    sessionSearch,
    sessionStatusFilter,
    sessionSubjectFilter,
    sessionSortMode,
    subjectNameMap,
    totalSessions,
    filteredSessionSeconds,
  } = args;

  const filterStatusLabel: Record<SessionStatusFilter, string> = {
    all: '全部状态',
    idle: '待开始',
    running: '进行中',
    finished: '已完成',
    interrupted: '已中断',
    emergency_exited: '已退出',
  };

  const filterSubjectLabel =
    sessionSubjectFilter === 'all'
      ? '全部科目'
      : sessionSubjectFilter === 'none'
        ? '未指定科目'
        : subjectNameMap.get(Number(sessionSubjectFilter)) ?? '未知科目';

  const filterSortLabel: Record<SessionSortMode, string> = {
    recent: '最近优先',
    longest: '最长优先',
    shortest: '最短优先',
  };

  const subjectHighlights = stats?.subjects.slice(0, 3).map((item) => `${item.subject.name} ${formatStudyTime(item.total_seconds)}`).join('；') || '暂无';
  const interruptionHighlights = interruptions.slice(0, 3).map((item) => `${item.process_name} ${item.interruption_count} 次`).join('；') || '暂无';
  const recentHighlights = filteredSessions.slice(0, 8).map((session) => {
    const subjectName = session.subject_id ? subjectNameMap.get(session.subject_id) ?? '未指定科目' : '未指定科目';
    return `- ${formatStudyTime(getSessionDuration(session))} · ${subjectName} · ${getSessionStatusLabel(session.status)} · ${formatSessionTimeRange(session)}`;
  });

  return [
    '考研专注学习统计摘要',
    `生成时间：${new Date().toLocaleString()}`,
    `学习总览：今日 ${formatStudyTime(stats?.today_seconds ?? 0)} / 本周 ${formatStudyTime(stats?.week_seconds ?? 0)} / 本月 ${formatStudyTime(stats?.month_seconds ?? 0)} / 累计干扰 ${stats?.interruption_count ?? 0} 次`,
    `筛选条件：关键词 ${sessionSearch.trim() || '无'}；状态 ${filterStatusLabel[sessionStatusFilter]}; 科目 ${filterSubjectLabel}; 排序 ${filterSortLabel[sessionSortMode]}; 显示 ${filteredSessions.length}/${totalSessions} 条；筛选后累计 ${formatStudyTime(filteredSessionSeconds)}`,
    `科目分布：${subjectHighlights}`,
    `干扰排行：${interruptionHighlights}`,
    '最近学习记录：',
    ...(recentHighlights.length > 0 ? recentHighlights : ['- 暂无记录']),
  ].join('\n');
}

export default function StatsPage() {
  const { confirm, confirmDialog } = useConfirmDialog();
  const [stats, setStats] = useState<FocusStatsSummary | null>(null);
  const [interruptions, setInterruptions] = useState<InterruptionSummary[]>([]);
  const [sessions, setSessions] = useState<FocusSession[]>([]);
  const [subjects, setSubjects] = useState<Subject[]>([]);
  const [savingSessionId, setSavingSessionId] = useState<number | null>(null);
  const [deletingSessionId, setDeletingSessionId] = useState<number | null>(null);
  const [loadingStats, setLoadingStats] = useState(false);
  const [openingDashboard, setOpeningDashboard] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [sessionSearch, setSessionSearch] = useState('');
  const [sessionStatusFilter, setSessionStatusFilter] = useState<SessionStatusFilter>('all');
  const [sessionSubjectFilter, setSessionSubjectFilter] = useState<SessionSubjectFilter>('all');
  const [sessionSortMode, setSessionSortMode] = useState<SessionSortMode>('recent');
  const [copyingSummary, setCopyingSummary] = useState(false);
  const [exportingCsv, setExportingCsv] = useState(false);

  const subjectNameMap = useMemo(() => new Map(subjects.map((subject) => [subject.id, subject.name])), [subjects]);
  const maxSubjectSeconds = useMemo(() => {
    const seconds = stats?.subjects.map((item) => item.total_seconds) ?? [];
    return Math.max(...seconds, 1);
  }, [stats]);

  const filteredSessions = useMemo(() => {
    const query = normalizeSearch(sessionSearch);
    const subjectFilter =
      sessionSubjectFilter === 'all' ? 'all' : sessionSubjectFilter === 'none' ? 'none' : Number(sessionSubjectFilter);

    return [...sessions]
      .filter((session) => {
        if (sessionStatusFilter !== 'all' && session.status !== sessionStatusFilter) {
          return false;
        }

        if (subjectFilter === 'none') {
          if (session.subject_id !== null) {
            return false;
          }
        } else if (subjectFilter !== 'all' && session.subject_id !== subjectFilter) {
          return false;
        }

        if (!query) {
          return true;
        }

        const subjectName = session.subject_id ? subjectNameMap.get(session.subject_id) ?? '未指定科目' : '未指定科目';
        return buildSessionSearchTarget(session, subjectName).includes(query);
      })
      .sort((left, right) => compareSessions(left, right, sessionSortMode));
  }, [sessionSearch, sessionSortMode, sessionStatusFilter, sessionSubjectFilter, sessions, subjectNameMap]);

  const filteredSessionSeconds = useMemo(
    () => filteredSessions.reduce((total, session) => total + getSessionDuration(session), 0),
    [filteredSessions],
  );
  const filteredSessionAverageSeconds = filteredSessions.length > 0 ? Math.round(filteredSessionSeconds / filteredSessions.length) : 0;

  const hasSessionFilters =
    sessionSearch.trim() !== '' || sessionStatusFilter !== 'all' || sessionSubjectFilter !== 'all' || sessionSortMode !== 'recent';

  useEffect(() => {
    void refreshStats();
  }, []);

  async function refreshStats(showLoading = true) {
    try {
      if (showLoading) {
        setLoadingStats(true);
      }
      setError(null);
      const [statsData, interruptionData, sessionData, subjectData] = await Promise.all([
        getFocusStatsSummary(),
        listInterruptionSummary(),
        listFocusSessions(RECENT_SESSION_LIMIT),
        listSubjects(),
      ]);
      setStats(statsData);
      setInterruptions(interruptionData);
      setSessions(sessionData);
      setSubjects(subjectData);
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      if (showLoading) {
        setLoadingStats(false);
      }
    }
  }

  async function handleSubjectChange(sessionId: number, value: string) {
    try {
      setSavingSessionId(sessionId);
      setError(null);
      setMessage(null);
      const subjectId = value === '' ? null : Number(value);
      const updated = await updateFocusSessionSubject(sessionId, subjectId);
      setSessions((current) => current.map((session) => (session.id === sessionId ? updated : session)));
      const nextStats = await getFocusStatsSummary();
      setStats(nextStats);
      setMessage('学习记录科目已更新。');
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setSavingSessionId(null);
    }
  }

  async function handleDeleteSession(sessionId: number) {
    const confirmed = await confirm({
      confirmLabel: '删除记录',
      message: '删除后今日、本周、本月与科目统计会立刻重新计算，这条记录无法从统计页恢复。',
      title: '删除学习记录？',
      tone: 'danger',
    });
    if (!confirmed) {
      return;
    }

    try {
      setDeletingSessionId(sessionId);
      setError(null);
      setMessage(null);
      await deleteFocusSession(sessionId);
      await refreshStats(false);
      setMessage('学习记录已删除。');
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setDeletingSessionId(null);
    }
  }

  async function handleOpenDashboard() {
    try {
      setOpeningDashboard(true);
      setError(null);
      setMessage(null);
      const launch = await openStudyDashboard();
      setMessage(`学习数据看板已打开：${launch.url}`);
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setOpeningDashboard(false);
    }
  }

  async function handleCopySummary() {
    try {
      setCopyingSummary(true);
      setError(null);
      setMessage(null);
      const summaryText = buildStatsSummaryText({
        stats,
        interruptions,
        filteredSessions,
        sessionSearch,
        sessionStatusFilter,
        sessionSubjectFilter,
        sessionSortMode,
        subjectNameMap,
        totalSessions: sessions.length,
        filteredSessionSeconds,
      });
      await copyTextToClipboard(summaryText);
      setMessage('统计摘要已复制到剪贴板。');
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setCopyingSummary(false);
    }
  }

  async function handleExportFilteredSessions() {
    try {
      setExportingCsv(true);
      setError(null);
      setMessage(null);
      const csv = buildSessionsCsv(filteredSessions, subjectNameMap);
      downloadTextFile(`kaoyan-focus-sessions-${formatDateKey()}.csv`, csv);
      setMessage(`已导出 ${filteredSessions.length} 条学习记录。`);
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setExportingCsv(false);
    }
  }

  function resetSessionFilters() {
    setSessionSearch('');
    setSessionStatusFilter('all');
    setSessionSubjectFilter('all');
    setSessionSortMode('recent');
  }

  return (
    <section className="page-shell stats-shell">
      <header className="page-header">
        <div>
          <p className="eyebrow">Local Analytics</p>
          <h2>学习统计</h2>
          <p>按今日、本周、本月和科目汇总本地专注记录，同时保留非白名单干扰排行。</p>
        </div>
        <div className="page-header-actions">
          <button
            className="secondary-action"
            disabled={openingDashboard}
            onClick={() => void handleOpenDashboard()}
            type="button"
          >
            <ExternalLink size={17} />
            {openingDashboard ? '打开中' : '打开数据看板'}
          </button>
          <button
            className="secondary-action"
            disabled={loadingStats}
            onClick={() => void refreshStats()}
            type="button"
          >
            <RefreshCw size={17} />
            {loadingStats ? '刷新中' : '刷新'}
          </button>
        </div>
      </header>

      {error && (
        <p className="alert error" role="alert">
          {error}
        </p>
      )}
      {message && (
        <p className="alert success" aria-live="polite">
          {message}
        </p>
      )}
      {loadingStats && (
        <p className="alert neutral" aria-live="polite">
          正在更新统计数据...
        </p>
      )}
      {confirmDialog}

      <div className="stats-hero-grid">
        <MetricCard icon={Clock3} label="今日学习" value={formatStudyTime(stats?.today_seconds ?? 0)} />
        <MetricCard icon={CalendarDays} label="本周学习" value={formatStudyTime(stats?.week_seconds ?? 0)} />
        <MetricCard icon={BarChart3} label="本月学习" value={formatStudyTime(stats?.month_seconds ?? 0)} />
        <MetricCard danger icon={ShieldAlert} label="累计干扰" value={`${stats?.interruption_count ?? 0} 次`} />
      </div>

      <div className="stats-board">
        <section className="command-panel">
          <div className="panel-title">
            <div>
              <p className="eyebrow">Interruptions</p>
              <h3>干扰排行</h3>
            </div>
            <ShieldAlert size={20} />
          </div>

          {interruptions.length === 0 ? (
            <div className="empty-state">
              <strong>暂无干扰记录</strong>
              <p>专注期间检测到非白名单应用后，这里会显示最常打断你的软件。</p>
            </div>
          ) : (
            <div className="rule-list">
              {interruptions.map((item) => (
                <article className="list-row interruption-row" key={item.process_name}>
                  <div className="row-main">
                    <span className="row-icon danger">
                      <ShieldAlert size={18} />
                    </span>
                    <div>
                      <strong>{item.process_name}</strong>
                      <p>{item.window_title || '无窗口标题'}</p>
                      {item.process_path && <p>{item.process_path}</p>}
                      <p>最近：{new Date(item.last_interrupted_at).toLocaleString()}</p>
                    </div>
                  </div>
                  <strong className="count-pill">{item.interruption_count} 次</strong>
                </article>
              ))}
            </div>
          )}
        </section>

        <section className="command-panel">
          <div className="panel-title">
            <div>
              <p className="eyebrow">Subjects</p>
              <h3>科目分布</h3>
            </div>
            <BarChart3 size={20} />
          </div>

          {!stats || stats.subjects.length === 0 ? (
            <div className="empty-state">
              <strong>暂无科目统计</strong>
              <p>完成一轮带科目的专注后，这里会显示各科累计学习时长。</p>
            </div>
          ) : (
            <div className="subject-bars">
              {stats.subjects.map((item) => {
                const width = Math.max(6, (item.total_seconds / maxSubjectSeconds) * 100);

                return (
                  <article className="subject-stat-row" key={item.subject.id}>
                    <div>
                      <span className="subject-dot" style={{ backgroundColor: item.subject.color ?? '#8fb5ff' }} />
                      <strong>{item.subject.name}</strong>
                      <small>{item.subject.enabled ? '已启用' : '已停用'}</small>
                    </div>
                    <div className="subject-bar">
                      <i style={{ width: `${width}%`, backgroundColor: item.subject.color ?? '#8fb5ff' }} />
                    </div>
                    <strong>{formatStudyTime(item.total_seconds)}</strong>
                  </article>
                );
              })}
            </div>
          )}
        </section>
      </div>

      <section className="command-panel stats-toolbar">
        <div className="panel-title">
          <div>
            <p className="eyebrow">Records</p>
            <h3>筛选学习记录</h3>
          </div>
          <Search size={20} />
        </div>

        <div className="stats-toolbar-grid">
          <label className="field-block stats-search-field">
            <span>关键词</span>
            <input
              className="text-input"
              placeholder="搜索科目、状态、时间、结束原因"
              value={sessionSearch}
              onChange={(event) => setSessionSearch(event.target.value)}
            />
          </label>

          <label className="field-block">
            <span>状态</span>
            <select
              className="select-input"
              onChange={(event) => setSessionStatusFilter(event.target.value as SessionStatusFilter)}
              value={sessionStatusFilter}
            >
              <option value="all">全部状态</option>
              <option value="idle">待开始</option>
              <option value="running">进行中</option>
              <option value="finished">已完成</option>
              <option value="interrupted">已中断</option>
              <option value="emergency_exited">已退出</option>
            </select>
          </label>

          <label className="field-block">
            <span>科目</span>
            <select
              className="select-input"
              onChange={(event) => setSessionSubjectFilter(event.target.value)}
              value={sessionSubjectFilter}
            >
              <option value="all">全部科目</option>
              <option value="none">未指定科目</option>
              {subjects.map((subject) => (
                <option key={subject.id} value={subject.id}>
                  {subject.name}
                </option>
              ))}
            </select>
          </label>

          <label className="field-block">
            <span>排序</span>
            <select
              className="select-input"
              onChange={(event) => setSessionSortMode(event.target.value as SessionSortMode)}
              value={sessionSortMode}
            >
              <option value="recent">最近优先</option>
              <option value="longest">最长优先</option>
              <option value="shortest">最短优先</option>
            </select>
          </label>
        </div>

        <div className="stats-toolbar-actions">
          <button className="secondary-action" disabled={copyingSummary || loadingStats} onClick={() => void handleCopySummary()} type="button">
            <Copy size={16} />
            {copyingSummary ? '复制中' : '复制摘要'}
          </button>
          <button className="ghost-action" disabled={exportingCsv || filteredSessions.length === 0} onClick={() => void handleExportFilteredSessions()} type="button">
            <Download size={16} />
            {exportingCsv ? '导出中' : '导出 CSV'}
          </button>
          <button className="secondary-action" disabled={!hasSessionFilters} onClick={resetSessionFilters} type="button">
            <RotateCcw size={16} />
            重置筛选
          </button>
          <span className="board-title-meta">
            <span>
              显示 {filteredSessions.length} / {sessions.length} 条 · 筛选后 {formatStudyTime(filteredSessionSeconds)} · 平均 {formatStudyTime(filteredSessionAverageSeconds)}
            </span>
          </span>
        </div>
      </section>

      <section className="command-panel">
        <div className="panel-title">
          <div>
            <p className="eyebrow">Records</p>
            <h3>最近学习记录（最多 {RECENT_SESSION_LIMIT} 条）</h3>
          </div>
          <Pencil size={20} />
        </div>

        {filteredSessions.length === 0 ? (
          <div className="empty-state">
            <strong>{sessions.length === 0 ? '暂无学习记录' : '没有符合筛选条件的记录'}</strong>
            <p>
              {sessions.length === 0
                ? '完成一轮番茄钟后，可以在这里补改科目。'
                : '试着放宽状态、科目或关键词条件。'}
            </p>
            {hasSessionFilters && sessions.length > 0 && (
              <button className="ghost-action" onClick={resetSessionFilters} type="button">
                <RotateCcw size={15} />
                清空筛选
              </button>
            )}
          </div>
        ) : (
          <div className="records-table">
            {filteredSessions.map((session) => (
              <article className="record-row" key={session.id}>
                <div className="record-time">
                  <span className="row-icon enabled">
                    <TimerReset size={18} />
                  </span>
                  <div>
                    <strong>{formatStudyTime(getSessionDuration(session))}</strong>
                    <p>
                      {formatSessionTimeRange(session)} / {getSessionStatusLabel(session.status)}
                    </p>
                    {session.end_reason && <p>结束原因：{session.end_reason}</p>}
                  </div>
                </div>
                <div className="record-subject">
                  <span>
                    {session.subject_id ? subjectNameMap.get(session.subject_id) ?? '未知科目' : '未指定科目'}
                  </span>
                  <div className="record-subject-actions">
                    <select
                      aria-label="修改记录科目"
                      className="select-input"
                      disabled={savingSessionId === session.id || deletingSessionId === session.id}
                      onChange={(event) => void handleSubjectChange(session.id, event.target.value)}
                      value={session.subject_id ?? ''}
                    >
                      <option value="">未指定</option>
                      {subjects.map((subject) => (
                        <option key={subject.id} value={subject.id}>
                          {subject.name}
                        </option>
                      ))}
                    </select>
                    <button
                      className="small-action danger"
                      disabled={deletingSessionId === session.id || savingSessionId === session.id}
                      onClick={() => void handleDeleteSession(session.id)}
                      type="button"
                    >
                      <Trash2 size={15} />
                      删除记录
                    </button>
                  </div>
                </div>
              </article>
            ))}
          </div>
        )}
      </section>
    </section>
  );
}

function MetricCard({
  danger = false,
  icon: Icon,
  label,
  value,
}: {
  danger?: boolean;
  icon: typeof Clock3;
  label: string;
  value: string;
}) {
  return (
    <article className={danger ? 'metric-card large danger' : 'metric-card large'}>
      <Icon size={20} />
      <span>{label}</span>
      <strong>{value}</strong>
    </article>
  );
}
