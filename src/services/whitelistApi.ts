import type { RecentBlockedApp, RunningProcess, WhitelistApp } from '../types/whitelist';

async function invokeCommand<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  const { invoke } = await import('@tauri-apps/api/core');
  return invoke<T>(command, args);
}

export function createWhitelistApp(name: string, processName: string, note?: string, path?: string | null): Promise<WhitelistApp> {
  return invokeCommand<WhitelistApp>('create_whitelist_app', {
    name,
    processName,
    path: path?.trim() ? path.trim() : null,
    note: note?.trim() ? note.trim() : null,
  });
}

export function listWhitelistApps(): Promise<WhitelistApp[]> {
  return invokeCommand<WhitelistApp[]>('list_whitelist_apps');
}

export function listRunningProcesses(): Promise<RunningProcess[]> {
  return invokeCommand<RunningProcess[]>('list_running_processes');
}

export function listRecentBlockedApps(): Promise<RecentBlockedApp[]> {
  return invokeCommand<RecentBlockedApp[]>('list_recent_blocked_apps');
}

export function setWhitelistAppEnabled(id: number, enabled: boolean): Promise<WhitelistApp> {
  return invokeCommand<WhitelistApp>('set_whitelist_app_enabled', {
    id,
    enabled,
  });
}

export function deleteWhitelistApp(id: number): Promise<void> {
  return invokeCommand<void>('delete_whitelist_app', { id });
}
