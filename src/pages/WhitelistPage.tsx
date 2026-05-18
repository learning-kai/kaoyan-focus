import { useEffect, useMemo, useState } from 'react';
import { ExternalLink, FolderSearch, Globe2, History, ListPlus, Power, PowerOff, Search, ShieldCheck, Trash2 } from 'lucide-react';
import { getStudyModeState, listSubjects } from '../services/focusApi';
import { isStudyModeLocked } from '../services/studyModeLock';
import { openExternalUrl } from '../services/systemApi';
import {
  createWhitelistApp,
  createWhitelistWebsite,
  deleteWhitelistApp,
  listRecentBlockedApps,
  listRunningProcesses,
  listWhitelistApps,
  setWhitelistAppEnabled,
  updateWhitelistSubject,
} from '../services/whitelistApi';
import type { StudyModeState, Subject } from '../types/focus';
import type { RecentBlockedApp, RunningProcess, WhitelistApp } from '../types/whitelist';

type WhitelistEntryType = 'app' | 'website';

function websiteUrlFromRule(rule: string) {
  const trimmed = rule.trim();
  if (/^https?:\/\//i.test(trimmed)) {
    return trimmed;
  }

  return `https://${trimmed.replace(/^\*+\./, '').replace(/^\/+/, '')}`;
}

