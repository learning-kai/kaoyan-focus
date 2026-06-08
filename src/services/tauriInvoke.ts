const DESKTOP_RUNTIME_MESSAGE = '请在 Windows 桌面应用中运行考研专注。浏览器预览只用于界面检查，无法访问本机数据、通知、更新和 Windows 守护能力。';

type TauriWindow = Window & {
  __TAURI_INTERNALS__?: unknown;
};

export function isTauriRuntime() {
  return typeof window !== 'undefined' && Boolean((window as TauriWindow).__TAURI_INTERNALS__);
}

export class DesktopRuntimeUnavailableError extends Error {
  constructor(message = DESKTOP_RUNTIME_MESSAGE) {
    super(message);
    this.name = 'DesktopRuntimeUnavailableError';
  }
}

export function normalizeTauriError(reason: unknown) {
  if (reason instanceof DesktopRuntimeUnavailableError) {
    return reason;
  }

  const message = reason instanceof Error ? reason.message : String(reason);
  if (
    message.includes('__TAURI_INTERNALS__') ||
    message.includes('Cannot read properties of undefined') ||
    message.includes('is not a function')
  ) {
    return new DesktopRuntimeUnavailableError();
  }

  return reason;
}

export async function invokeCommand<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  if (!isTauriRuntime()) {
    throw new DesktopRuntimeUnavailableError();
  }

  try {
    const { invoke } = await import('@tauri-apps/api/core');
    return await invoke<T>(command, args);
  } catch (reason) {
    throw normalizeTauriError(reason);
  }
}

export { DESKTOP_RUNTIME_MESSAGE };
