import { existsSync, readFileSync } from 'node:fs';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import { execFileSync } from 'node:child_process';
import { dirname, resolve } from 'node:path';

const UPDATE_BASE_URL_FLAGS = new Set([
  '--update-base-url',
  '--release-base-url',
  '--download-base-url',
]);

export function extractUpdateBaseUrlArgs(rawArgs, env = process.env) {
  const args = [];
  let updateBaseUrl = env.UPDATE_BASE_URL;
  let source = updateBaseUrl ? 'UPDATE_BASE_URL' : null;

  function setUpdateBaseUrl(value, flag) {
    if (source && source !== 'UPDATE_BASE_URL') {
      throw new Error(`Duplicate update base URL. Keep only one ${flag} argument.`);
    }

    updateBaseUrl = value;
    source = flag;
  }

  for (let index = 0; index < rawArgs.length; index += 1) {
    const arg = rawArgs[index];
    const inlineMatch = arg.match(/^(--(?:update-base-url|release-base-url|download-base-url))=(.*)$/);

    if (inlineMatch) {
      setUpdateBaseUrl(inlineMatch[2], inlineMatch[1]);
      continue;
    }

    if (UPDATE_BASE_URL_FLAGS.has(arg)) {
      const value = rawArgs[index + 1];
      if (value === undefined || value.startsWith('--')) {
        throw new Error(`${arg} requires an https:// release download base URL.`);
      }

      setUpdateBaseUrl(value, arg);
      index += 1;
      continue;
    }

    args.push(arg);
  }

  return { args, updateBaseUrl, source };
}

export function resolveUpdateBaseUrlFromArgs(rawArgs, env = process.env, action = 'building a release') {
  const parsed = extractUpdateBaseUrlArgs(rawArgs, env);
  const inferred = parsed.updateBaseUrl ?? inferUpdateBaseUrlFromGitRemote(env);
  return {
    ...parsed,
    updateBaseUrl: normalizeUpdateBaseUrl(inferred, action),
  };
}

export function resolveOptionalUpdateBaseUrlFromArgs(rawArgs, env = process.env) {
  const parsed = extractUpdateBaseUrlArgs(rawArgs, env);
  const inferred = parsed.updateBaseUrl ?? inferUpdateBaseUrlFromGitRemote(env);
  return {
    ...parsed,
    updateBaseUrl: inferred ? normalizeUpdateBaseUrl(inferred) : null,
  };
}

export function normalizeUpdateBaseUrl(updateBaseUrl, action = 'building update metadata') {
  const normalized = updateBaseUrl?.trim();
  if (!normalized) {
    throw new Error(
      `Missing UPDATE_BASE_URL. Set it or pass --update-base-url <https://...> before ${action}.`,
    );
  }

  if (/(OWNER\/REPO|<owner>|<repo>|updates\.invalid)/i.test(normalized)) {
    throw new Error('UPDATE_BASE_URL still contains a placeholder value.');
  }

  let url;
  try {
    url = new URL(normalized);
  } catch {
    throw new Error(`Invalid UPDATE_BASE_URL: ${normalized}`);
  }

  if (url.protocol !== 'https:') {
    throw new Error('UPDATE_BASE_URL must use https://.');
  }

  if (url.search || url.hash) {
    throw new Error('UPDATE_BASE_URL must not include query parameters or fragments.');
  }

  if (/\/latest\.json$/i.test(url.pathname)) {
    throw new Error('UPDATE_BASE_URL must point to the release download directory, not latest.json.');
  }

  return url.toString().replace(/\/$/, '');
}

function setUpdaterEndpoint(config, updateBaseUrl) {
  config.plugins ??= {};
  config.plugins.updater ??= {};
  config.plugins.updater.endpoints = updateBaseUrl
    ? [`${normalizeUpdateBaseUrl(updateBaseUrl)}/latest.json`]
    : [];
  return config;
}

export async function writeTauriReleaseConfig(sourcePath, targetPath, updateBaseUrl) {
  const config = setUpdaterEndpoint(
    JSON.parse(await readFile(sourcePath, 'utf8')),
    updateBaseUrl,
  );
  await mkdir(dirname(targetPath), { recursive: true });
  await writeFile(targetPath, `${JSON.stringify(config, null, 2)}\n`, 'utf8');
}

export async function updateTauriUpdaterEndpoint(tauriConfigPath, updateBaseUrl) {
  const config = setUpdaterEndpoint(
    JSON.parse(await readFile(tauriConfigPath, 'utf8')),
    updateBaseUrl,
  );
  await writeFile(tauriConfigPath, `${JSON.stringify(config, null, 2)}\n`, 'utf8');
}

export function inferUpdateBaseUrlFromGitRemote(env = process.env) {
  const cwd = env.KAOYAN_RELEASE_CWD ?? process.cwd();
  const remoteUrl = readGitRemoteUrl(cwd);
  if (!remoteUrl) {
    return inferUpdateBaseUrlFromConfigFile(cwd);
  }

  const githubMatch = remoteUrl.match(
    /^(?:git@github\.com:|https:\/\/github\.com\/)([^/]+)\/([^/]+?)(?:\.git)?$/i,
  );
  if (!githubMatch) {
    return null;
  }

  const owner = githubMatch[1];
  const repo = githubMatch[2];
  return `https://github.com/${owner}/${repo}/releases/latest/download`;
}

function inferUpdateBaseUrlFromConfigFile(cwd) {
  const configPath = resolve(cwd, 'scripts', 'release-base-url.txt');
  if (!existsSync(configPath)) {
    return null;
  }

  try {
    const value = readFileSync(configPath, 'utf8').trim();
    if (!value || /(OWNER\/REPO|<owner>|<repo>|updates\.invalid)/i.test(value)) {
      return null;
    }

    return value;
  } catch {
    return null;
  }
}

function readGitRemoteUrl(cwd) {
  try {
    return execFileSync('git', ['remote', 'get-url', 'origin'], {
      cwd: resolve(cwd),
      encoding: 'utf8',
      stdio: ['ignore', 'pipe', 'pipe'],
    }).trim();
  } catch {
    try {
      return execFileSync('cmd.exe', ['/d', '/s', '/c', 'git', 'remote', 'get-url', 'origin'], {
        cwd: resolve(cwd),
        encoding: 'utf8',
        stdio: ['ignore', 'pipe', 'pipe'],
      }).trim();
    } catch {
      return null;
    }
  }
}
