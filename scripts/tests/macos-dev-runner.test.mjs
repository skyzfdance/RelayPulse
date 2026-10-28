import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { once } from 'node:events';
import {
  chmod,
  copyFile,
  mkdir,
  mkdtemp,
  readFile,
  readdir,
  rm,
  writeFile,
} from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { test } from 'node:test';
import { setTimeout } from 'node:timers/promises';

async function fixture(t, body, { legacy = false, registrationFails = false } = {}) {
  const root = await mkdtemp(path.join(os.tmpdir(), 'relaypulse-runner-test-'));
  t.after(() => rm(root, { recursive: true, force: true }));
  for (const directory of ['scripts', 'bin', 'backend/icons'])
    await mkdir(path.join(root, directory), { recursive: true });
  for (const file of ['macos-tauri-dev-runner.sh', 'macos-dev-Info.plist']) {
    await copyFile(new URL(`../${file}`, import.meta.url), path.join(root, 'scripts', file));
  }
  await writeFile(path.join(root, 'backend/icons/icon.icns'), 'fixture');
  await writeFile(
    path.join(root, 'bin/codesign'),
    '#!/bin/sh\nprintf "%s\\n" "$@" >> "$TEST_SIGN_LOG"\n',
  );
  await chmod(path.join(root, 'bin/codesign'), 0o755);
  await writeFile(
    path.join(root, 'bin/lsregister'),
    `#!/bin/sh\nprintf "%s\\n" "$@" >> "$TEST_REGISTER_LOG"\nexit ${registrationFails ? 9 : 0}\n`,
  );
  await chmod(path.join(root, 'bin/lsregister'), 0o755);
  const runners = path.join(root, 'backend/target/development.noindex/macos-dev-runners');
  if (legacy) {
    await mkdir(path.join(runners, 'relaypulse.previous/RelayPulse Dev.app'), { recursive: true });
    await mkdir(path.join(root, 'other-project/RelayPulse.app'), { recursive: true });
  }
  const binary = path.join(root, 'fake-app');
  await writeFile(binary, `#!/bin/sh\n${body}\n`);
  await chmod(binary, 0o755);
  const args = [path.join(root, 'scripts/macos-tauri-dev-runner.sh'), binary, 'argument with spaces'];
  const options = {
    env: {
      ...process.env,
      PATH: `${path.join(root, 'bin')}:${process.env.PATH}`,
      TEST_SIGN_LOG: path.join(root, 'sign.log'),
      TEST_REGISTER_LOG: path.join(root, 'register.log'),
      TEST_READY: path.join(root, 'ready'),
    },
    stdio: 'ignore',
  };
  const child = spawn('sh', args, options);
  const exited = once(child, 'exit');
  t.after(() => {
    if (child.exitCode === null && child.signalCode === null) child.kill('SIGTERM');
  });
  return {
    root,
    child,
    exited,
    runners,
    args,
    options,
  };
}

test(
  'runner signs only the development identity, registers a stable app path and forwards arguments',
  { skip: process.platform === 'win32' },
  async (t) => {
    const { root, exited, runners, args, options } = await fixture(
      t,
      'test -s "$TEST_REGISTER_LOG" || exit 1\nprintf "%s" "$1" > "$TEST_READY"\nexit 7',
    );
    assert.deepEqual(await exited, [7, null]);
    assert.equal(await readFile(path.join(root, 'ready'), 'utf8'), 'argument with spaces');
    const signing = await readFile(path.join(root, 'sign.log'), 'utf8');
    assert.match(signing, /com\.relaypulse\.app\.dev/u);
    assert.doesNotMatch(signing, /com\.relaypulse\.app\n/u);
    const bundle = path.join(runners, 'RelayPulse Dev.app');
    assert.deepEqual(await readdir(runners), ['RelayPulse Dev.app']);
    assert.equal(await readFile(path.join(root, 'register.log'), 'utf8'), `-f\n${bundle}\n`);
    const second = spawn('sh', args, options);
    assert.deepEqual(await once(second, 'exit'), [7, null]);
    assert.deepEqual(await readdir(runners), ['RelayPulse Dev.app']);
    assert.equal(
      await readFile(path.join(root, 'register.log'), 'utf8'),
      `-f\n${bundle}\n-f\n${bundle}\n`,
    );
  },
);

test(
  'SIGTERM reaches the application PID directly without leaving a child process',
  { skip: process.platform === 'win32', timeout: 10000 },
  async (t) => {
    const { root, child, exited, runners } = await fixture(
      t,
      'echo $$ > "$TEST_READY"\nexec sleep 30',
    );
    let pid;
    for (let attempt = 0; attempt < 100; attempt++) {
      try {
        pid = Number((await readFile(path.join(root, 'ready'), 'utf8')).trim());
        break;
      } catch {
        await setTimeout(20);
      }
    }
    assert.ok(pid, 'fake application started');
    assert.equal(pid, child.pid, 'runner must exec the app, not wait for a child');
    child.kill('SIGTERM');
    assert.deepEqual(await exited, [null, 'SIGTERM']);
    assert.throws(() => process.kill(pid, 0), { code: 'ESRCH' });
    assert.deepEqual(await readdir(runners), ['RelayPulse Dev.app']);
  },
);

test(
  'runner unregisters only this project’s legacy dev bundles before registering the current path',
  { skip: process.platform === 'win32' },
  async (t) => {
    const { root, exited, runners } = await fixture(t, 'exit 0', { legacy: true });
    assert.deepEqual(await exited, [0, null]);
    assert.equal(
      await readFile(path.join(root, 'register.log'), 'utf8'),
      `-u\n${path.join(runners, 'relaypulse.previous/RelayPulse Dev.app')}\n-f\n${path.join(runners, 'RelayPulse Dev.app')}\n`,
    );
  },
);

test(
  'registration failure prevents launching a bundle with unresolved notification identity',
  { skip: process.platform === 'win32' },
  async (t) => {
    const { root, exited } = await fixture(t, 'echo launched > "$TEST_READY"', {
      registrationFails: true,
    });
    assert.deepEqual(await exited, [9, null]);
    await assert.rejects(readFile(path.join(root, 'ready')), { code: 'ENOENT' });
  },
);
