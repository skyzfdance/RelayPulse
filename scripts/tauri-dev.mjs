import { spawn } from 'node:child_process';
import path from 'node:path';
import process from 'node:process';
import { fileURLToPath } from 'node:url';

const scriptDirectory = path.dirname(fileURLToPath(import.meta.url));
const environment = { ...process.env };

if (process.platform === 'darwin') {
  const target =
    process.arch === 'arm64'
      ? 'AARCH64_APPLE_DARWIN'
      : process.arch === 'x64'
        ? 'X86_64_APPLE_DARWIN'
        : null;

  if (!target) {
    throw new Error(`Unsupported macOS development architecture: ${process.arch}`);
  }

  environment[`CARGO_TARGET_${target}_RUNNER`] = path.join(
    scriptDirectory,
    'macos-tauri-dev-runner.sh',
  );
}

const command = process.platform === 'win32' ? 'pnpm.cmd' : 'pnpm';
const child = spawn(command, ['exec', 'tauri', 'dev', ...process.argv.slice(2)], {
  env: environment,
  stdio: 'inherit',
});

const terminationSignals =
  process.platform === 'win32' ? ['SIGINT', 'SIGTERM'] : ['SIGHUP', 'SIGINT', 'SIGTERM'];
const signalHandlers = new Map();
let childFinished = false;
let forwardedSignal = null;

function removeSignalHandlers() {
  for (const [signal, handler] of signalHandlers) {
    process.removeListener(signal, handler);
  }
  signalHandlers.clear();
}

function forwardSignal(signal) {
  if (childFinished || forwardedSignal !== null) return;
  forwardedSignal = signal;
  child.kill(signal);
}

for (const signal of terminationSignals) {
  const handler = () => forwardSignal(signal);
  signalHandlers.set(signal, handler);
  process.on(signal, handler);
}

child.on('error', (error) => {
  if (childFinished) return;
  childFinished = true;
  removeSignalHandlers();
  console.error(error);
  process.exit(1);
});

child.on('exit', (code, signal) => {
  if (childFinished) return;
  childFinished = true;
  removeSignalHandlers();
  if (signal) {
    process.kill(process.pid, signal);
    return;
  }
  process.exit(code ?? 1);
});
