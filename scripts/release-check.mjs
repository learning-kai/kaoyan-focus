import { existsSync } from 'node:fs';
import { readFile } from 'node:fs/promises';
import { spawnSync } from 'node:child_process';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const scriptDir = dirname(fileURLToPath(import.meta.url));
const projectRoot = resolve(scriptDir, '..');
const args = process.argv.slice(2);

if (args.includes('--help') || args.includes('-h')) {
  printUsage();
  process.exit(0);
}

const options = parseArgs(args);
const failures = [];

await checkVersionConsistency();
await checkChangelog();
checkSensitivePaths();
if (options.requireClean) {
  checkCleanWorkingTree();
}

if (failures.length > 0) {
  console.error('Release check failed:');
  for (const failure of failures) {
    console.error(`- ${failure}`);
  }
  process.exit(1);
}

console.log(options.requireClean
  ? 'Release check passed with a clean working tree.'
  : 'Release check passed.');

function parseArgs(rawArgs) {
  const parsed = {
    requireClean: false,
  };

  for (const arg of rawArgs) {
    if (arg === '--require-clean') {
      parsed.requireClean = true;
      continue;
    }

    throw new Error(`Unknown release-check argument: ${arg}`);
  }

  return parsed;
}

async function checkVersionConsistency() {
  const packageJson = await readJson('package.json');
  const version = packageJson.version;
  if (!/^\d+\.\d+\.\d+$/.test(version ?? '')) {
    failures.push(`package.json version is not semver-like: ${version}`);
    return;
  }

  if (existsSync(resolve(projectRoot, 'package-lock.json'))) {
    const packageLock = await readJson('package-lock.json');
    if (packageLock.version !== version) {
      failures.push(`package-lock.json version ${packageLock.version} does not match package.json ${version}`);
    }
    const rootPackage = packageLock.packages?.[''];
    if (rootPackage?.version !== version) {
      failures.push(`package-lock.json packages[""].version ${rootPackage?.version} does not match package.json ${version}`);
    }
  }

  const cargoToml = await readText('src-tauri/Cargo.toml');
  const cargoVersion = cargoToml.match(/^version\s*=\s*"([^"]+)"/m)?.[1];
  if (cargoVersion !== version) {
    failures.push(`src-tauri/Cargo.toml version ${cargoVersion ?? '<missing>'} does not match package.json ${version}`);
  }

  const tauriConfig = await readJson('src-tauri/tauri.conf.json');
  if (tauriConfig.version !== version) {
    failures.push(`src-tauri/tauri.conf.json version ${tauriConfig.version} does not match package.json ${version}`);
  }
}

async function checkChangelog() {
  if (!existsSync(resolve(projectRoot, 'CHANGELOG.md'))) {
    failures.push('CHANGELOG.md is missing.');
    return;
  }

  const packageJson = await readJson('package.json');
  const changelog = await readText('CHANGELOG.md');
  const escapedVersion = escapeRegExp(packageJson.version);
  const versionHeading = new RegExp(`^##\\s+(?:v)?${escapedVersion}(?:\\s|$)`, 'm');
  if (!versionHeading.test(changelog)) {
    failures.push(`CHANGELOG.md does not contain a heading for v${packageJson.version}`);
  }
}

function checkSensitivePaths() {
  const tracked = git(['ls-files', '-z'])
    .split('\0')
    .filter(Boolean);
  const untracked = git(['ls-files', '--others', '--exclude-standard', '-z'])
    .split('\0')
    .filter(Boolean);

  for (const path of [...tracked, ...untracked]) {
    if (isSensitivePath(path)) {
      failures.push(`sensitive file is tracked or non-ignored: ${path}`);
    }
  }
}

function checkCleanWorkingTree() {
  const status = git(['status', '--porcelain=v1', '--untracked-files=all']).trim();
  if (status) {
    failures.push('working tree is not clean; commit or discard changes before release/tag creation');
  }
}

function isSensitivePath(path) {
  const normalized = path.replace(/\\/g, '/');
  const lower = normalized.toLowerCase();
  const fileName = lower.split('/').pop() ?? lower;

  if (lower === '.env.example' || lower.endsWith('/.env.example')) {
    return false;
  }

  if (lower === '.env' || lower.startsWith('.env.') || lower.endsWith('/.env') || lower.includes('/.env.')) {
    return true;
  }

  if (lower === 'src-tauri/updater/tauri-update.key') {
    return true;
  }

  if (fileName === 'id_rsa' || fileName === 'id_ed25519') {
    return true;
  }

  return ['.pem', '.p12', '.pfx', '.key', '.sqlite', '.sqlite3', '.db', '.log', '.sig']
    .some((extension) => lower.endsWith(extension));
}

async function readJson(relativePath) {
  return JSON.parse(await readText(relativePath));
}

async function readText(relativePath) {
  return readFile(resolve(projectRoot, relativePath), 'utf8');
}

function git(gitArgs) {
  const result = spawnSync('git', gitArgs, {
    cwd: projectRoot,
    encoding: 'utf8',
    stdio: ['ignore', 'pipe', 'pipe'],
  });

  if (result.error) {
    throw result.error;
  }
  if (result.status !== 0) {
    throw new Error(result.stderr || `git ${gitArgs.join(' ')} failed with exit code ${result.status}`);
  }

  return result.stdout;
}

function escapeRegExp(value) {
  return value.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}

function printUsage() {
  console.log(`
Usage:
  node scripts/release-check.mjs [--require-clean]

Checks version consistency, changelog coverage and sensitive file paths.

Options:
  --require-clean    Also require git status --porcelain to be empty.
`);
}
