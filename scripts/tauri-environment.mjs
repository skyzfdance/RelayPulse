import { spawn } from 'node:child_process';
import path from 'node:path';
import process from 'node:process';
import { fileURLToPath } from 'node:url';

const scriptDirectory = path.dirname(fileURLToPath(import.meta.url));
const projectDirectory = path.dirname(scriptDirectory);

export function tauriInvocation(
  args,
  { platform = process.platform, arch = process.arch, env = process.env } = {},
) {
  const [action, ...options] = args;
  const production = options.includes('--production');
  if (production && !['build', 'bundle'].includes(action)) {
    throw new Error('--production is only supported for build and bundle.');
  }
  // A configuration override could silently restore the production identity in a dev run.
  if (options.some((arg) => /^(--config(?:=|$)|-c)/u.test(arg))) {
    throw new Error(
      'Use --production to select the release configuration; --config is not supported.',
    );
  }
  const environment = { ...env };
  delete environment.TAURI_CONFIG;
  environment.CARGO_TARGET_DIR = path.join(
    projectDirectory,
    'src-tauri/target',
    production ? 'production' : 'development.noindex',
  );
  const cliArgs = options.filter((arg) => arg !== '--production');
  if (production) {
    cliArgs.push('--config', path.join(projectDirectory, 'src-tauri/tauri.production.conf.json'));
  }
  if (platform === 'darwin' && action === 'dev') {
    const targets = { arm64: 'AARCH64_APPLE_DARWIN', x64: 'X86_64_APPLE_DARWIN' };
    const target = targets[arch];
    if (!target) throw new Error(`Unsupported macOS development architecture: ${arch}`);
    environment[`CARGO_TARGET_${target}_RUNNER`] = path.join(
      scriptDirectory,
      'macos-tauri-dev-runner.sh',
    );
  }
  return {
    command: platform === 'win32' ? 'pnpm.cmd' : 'pnpm',
    args: ['exec', 'tauri', ...(action ? [action] : []), ...cliArgs],
    options: { cwd: projectDirectory, env: environment, stdio: 'inherit' },
  };
}

export function runTauri(args) {
  const invocation = tauriInvocation(args);
  const child = spawn(invocation.command, invocation.args, invocation.options);
  const signals =
    process.platform === 'win32' ? ['SIGINT', 'SIGTERM'] : ['SIGHUP', 'SIGINT', 'SIGTERM'];
  const handlers = new Map();
  let finished = false;
  let forwarded = false;
  function cleanup() {
    for (const [signal, handler] of handlers) process.removeListener(signal, handler);
  }
  for (const signal of signals) {
    const handler = () => {
      if (finished || forwarded) return;
      forwarded = true;
      child.kill(signal);
    };
    handlers.set(signal, handler);
    process.on(signal, handler);
  }
  child.on('error', (error) => {
    if (finished) return;
    finished = true;
    cleanup();
    console.error(error);
    process.exitCode = 1;
  });
  child.on('exit', (code, signal) => {
    if (finished) return;
    finished = true;
    cleanup();
    if (signal) process.kill(process.pid, signal);
    else process.exitCode = code ?? 1;
  });
}
