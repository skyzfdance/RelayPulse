import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import path from 'node:path';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';

import { tauriInvocation } from '../tauri-environment.mjs';

const root = fileURLToPath(new URL('../../', import.meta.url));
const readJson = (name) => JSON.parse(readFileSync(path.join(root, name), 'utf8'));

test('dev and release-optimized development builds cannot inherit production config or output paths', () => {
  for (const args of [['dev'], ['dev', '--release'], ['build'], ['build', '--debug'], ['bundle']]) {
    const invocation = tauriInvocation(args, {
      platform: 'darwin',
      arch: 'arm64',
      env: { TAURI_CONFIG: '{"identifier":"com.relaypulse.app"}', CARGO_TARGET_DIR: '/production' },
    });
    assert.equal(invocation.options.env.TAURI_CONFIG, undefined);
    assert.equal(
      invocation.options.env.CARGO_TARGET_DIR,
      path.join(root, 'src-tauri/target/development.noindex'),
    );
    assert.deepEqual(invocation.args, ['exec', 'tauri', ...args]);
    if (args[0] === 'dev') {
      assert.equal(
        invocation.options.env.CARGO_TARGET_AARCH64_APPLE_DARWIN_RUNNER,
        path.join(root, 'scripts/macos-tauri-dev-runner.sh'),
      );
    }
  }
});

test('production packaging explicitly selects the production identity and a separate output directory', () => {
  const invocation = tauriInvocation(['build', '--production', '--bundles', 'app']);
  assert.deepEqual(invocation.args, [
    'exec',
    'tauri',
    'build',
    '--bundles',
    'app',
    '--config',
    path.join(root, 'src-tauri/tauri.production.conf.json'),
  ]);
  assert.equal(
    invocation.options.env.CARGO_TARGET_DIR,
    path.join(root, 'src-tauri/target/production'),
  );
  assert.throws(() => tauriInvocation(['dev', '--production']), /only supported/u);
  for (const args of [
    ['dev', '--config', 'prod.json'],
    ['build', '--config=prod.json'],
    ['dev', '-cprod.json'],
  ]) {
    assert.throws(() => tauriInvocation(args), /not supported/u);
  }
});

test('the default config separates data, single-instance and autostart identities from the installed app', () => {
  const dev = readJson('src-tauri/tauri.conf.json');
  const production = readJson('src-tauri/tauri.production.conf.json');
  assert.equal(dev.identifier, 'com.relaypulse.app.dev');
  assert.equal(dev.productName, 'RelayPulse Dev');
  assert.equal(production.identifier, 'com.relaypulse.app');
  assert.equal(production.productName, 'RelayPulse');
  assert.notEqual(dev.app.windows[0].title, production.app.windows[0].title);
  assert.notEqual(dev.build.frontendDist, production.build.frontendDist);
  assert.notEqual(dev.build.beforeBuildCommand, production.build.beforeBuildCommand);
  const plist = readFileSync(path.join(root, 'scripts/macos-dev-Info.plist'), 'utf8');
  assert.ok(plist.includes(`<string>${dev.identifier}</string>`));
  assert.ok(plist.includes(`<string>${dev.productName}</string>`));
  const localization = readFileSync(
    path.join(root, 'src-tauri/infoplist/zh-Hans.lproj/InfoPlist.strings'),
    'utf8',
  );
  assert.doesNotMatch(localization, /CFBundle(?:Display)?Name\s*=/u);
});

test('Windows and Linux wrappers retain the same isolated defaults', () => {
  for (const platform of ['win32', 'linux']) {
    const invocation = tauriInvocation(['dev'], { platform, env: {} });
    assert.equal(invocation.command, platform === 'win32' ? 'pnpm.cmd' : 'pnpm');
    assert.equal(
      invocation.options.env.CARGO_TARGET_DIR,
      path.join(root, 'src-tauri/target/development.noindex'),
    );
  }
});
