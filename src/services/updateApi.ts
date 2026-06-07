import { isTauriRuntime, DesktopRuntimeUnavailableError, normalizeTauriError } from './tauriInvoke';
import type { DownloadEvent, Update } from '@tauri-apps/plugin-updater';

export type AppUpdate = Update;
export type UpdateProgress = {
  downloadedBytes: number;
  totalBytes: number | null;
};

export type UpdatePhase = 'download' | 'relaunch';

export async function checkForAppUpdate(): Promise<AppUpdate | null> {
  if (!isTauriRuntime()) {
    throw new DesktopRuntimeUnavailableError();
  }

  try {
    const { check } = await import('@tauri-apps/plugin-updater');
    return await check();
  } catch (reason) {
    throw normalizeTauriError(reason);
  }
}

export async function installAppUpdate(
  update: AppUpdate,
  onProgress: (progress: UpdateProgress) => void,
  onPhase: (phase: UpdatePhase) => void = () => {},
): Promise<void> {
  if (!isTauriRuntime()) {
    throw new DesktopRuntimeUnavailableError();
  }

  let downloadedBytes = 0;
  let totalBytes: number | null = null;

  onPhase('download');
  await update.downloadAndInstall((event: DownloadEvent) => {
    if (event.event === 'Started') {
      downloadedBytes = 0;
      totalBytes = event.data.contentLength ?? null;
      onProgress({ downloadedBytes, totalBytes });
      return;
    }

    if (event.event === 'Progress') {
      downloadedBytes += event.data.chunkLength;
      onProgress({ downloadedBytes, totalBytes });
      return;
    }

    if (event.event === 'Finished') {
      onProgress({ downloadedBytes: totalBytes ?? downloadedBytes, totalBytes });
    }
  });

  try {
    onPhase('relaunch');
    const { relaunch } = await import('@tauri-apps/plugin-process');
    await relaunch();
  } catch (reason) {
    throw normalizeTauriError(reason);
  }
}
