import { check, type DownloadEvent, type Update } from '@tauri-apps/plugin-updater';
import { relaunch } from '@tauri-apps/plugin-process';

export type AppUpdate = Update;
export type UpdateProgress = {
  downloadedBytes: number;
  totalBytes: number | null;
};

export async function checkForAppUpdate(): Promise<AppUpdate | null> {
  return check();
}

export async function installAppUpdate(
  update: AppUpdate,
  onProgress: (progress: UpdateProgress) => void,
): Promise<void> {
  let downloadedBytes = 0;
  let totalBytes: number | null = null;

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

  await relaunch();
}
