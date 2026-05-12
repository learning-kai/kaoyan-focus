import { readFile, writeFile } from 'node:fs/promises';
import { basename, dirname, join } from 'node:path';

const artifactPath = process.argv[2];
const downloadBaseUrl = process.argv[3];

if (!artifactPath || !downloadBaseUrl) {
  console.error('Usage: node scripts/generate-latest-json.mjs <installer.exe> <download-base-url>');
  process.exit(1);
}

const packageJson = JSON.parse(await readFile('package.json', 'utf8'));
const signature = (await readFile(`${artifactPath}.sig`, 'utf8')).trim();
const fileName = basename(artifactPath);
const normalizedBaseUrl = downloadBaseUrl.replace(/\/$/, '');

const latestJson = {
  version: packageJson.version,
  notes: `考研专注 ${packageJson.version} 更新`,
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
