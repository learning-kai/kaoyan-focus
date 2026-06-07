import { existsSync, readdirSync, readFileSync } from 'node:fs';
import { copyFile, mkdir, rm, writeFile } from 'node:fs/promises';
import { spawnSync } from 'node:child_process';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { resolveUpdateBaseUrlFromArgs, writeTauriReleaseConfig } from './release-config.mjs';

const scriptDir = dirname(fileURLToPath(import.meta.url));
const projectRoot = resolve(scriptDir, '..');
loadLocalEnvFile(resolve(projectRoot, '.env.local'));

const packageJson = JSON.parse(readFileSync(resolve(projectRoot, 'package.json'), 'utf8'));
const keyPath = resolve(projectRoot, 'src-tauri', 'updater', 'tauri-update.key');
const tauriCliPath = resolve(projectRoot, 'node_modules', '@tauri-apps', 'cli', 'tauri.js');
const nsisBundleDir = resolve(projectRoot, 'src-tauri', 'target', 'release', 'bundle', 'nsis');
const tauriConfigPath = resolve(projectRoot, 'src-tauri', 'tauri.conf.json');
const releaseConfigPath = resolve(projectRoot, 'src-tauri', 'target', '.tauri.release.conf.json');
const buildEnv = createWindowsRustEnv(process.env);
const releaseArgs = resolveUpdateBaseUrlFromArgs(
  process.argv.slice(2),
  process.env,
  'building Windows update metadata',
);
const updateBaseUrl = releaseArgs.updateBaseUrl;

if (releaseArgs.args.length > 0) {
  throw new Error('Usage: node scripts/release-win-update.mjs [--update-base-url <https://...>]');
}

if (!existsSync(keyPath)) {
  console.error(`Missing updater signing key: ${keyPath}`);
  console.error('Run: npm.cmd run tauri signer generate -- --ci -w src-tauri\\updater\\tauri-update.key');
  process.exit(1);
}

if (isEncryptedSigningKey(keyPath) && !process.env.TAURI_SIGNING_PRIVATE_KEY_PASSWORD) {
  console.error('Missing TAURI_SIGNING_PRIVATE_KEY_PASSWORD for encrypted updater signing key.');
  console.error('Set it before running release:win:update, for example:');
  console.error('$env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = "<your updater key password>"');
  process.exit(1);
}

if (isEncryptedSigningKey(keyPath)) {
  await assertSigningPasswordWorks();
}

if (!updateBaseUrl) {
  console.error('Missing UPDATE_BASE_URL. Pass --update-base-url <https://...> or set the environment variable before building the update release.');
  process.exit(1);
}

let releaseError;

try {
  await writeTauriReleaseConfig(tauriConfigPath, releaseConfigPath, updateBaseUrl);
  console.log(`Using update base URL: ${updateBaseUrl}`);

  const build = spawnSync(process.execPath, [tauriCliPath, 'build', '--config', releaseConfigPath, '--bundles', 'nsis'], {
    cwd: projectRoot,
    env: {
      ...buildEnv,
    },
    shell: false,
    stdio: 'inherit',
  });

  if (build.error) {
    throw build.error;
  }

  if (build.status !== 0) {
    throw new Error(`tauri build failed with exit code ${build.status ?? 1}`);
  }

  const installerPath = findNsisInstaller(packageJson.version);
  const publishInstallerPath = await preparePublishInstaller(installerPath, packageJson.version);
  console.log(`Using installer: ${publishInstallerPath}`);

  const sign = spawnSync(
    process.execPath,
    [tauriCliPath, 'signer', 'sign', '--private-key-path', keyPath, publishInstallerPath],
    {
      cwd: projectRoot,
      shell: false,
      stdio: 'inherit',
    },
  );

  if (sign.error) {
    throw sign.error;
  }

  if (sign.status !== 0) {
    throw new Error(`tauri signer failed with exit code ${sign.status ?? 1}`);
  }

  const metadata = spawnSync(
    process.execPath,
    ['scripts/generate-latest-json.mjs', publishInstallerPath],
    {
      cwd: projectRoot,
      env: {
        ...process.env,
        UPDATE_BASE_URL: updateBaseUrl,
      },
      shell: false,
      stdio: 'inherit',
    },
  );

  if (metadata.error) {
    throw metadata.error;
  }

  if (metadata.status !== 0) {
    throw new Error(`metadata generation failed with exit code ${metadata.status ?? 1}`);
  }
} catch (error) {
  releaseError = error;
} finally {
  await rm(releaseConfigPath, { force: true });
}

if (releaseError) {
  console.error(releaseError);
  process.exitCode = 1;
}

