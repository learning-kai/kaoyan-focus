import { existsSync, readFileSync } from 'node:fs';
import { spawnSync } from 'node:child_process';
import { join, resolve } from 'node:path';

const packageJson = JSON.parse(readFileSync('package.json', 'utf8'));
const keyPath = resolve('src-tauri', 'updater', 'tauri-update.key');
const tauriCliPath = resolve('node_modules', '@tauri-apps', 'cli', 'tauri.js');
const installerPath = join('src-tauri', 'target', 'release', 'bundle', 'nsis', `考研专注_${packageJson.version}_x64-setup.exe`);
const updateBaseUrl = process.env.UPDATE_BASE_URL ?? 'https://github.com/OWNER/REPO/releases/latest/download';

if (!existsSync(keyPath)) {
  console.error(`Missing updater signing key: ${keyPath}`);
  console.error('Run: npm.cmd run tauri signer generate -- --ci -w src-tauri\\updater\\tauri-update.key');
  process.exit(1);
}

const build = spawnSync(process.execPath, [tauriCliPath, 'build', '--bundles', 'nsis'], {
  env: {
    ...process.env,
  },
  shell: false,
  stdio: 'inherit',
});

if (build.error) {
  console.error(build.error);
  process.exit(1);
}

if (build.status !== 0) {
  process.exit(build.status ?? 1);
}

const sign = spawnSync(
  process.execPath,
  [tauriCliPath, 'signer', 'sign', '--private-key-path', keyPath, installerPath],
  {
    shell: false,
    stdio: 'inherit',
  },
);

if (sign.error) {
  console.error(sign.error);
  process.exit(1);
}

if (sign.status !== 0) {
  process.exit(sign.status ?? 1);
}

const metadata = spawnSync(
  'node',
  ['scripts/generate-latest-json.mjs', installerPath, updateBaseUrl],
  {
    shell: false,
    stdio: 'inherit',
  },
);

if (metadata.error) {
  console.error(metadata.error);
  process.exit(1);
}

if (metadata.status !== 0) {
  process.exit(metadata.status ?? 1);
}
