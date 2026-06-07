import { existsSync } from 'node:fs';
import { readFile } from 'node:fs/promises';
import { execFileSync, spawnSync } from 'node:child_process';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const scriptDir = dirname(fileURLToPath(import.meta.url));
const desktopRoot = resolve(scriptDir, '..');
const defaultAndroidRoot = resolve(desktopRoot, '..', 'UltraFocus-master');
const options = parseArgs(process.argv.slice(2), process.env);
const androidRoot = resolve(options.androidProjectDir);
const androidGradlePath = resolve(androidRoot, 'app', 'build.gradle.kts');

if (options.includeAndroid && !existsSync(androidGradlePath)) {
  throw new Error(`Android project not found. Expected: ${androidGradlePath}`);
}

const packageJson = JSON.parse(await readFile(resolve(desktopRoot, 'package.json'), 'utf8'));
const version = packageJson.version;
const tag = `v${version}`;

if (options.includeAndroid) {
  const androidGradle = await readFile(androidGradlePath, 'utf8');
  const androidVersion = androidGradle.match(/versionName\s*=\s*"([^"]+)"/)?.[1];
  if (androidVersion !== version) {
    throw new Error(`Version mismatch: desktop=${version}, android=${androidVersion}`);
  }
}

ensureTagAvailable(desktopRoot, tag, 'Desktop');
if (options.includeAndroid) {
  ensureTagAvailable(androidRoot, tag, 'Android');
}

git(desktopRoot, ['tag', tag]);
if (options.includeAndroid) {
  git(androidRoot, ['tag', tag]);
}

console.log(options.includeAndroid
  ? `Created ${tag} in desktop and Android repositories.`
  : `Created ${tag} in desktop repository.`);

function parseArgs(args, env) {
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

    throw new Error(`Unknown tag-release argument: ${arg}`);
  }

  return {
    includeAndroid,
    androidProjectDir,
  };
}

function isAndroidReleaseEnabled(env) {
  return /^(1|true|yes)$/i.test(env.INCLUDE_ANDROID_RELEASE ?? '');
}

function ensureTagAvailable(repoRoot, tagName, label) {
  const exists = git(repoRoot, ['tag', '--list', tagName]).trim();
  if (exists) throw new Error(`${label} tag already exists: ${tagName}`);
}

function git(cwd, args) {
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
