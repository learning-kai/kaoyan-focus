import { isTauriRuntime, normalizeTauriError } from './tauriInvoke';

type EventCallback<T> = (event: { payload: T }) => void;

export async function listenTauriEvent<T>(
  eventName: string,
  callback: EventCallback<T>,
): Promise<() => void> {
  if (!isTauriRuntime()) {
    return () => {};
  }

  try {
    const { listen } = await import('@tauri-apps/api/event');
    return await listen<T>(eventName, callback);
  } catch (reason) {
    throw normalizeTauriError(reason);
  }
}
