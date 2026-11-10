import { mkdtempSync, mkdirSync, writeFileSync, rmSync, readFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import test from 'node:test';
import assert from 'node:assert/strict';
import { validateGraph, inspectWorkspace } from '../check-architecture.mjs';
const pkg = (name, deps = []) => ({ name, dependencies: deps.map((name) => ({ name, kind: null })) });

test('rejects business dependencies in public SDK, host dependencies and cycles', () => {
  const failures = validateGraph([pkg('relaypulse-runtime', ['relaypulse-iq', 'tauri']), pkg('relaypulse-iq', ['relaypulse-runtime'])]);
  for (const reason of ['Forbidden direction', 'desktop host', 'cycle', 'Unclassified']) assert.ok(failures.some((error) => error.includes(reason)));
});
test('rejects undeclared public library edges and new unclassified libraries', () => {
  assert.ok(validateGraph([pkg('relaypulse-storage', ['relaypulse-model-client']), pkg('relaypulse-model-client')]).some((error) => error.includes('Forbidden direction')));
  assert.ok(validateGraph([pkg('relaypulse-new')]).some((error) => error.includes('Unclassified')));
});
test('normal, dev and build dependencies all obey SDK directions', () => {
  assert.deepEqual(validateGraph([pkg('relaypulse-storage', ['relaypulse-runtime']), pkg('relaypulse-runtime')]), []);
  for (const kind of ['dev', 'build']) {
    const packages = [pkg('relaypulse-runtime'), pkg('relaypulse-storage')];
    packages[0].dependencies.push({ name: 'relaypulse-storage', kind });
    assert.ok(validateGraph(packages).some((error) => error.includes('Forbidden direction')));
  }
});
function fixture(run, libraries = ['runtime']) {
  const directory = mkdtempSync(path.join(tmpdir(), 'relaypulse-architecture-'));
  const write = (file, text) => { const target = path.join(directory, 'backend', file); mkdirSync(path.dirname(target), { recursive: true }); writeFileSync(target, text); };
  try {
    write('Cargo.toml', '[package]\nname="relaypulse"\nversion="2.0.1"\nedition="2021"\n[workspace]\nmembers=["crates/*"]\n');
    write('src/lib.rs', 'mod modules;');
    write('src/modules/mod.rs', 'mod configuration; mod sites; mod monitoring; mod iq_test;');
    for (const name of ['configuration', 'sites', 'monitoring', 'iq_test']) write(`src/modules/${name}/mod.rs`, 'pub(crate) fn business() {}');
    for (const name of libraries) {
      write(`crates/${name}/Cargo.toml`, `[package]\nname="relaypulse-${name}"\nversion="0.1.0"\nedition="2021"\n`);
      write(`crates/${name}/src/lib.rs`, '');
    }
    run({ directory, write, inspect: () => inspectWorkspace(directory), append: (file, text) => write(file, readFileSync(path.join(directory, 'backend', file), 'utf8') + text) });
  } finally { rmSync(directory, { recursive: true, force: true }); }
}
test('real Cargo manifests reject a reversed production dependency and a re-extracted sites crate', () => fixture(({ append, inspect }) => {
  append('crates/runtime/Cargo.toml', '[dependencies]\nrelaypulse-sites={path="../sites",version="0.1"}\n');
  const errors = inspect().errors;
  assert.ok(errors.some((error) => error.includes('Forbidden direction relaypulse-runtime -> relaypulse-sites')));
  assert.ok(errors.some((error) => error.includes('Unclassified library relaypulse-sites')));
}, ['runtime', 'sites']));
test('real Cargo manifests reject SDK dev and build dependencies on the desktop host', () => {
  for (const section of ['dev-dependencies', 'build-dependencies']) fixture(({ append, inspect }) => {
    append('crates/runtime/Cargo.toml', `[${section}]\nrelaypulse={path="../.."}\n`);
    assert.ok(inspect().errors.some((error) => error.includes('depends on desktop host relaypulse')));
  });
});
test('a business crate is rejected even without a dependency edge', () => fixture(({ inspect }) => {
  assert.ok(inspect().errors.some((error) => error.includes('Unclassified library relaypulse-sites')));
}, ['runtime', 'sites']));
test('host-only version changes keep independently declared SDK versions valid', () => fixture(({ directory, write, inspect }) => {
  assert.deepEqual(inspect().errors, []);
  const manifest = readFileSync(path.join(directory, 'backend/Cargo.toml'), 'utf8');
  write('Cargo.toml', manifest.replace('2.0.1', '99.4.7'));
  assert.deepEqual(inspect().errors, []);
  assert.match(readFileSync(path.join(directory, 'backend/crates/runtime/Cargo.toml'), 'utf8'), /version="0\.1\.0"/);
}));
test('SDK dependency declarations require both a path and compatible version', () => fixture(({ append, inspect }) => {
  append('Cargo.toml', '[dependencies]\nrelaypulse-runtime={path="crates/runtime"}\n');
  assert.ok(inspect().errors.some((error) => error.includes('explicit path and compatible version')));
}));
test('SDK source paths cannot include host business code', () => fixture(({ write, inspect }) => {
  write('crates/runtime/src/lib.rs', '#[path="../../../src/modules/sites/mod.rs"]\nmod business;');
  assert.ok(inspect().errors.some((error) => error.includes('Source inclusion crosses library boundary')));
}));
test('SDK test fixtures cannot transitively include host business code', () => fixture(({ write, inspect }) => {
  write('crates/runtime/src/lib.rs', '#[cfg(test)]\n#[path="../../../tests/support/fixture.rs"]\nmod fixture;');
  write('tests/support/fixture.rs', '#[path="../../src/modules/sites/mod.rs"]\nmod business;');
  assert.ok(inspect().errors.some((error) => error.includes('Source inclusion crosses library boundary')));
}));
test('SDK fixtures cannot import product modules through crate namespaces', () => fixture(({ write, inspect }) => {
  write('crates/runtime/src/lib.rs', '#[cfg(test)]\n#[path="../../../tests/support/fixture.rs"]\nmod fixture;');
  write('tests/support/fixture.rs', 'use crate::modules::sites::SiteService;');
  assert.ok(inspect().errors.some((error) => error.includes('imports product business modules')));
}));
test('SDK include macros cannot bypass source or data boundaries', () => {
  for (const source of ['include!("../../../src/modules/sites/mod.rs");', 'const S: &str = include_str!("../../../src/modules/sites/mod.rs");', 'include!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../../src/modules/sites/mod.rs"));']) fixture(({ write, inspect }) => {
    write('crates/runtime/src/lib.rs', source);
    assert.ok(inspect().errors.some((error) => /inclusion|Included data/.test(error)));
  });
});
test('business modules must have host implementations instead of forwarding shells', () => fixture(({ write, inspect }) => {
  write('src/modules/mod.rs', 'pub use relaypulse_sites::modules::sites;');
  assert.ok(inspect().errors.some((error) => error.includes('forwarding shells')));
}));

test('SDK conditional paths and interposed attributes cannot bypass source checks', () => {
  for (const source of ['#[cfg_attr(test, path="../../../src/modules/sites/mod.rs")]\nmod business;', '#[path="../../../src/modules/sites/mod.rs"]\n#[cfg(test)]\nmod business;']) fixture(({ write, inspect }) => {
    write('crates/runtime/src/lib.rs', source);
    assert.ok(inspect().errors.some((error) => /source path|source path attribute/.test(error)));
  });
});
test('SDK cannot bypass the allowlist with an unclassified local helper dependency', () => {
  const packages = [pkg('relaypulse-runtime')];
  packages[0].dependencies.push({ name: 'product-helper', kind: 'dev', path: '/fixture/business' });
  assert.ok(validateGraph(packages).some((error) => error.includes('local product/helper')));
});
