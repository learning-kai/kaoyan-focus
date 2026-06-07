import { existsSync } from 'node:fs';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import { spawnSync } from 'node:child_process';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { resolveUpdateBaseUrlFromArgs } from './release-config.mjs';

const scriptDir = dirname(fileURLToPath(import.meta.url));
const projectRoot = resolve(scriptDir, '..');
const rawArgs = process.argv.slice(2);

if (rawArgs.includes('--help') || rawArgs.includes('-h')) {
  printUsage();
  process.exit(0);
}

const releaseArgs = resolveUpdateBaseUrlFromArgs(rawArgs, process.env, 'running the automatic release workflow');
const options = parseArgs(releaseArgs.args);
const updateBaseUrl = releaseArgs.updateBaseUrl;
const packageBefore = await readPackageJson();
const plannedVersion = resolvePlannedVersion(packageBefore.version, options);
const updateRepo = options.repo ?? inferGithubRepoFromUpdateBaseUrl(updateBaseUrl);

if (!updateRepo) {
  throw new Error('Unable to infer GitHub update repo. Pass --repo OWNER/REPO.');
}

if (options.dryRun) {
  printPlan({ packageName: packageBefore.name, version: plannedVersion, updateBaseUrl, updateRepo, options });
  process.exit(0);
}

await runReleaseCheck(['--require-clean']);

if (!options.skipPrepare) {
  await runNodeScript('scripts/prepare-release.mjs', [
    ...buildPrepareArgs(options),
    '--update-base-url',
    updateBaseUrl,
  ]);
}

await runReleaseCheck();

let packageJson = await readPackageJson();
let version = packageJson.version;
let tagName = `v${version}`;

if (!options.skipGit) {
  await createReleaseCommitAndTag(tagName);
}

if (!options.skipBuild) {
  await runNodeScript('scripts/release-win-update.mjs', ['--update-base-url', updateBaseUrl]);
}

packageJson = await readPackageJson();
version = packageJson.version;
tagName = `v${version}`;
const assets = resolveReleaseAssets(packageJson.name, version);
assertReleaseAssets(assets);

if (!options.skipPublish) {
  assertGhReady();
  if (!options.skipGit && !options.skipPush) {
    pushReleaseGitState(tagName);
  }
  const notesFile = await resolveNotesFile(version, options);
  await publishGithubRelease({
    repo: updateRepo,
    tagName,
    title: tagName,
    notesFile,
    assets,
    latest: options.latest,
    prerelease: options.prerelease,
    replaceAssets: options.replaceAssets,
  });
}

console.log('');
console.log(`Release ${tagName} finished.`);
console.log(`Update repo: ${updateRepo}`);
console.log(`latest.json: ${updateBaseUrl}/latest.json`);
console.log('Assets:');
for (const asset of assets) {
  console.log(`- ${asset}`);
}

