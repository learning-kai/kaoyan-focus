import { existsSync } from 'node:fs';
import { readFile, writeFile } from 'node:fs/promises';
import { execFileSync, spawnSync } from 'node:child_process';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { resolveUpdateBaseUrlFromArgs, updateTauriUpdaterEndpoint } from './release-config.mjs';

const scriptDir = dirname(fileURLToPath(import.meta.url));
const desktopRoot = resolve(scriptDir, '..');
const defaultAndroidRoot = resolve(desktopRoot, '..', 'UltraFocus-master');
const changelogPath = resolve(desktopRoot, 'CHANGELOG.md');

const rawArgs = process.argv.slice(2);
const packageJsonPath = resolve(desktopRoot, 'package.json');
const packageLockPath = resolve(desktopRoot, 'package-lock.json');
const cargoTomlPath = resolve(desktopRoot, 'src-tauri', 'Cargo.toml');
const cargoLockPath = resolve(desktopRoot, 'src-tauri', 'Cargo.lock');
const tauriConfigPath = resolve(desktopRoot, 'src-tauri', 'tauri.conf.json');
const releaseArgs = resolveUpdateBaseUrlFromArgs(rawArgs, process.env, 'preparing a release');
const options = parseArgs(releaseArgs.args, process.env);
const androidRoot = resolve(options.androidProjectDir);
const androidGradlePath = resolve(androidRoot, 'app', 'build.gradle.kts');

if (options.includeAndroid && !existsSync(androidGradlePath)) {
  throw new Error(`Android project not found. Expected: ${androidGradlePath}`);
}

const packageJson = JSON.parse(await readFile(packageJsonPath, 'utf8'));
const currentVersion = packageJson.version;
const nextVersion = resolveNextVersion(currentVersion, options.versionArgs);
const today = formatLocalDate(new Date());

await updatePackageJson(nextVersion);
await replaceVersionInFile(cargoTomlPath, /^version = "([^"]+)"/m, `version = "${nextVersion}"`);
await updateCargoLock(nextVersion);
await updateTauriConfig(nextVersion);
if (releaseArgs.updateBaseUrl) {
  await updateTauriUpdaterEndpoint(tauriConfigPath, releaseArgs.updateBaseUrl);
}
if (options.includeAndroid) {
  await updateAndroidGradle(nextVersion);
}
await updateChangelog(nextVersion, today, options.includeAndroid);

console.log(`Prepared release v${nextVersion}`);
console.log(`Desktop: ${desktopRoot}`);
if (options.includeAndroid) {
  console.log(`Android: ${androidRoot}`);
} else {
  console.log('Android: skipped (pass --include-android or set INCLUDE_ANDROID_RELEASE=1 to sync it)');
}

function parseArgs(args, env) {
  const versionArgs = [];
  let includeAndroid = isAndroidReleaseEnabled(env);
  let androidProjectDir = env.ANDROID_PROJECT_DIR ?? defaultAndroidRoot;

  for (let index = 0; index < args.length; index += 1) {
    const arg = args[index];
    const inline = arg.match(/^(--android-project-dir)=(.*)$/);

    if (arg === '--include-android') {
      includeAndroid = true;
      continue;
    }

    if (inline) {
      androidProjectDir = inline[2];
      continue;
    }

    if (arg === '--android-project-dir') {
      const value = args[index + 1];
      if (!value || value.startsWith('--')) {
        throw new Error('--android-project-dir requires a path.');
      }

      androidProjectDir = value;
      index += 1;
      continue;
    }

    versionArgs.push(arg);
  }

  return {
    versionArgs,
    includeAndroid,
    androidProjectDir,
  };
}

function isAndroidReleaseEnabled(env) {
  return /^(1|true|yes)$/i.test(env.INCLUDE_ANDROID_RELEASE ?? '');
}

function formatLocalDate(date) {
  const year = date.getFullYear();
  const month = String(date.getMonth() + 1).padStart(2, '0');
  const day = String(date.getDate()).padStart(2, '0');
  return `${year}-${month}-${day}`;
}

