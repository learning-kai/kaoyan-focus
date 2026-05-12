import { useEffect, useState } from 'react';
import { FolderSearch, Globe2, History, ListPlus, Power, PowerOff, Search, ShieldCheck, Trash2 } from 'lucide-react';
import {
  createWhitelistApp,
  createWhitelistWebsite,
  deleteWhitelistApp,
  listRecentBlockedApps,
  listRunningProcesses,
  listWhitelistApps,
  setWhitelistAppEnabled,
} from '../services/whitelistApi';
import type { RecentBlockedApp, RunningProcess, WhitelistApp } from '../types/whitelist';

export default function WhitelistPage() {
  const [apps, setApps] = useState<WhitelistApp[]>([]);
  const [entryType, setEntryType] = useState<'app' | 'website'>('app');
  const [name, setName] = useState('');
  const [processName, setProcessName] = useState('');
  const [domain, setDomain] = useState('');
  const [processPath, setProcessPath] = useState<string | null>(null);
  const [note, setNote] = useState('');
  const [runningProcesses, setRunningProcesses] = useState<RunningProcess[]>([]);
  const [recentBlockedApps, setRecentBlockedApps] = useState<RecentBlockedApp[]>([]);
  const [processPickerOpen, setProcessPickerOpen] = useState(false);
  const [blockedPickerOpen, setBlockedPickerOpen] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [processLoading, setProcessLoading] = useState(false);
  const [blockedLoading, setBlockedLoading] = useState(false);

  const enabledCount = apps.filter((app) => app.enabled).length;
  const websiteCount = apps.filter((app) => app.match_type === 'website_domain').length;
  const canCreate = name.trim().length > 0 && (entryType === 'website' ? domain.trim().length > 0 : processName.trim().length > 0);

  useEffect(() => {
    void refreshApps();
  }, []);

  async function refreshApps() {
    try {
      setApps(await listWhitelistApps());
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    }
  }

  async function handleCreate() {
    try {
      setError(null);
      setLoading(true);
      if (entryType === 'website') {
        await createWhitelistWebsite(name, domain, note);
      } else {
        await createWhitelistApp(name, processName, note, processPath);
      }
      setName('');
      setProcessName('');
      setDomain('');
      setProcessPath(null);
      setNote('');
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
    const displayName = process.process_name.replace(/\.exe$/i, '');
    setName(displayName);
    setProcessName(process.process_name);
    setProcessPath(process.process_path);
    setProcessPickerOpen(false);
  }

  async function handleAddBlockedApp(blockedApp: RecentBlockedApp) {
    try {
      setError(null);
      const displayName = blockedApp.process_name.replace(/\.exe$/i, '');
      await createWhitelistApp(displayName, blockedApp.process_name, '从最近干扰记录加入', blockedApp.process_path);
      setBlockedPickerOpen(false);
      await refreshApps();
      await refreshRecentBlockedApps();
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    }
  }

  async function handleToggle(app: WhitelistApp) {
    try {
      setError(null);
      await setWhitelistAppEnabled(app.id, !app.enabled);
      await refreshApps();
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    }
  }

  async function handleDelete(id: number) {
    try {
      setError(null);
      await deleteWhitelistApp(id);
      await refreshApps();
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    }
  }

  return (
    <section className="page-shell">
      <header className="page-header">
        <div>
          <p className="eyebrow">白名单 / 强制执行</p>
          <h2>软件与网站白名单</h2>
          <p>学习阶段只放行必要工具。网站识别沿用浏览器窗口标题方案，后台监控会在学习中持续执行。</p>
        </div>
        <div className="header-metrics">
          <article>
            <span>启用</span>
            <strong>{enabledCount}</strong>
          </article>
          <article>
            <span>网站</span>
            <strong>{websiteCount}</strong>
          </article>
        </div>
      </header>

      {error && <p className="alert error">{error}</p>}

      <div className="content-grid two">
        <section className="panel">
          <div className="panel-title">
            <div>
              <p className="eyebrow">Add</p>
              <h3>添加白名单</h3>
            </div>
            <ListPlus size={20} />
          </div>

          <div className="segmented-control">
            <button className={entryType === 'app' ? 'active' : ''} onClick={() => setEntryType('app')} type="button">
              <ShieldCheck size={16} />
              软件
            </button>
            <button className={entryType === 'website' ? 'active' : ''} onClick={() => setEntryType('website')} type="button">
              <Globe2 size={16} />
              网站
            </button>
          </div>

          <div className="form-stack">
            <input
              className="text-input"
              onChange={(event) => setName(event.target.value)}
              placeholder={entryType === 'website' ? '网站名称，例如 中国大学 MOOC' : '软件名称，例如 Anki'}
              value={name}
            />
            {entryType === 'website' ? (
              <input
                className="text-input"
                onChange={(event) => setDomain(event.target.value)}
                placeholder="域名，例如 icourse163.org"
                value={domain}
              />
            ) : (
              <input
                className="text-input"
                onChange={(event) => setProcessName(event.target.value)}
                placeholder="进程名，例如 anki.exe"
                value={processName}
              />
            )}
            <input className="text-input" onChange={(event) => setNote(event.target.value)} placeholder="备注，可选" value={note} />
            <button className="primary-action" disabled={!canCreate || loading} onClick={() => void handleCreate()} type="button">
              <ListPlus size={18} />
              {loading ? '添加中' : '加入白名单'}
            </button>
          </div>
          {processPath && <p className="path-hint">已选择路径：{processPath}</p>}
        </section>

        <section className="panel">
          <div className="panel-title">
            <div>
              <p className="eyebrow">Sources</p>
              <h3>快速来源</h3>
            </div>
            <Search size={20} />
          </div>

          <div className="tool-grid single">
            <div className="tool-card">
              <div>
                <h4>从运行进程选择</h4>
                <p>适合把当前打开的阅读器、词典、笔记软件快速加入白名单。</p>
              </div>
              <button className="secondary-action" disabled={processLoading} onClick={() => void handleLoadRunningProcesses()} type="button">
                <FolderSearch size={17} />
                {processLoading ? '读取中' : '读取进程'}
              </button>
            </div>

            <div className="tool-card">
              <div>
                <h4>最近干扰记录</h4>
                <p>把误判或临时需要的软件从拦截记录中一键放行。</p>
              </div>
              <button className="secondary-action" disabled={blockedLoading} onClick={() => void handleLoadRecentBlockedApps()} type="button">
                <History size={17} />
                {blockedLoading ? '读取中' : '查看记录'}
              </button>
            </div>
          </div>
        </section>
      </div>

      {processPickerOpen && (
        <section className="panel picker-panel">
          <div className="panel-title">
            <h3>选择运行进程</h3>
            <button className="ghost-action" onClick={() => setProcessPickerOpen(false)} type="button">收起</button>
          </div>
          {runningProcesses.length === 0 ? (
            <p className="empty-state compact">没有读取到可用进程，请稍后重试。</p>
          ) : (
            <div className="process-picker">
              {runningProcesses.map((process) => (
                <button className="process-option" key={`${process.process_name}-${process.process_id}`} onClick={() => handleSelectProcess(process)} type="button">
                  <strong>{process.process_name}</strong>
                  <span>{process.process_path ?? '无法读取路径'}</span>
                </button>
              ))}
            </div>
          )}
        </section>
      )}

      {blockedPickerOpen && (
        <section className="panel picker-panel">
          <div className="panel-title">
            <h3>最近干扰记录</h3>
            <button className="ghost-action" onClick={() => setBlockedPickerOpen(false)} type="button">收起</button>
          </div>
          {recentBlockedApps.length === 0 ? (
            <p className="empty-state compact">暂无可加入的干扰记录。</p>
          ) : (
            <div className="process-picker">
              {recentBlockedApps.map((blockedApp) => (
                <div className="blocked-option" key={`${blockedApp.process_name}-${blockedApp.last_blocked_at}`}>
                  <div>
                    <strong>{blockedApp.process_name}</strong>
                    <span>{blockedApp.window_title || '无窗口标题'}</span>
                    <span>{blockedApp.process_path ?? '无法读取路径'}</span>
                    <span>最近：{new Date(blockedApp.last_blocked_at).toLocaleString()} · {blockedApp.blocked_count} 次</span>
                  </div>
                  <button className="secondary-action compact-button" onClick={() => void handleAddBlockedApp(blockedApp)} type="button">
                    加入
                  </button>
                </div>
              ))}
            </div>
          )}
        </section>
      )}

      <section className="panel">
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
          <div className="list-card">
            {apps.map((app) => (
              <article className="list-row whitelist-row" key={app.id}>
                <div className="row-main">
                  <span className={app.enabled ? 'row-icon enabled' : 'row-icon'}>
                    {app.match_type === 'website_domain' ? <Globe2 size={18} /> : <ShieldCheck size={18} />}
                  </span>
                  <div>
                    <strong>{app.name}</strong>
                    <p>{app.match_type === 'website_domain' ? `网站域名：${app.process_name}` : `进程名：${app.process_name}`}</p>
                    {app.path && <p>{app.path}</p>}
                    {app.note && <p>{app.note}</p>}
                  </div>
                </div>
                <div className="row-actions">
                  <button className={app.enabled ? 'small-action enabled' : 'small-action'} onClick={() => void handleToggle(app)} type="button">
                    {app.enabled ? <Power size={15} /> : <PowerOff size={15} />}
                    {app.enabled ? '启用中' : '已停用'}
                  </button>
                  <button className="small-action danger" onClick={() => void handleDelete(app.id)} type="button">
                    <Trash2 size={15} />
                    删除
                  </button>
                </div>
              </article>
            ))}
          </div>
        )}
      </section>
    </section>
  );
}
