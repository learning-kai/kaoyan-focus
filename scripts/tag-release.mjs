import { existsSync } from 'node:fs';
import { readFile } from 'node:fs/promises';
import { execFileSync, spawnSync } from 'node:child_process';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const scriptDir = dirname(fileURLToPath(import.meta.url));
const desktopRoot = resolve(scriptDir, '..');
const androidRoot = resolve(process.env.ANDROID_PROJECT_DIR ?? resolve(desktopRoot, '..', 'UltraFocus-master'));
const androidGradlePath = resolve(androidRoot, 'app', 'build.gradle.kts');

if (!existsSync(androidGradlePath)) {
  throw new Error(`Android project not found. Expected: ${androidGradlePath}`);
}

const packageJson = JSON.parse(await readFile(resolve(desktopRoot, 'package.json'), 'utf8'));
const version = packageJson.version;
const tag = `v${version}`;

const androidGradle = await readFile(androidGradlePath, 'utf8');
const androidVersion = androidGradle.match(/versionName\s*=\s*"([^"]+)"/)?.[1];
if (androidVersion !== version) {
  throw new Error(`Version mismatch: desktop=${version}, android=${androidVersion}`);
}

ensureTagAvailable(desktopRoot, tag, 'Desktop');
ensureTagAvailable(androidRoot, tag, 'Android');

git(desktopRoot, ['tag', tag]);
git(androidRoot, ['tag', tag]);

console.log(`Created ${tag} in both repositories.`);

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
