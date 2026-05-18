import { cp, mkdir, rm } from 'node:fs/promises';
import { spawnSync } from 'node:child_process';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const scriptDir = dirname(fileURLToPath(import.meta.url));
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

if (!outputBundleDir.startsWith(safeOutputRoot)) {
  throw new Error(`Refusing to write outside project target directory: ${outputBundleDir}`);
}

const { prepareArgs, shouldPrepare, shouldTag } = parseArgs(process.argv.slice(2));

if (shouldPrepare) {
  runStep('Preparing release version and changelog', prepareReleasePath, prepareArgs);
} else {
  console.log('Skipping release prepare.');
}

console.log(`Using Cargo target dir: ${cargoTargetDir}`);

const build = spawnSync(process.execPath, [tauriCliPath, 'build', '--bundles', 'nsis'], {
  cwd: projectRoot,
  env: {
    ...process.env,
    CARGO_TARGET_DIR: cargoTargetDir,
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

await rm(outputBundleDir, { recursive: true, force: true });
await mkdir(outputBundleDir, { recursive: true });
await cp(sourceBundleDir, outputBundleDir, { recursive: true });
console.log(`Copied NSIS bundle to ${outputBundleDir}`);

if (shouldTag) {
  runStep('Creating desktop and Android release tags', tagReleasePath, []);
} else {
  console.log('Skipping release tag.');
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
    console.error(result.error);
    process.exit(1);
  }

  if (result.status !== 0) {
    process.exit(result.status ?? 1);
  }
}
