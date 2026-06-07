import { cp, mkdir, rm } from 'node:fs/promises';
import { existsSync } from 'node:fs';
import { spawnSync } from 'node:child_process';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { resolveOptionalUpdateBaseUrlFromArgs, writeTauriReleaseConfig } from './release-config.mjs';

const scriptDir = resolve(fileURLToPath(import.meta.url), '..');
const projectRoot = resolve(scriptDir, '..');
const tauriCliPath = resolve(projectRoot, 'node_modules', '@tauri-apps', 'cli', 'tauri.js');
const prepareReleasePath = resolve(scriptDir, 'prepare-release.mjs');
const tagReleasePath = resolve(scriptDir, 'tag-release.mjs');
const cargoTargetDir = resolve(
  process.env.KAOYAN_CARGO_TARGET_DIR
    ?? process.env.CARGO_TARGET_DIR
    ?? resolve(process.env.LOCALAPPDATA ?? projectRoot, 'kaoyan-focus-cargo-target'),
);
const sourceBundleDir = resolve(cargoTargetDir, 'release', 'bundle', 'nsis');
const outputBundleDir = resolve(projectRoot, 'src-tauri', 'target', 'release', 'bundle', 'nsis');
const safeOutputRoot = resolve(projectRoot, 'src-tauri', 'target');
const buildEnv = createWindowsRustEnv(process.env);
const releaseArgs = resolveOptionalUpdateBaseUrlFromArgs(
  process.argv.slice(2),
  process.env,
);
const updateBaseUrl = releaseArgs.updateBaseUrl;
const tauriConfigPath = resolve(projectRoot, 'src-tauri', 'tauri.conf.json');
const releaseConfigPath = resolve(projectRoot, 'src-tauri', 'target', '.tauri.release.conf.json');

if (!outputBundleDir.startsWith(safeOutputRoot)) {
  throw new Error(`Refusing to write outside project target directory: ${outputBundleDir}`);
}

const { prepareArgs, shouldPrepare, shouldTag } = parseArgs(releaseArgs.args);
const localBuildOnly = !updateBaseUrl;
let releaseError;

try {
  if (shouldPrepare) {
    runStep('Preparing release version and changelog', prepareReleasePath, prepareArgs);
  } else {
    console.log('Skipping release prepare.');
  }

  await writeTauriReleaseConfig(tauriConfigPath, releaseConfigPath, updateBaseUrl);
  if (updateBaseUrl) {
    console.log(`Using update base URL: ${updateBaseUrl}`);
  } else {
    console.log('Building local Windows installer without updater endpoint. Pass --update-base-url <https://...> for online update releases.');
  }
  console.log(`Using Cargo target dir: ${cargoTargetDir}`);

  const build = spawnSync(process.execPath, [tauriCliPath, 'build', '--config', releaseConfigPath, '--bundles', 'nsis'], {
    cwd: projectRoot,
    env: {
      ...buildEnv,
      CARGO_TARGET_DIR: cargoTargetDir,
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

  await rm(outputBundleDir, { recursive: true, force: true });
  await mkdir(outputBundleDir, { recursive: true });
  await cp(sourceBundleDir, outputBundleDir, { recursive: true });
  console.log(`Copied NSIS bundle to ${outputBundleDir}`);

  if (shouldTag && !localBuildOnly) {
    runStep('Creating desktop and Android release tags', tagReleasePath, []);
  } else if (shouldTag && localBuildOnly) {
    console.log('Skipping release tag for local build without update base URL.');
  } else {
    console.log('Skipping release tag.');
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

function parseArgs(args) {
  const prepareArgs = [];
  let shouldPrepare = true;
  let shouldTag = true;

  for (const arg of args) {
    if (arg === '--no-prepare') {
      shouldPrepare = false;
    } else if (arg === '--no-tag') {
      shouldTag = false;
    } else {
      prepareArgs.push(arg);
    }
  }

  return { prepareArgs, shouldPrepare, shouldTag };
}

function runStep(label, scriptPath, args) {
  console.log(`\n${label}...`);
  const result = spawnSync(process.execPath, [scriptPath, ...args], {
    cwd: projectRoot,
    env: process.env,
    shell: false,
    stdio: 'inherit',
  });

  if (result.error) {
    throw result.error;
  }

  if (result.status !== 0) {
    throw new Error(`Step failed with exit code ${result.status ?? 1}: ${label}`);
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
