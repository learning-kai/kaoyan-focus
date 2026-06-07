import { existsSync } from 'node:fs';
import { readFile, readdir, writeFile } from 'node:fs/promises';
import { basename, dirname, join } from 'node:path';
import { resolveUpdateBaseUrlFromArgs } from './release-config.mjs';

const releaseArgs = resolveUpdateBaseUrlFromArgs(
  process.argv.slice(2),
  process.env,
  'generating latest.json',
);
const args = releaseArgs.args;
if (args.length > 1) {
  console.error('Usage: node scripts/generate-latest-json.mjs [installer.exe] [--update-base-url <https://...>]');
  process.exit(1);
}

const packageJson = JSON.parse(await readFile('package.json', 'utf8'));
const tauriConfig = JSON.parse(await readFile('src-tauri/tauri.conf.json', 'utf8'));
const downloadBaseUrl = releaseArgs.updateBaseUrl;
const artifactPath = args[0] ?? await findInstallerPath(packageJson.name, tauriConfig.productName, packageJson.version);

if (!existsSync(artifactPath)) {
  throw new Error(`Installer not found: ${artifactPath}`);
}
if (!existsSync(`${artifactPath}.sig`)) {
  throw new Error(`Installer signature not found: ${artifactPath}.sig`);
}

const signature = (await readFile(`${artifactPath}.sig`, 'utf8')).trim();
const fileName = basename(artifactPath);
const notes = await readReleaseNotes(packageJson.version);

const latestJson = {
  version: packageJson.version,
  notes,
  pub_date: new Date().toISOString(),
  platforms: {
    'windows-x86_64': {
      signature,
      url: `${downloadBaseUrl.replace(/\/$/, '')}/${encodeURIComponent(fileName)}`,
    },
  },
};

const outputPath = join(dirname(artifactPath), 'latest.json');
await writeFile(outputPath, `${JSON.stringify(latestJson, null, 2)}\n`, 'utf8');
console.log(`Wrote ${outputPath}`);

async function readReleaseNotes(version) {
  try {
    const changelog = await readFile('CHANGELOG.md', 'utf8');
    const match = changelog.match(new RegExp(`(^## v${escapeRegExp(version)} - \\d{4}-\\d{2}-\\d{2}\\n[\\s\\S]*?)(?=\\n## v|$)`, 'm'));
    return match?.[1]?.trim() || fallbackNotes(version);
  } catch {
    return fallbackNotes(version);
  }
}

function fallbackNotes(version) {
  return `考研专注 ${version} 更新`;
}

function escapeRegExp(value) {
  return value.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}

async function findInstallerPath(packageName, productName, version) {
  const bundleDir = join('src-tauri', 'target', 'release', 'bundle', 'nsis');
  const entries = await readdir(bundleDir, { withFileTypes: true }).catch(() => []);
  const publishFileName = getPublishInstallerFileName(packageName, version);
  const publishPath = join(bundleDir, publishFileName);
  if (existsSync(publishPath)) {
    return publishPath;
  }

  const matches = entries
    .filter((entry) => entry.isFile())
    .map((entry) => join(bundleDir, entry.name))
    .filter((filePath) => filePath.endsWith('-setup.exe') && filePath.includes(`_${version}_`));

  if (matches.length === 1) {
    return matches[0];
  }

  const fallback = join(bundleDir, `${productName}_${version}_x64-setup.exe`);
  if (matches.length === 0) {
    return existsSync(fallback) ? fallback : publishPath;
  }

  throw new Error(
    `Multiple NSIS installers found for version ${version}; pass the installer path explicitly.`,
  );
}

function getPublishInstallerFileName(packageName, version) {
  return `${packageName}_${version}_x64-setup.exe`;
}
