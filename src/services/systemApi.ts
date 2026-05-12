export async function pingBackend(): Promise<string> {
  const { invoke } = await import('@tauri-apps/api/core');
  return invoke<string>('ping');
}
