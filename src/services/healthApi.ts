import type { RuntimeHealth } from '../types/settings';
import { invokeCommand } from './tauriInvoke';

export function getRuntimeHealth(): Promise<RuntimeHealth> {
  return invokeCommand<RuntimeHealth>('get_runtime_health');
}