function parseArgs(args) {
  const options = {
    bump: null,
    version: null,
    repo: process.env.UPDATE_REPO ?? null,
    notes: null,
    notesFile: null,
    skipPrepare: false,
    skipBuild: false,
    skipPublish: false,
    skipGit: false,
    skipPush: false,
    replaceAssets: false,
    dryRun: false,
    prerelease: false,
    latest: true,
    includeAndroid: isAndroidReleaseEnabled(process.env),
    androidProjectDir: null,
  };

  for (let index = 0; index < args.length; index += 1) {
    const arg = args[index];
    const inline = arg.match(/^(--[^=]+)=(.*)$/);
    const flag = inline?.[1] ?? arg;
    const inlineValue = inline?.[2];

    if (['major', 'minor', 'patch', '--major', '--minor', '--patch'].includes(arg)) {
      setBump(options, arg.replace(/^--/, ''));
      continue;
    }

    if (flag === '--version') {
      options.version = takeValue(args, inlineValue, index, flag);
      if (inlineValue === undefined) index += 1;
      continue;
    }

    if (flag === '--repo') {
      options.repo = takeValue(args, inlineValue, index, flag);
      if (inlineValue === undefined) index += 1;
      continue;
    }

    if (flag === '--notes') {
      options.notes = takeValue(args, inlineValue, index, flag);
      if (inlineValue === undefined) index += 1;
      continue;
    }

    if (flag === '--notes-file') {
      options.notesFile = resolve(projectRoot, takeValue(args, inlineValue, index, flag));
      if (inlineValue === undefined) index += 1;
      continue;
    }

    if (arg === '--skip-prepare') {
      options.skipPrepare = true;
      continue;
    }

    if (arg === '--skip-build') {
      options.skipBuild = true;
      continue;
    }

    if (arg === '--skip-publish') {
      options.skipPublish = true;
      continue;
    }

    if (arg === '--skip-git') {
      options.skipGit = true;
      options.skipPush = true;
      continue;
    }

    if (arg === '--skip-push') {
      options.skipPush = true;
      continue;
    }

    if (arg === '--replace-assets') {
      options.replaceAssets = true;
      continue;
    }

    if (arg === '--dry-run') {
      options.dryRun = true;
      continue;
    }

    if (arg === '--prerelease') {
      options.prerelease = true;
      options.latest = false;
      continue;
    }

    if (arg === '--no-latest') {
      options.latest = false;
      continue;
    }

    if (arg === '--include-android') {
      options.includeAndroid = true;
      continue;
    }

    if (flag === '--android-project-dir') {
      options.androidProjectDir = takeValue(args, inlineValue, index, flag);
      if (inlineValue === undefined) index += 1;
      continue;
    }

    throw new Error(`Unknown release:auto argument: ${arg}`);
  }

  if (options.version && options.bump) {
    throw new Error('Use either --version X.Y.Z or a bump type, not both.');
  }

  if (options.skipPrepare && options.version) {
    throw new Error('--skip-prepare builds the current package version, so it cannot be combined with --version.');
  }

  if (options.notes && options.notesFile) {
    throw new Error('Use either --notes or --notes-file, not both.');
  }

  if (options.version && !/^\d+\.\d+\.\d+$/.test(options.version)) {
    throw new Error(`Invalid --version value: ${options.version}. Expected X.Y.Z.`);
  }

  options.bump ??= 'patch';
  return options;
}

function setBump(options, bump) {
  if (options.bump && options.bump !== bump) {
    throw new Error(`Duplicate bump type: ${options.bump} and ${bump}.`);
  }

  options.bump = bump;
}

function takeValue(args, inlineValue, index, flag) {
  if (inlineValue !== undefined) {
    if (!inlineValue) throw new Error(`${flag} requires a value.`);
    return inlineValue;
  }

  const value = args[index + 1];
  if (!value || value.startsWith('--')) {
    throw new Error(`${flag} requires a value.`);
  }

  return value;
}

function resolvePlannedVersion(currentVersion, options) {
  if (options.skipPrepare) return currentVersion;
  if (options.version) return options.version;
  return bumpVersion(currentVersion, options.bump);
}

function bumpVersion(version, bump) {
  const parts = version.split('.').map(Number);
  if (parts.length !== 3 || parts.some((part) => !Number.isInteger(part) || part < 0)) {
    throw new Error(`Current version is not semver-like: ${version}`);
  }

  if (bump === 'major') return `${parts[0] + 1}.0.0`;
  if (bump === 'minor') return `${parts[0]}.${parts[1] + 1}.0`;
  return `${parts[0]}.${parts[1]}.${parts[2] + 1}`;
}

function buildPrepareArgs(options) {
  const args = [];
  if (options.version) {
    args.push('--version', options.version);
  } else {
    args.push(options.bump);
  }

  if (options.includeAndroid) {
    args.push('--include-android');
  }

  if (options.androidProjectDir) {
    args.push('--android-project-dir', options.androidProjectDir);
  }

  return args;
}

async function readPackageJson() {
  return JSON.parse(await readFile(resolve(projectRoot, 'package.json'), 'utf8'));
}

async function runNodeScript(scriptPath, args) {
  run(process.execPath, [scriptPath, ...args]);
}

async function runReleaseCheck(args = []) {
  run(process.execPath, ['scripts/release-check.mjs', ...args]);
}

async function createReleaseCommitAndTag(tagName) {
  stageReleaseMetadata();

  if (runQuiet('git', ['diff', '--cached', '--quiet']).status !== 0) {
    run('git', ['commit', '-m', `chore: release ${tagName}`]);
  }

  await runReleaseCheck(['--require-clean']);
  ensureLocalTag(tagName);
}

