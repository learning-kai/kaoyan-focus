async function invokeCommand<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  const { invoke } = await import('@tauri-apps/api/core');
  return invoke<T>(command, args);
}

export async function pingBackend(): Promise<string> {
  return invokeCommand<string>('ping');
}

export async function showStudyReminder(title: string, body: string): Promise<void> {
  await invokeCommand<void>('show_study_reminder', { title, body });
}

export async function openExternalUrl(url: string): Promise<void> {
  await invokeCommand<void>('open_external_url', { url });
}

export async function setStudyFullscreen(enabled: boolean): Promise<void> {
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
}