function resolveNextVersion(current, rawArgs) {
  const explicitInline = rawArgs.find((arg) => arg.startsWith('--version='));
  if (explicitInline) {
    const explicit = explicitInline.split('=', 2)[1];
    assertVersion(explicit);
    return explicit;
  }

  const explicitIndex = rawArgs.indexOf('--version');
  if (explicitIndex !== -1) {
    const explicit = rawArgs[explicitIndex + 1];
    assertVersion(explicit);
    return explicit;
  }

  const bump =
    rawArgs
      .find((arg) => ['major', 'minor', 'patch', '--major', '--minor', '--patch'].includes(arg))
      ?.replace(/^--/, '') ?? 'patch';
  const parts = current.split('.').map(Number);
  if (parts.length !== 3 || parts.some((part) => !Number.isInteger(part) || part < 0)) {
    throw new Error(`Current version is not semver-like: ${current}`);
  }
  if (bump === 'major') return `${parts[0] + 1}.0.0`;
  if (bump === 'minor') return `${parts[0]}.${parts[1] + 1}.0`;
  return `${parts[0]}.${parts[1]}.${parts[2] + 1}`;
}

function assertVersion(value) {
  if (!/^\d+\.\d+\.\d+$/.test(value ?? '')) {
    throw new Error(`Invalid version: ${value}. Expected X.Y.Z`);
  }
}

async function updatePackageJson(version) {
  const next = { ...packageJson, version };
  await writeFile(packageJsonPath, `${JSON.stringify(next, null, 2)}\n`, 'utf8');
  await updatePackageLock(version);
}

async function updatePackageLock(version) {
  if (!existsSync(packageLockPath)) {
    return;
  }

  const lock = JSON.parse(await readFile(packageLockPath, 'utf8'));
  lock.version = version;
  if (lock.packages?.['']) {
    lock.packages[''].version = version;
  }
  await writeFile(packageLockPath, `${JSON.stringify(lock, null, 2)}\n`, 'utf8');
}