function stageReleaseMetadata() {
  const files = [
    'package.json',
    'package-lock.json',
    'src-tauri/Cargo.toml',
    'src-tauri/Cargo.lock',
    'src-tauri/tauri.conf.json',
    'CHANGELOG.md',
  ].filter((file) => existsSync(resolve(projectRoot, file)));

  run('git', ['add', ...files]);
}

function ensureLocalTag(tagName) {
  const exists = runQuiet('git', ['rev-parse', '-q', '--verify', `refs/tags/${tagName}`]).status === 0;
  if (exists) {
    return;
  }

  run('git', ['tag', tagName]);
}

function pushReleaseGitState(tagName) {
  const branch = runQuiet('git', ['branch', '--show-current']).stdout.trim();
  if (!branch) {
    throw new Error('Cannot push release commit from a detached HEAD. Check out a branch or pass --skip-push.');
  }

  run('git', ['push', 'origin', `HEAD:${branch}`]);
  run('git', ['push', 'origin', tagName]);
}

function assertGhReady() {
  run('gh', ['auth', 'status']);
}

function run(command, args, options = {}) {
  console.log('');
  console.log(`> ${formatCommand(command, args)}`);
  const invocation = resolveSpawnInvocation(command, args);
  const result = spawnSync(invocation.command, invocation.args, {
    cwd: projectRoot,
    env: process.env,
    shell: false,
    stdio: options.stdio ?? 'inherit',
    encoding: options.encoding ?? 'utf8',
  });

  if (result.error) {
    throw result.error;
  }

  if (result.status !== 0) {
    throw new Error(`${command} failed with exit code ${result.status ?? 1}`);
  }

  return result;
}

function runQuiet(command, args) {
  const invocation = resolveSpawnInvocation(command, args);
  return spawnSync(invocation.command, invocation.args, {
    cwd: projectRoot,
    env: process.env,
    shell: false,
    stdio: ['ignore', 'pipe', 'pipe'],
    encoding: 'utf8',
  });
}

function resolveSpawnInvocation(command, args) {
  if (process.platform === 'win32' && command === 'gh') {
    return {
      command: 'cmd.exe',
      args: ['/d', '/s', '/c', 'gh', ...args],
    };
  }

  return { command, args };
}

function resolveReleaseAssets(packageName, version) {
  const bundleDir = resolve(projectRoot, 'src-tauri', 'target', 'release', 'bundle', 'nsis');
  const installer = resolve(bundleDir, `${packageName}_${version}_x64-setup.exe`);
  return [installer, `${installer}.sig`, resolve(bundleDir, 'latest.json')];
}

function assertReleaseAssets(assets) {
  for (const asset of assets) {
    if (!existsSync(asset)) {
      throw new Error(`Missing release asset: ${asset}`);
    }
  }
}

async function resolveNotesFile(version, options) {
  if (options.notesFile) {
    if (!existsSync(options.notesFile)) {
      throw new Error(`Release notes file not found: ${options.notesFile}`);
    }

    return options.notesFile;
  }

  const targetDir = resolve(projectRoot, 'src-tauri', 'target');
  await mkdir(targetDir, { recursive: true });
  const notesPath = resolve(targetDir, `release-notes-v${version}.md`);
  const notes = options.notes ?? (await readChangelogNotes(version)) ?? `kaoyan-focus v${version} update`;
  await writeFile(notesPath, `${notes.trim()}\n`, 'utf8');
  return notesPath;
}

async function readChangelogNotes(version) {
  const changelogPath = resolve(projectRoot, 'CHANGELOG.md');
  if (!existsSync(changelogPath)) {
    return null;
  }

  const changelog = await readFile(changelogPath, 'utf8');
  const escaped = escapeRegExp(version);
  const match = changelog.match(
    new RegExp(`(^##\\s+v?${escaped}(?:\\s+-\\s+\\d{4}-\\d{2}-\\d{2})?\\n[\\s\\S]*?)(?=\\n##\\s+v?\\d|$)`, 'm'),
  );
  return match?.[1]?.trim() || null;
}

