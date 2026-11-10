import { readFileSync, readdirSync, writeFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import path from 'node:path';

const root = fileURLToPath(new URL('..', import.meta.url));
const snapshot = path.join(root, 'scripts/sdk-api.snapshot.json');
const libraries = ['runtime', 'storage', 'model-client', 'diagnostics', 'notifications', 'operations'];
function files(directory) {
  return readdirSync(directory, { withFileTypes: true }).flatMap((entry) => entry.isDirectory()
    ? files(path.join(directory, entry.name))
    : /\.(rs|sql)$/.test(entry.name) ? [path.join(directory, entry.name)] : []);
}
// Mask comments and literals while preserving byte positions for balanced declaration bodies.
function mask(source) {
  return source.replace(/\/\*[\s\S]*?\*\/|\/\/[^\n]*|r(#+)?"[\s\S]*?"\1|"(?:\\.|[^"\\])*"|'(?:\\.|[^'\\])'/g,
    (value) => ' '.repeat(value.length));
}
// Conservative source guard, not a replacement for external consumer compilation.
// Walk item scopes, retaining module visibility, impl owners/bounds and trait implementations.
// Function bodies are skipped, including default trait methods and trait implementations.
export function fingerprintSdkSource(source) {
  const masked = mask(source);
  const publicItems = [];
  const normalized = (start, end) => source.slice(start, end).replace(/\s+/g, ' ').trim();
  function closing(start, limit) {
    let depth = 1;
    let end = start + 1;
    while (end < limit && depth) {
      if (masked[end] === '{') depth++;
      else if (masked[end] === '}') depth--;
      end++;
    }
    if (depth) throw new Error('Unbalanced Rust item; refusing to record an incomplete API.');
    return end;
  }
  function walk(start, limit, scope, traitMembers = false) {
    let cursor = start;
    while (cursor < limit) {
      while (/\s|;/.test(masked[cursor] ?? '') && cursor < limit) cursor++;
      if (cursor >= limit) break;
      const begin = cursor;
      // Attributes may contain punctuation that otherwise terminates an item header.
      while (masked.slice(cursor, cursor + 2) === '#[') {
        let depth = 1;
        cursor += 2;
        while (cursor < limit && depth) {
          if (masked[cursor] === '[') depth++;
          else if (masked[cursor] === ']') depth--;
          cursor++;
        }
        while (/\s/.test(masked[cursor] ?? '') && cursor < limit) cursor++;
      }
      const declaration = cursor;
      let parens = 0, brackets = 0, angles = 0;
      while (cursor < limit) {
        const char = masked[cursor];
        if (!parens && !brackets && !angles && (char === '{' || char === ';')) break;
        if (char === '<') angles++;
        if (char === '>' && masked[cursor - 1] !== '-' && angles) angles--;
        if (char === '(') parens++;
        if (char === ')') parens--;
        if (char === '[') brackets++;
        if (char === ']') brackets--;
        cursor++;
      }
      const header = masked.slice(declaration, cursor).trim();
      const match = header.match(/^(pub(?:\([^)]*\))?\s+)?(?:(?:async|const|unsafe|default|auto|extern)\s+)*(fn|struct|enum|trait|type|use|mod|const|static|impl)\b/);
      const kind = match?.[2];
      const visible = !!match?.[1];
      const block = masked[cursor] === '{';
      let end = block ? closing(cursor, limit) : Math.min(cursor + 1, limit);
      if (kind === 'use' && block) {
        while (end < limit && masked[end] !== ';') end++;
        end++;
      }
      const signature = normalized(begin, block ? cursor : end);
      const context = scope.join(' > ');
      const emit = (value) => publicItems.push(`${context}${context ? ' > ' : ''}${value}`);
      if (kind === 'mod') {
        // Private modules matter too: pub use and #[path] determine reachable public paths.
        emit(signature);
        if (block) walk(cursor + 1, end - 1, [...scope, signature]);
      } else if (kind === 'impl') {
        const traitImpl = /\bfor\b/.test(header);
        if (traitImpl) emit(signature);
        if (block) walk(cursor + 1, end - 1, [...scope, signature], traitImpl);
      } else if (kind === 'trait' && visible) {
        emit(signature);
        if (block) walk(cursor + 1, end - 1, [...scope, signature], true);
      } else if (visible || traitMembers) {
        if (kind === 'fn') emit(signature);
        else if (kind) emit(normalized(begin, end));
      }
      cursor = end;
    }
  }
  walk(0, masked.length, []);
  const wire = [...source.matchAll(/#\[serde\([\s\S]*?\)\]/g)].map((m) => m[0].replace(/\s+/g, ' '));
  return { publicItems, wire };
}
export function currentSdkBaseline() {
  const current = { packageVersions: Object.fromEntries(libraries.map((library) => {
    const manifest = readFileSync(path.join(root, 'backend/crates', library, 'Cargo.toml'), 'utf8');
    return [library, manifest.match(/^version\s*=\s*"([^"]+)"/m)[1]];
  })) };
  for (const library of libraries) for (const file of files(path.join(root, 'backend/crates', library, 'src')).sort()) {
    const source = readFileSync(file, 'utf8');
    const key = path.relative(root, file);
    if (file.endsWith('.sql')) current[key] = source;
    else {
      const signature = fingerprintSdkSource(source);
      if (signature.publicItems.length || signature.wire.length) current[key] = signature;
    }
  }
  return current;
}
if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const current = currentSdkBaseline();
  if (process.argv.includes('--update')) {
    writeFileSync(snapshot, JSON.stringify(current, null, 2) + '\n');
    console.log('SDK declarations, enum variants, wire attributes and shared SQL baseline recorded.');
  } else if (JSON.stringify(JSON.parse(readFileSync(snapshot, 'utf8'))) !== JSON.stringify(current)) {
    console.error('Public SDK declarations or shared SQL changed. Review compatibility and update the baseline explicitly after external consumer tests pass.');
    process.exitCode = 1;
  } else console.log('Public SDK declaration, wire and shared SQL baseline matched.');
}