function loadLocalEnvFile(filePath) {
  if (!existsSync(filePath)) {
    return;
  }

  const content = readFileSync(filePath, 'utf8');
  for (const line of content.split(/\r?\n/)) {
    const trimmed = line.trim();
    if (!trimmed || trimmed.startsWith('#')) {
      continue;
    }

    const match = trimmed.match(/^([A-Za-z_][A-Za-z0-9_]*)=(.*)$/);
    if (!match) {
      continue;
    }

    const [, key, rawValue] = match;
    if (process.env[key] !== undefined && key !== 'TAURI_SIGNING_PRIVATE_KEY_PASSWORD') {
      continue;
    }

    process.env[key] = rawValue.replace(/^(['"])(.*)\1$/, '$2');
  }
}

function findNsisInstaller(version) {
  if (!existsSync(nsisBundleDir)) {
    console.error(`Missing NSIS bundle directory: ${resolve(nsisBundleDir)}`);
    process.exit(1);
  }

  const allMatches = readdirSync(nsisBundleDir)
    .filter((fileName) => (
      fileName.endsWith('-setup.exe')
      && fileName.includes(`_${version}_`)
    ));
  const publishFileName = getPublishInstallerFileName(version);
  const matches = allMatches.filter((fileName) => fileName !== publishFileName);
  if (matches.length === 0 && allMatches.length === 1) {
    return join(nsisBundleDir, allMatches[0]);
  }

  if (matches.length === 0) {
    console.error(`No NSIS installer found for version ${version} in ${resolve(nsisBundleDir)}.`);
    process.exit(1);
  }

  if (matches.length > 1) {
    console.error(`Multiple NSIS installers found for version ${version}:`);
    for (const fileName of matches) {
      console.error(`- ${fileName}`);
    }
    process.exit(1);
  }

  return join(nsisBundleDir, matches[0]);
}

async function preparePublishInstaller(installerPath, version) {
  const publishPath = join(nsisBundleDir, getPublishInstallerFileName(version));
  if (resolve(installerPath) !== resolve(publishPath)) {
    await copyFile(installerPath, publishPath);
  }

  await rm(`${publishPath}.sig`, { force: true });
  return publishPath;
}

function getPublishInstallerFileName(version) {
  return `${packageJson.name}_${version}_x64-setup.exe`;
}

function isEncryptedSigningKey(filePath) {
  try {
    const raw = readFileSync(filePath, 'utf8').trim();
    const decoded = Buffer.from(raw, 'base64').toString('utf8');
    return decoded.includes('encrypted secret key');
  } catch {
    return false;
  }
}

async function assertSigningPasswordWorks() {
  const targetDir = resolve(projectRoot, 'src-tauri', 'target');
  const checkPath = resolve(targetDir, '.tauri-updater-sign-check.txt');
  await mkdir(targetDir, { recursive: true });
  await writeFile(checkPath, 'tauri updater signing password check\n', 'utf8');

  try {
    const sign = spawnSync(
      process.execPath,
      [tauriCliPath, 'signer', 'sign', '--private-key-path', keyPath, checkPath],
      {
        cwd: projectRoot,
        shell: false,
        stdio: ['ignore', 'pipe', 'pipe'],
        encoding: 'utf8',
      },
    );

    if (sign.error) {
      throw sign.error;
    }

    if (sign.status !== 0) {
      console.error('The updater signing key password is incorrect.');
      console.error('Update TAURI_SIGNING_PRIVATE_KEY_PASSWORD in .env.local, or regenerate the updater key pair if this app has not been distributed yet.');
      if (sign.stderr?.trim()) {
        console.error(sign.stderr.trim());
      }
      process.exit(1);
    }
  } finally {
    await rm(checkPath, { force: true });
    await rm(`${checkPath}.sig`, { force: true });
  }
}

function createWindowsRustEnv(baseEnv) {
  const userProfile = baseEnv.USERPROFILE;
  const cargoHome = baseEnv.CARGO_HOME ?? (userProfile ? resolve(userProfile, '.cargo') : undefined);
  const rustupHome = baseEnv.RUSTUP_HOME ?? (userProfile ? resolve(userProfile, '.rustup') : undefined);
  const cargoBinDir = cargoHome ? resolve(cargoHome, 'bin') : undefined;
  const cargoExe = baseEnv.CARGO ?? resolveExisting(cargoBinDir, 'cargo.exe');
  const rustcExe = baseEnv.RUSTC ?? resolveExisting(cargoBinDir, 'rustc.exe');
  const pathValue = buildPathValue(baseEnv, cargoBinDir);

  return {
    ...baseEnv,
    ...(cargoHome ? { CARGO_HOME: cargoHome } : {}),
    ...(rustupHome ? { RUSTUP_HOME: rustupHome } : {}),
    ...(cargoExe ? { CARGO: cargoExe } : {}),
    ...(rustcExe ? { RUSTC: rustcExe } : {}),
    ...(pathValue ? { PATH: pathValue, Path: pathValue } : {}),
  };
}

function resolveExisting(directory, fileName) {
  if (!directory) {
    return undefined;
  }

  const filePath = resolve(directory, fileName);
  return existsSync(filePath) ? filePath : undefined;
}

function buildPathValue(baseEnv, cargoBinDir) {
  const seen = new Set();
  const entries = [cargoBinDir, baseEnv.PATH, baseEnv.Path]
    .filter(Boolean)
    .flatMap((value) => value.split(';'))
    .map((value) => value.trim())
    .filter(Boolean)
    .filter((value) => {
      const key = value.toLowerCase();
      if (seen.has(key)) {
        return false;
      }
      seen.add(key);
      return true;
    });

  return entries.join(';');
}
