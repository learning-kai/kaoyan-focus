import type {
  FeishuLoginPollResult,
  FeishuOAuthLogin,
  FeishuRebuildResult,
  FeishuSyncResult,
  FeishuSyncRunSummary,
  FeishuSyncSettings,
  FeishuSyncStatus,
} from '../types/settings';
import { invokeCommand } from './tauriInvoke';

export const FEISHU_SYNC_REFRESH_EVENT = 'ultrafocus-feishu-sync-refresh';

export function getFeishuSyncSettings(): Promise<FeishuSyncSettings> {
  return invokeCommand<FeishuSyncSettings>('get_feishu_sync_settings');
}

export function saveFeishuSyncSettings(settings: FeishuSyncSettings): Promise<FeishuSyncSettings> {
  return invokeCommand<FeishuSyncSettings>('save_feishu_sync_settings', { settings });
}

export function getFeishuSyncStatus(): Promise<FeishuSyncStatus> {
  return invokeCommand<FeishuSyncStatus>('get_feishu_sync_status');
}

export function startFeishuOAuthLogin(): Promise<FeishuOAuthLogin> {
  return invokeCommand<FeishuOAuthLogin>('start_feishu_oauth_login');
}

export function pollFeishuOAuthLogin(): Promise<FeishuLoginPollResult> {
  return invokeCommand<FeishuLoginPollResult>('poll_feishu_oauth_login');
}

export function logoutFeishu(): Promise<void> {
  return invokeCommand<void>('logout_feishu');
}

export function syncFeishuBridge(trigger = 'manual'): Promise<FeishuSyncResult> {
  return invokeCommand<FeishuSyncResult>('sync_feishu_bridge', { trigger });
}

export function rebuildFeishuTasklistsFromLocal(): Promise<FeishuRebuildResult> {
  return invokeCommand<FeishuRebuildResult>('rebuild_feishu_tasklists_from_local');
}

export function listFeishuSyncRuns(limit = 5): Promise<FeishuSyncRunSummary[]> {
  return invokeCommand<FeishuSyncRunSummary[]>('list_feishu_sync_runs', { limit });
}