async function publishGithubRelease({ repo, tagName, title, notesFile, assets, latest, prerelease, replaceAssets }) {
  const exists = runQuiet('gh', ['release', 'view', tagName, '--repo', repo]).status === 0;

  if (exists) {
    if (!replaceAssets) {
      throw new Error(
        `GitHub Release ${tagName} already exists. Re-run with --replace-assets to edit notes and overwrite assets.`,
      );
    }

    const editArgs = ['release', 'edit', tagName, '--repo', repo, '--title', title, '--notes-file', notesFile];
    if (latest) editArgs.push('--latest');
    if (prerelease) editArgs.push('--prerelease');
    run('gh', editArgs);
    run('gh', ['release', 'upload', tagName, ...assets, '--repo', repo, '--clobber']);
    return;
  }

  const createArgs = [
    'release',
    'create',
    tagName,
    ...assets,
    '--repo',
    repo,
    '--title',
    title,
    '--notes-file',
    notesFile,
  ];
  if (latest) createArgs.push('--latest');
  if (!latest && !prerelease) createArgs.push('--latest=false');
  if (prerelease) createArgs.push('--prerelease');
  run('gh', createArgs);
}

function inferGithubRepoFromUpdateBaseUrl(updateBaseUrl) {
  try {
    const url = new URL(updateBaseUrl);
    if (url.hostname.toLowerCase() !== 'github.com') {
      return null;
    }

    const [owner, repo] = url.pathname.split('/').filter(Boolean);
    return owner && repo ? `${owner}/${repo}` : null;
  } catch {
    return null;
  }
}

function formatCommand(command, args) {
  return [command, ...args].map((part) => (/[\s"`]/.test(part) ? `"${part.replace(/"/g, '\\"')}"` : part)).join(' ');
}

function escapeRegExp(value) {
  return value.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}

function printPlan({ packageName, version, updateBaseUrl, updateRepo, options }) {
  console.log('Automatic release dry run');
  console.log(`Package: ${packageName}`);
  console.log(`Version: ${version}`);
  console.log(`Update base URL: ${updateBaseUrl}`);
  console.log(`GitHub update repo: ${updateRepo}`);
  console.log(`Prepare: ${options.skipPrepare ? 'skip' : 'yes'}`);
  console.log(`Build/sign/latest.json: ${options.skipBuild ? 'skip' : 'yes'}`);
  console.log(`Release commit/tag: ${options.skipGit ? 'skip' : 'yes'}`);
  console.log(`Push commit/tag: ${options.skipGit || options.skipPush ? 'skip' : 'yes'}`);
  console.log(`Replace existing release assets: ${options.replaceAssets ? 'yes' : 'no'}`);
  console.log(`Publish GitHub Release: ${options.skipPublish ? 'skip' : 'yes'}`);
  console.log(`Android sync: ${options.includeAndroid ? 'yes' : 'skip'}`);
}

function printUsage() {
  console.log(`
Usage:
  npm.cmd run release:auto -- --version 1.7.4
  npm.cmd run release:auto -- patch
  npm.cmd run release:auto -- minor --repo learning-kai/kaoyan-focus
  npm.cmd run release:auto -- --version 1.7.4 --include-android

Options:
  --version X.Y.Z          Set an exact version instead of bumping.
  patch|minor|major        Bump type. Defaults to patch.
  --update-base-url URL    Release download base URL. Defaults to the GitHub origin or scripts/release-base-url.txt.
  --repo OWNER/REPO        GitHub repo that stores update assets.
  --notes TEXT             GitHub Release notes.
  --notes-file PATH        GitHub Release notes file.
  --skip-prepare           Do not update version/changelog/config.
  --skip-build             Do not rebuild/sign; publish existing assets.
  --skip-publish           Build only; do not upload to GitHub.
  --skip-git               Do not create or push the release commit/tag.
  --skip-push              Create the local release commit/tag but do not push them.
  --replace-assets         Allow overwriting assets when the GitHub Release already exists.
  --dry-run                Print the plan only.
  --prerelease             Mark the GitHub Release as prerelease.
  --no-latest              Do not explicitly mark the release as latest.
  --include-android        Also sync and validate the internal Android project.
  --android-project-dir    Android project path used with --include-android.
`);
}

function isAndroidReleaseEnabled(env) {
  return /^(1|true|yes)$/i.test(env.INCLUDE_ANDROID_RELEASE ?? '');
}
