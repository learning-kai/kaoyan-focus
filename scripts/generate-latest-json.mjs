import { readFile, writeFile } from 'node:fs/promises';
import { basename, dirname, join } from 'node:path';

const firstArg = process.argv[2];
const secondArg = process.argv[3];

if (!firstArg) {
  console.error('Usage: node scripts/generate-latest-json.mjs [installer.exe] <download-base-url>');
  process.exit(1);
}

const packageJson = JSON.parse(await readFile('package.json', 'utf8'));
const tauriConfig = JSON.parse(await readFile('src-tauri/tauri.conf.json', 'utf8'));
const artifactPath = secondArg
  ? firstArg
  : join(
    'src-tauri',
    'target',
    'release',
    'bundle',
    'nsis',
    `${tauriConfig.productName}_${packageJson.version}_x64-setup.exe`,
  );
const downloadBaseUrl = secondArg ?? firstArg;
const signature = (await readFile(`${artifactPath}.sig`, 'utf8')).trim();
const fileName = basename(artifactPath);
const normalizedBaseUrl = downloadBaseUrl.replace(/\/$/, '');
const notes = await readReleaseNotes(packageJson.version);

const latestJson = {
  version: packageJson.version,
  notes,
  pub_date: new Date().toISOString(),
  platforms: {
    'windows-x86_64': {
      signature,
      url: `${normalizedBaseUrl}/${encodeURIComponent(fileName)}`,
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
