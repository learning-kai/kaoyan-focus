import type { PotPlayerMediaInfo, RecentBlockedApp, RunningProcess, WhitelistApp } from '../types/whitelist';
import { invokeCommand } from './tauriInvoke';

export function createWhitelistApp(
  name: string,
  processName: string,
  note?: string,
  path?: string | null,
  subjectId?: number | null,
): Promise<WhitelistApp> {
  return invokeCommand<WhitelistApp>('create_whitelist_app', {
    name,
    processName,
    path: path?.trim() ? path.trim() : null,
    note: note?.trim() ? note.trim() : null,
    subjectId: subjectId ?? null,
  });
}

export function createWhitelistWebsite(name: string, domain: string, note?: string, subjectId?: number | null): Promise<WhitelistApp> {
  return invokeCommand<WhitelistApp>('create_whitelist_website', {
    name,
    domain,
    note: note?.trim() ? note.trim() : null,
    subjectId: subjectId ?? null,
  });
}

export function createPotPlayerVideoWhitelistFile(
  name: string,
  videoPath: string,
  note?: string,
  subjectId?: number | null,
): Promise<WhitelistApp> {
  return invokeCommand<WhitelistApp>('create_potplayer_video_whitelist_file', {
    name,
    videoPath,
    note: note?.trim() ? note.trim() : null,
    subjectId: subjectId ?? null,
  });
}

export function createPotPlayerVideoWhitelistDirectory(
  name: string,
  directoryPath: string,
  note?: string,
  subjectId?: number | null,
): Promise<WhitelistApp> {
  return invokeCommand<WhitelistApp>('create_potplayer_video_whitelist_directory', {
    name,
    directoryPath,
    note: note?.trim() ? note.trim() : null,
    subjectId: subjectId ?? null,
  });
}

export function getCurrentPotPlayerMedia(): Promise<PotPlayerMediaInfo> {
  return invokeCommand<PotPlayerMediaInfo>('get_current_potplayer_media');
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

export function updateWhitelistSubject(id: number, subjectId: number | null): Promise<WhitelistApp> {
  return invokeCommand<WhitelistApp>('update_whitelist_subject', {
    id,
    subjectId,
  });
}

export function deleteWhitelistApp(id: number): Promise<void> {
  return invokeCommand<void>('delete_whitelist_app', { id });
}
