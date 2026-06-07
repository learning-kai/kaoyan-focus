import { existsSync } from 'node:fs';
import { join } from 'node:path';
import { spawnSync } from 'node:child_process';

const cargo = resolveCargoCommand();
const steps = [
  ['fmt', '--manifest-path', 'src-tauri/Cargo.toml', '--', '--check'],
  ['clippy', '--manifest-path', 'src-tauri/Cargo.toml', '--all-targets', '--', '-D', 'warnings'],
  ['test', '--manifest-path', 'src-tauri/Cargo.toml'],
];

for (const args of steps) {
  run(cargo, args);
}

function resolveCargoCommand() {
  const candidates = [];
  if (process.env.CARGO) candidates.push(process.env.CARGO);

  if (process.platform === 'win32') {
    if (process.env.CARGO_HOME) candidates.push(join(process.env.CARGO_HOME, 'bin', 'cargo.exe'));
    if (process.env.USERPROFILE) candidates.push(join(process.env.USERPROFILE, '.cargo', 'bin', 'cargo.exe'));
    if (process.env.HOME) candidates.push(join(process.env.HOME, '.cargo', 'bin', 'cargo.exe'));
  } else {
    if (process.env.CARGO_HOME) candidates.push(join(process.env.CARGO_HOME, 'bin', 'cargo'));
    if (process.env.HOME) candidates.push(join(process.env.HOME, '.cargo', 'bin', 'cargo'));
  }

  return candidates.find((candidate) => existsSync(candidate)) ?? 'cargo';
}

function run(command, args) {
  console.log(`\n> ${['cargo', ...args].join(' ')}`);
  const result = spawnSync(command, args, {
    cwd: process.cwd(),
    stdio: 'inherit',
    shell: false,
  });

  if (result.error) {
    throw result.error;
  }
  if (result.status !== 0) {
    process.exit(result.status ?? 1);
  }
}
