import { execFileSync } from 'node:child_process';
import { existsSync, readFileSync, readdirSync, realpathSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import path from 'node:path';

const root = fileURLToPath(new URL('..', import.meta.url));
// Product modules are owned by the host. This list contains only public SDK packages.
export const policy = {
  'relaypulse-runtime': [],
  'relaypulse-model-client': ['relaypulse-runtime'],
  'relaypulse-storage': ['relaypulse-runtime'],
  'relaypulse-diagnostics': ['relaypulse-runtime'],
  'relaypulse-operations': ['relaypulse-runtime', 'relaypulse-storage'],
  'relaypulse-notifications': ['relaypulse-storage', 'relaypulse-diagnostics'],
};
const testEdges = { 'relaypulse-diagnostics': ['relaypulse-storage'] };
const businessModules = ['configuration', 'sites', 'monitoring', 'iq_test'];
export function validateGraph(packages) {
  const errors = [];
  const names = new Set(packages.map((pkg) => pkg.name));
  const graph = new Map();
  for (const pkg of packages) {
    graph.set(pkg.name, pkg.dependencies.filter((dep) => dep.kind !== 'dev').map((dep) => dep.name));
    if (pkg.name === 'relaypulse') continue;
    if (!Object.hasOwn(policy, pkg.name)) errors.push(`Unclassified library ${pkg.name}; product modules belong in backend/src/modules`);
    for (const dep of pkg.dependencies) {
      if (dep.name === 'tauri' || dep.name.startsWith('tauri-') || dep.name === 'relaypulse') errors.push(`${pkg.name} depends on desktop host ${dep.name} (${dep.kind ?? 'normal'})`);
      if (dep.path && !Object.hasOwn(policy, dep.name)) errors.push(`Public SDK cannot depend on a local product/helper package: ${pkg.name} -> ${dep.name}`);
      const internal = names.has(dep.name) || dep.name.startsWith('relaypulse-');
      const allowed = policy[pkg.name]?.includes(dep.name) || (dep.kind === 'dev' && testEdges[pkg.name]?.includes(dep.name));
      if (internal && !allowed) errors.push(`Forbidden direction ${pkg.name} -> ${dep.name} (${dep.kind ?? 'normal'})`);
    }
  }
  const active = new Set(); const done = new Set();
  function visit(name) {
    if (active.has(name)) { errors.push(`Dependency cycle through ${name}`); return; }
    if (done.has(name)) return;
    active.add(name);
    for (const dep of graph.get(name) ?? []) if (names.has(dep)) visit(dep);
    active.delete(name); done.add(name);
  }
  for (const name of names) visit(name);
  return errors;
}
function rustFiles(directory) {
  if (!existsSync(directory)) return [];
  return readdirSync(directory, { withFileTypes: true }).flatMap((item) => item.isDirectory() ? rustFiles(path.join(directory, item.name)) : item.name.endsWith('.rs') ? [path.join(directory, item.name)] : []);
}
const within = (file, directory) => file === directory || file.startsWith(directory + path.sep);
// Keep literals for path attributes; mask comments so examples cannot create source edges.
const withoutComments = (source) => source.replace(/r(#+)?"[\s\S]*?"\1|"(?:\\.|[^"\\])*"|\/\*[\s\S]*?\*\/|\/\/[^\n]*/g, (token) => token.startsWith('//') || token.startsWith('/*') ? ' '.repeat(token.length) : token);
const withoutLiterals = (source) => source.replace(/r(#+)?"[\s\S]*?"\1|"(?:\\.|[^"\\])*"/g, (token) => ' '.repeat(token.length));
function inspectSources(pkg, workspaceRoot, errors) {
  const packageRoot = realpathSync(path.dirname(pkg.manifest_path));
  const testRoot = path.join(workspaceRoot, 'backend/tests');
  const visited = new Set();
  function inspect(file, attachedTest = false) {
    if (!existsSync(file)) { errors.push(`Missing attached source: ${file}`); return; }
    file = realpathSync(file);
    if (!within(file, packageRoot) && !(attachedTest && within(file, testRoot))) { errors.push(`Source inclusion crosses library boundary: ${file}`); return; }
    if (visited.has(file)) return;
    visited.add(file);
    const source = withoutComments(readFileSync(file, 'utf8'));
    const code = withoutLiterals(source);
    if (/\b(?:crate|self|super)::(?:modules::)?(?:configuration|sites|monitoring|iq_test)\b|\bmod\s+(?:configuration|sites|monitoring|iq_test)\b|\b(?:configuration|sites|monitoring|iq_test)::|\brelaypulse_(?:sites|settings|monitoring|iq)::/.test(code)) errors.push(`Public SDK imports product business modules: ${file}`);
    if (/\binclude\s*!/.test(code)) errors.push(`Unclassified Rust source inclusion: ${file}`);
    const dataIncludes = [...source.matchAll(/\binclude_(?:str|bytes)\s*!/g)].length;
    const literalIncludes = [...source.matchAll(/\binclude_(?:str|bytes)\s*!\s*\(\s*"([^"]+)"\s*\)/g)];
    if (dataIncludes !== literalIncludes.length) errors.push(`Unclassified included data: ${file}`);
    for (const match of source.matchAll(/\binclude_(?:str|bytes)\s*!\s*\(\s*"([^"]+)"\s*\)/g)) {
      const target = path.resolve(path.dirname(file), match[1]);
      if (!existsSync(target) || !within(realpathSync(target), packageRoot)) errors.push(`Included data crosses SDK boundary: ${file} -> ${target}`);
    }
    if (/#\[cfg_attr\([^\]]*\bpath\s*=/.test(source)) errors.push(`Conditional source path requires an explicit SDK boundary check: ${file}`);
    // Follow test attachments transitively, including any path declared in their fixtures.
    const pathEdges = [...source.matchAll(/#\[path\s*=\s*"([^"]+)"\]\s*(?:pub(?:\([^)]*\))?\s+)?mod\s+\w+\s*;/g)];
    if ([...source.matchAll(/#\[path\s*=/g)].length !== pathEdges.length) errors.push(`Unclassified source path attribute: ${file}`);
    for (const match of pathEdges) {
      const target = path.resolve(path.dirname(file), match[1]);
      const resolved = existsSync(target) ? realpathSync(target) : target;
      const testGate = /#\[cfg\([^\]]*\btest\b[^\]]*\)\]\s*$/.test(source.slice(0, match.index));
      const isTest = within(resolved, testRoot) && (attachedTest || testGate);
      if (!within(resolved, packageRoot) && !isTest) errors.push(`Source inclusion crosses library boundary: ${file} -> ${resolved}`);
      else inspect(resolved, isTest);
    }
    if (attachedTest) {
      const directory = ['mod.rs', 'lib.rs', 'main.rs'].includes(path.basename(file)) ? path.dirname(file) : path.join(path.dirname(file), path.basename(file, '.rs'));
      for (const match of code.matchAll(/\bmod\s+(\w+)\s*;/g)) {
        const candidates = [path.join(directory, match[1] + '.rs'), path.join(directory, match[1], 'mod.rs')];
        const target = candidates.find(existsSync);
        if (target) inspect(target, true);
      }
    }
  }
  for (const file of rustFiles(path.join(packageRoot, 'src'))) inspect(file);
}
export function inspectWorkspace(workspaceRoot = root) {
  workspaceRoot = realpathSync(workspaceRoot);
  const backend = path.join(workspaceRoot, 'backend');
  const metadata = JSON.parse(execFileSync('cargo', ['metadata', '--manifest-path', path.join(backend, 'Cargo.toml'), '--format-version', '1', '--no-deps'], { encoding: 'utf8' }));
  const members = metadata.packages.filter((pkg) => metadata.workspace_members.includes(pkg.id));
  const errors = validateGraph(members);
  for (const pkg of members) {
    for (const dep of pkg.dependencies) if (Object.hasOwn(policy, dep.name) && (!dep.path || dep.req === '*')) errors.push(`SDK dependency requires explicit path and compatible version: ${pkg.name} -> ${dep.name}`);
    if (!Object.hasOwn(policy, pkg.name)) continue;
    const manifest = readFileSync(pkg.manifest_path, 'utf8');
    if (/version\.workspace\s*=\s*true/.test(manifest) || !/^version\s*=\s*"[^"]+"/m.test(manifest)) errors.push(`SDK version must be declared independently: ${pkg.name}`);
    inspectSources(pkg, workspaceRoot, errors);
  }
  if (members.some((pkg) => pkg.name === 'relaypulse')) {
    const moduleRoot = path.join(backend, 'src/modules');
    for (const name of businessModules) if (!existsSync(path.join(moduleRoot, name, 'mod.rs'))) errors.push(`Business implementation missing from host modules: ${name}`);
    const entry = path.join(moduleRoot, 'mod.rs');
    if (existsSync(entry) && /relaypulse_(?:sites|settings|monitoring|iq)::/.test(readFileSync(entry, 'utf8'))) errors.push('Host business modules cannot be forwarding shells');
  }
  return { libraries: members.map((pkg) => pkg.name), errors };
}
if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const result = inspectWorkspace();
  if (result.errors.length) { console.error(result.errors.join('\n')); process.exitCode = 1; }
  else console.log(`Architecture passed: host business modules and six public SDK boundaries; normal/dev/build dependencies and source attachments checked.`);
}