async function updateCargoLock(version) {
  if (!existsSync(cargoLockPath)) {
    return;
  }

  const content = await readFile(cargoLockPath, 'utf8');
  const next = content.replace(/(\[\[package\]\]\r?\nname = "kaoyan-focus"\r?\nversion = ")[^"]+(")/, `$1${version}$2`);
  if (next === content) {
    throw new Error(`Unable to find kaoyan-focus package version in ${cargoLockPath}`);
  }
  await writeFile(cargoLockPath, next, 'utf8');
}

async function updateTauriConfig(version) {
  const config = JSON.parse(await readFile(tauriConfigPath, 'utf8'));
  config.version = version;
  await writeFile(tauriConfigPath, `${JSON.stringify(config, null, 2)}\n`, 'utf8');
}

async function updateAndroidGradle(version) {
  const content = await readFile(androidGradlePath, 'utf8');
  const versionCodeMatch = content.match(/versionCode\s*=\s*(\d+)/);
  const versionNameMatch = content.match(/versionName\s*=\s*"([^"]+)"/);
  if (!versionCodeMatch) throw new Error(`Unable to find Android versionCode in ${androidGradlePath}`);
  if (!versionNameMatch) throw new Error(`Unable to find Android versionName in ${androidGradlePath}`);
  const currentVersionCode = Number(versionCodeMatch[1]);
  const currentVersionName = versionNameMatch[1];
  const nextVersionCode = currentVersionName === version ? currentVersionCode : currentVersionCode + 1;
  const next = content
    .replace(/versionCode\s*=\s*\d+/, `versionCode = ${nextVersionCode}`)
    .replace(/versionName\s*=\s*"[^"]+"/, `versionName = "${version}"`);
  await writeFile(androidGradlePath, next, 'utf8');
}

async function replaceVersionInFile(filePath, pattern, replacement) {
  const content = await readFile(filePath, 'utf8');
  if (!pattern.test(content)) throw new Error(`Unable to find version in ${filePath}`);
  await writeFile(filePath, content.replace(pattern, replacement), 'utf8');
}

async function updateChangelog(version, date, includeAndroid) {
  const existing = existsSync(changelogPath) ? await readFile(changelogPath, 'utf8') : '# Changelog\n\n';
  const heading = `## v${version} - ${date}`;
  const withoutDuplicate = existing
    .replace(new RegExp(`\\n?## v${escapeRegExp(version)} - \\d{4}-\\d{2}-\\d{2}[\\s\\S]*?(?=\\n## v|$)`), '')
    .trimEnd();
  const sections = [heading, '', '### Desktop', formatCommitGroups(desktopRoot)];

  if (includeAndroid) {
    sections.push('', '### Android', formatCommitGroups(androidRoot));
  }

  const entry = [...sections, ''].join('\n');

  const base = withoutDuplicate.startsWith('# Changelog') ? withoutDuplicate : `# Changelog\n\n${withoutDuplicate}`;
  const normalizedBase = base.trimEnd() === '# Changelog' ? '# Changelog' : base.trimEnd();
  await writeFile(
    changelogPath,
    `${normalizedBase}\n\n${entry}${normalizedBase === '# Changelog' ? '' : '\n'}\n`,
    'utf8',
  );
}

function formatCommitGroups(repoRoot) {
  const commits = getReleaseCommits(repoRoot);
  if (commits.length === 0) return '- No commits found.';
  const groups = {
    Added: [],
    Fixed: [],
    Changed: [],
  };
  for (const commit of commits) {
    groups[classifyCommit(commit.subject)].push(commit);
  }
  return Object.entries(groups)
    .filter(([, items]) => items.length > 0)
    .map(([title, items]) => [`#### ${title}`, ...items.map((item) => `- ${item.subject} (${item.hash})`)].join('\n'))
    .join('\n\n');
}

function getReleaseCommits(repoRoot) {
  const lastTag = git(
    repoRoot,
    ['-c', 'i18n.logOutputEncoding=utf-8', 'describe', '--tags', '--match', 'v*', '--abbrev=0'],
    { allowFailure: true },
  ).trim();
  const rangeArgs = lastTag ? [`${lastTag}..HEAD`] : ['-30'];
  const output = git(
    repoRoot,
    ['-c', 'i18n.logOutputEncoding=utf-8', 'log', ...rangeArgs, '--pretty=format:%h%x09%s'],
    { allowFailure: true },
  ).trim();
  if (!output) return [];
  return output
    .split('\n')
    .map((line) => {
      const [hash, ...subjectParts] = line.split('\t');
      return { hash, subject: subjectParts.join('\t').trim() };
    })
    .filter((item) => item.hash && item.subject);
}

function classifyCommit(subject) {
  const normalized = subject.trim().toLowerCase();
  if (/^(feat|feature|add)(\b|[:(])/.test(normalized) || subject.includes('新增') || subject.includes('增加'))
    return 'Added';
  if (/^(fix|bugfix)(\b|[:(])/.test(normalized) || subject.includes('修复')) return 'Fixed';
  return 'Changed';
}

function git(cwd, args, options = {}) {
  try {
    return execGit(cwd, args);
  } catch (error) {
    if (options.allowFailure) return '';
    throw error;
  }
}

function execGit(cwd, args) {
  const direct = spawnSync('git', args, { cwd, encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'] });
  if (!direct.error) {
    if (direct.status === 0) return direct.stdout;
    throw new Error(direct.stderr || `git ${args.join(' ')} failed with exit code ${direct.status}`);
  }

  return execFileSync('cmd.exe', ['/d', '/s', '/c', 'git', ...args], {
    cwd,
    encoding: 'utf8',
    stdio: ['ignore', 'pipe', 'pipe'],
  });
}

function escapeRegExp(value) {
  return value.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}
