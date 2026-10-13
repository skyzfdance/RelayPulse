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

async function fixture(t, body) {
  const root = await mkdtemp(path.join(os.tmpdir(), 'relaypulse-runner-test-'));
  t.after(() => rm(root, { recursive: true, force: true }));
  for (const directory of ['scripts', 'bin', 'src-tauri/icons'])
    await mkdir(path.join(root, directory), { recursive: true });
  for (const file of ['macos-tauri-dev-runner.sh', 'macos-dev-Info.plist']) {
    await copyFile(new URL(`../${file}`, import.meta.url), path.join(root, 'scripts', file));
  }
  await writeFile(path.join(root, 'src-tauri/icons/icon.icns'), 'fixture');
  await writeFile(
    path.join(root, 'bin/codesign'),
    '#!/bin/sh\nprintf "%s\\n" "$@" >> "$TEST_SIGN_LOG"\n',
  );
  await chmod(path.join(root, 'bin/codesign'), 0o755);
  const binary = path.join(root, 'fake-app');
  await writeFile(binary, `#!/bin/sh\n${body}\n`);
  await chmod(binary, 0o755);
  const child = spawn(
    'sh',
    [path.join(root, 'scripts/macos-tauri-dev-runner.sh'), binary, 'argument with spaces'],
    {
      env: {
        ...process.env,
        PATH: `${path.join(root, 'bin')}:${process.env.PATH}`,
        TEST_SIGN_LOG: path.join(root, 'sign.log'),
        TEST_READY: path.join(root, 'ready'),
      },
      stdio: 'ignore',
    },
  );
  const exited = once(child, 'exit');
  t.after(() => {
    if (child.exitCode === null && child.signalCode === null) child.kill('SIGTERM');
  });
  return {
    root,
    child,
    exited,
    runners: path.join(root, 'src-tauri/target/development.noindex/macos-dev-runners'),
  };
}

test(
  'runner signs only the development identity, forwards arguments, and removes the temporary app',
  { skip: process.platform === 'win32' },
  async (t) => {
    const { root, exited, runners } = await fixture(t, 'printf "%s" "$1" > "$TEST_READY"\nexit 7');
    assert.deepEqual(await exited, [7, null]);
    assert.equal(await readFile(path.join(root, 'ready'), 'utf8'), 'argument with spaces');
    const signing = await readFile(path.join(root, 'sign.log'), 'utf8');
    assert.match(signing, /com\.relaypulse\.app\.dev/u);
    assert.doesNotMatch(signing, /com\.relaypulse\.app\n/u);
    assert.deepEqual(await readdir(runners), []);
  },
);

test(
  'SIGTERM stops the app and cleans its temporary bundle',
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
    child.kill('SIGTERM');
    assert.deepEqual(await exited, [143, null]);
    assert.throws(() => process.kill(pid, 0), { code: 'ESRCH' });
    assert.deepEqual(await readdir(runners), []);
  },
);
