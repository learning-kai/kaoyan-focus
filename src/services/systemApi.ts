import { invokeCommand, isTauriRuntime, normalizeTauriError } from './tauriInvoke';

export async function pingBackend(): Promise<string> {
  return invokeCommand<string>('ping');
}

export async function showStudyReminder(title: string, body: string, soundId?: string, notificationId?: string): Promise<void> {
  await invokeCommand<void>('show_study_reminder', { title, body, soundId, notificationId });
}

export async function openExternalUrl(url: string): Promise<void> {
  await invokeCommand<void>('open_external_url', { url });
}

export type StudyDashboardLaunch = {
  url: string;
};

export async function openStudyDashboard(): Promise<StudyDashboardLaunch> {
  return invokeCommand<StudyDashboardLaunch>('open_study_dashboard');
}

export async function setStudyFullscreen(enabled: boolean): Promise<void> {
  if (!isTauriRuntime()) {
    return;
  }

  try {
    const { getCurrentWindow } = await import('@tauri-apps/api/window');
    const currentWindow = getCurrentWindow();

    if (enabled) {
      await currentWindow.show();
      await currentWindow.setFullscreen(true);
      await currentWindow.setFocus();
      return;
    }

    const isFullscreen = await currentWindow.isFullscreen();
    if (isFullscreen) {
      await currentWindow.setFullscreen(false);
    }
  } catch (reason) {
    throw normalizeTauriError(reason);
  }
}
