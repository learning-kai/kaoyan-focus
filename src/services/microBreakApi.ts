import { invokeCommand } from './tauriInvoke';
import { normalizeMicroBreakSettings, type MicroBreakSettings } from '../utils/microBreak';

export async function getMicroBreakSettings(): Promise<MicroBreakSettings> {
  return normalizeMicroBreakSettings(await invokeCommand<MicroBreakSettings>('get_micro_break_settings'));
}

export async function saveMicroBreakSettings(settings: MicroBreakSettings): Promise<MicroBreakSettings> {
  return normalizeMicroBreakSettings(
    await invokeCommand<MicroBreakSettings>('save_micro_break_settings', { settings: normalizeMicroBreakSettings(settings) }),
  );
}