export default function WhitelistPage() {
  const [apps, setApps] = useState<WhitelistApp[]>([]);
  const [subjects, setSubjects] = useState<Subject[]>([]);
  const [entryType, setEntryType] = useState<WhitelistEntryType>('app');
  const [name, setName] = useState('');
  const [processName, setProcessName] = useState('');
  const [domain, setDomain] = useState('');
  const [processPath, setProcessPath] = useState<string | null>(null);
  const [note, setNote] = useState('');
  const [subjectId, setSubjectId] = useState<number | null>(null);
  const [runningProcesses, setRunningProcesses] = useState<RunningProcess[]>([]);
  const [recentBlockedApps, setRecentBlockedApps] = useState<RecentBlockedApp[]>([]);
  const [processPickerOpen, setProcessPickerOpen] = useState(false);
  const [blockedPickerOpen, setBlockedPickerOpen] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [processLoading, setProcessLoading] = useState(false);
  const [blockedLoading, setBlockedLoading] = useState(false);
  const [studyState, setStudyState] = useState<StudyModeState | null>(null);

  const enabledCount = apps.filter((app) => app.enabled).length;
  const websiteCount = apps.filter((app) => app.match_type === 'website_domain').length;
  const appCount = apps.length - websiteCount;
  const whitelistLocked = isStudyModeLocked(studyState);
  const canCreate = !whitelistLocked
    && name.trim().length > 0
    && (entryType === 'website' ? domain.trim().length > 0 : processName.trim().length > 0);
  const groupedApps = useMemo(() => {
    const groups = [
      { id: null as number | null, name: '未指定科目', items: apps.filter((app) => app.subject_id === null) },
      ...subjects.map((subject) => ({
        id: subject.id as number | null,
        name: subject.name,
        items: apps.filter((app) => app.subject_id === subject.id),
      })),
    ];
    const knownSubjectIds = new Set(subjects.map((subject) => subject.id));
    const unknownItems = apps.filter((app) => app.subject_id !== null && !knownSubjectIds.has(app.subject_id));
    if (unknownItems.length > 0) {
      groups.push({ id: -1, name: '未知科目', items: unknownItems });
    }
    return groups.filter((group) => group.items.length > 0);
  }, [apps, subjects]);

  useEffect(() => {
    void initializeWhitelistPage();
  }, []);

  useEffect(() => {
    if (!whitelistLocked) {
      return;
    }

    const intervalId = window.setInterval(() => {
      void refreshStudyState();
    }, 5000);

    return () => window.clearInterval(intervalId);
  }, [whitelistLocked]);

  async function initializeWhitelistPage() {
    await Promise.all([refreshApps(), refreshSubjects(), refreshStudyState()]);
  }

  async function refreshStudyState() {
    try {
      setStudyState(await getStudyModeState());
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    }
  }

  async function refreshSubjects() {
    try {
      setSubjects(await listSubjects());
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    }
  }

  async function refreshApps() {
    try {
      setApps(await listWhitelistApps());
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    }
  }

  async function handleCreate() {
    if (whitelistLocked) {
      return;
    }

    try {
      setError(null);
      setLoading(true);
      if (entryType === 'website') {
        await createWhitelistWebsite(name, domain, note, subjectId);
      } else {
        await createWhitelistApp(name, processName, note, processPath, subjectId);
      }
      setName('');
      setProcessName('');
      setDomain('');
      setProcessPath(null);
      setNote('');
      setSubjectId(null);
      await refreshApps();
      await refreshRecentBlockedApps();
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setLoading(false);
    }
  }

  async function refreshRecentBlockedApps() {
    try {
      setRecentBlockedApps(await listRecentBlockedApps());
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    }
  }

  async function handleLoadRunningProcesses() {
    if (whitelistLocked) {
      return;
    }

    try {
      setError(null);
      setProcessLoading(true);
      setRunningProcesses(await listRunningProcesses());
      setProcessPickerOpen(true);
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setProcessLoading(false);
    }
  }

  async function handleLoadRecentBlockedApps() {
    if (whitelistLocked) {
      return;
    }

    try {
      setError(null);
      setBlockedLoading(true);
      setRecentBlockedApps(await listRecentBlockedApps());
      setBlockedPickerOpen(true);
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setBlockedLoading(false);
    }
  }

  function handleSelectProcess(process: RunningProcess) {
    if (whitelistLocked) {
      return;
    }

    const displayName = process.process_name.replace(/\.exe$/i, '');
    setName(displayName);
    setProcessName(process.process_name);
    setProcessPath(process.process_path);
    setProcessPickerOpen(false);
  }

  async function handleAddBlockedApp(blockedApp: RecentBlockedApp) {
    if (whitelistLocked) {
      return;
    }

    try {
      setError(null);
      const displayName = blockedApp.process_name.replace(/\.exe$/i, '');
      await createWhitelistApp(displayName, blockedApp.process_name, '从最近拦截记录加入', blockedApp.process_path, subjectId);
      setBlockedPickerOpen(false);
      await refreshApps();
      await refreshRecentBlockedApps();
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    }
  }

  async function handleToggle(app: WhitelistApp) {
    if (whitelistLocked) {
      return;
    }

    try {
      setError(null);
      await setWhitelistAppEnabled(app.id, !app.enabled);
      await refreshApps();
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    }
  }

  async function handleSubjectChange(app: WhitelistApp, value: string) {
    if (whitelistLocked) {
      return;
    }

    try {
      setError(null);
      const nextSubjectId = value === '' ? null : Number(value);
      const updated = await updateWhitelistSubject(app.id, nextSubjectId);
      setApps((current) => current.map((item) => (item.id === app.id ? updated : item)));
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    }
  }

  async function handleDelete(id: number) {
    if (whitelistLocked) {
      return;
    }

    try {
      setError(null);
      await deleteWhitelistApp(id);
      await refreshApps();
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    }
  }

  async function handleOpenWebsite(app: WhitelistApp) {
    try {
      setError(null);
      await openExternalUrl(websiteUrlFromRule(app.path?.trim() || app.process_name));
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    }
  }

  function subjectNameFor(id: number | null) {
    if (id === null) return '不自动切科';
    return subjects.find((subject) => subject.id === id)?.name ?? '未知科目';
  }

  return (
    <section className="page-shell whitelist-shell">
      <header className="page-header">
        <div>
          <p className="eyebrow">Allowlist Control</p>
          <h2>软件与网站白名单</h2>
          <p>学习阶段只放行必要工具。白名单可绑定科目，专注中切到对应软件或网页时会自动切换当前科目。</p>
        </div>
        <div className="header-metrics">
          <article>
            <span>启用</span>
            <strong>{enabledCount}</strong>
          </article>
          <article>
            <span>软件</span>
            <strong>{appCount}</strong>
          </article>
          <article>
            <span>网站</span>
            <strong>{websiteCount}</strong>
          </article>
        </div>
      </header>

      {error && <p className="alert error">{error}</p>}
      {whitelistLocked && <p className="alert neutral">学习模式正在运行，白名单配置已锁定；当前页面只允许查看规则和记录。</p>}

      <div className="whitelist-workbench">
        <section className="command-panel add-rule-panel">
          <div className="panel-title">
            <div>
              <p className="eyebrow">Add Rule</p>
              <h3>加入白名单</h3>
            </div>
            <ListPlus size={20} />
          </div>

          <div className="segmented-control">
            <button className={entryType === 'app' ? 'active' : ''} disabled={whitelistLocked} onClick={() => setEntryType('app')} type="button">
              <ShieldCheck size={16} />
              软件
            </button>
            <button className={entryType === 'website' ? 'active' : ''} disabled={whitelistLocked} onClick={() => setEntryType('website')} type="button">
              <Globe2 size={16} />
              网站
            </button>
          </div>

          <div className="form-stack">
            <label className="field-block">
              <span>{entryType === 'website' ? '网站名称' : '软件名称'}</span>
              <input
                className="text-input"
                disabled={whitelistLocked}
                onChange={(event) => setName(event.target.value)}
                placeholder={entryType === 'website' ? '例如：中国大学 MOOC' : '例如：Anki'}
                value={name}
              />
            </label>
            {entryType === 'website' ? (
              <label className="field-block">
                <span>网址或域名</span>
                <input
                  className="text-input"
                  disabled={whitelistLocked}
                  onChange={(event) => setDomain(event.target.value)}
                  placeholder="例如：https://www.bilibili.com/video/BV..."
                  value={domain}
                />
              </label>
            ) : (
              <label className="field-block">
                <span>进程名</span>
                <input
                  className="text-input"
                  disabled={whitelistLocked}
                  onChange={(event) => setProcessName(event.target.value)}
                  placeholder="例如：anki.exe"
                  value={processName}
                />
              </label>
            )}
            <label className="field-block">
              <span>自动切换科目</span>
              <select
                className="select-input"
                disabled={whitelistLocked}
                onChange={(event) => setSubjectId(event.target.value ? Number(event.target.value) : null)}
                value={subjectId ?? ''}
              >
                <option value="">不自动切科</option>
                {subjects.map((subject) => (
                  <option key={subject.id} value={subject.id}>{subject.name}</option>
                ))}
              </select>
            </label>
            <label className="field-block">
              <span>备注</span>
              <input className="text-input" disabled={whitelistLocked} onChange={(event) => setNote(event.target.value)} placeholder="可选" value={note} />
            </label>
            <button className="primary-action" disabled={!canCreate || loading} onClick={() => void handleCreate()} type="button">
              <ListPlus size={18} />
              {loading ? '添加中' : '加入白名单'}
            </button>
          </div>
          {processPath && <p className="path-hint">已选择路径：{processPath}</p>}
        </section>

        <section className="command-panel source-panel">
          <div className="panel-title">
            <div>
              <p className="eyebrow">Sources</p>
              <h3>快速来源</h3>
            </div>
            <Search size={20} />
          </div>

          <div className="source-actions">
            <article className="tool-row">
              <div>
                <h4>从运行进程选择</h4>
                <p>适合把当前打开的阅读器、词典、笔记软件快速加入白名单。</p>
              </div>
              <button className="secondary-action" disabled={processLoading || whitelistLocked} onClick={() => void handleLoadRunningProcesses()} type="button">
                <FolderSearch size={17} />
                {processLoading ? '读取中' : '读取进程'}
              </button>
            </article>

            <article className="tool-row">
              <div>
                <h4>最近拦截记录</h4>
                <p>把误判或临时需要的软件从拦截记录中一键放行。</p>
              </div>
              <button className="secondary-action" disabled={blockedLoading} onClick={() => void handleLoadRecentBlockedApps()} type="button">
                <History size={17} />
                {blockedLoading ? '读取中' : '查看记录'}
              </button>
            </article>
          </div>
        </section>
      </div>

      {processPickerOpen && (
        <section className="command-panel picker-panel">
          <div className="panel-title">
            <div>
              <p className="eyebrow">Processes</p>
              <h3>选择运行进程</h3>
            </div>
            <button className="ghost-action" onClick={() => setProcessPickerOpen(false)} type="button">收起</button>
          </div>
          {runningProcesses.length === 0 ? (
            <p className="empty-state compact">没有读取到可用进程，请稍后重试。</p>
          ) : (
            <div className="process-picker">
              {runningProcesses.map((process) => (
                <button className="process-option" disabled={whitelistLocked} key={`${process.process_name}-${process.process_id}`} onClick={() => handleSelectProcess(process)} type="button">
                  <strong>{process.process_name}</strong>
                  <span>{process.process_path ?? '无法读取路径'}</span>
                </button>
              ))}
            </div>
          )}
        </section>
      )}

      {blockedPickerOpen && (
        <section className="command-panel picker-panel">
          <div className="panel-title">
            <div>
              <p className="eyebrow">Blocked</p>
              <h3>最近拦截记录</h3>
            </div>
            <button className="ghost-action" onClick={() => setBlockedPickerOpen(false)} type="button">收起</button>
          </div>
          {recentBlockedApps.length === 0 ? (
            <p className="empty-state compact">暂无可加入的拦截记录。</p>
          ) : (
            <div className="process-picker">
              {recentBlockedApps.map((blockedApp) => (
                <div className="blocked-option" key={`${blockedApp.process_name}-${blockedApp.last_blocked_at}`}>
                  <div>
                    <strong>{blockedApp.process_name}</strong>
                    <span>{blockedApp.window_title || '无窗口标题'}</span>
                    <span>{blockedApp.process_path ?? '无法读取路径'}</span>
                    <span>最近：{new Date(blockedApp.last_blocked_at).toLocaleString()} / {blockedApp.blocked_count} 次</span>
                  </div>
                  <button className="secondary-action compact-button" disabled={whitelistLocked} onClick={() => void handleAddBlockedApp(blockedApp)} type="button">
                    加入
                  </button>
                </div>
              ))}
            </div>
          )}
        </section>
      )}

      <section className="command-panel">
        <div className="panel-title">
          <div>
            <p className="eyebrow">Rules</p>
            <h3>当前白名单</h3>
          </div>
          <ShieldCheck size={20} />
        </div>

        {apps.length === 0 ? (
          <div className="empty-state">
            <strong>还没有白名单条目</strong>
            <p>先添加常用学习软件，例如 Word、PDF 阅读器、Anki、词典或必要学习网站。</p>
          </div>
        ) : (
          <div className="rule-list">
            {groupedApps.map((group) => (
              <div className="whitelist-subject-group" key={group.id ?? 'none'}>
                <div className="whitelist-subject-heading">
                  <strong>{group.name}</strong>
                  <span>{group.items.length} 条</span>
                </div>
                {group.items.map((app) => (
                  <article className="list-row whitelist-row" key={app.id}>
                    <div className="row-main">
                      <span className={app.enabled ? 'row-icon enabled' : 'row-icon'}>
                        {app.match_type === 'website_domain' ? <Globe2 size={18} /> : <ShieldCheck size={18} />}
                      </span>
                      <div>
                        <strong>{app.name}</strong>
                        <p>{app.match_type === 'website_domain' ? `网站规则：${app.path ?? app.process_name}` : `进程名：${app.process_name}`}</p>
                        <p>自动切科：{subjectNameFor(app.subject_id)}</p>
                        {app.note && <p>{app.note}</p>}
                      </div>
                    </div>
                    <div className="row-actions">
                      <select
                        aria-label="白名单科目"
                        className="select-input whitelist-subject-select"
                        disabled={whitelistLocked}
                        onChange={(event) => void handleSubjectChange(app, event.target.value)}
                        value={app.subject_id ?? ''}
                      >
                        <option value="">不自动切科</option>
                        {subjects.map((subject) => (
                          <option key={subject.id} value={subject.id}>{subject.name}</option>
                        ))}
                      </select>
                      {app.match_type === 'website_domain' && (
                        <button className="small-action" onClick={() => void handleOpenWebsite(app)} type="button">
                          <ExternalLink size={15} />
                          打开
                        </button>
                      )}
                      <button className={app.enabled ? 'small-action enabled' : 'small-action'} disabled={whitelistLocked} onClick={() => void handleToggle(app)} type="button">
                        {app.enabled ? <Power size={15} /> : <PowerOff size={15} />}
                        {app.enabled ? '启用中' : '已停用'}
                      </button>
                      <button className="small-action danger" disabled={whitelistLocked} onClick={() => void handleDelete(app.id)} type="button">
                        <Trash2 size={15} />
                        删除
                      </button>
                    </div>
                  </article>
                ))}
              </div>
            ))}
          </div>
        )}
      </section>
    </section>
  );
}
