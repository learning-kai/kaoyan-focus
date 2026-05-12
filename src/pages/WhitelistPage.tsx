import { useEffect, useState } from 'react';
import { createWhitelistApp, deleteWhitelistApp, listRecentBlockedApps, listRunningProcesses, listWhitelistApps, setWhitelistAppEnabled } from '../services/whitelistApi';
import type { RecentBlockedApp, RunningProcess, WhitelistApp } from '../types/whitelist';

export default function WhitelistPage() {
  const [apps, setApps] = useState<WhitelistApp[]>([]);
  const [name, setName] = useState('');
  const [processName, setProcessName] = useState('');
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
      await createWhitelistApp(name, processName, note, processPath);
      setName('');
      setProcessName('');
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
    <section className="page-card">
      <div className="page-heading">
        <p className="eyebrow">阶段 8 / 白名单增强</p>
        <h2>软件白名单</h2>
        <p>当前版本支持手动添加，也可以从正在运行的进程中选择学习软件。</p>
      </div>

      <div className="tool-grid">
        <div className="tool-card">
          <div>
            <h3>从运行进程选择</h3>
            <p>适合把当前打开的阅读器、词典、笔记软件快速加入白名单。</p>
          </div>
          <button className="small-action enabled" disabled={processLoading} onClick={() => void handleLoadRunningProcesses()} type="button">
            {processLoading ? '读取中' : '读取运行进程'}
          </button>
        </div>

        <div className="tool-card">
          <div>
            <h3>最近干扰记录</h3>
            <p>把被误判或临时需要的软件一键加入白名单。</p>
          </div>
          <button className="small-action enabled" disabled={blockedLoading} onClick={() => void handleLoadRecentBlockedApps()} type="button">
            {blockedLoading ? '读取中' : '查看干扰记录'}
          </button>
        </div>
      </div>

      {processPickerOpen && (
        <div className="process-picker">
          {runningProcesses.length === 0 ? (
            <p className="muted">没有读取到可用进程，请稍后重试。</p>
          ) : (
            runningProcesses.map((process) => (
              <button className="process-option" key={`${process.process_name}-${process.process_id}`} onClick={() => handleSelectProcess(process)} type="button">
                <strong>{process.process_name}</strong>
                <span>{process.process_path ?? '无法读取路径'}</span>
              </button>
            ))
          )}
        </div>
      )}

      {blockedPickerOpen && (
        <div className="process-picker">
          {recentBlockedApps.length === 0 ? (
            <p className="muted">暂无可加入的干扰记录。</p>
          ) : (
            recentBlockedApps.map((blockedApp) => (
              <div className="blocked-option" key={`${blockedApp.process_name}-${blockedApp.last_blocked_at}`}>
                <div>
                  <strong>{blockedApp.process_name}</strong>
                  <span>{blockedApp.window_title || '无窗口标题'}</span>
                  <span>{blockedApp.process_path ?? '无法读取路径'}</span>
                  <span>最近：{new Date(blockedApp.last_blocked_at).toLocaleString()} · {blockedApp.blocked_count} 次</span>
                </div>
                <button className="small-action enabled" onClick={() => void handleAddBlockedApp(blockedApp)} type="button">
                  加入白名单
                </button>
              </div>
            ))
          )}
        </div>
      )}

      <div className="form-row whitelist-form">
        <input onChange={(event) => setName(event.target.value)} placeholder="软件名称，例如 Anki" value={name} />
        <input onChange={(event) => setProcessName(event.target.value)} placeholder="进程名，例如 anki.exe" value={processName} />
        <input onChange={(event) => setNote(event.target.value)} placeholder="备注，可选" value={note} />
        <button disabled={loading} onClick={() => void handleCreate()} type="button">添加</button>
      </div>
      {processPath && <p className="path-hint">已选择路径：{processPath}</p>}

      {error && <p className="error-text">{error}</p>}

      {apps.length === 0 ? (
        <div className="empty-state">
          <strong>还没有白名单软件</strong>
          <p>先添加常用学习软件，例如 Word、PDF 阅读器、Anki 或词典。</p>
        </div>
      ) : (
        <div className="list-card">
          {apps.map((app) => (
            <div className="list-row whitelist-row" key={app.id}>
              <div>
                <strong>{app.name}</strong>
                <p>{app.process_name}</p>
                {app.path && <p>{app.path}</p>}
                {app.note && <p>{app.note}</p>}
              </div>
              <div className="row-actions">
                <button className={app.enabled ? 'small-action enabled' : 'small-action'} onClick={() => void handleToggle(app)} type="button">
                  {app.enabled ? '启用中' : '已停用'}
                </button>
                <button className="small-action danger" onClick={() => void handleDelete(app.id)} type="button">删除</button>
              </div>
            </div>
          ))}
        </div>
      )}
    </section>
  );
}
