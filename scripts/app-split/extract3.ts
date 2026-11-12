// Extract one route branch of the root view into a child component. Types come from a probe loop:
// the compiler reports one provided name's real type per build, so the probe fixes one and rebuilds.
// usage: bun extract3.ts <appdir> <branchHeader> <nextHeader> <Component> <relFile> <names,...> [props]
import { readFileSync, writeFileSync, mkdirSync, readdirSync, statSync, unlinkSync } from 'node:fs';
import { join, dirname, relative } from 'node:path';
import { spawnSync } from 'node:child_process';

const [app, headerArg, endArg, comp, rel, namesArg, propsArg = ''] = process.argv.slice(2);
const CLI = '/Users/user/Documents/dev/Contrib/exact/target/debug/contract';
const REUSE = join(dirname(new URL(import.meta.url).pathname), 'reuse.ts');
const names = namesArg.split(',').filter(Boolean);
const props = propsArg ? propsArg.split(',') : [];
const root = join(app, 'app.contract');
const original = readFileSync(root, 'utf8');
const L = original.split('\n');
const s = L.findIndex(l => l === headerArg);
const e = L.findIndex((l, i) => i > s && l === endArg);
if (s < 0 || e < 0) throw new Error(`branch not found: ${s} ${e}`);

const walk = (d: string): string[] => readdirSync(d).flatMap(x => {
  if (['node_modules', 'target', '.exact'].includes(x)) return [];
  const p = join(d, x);
  return statSync(p).isDirectory() ? walk(p) : p.endsWith('.contract') && !p.endsWith('.test.contract') ? [p] : [];
});
const actions = new Set<string>();
for (const f of walk(app).filter(f => f.endsWith('/app.contract') || /^members Bots/m.test(readFileSync(f, 'utf8')))) {
  for (const l of readFileSync(f, 'utf8').split('\n')) {
    const m = l.match(/^  action ([A-Za-z_][A-Za-z0-9_]*)/);
    if (m) actions.add(m[1]);
  }
}

let text = L.slice(s + 1, e).map(l => (l.startsWith('        ') ? l.slice(8) : l.trimStart())).join('\n').replace(/\be\.id\b/g, 'route');
if (/\be\.params\b/.test(text)) console.error('warning: branch reads e.params; pass it as a prop by hand');
const flagDerives: string[] = [];
const flagNames: string[] = [];
for (const m of new Set([...text.matchAll(/\b(pending|failed)\((\w+)\)/g)].map(m => `${m[1]}|${m[2]}`))) {
  const [kind, x] = m.split('|');
  const name = `${x}${kind === 'pending' ? 'Pending' : 'Failed'}`;
  text = text.replace(new RegExp(`\\b${kind}\\(${x}\\)`, 'g'), name);
  flagDerives.push(`  derive ${name} = ${kind}(${x})`);
  flagNames.push(name);
}
const allNames = [...names, ...flagNames];
const isAction = (n: string) => actions.has(n);

// root edits: add derives before view, provide names, and an optional import / call
function editRoot(base: string[], provides: string[], derives: string[], extra: { use?: string; mainCall?: string; branch?: string[] }): string[] {
  const out = [...base];
  if (extra.branch) {
    const i = out.indexOf(extra.branch[0]);
    out.splice(i, extra.branch.length, ...extra.branch.slice(0, 1), extra.branch[1] ?? '');
  }
  const p = out.indexOf('  provide');
  if (p >= 0) {
    let k = p + 1;
    while (k < out.length && out[k].startsWith('    ')) k++;
    const have = new Set(out.slice(p + 1, k).map(l => l.trim()));
    out.splice(k, 0, ...provides.filter(n => !have.has(n)).map(n => '    ' + n));
  } else {
    out.splice(out.indexOf('  view'), 0, '  provide', ...provides.map(n => '    ' + n));
  }
  const v = out.indexOf('  view');
  if (derives.length) out.splice(v, 0, ...derives);
  if (extra.mainCall) out.splice(out.findIndex(l => l.startsWith('    main id=')) + 1, 0, extra.mainCall);
  if (extra.use) out.unshift(extra.use);
  return out;
}

function build(): string {
  spawnSync('bun', [REUSE, app], { stdio: 'ignore' });
  const r = spawnSync(CLI, ['build', root, '--errors-only'], { encoding: 'utf8' });
  return (r.stdout ?? '') + (r.stderr ?? '');
}

// (1) probe loop: one component with an empty view injects every name; fix the reported type and rebuild
const probeFile = join(app, '_probe.contract');
const probeTypes: Record<string, string> = {};
for (const n of allNames) probeTypes[n] = isAction(n) ? 'action' : 'bool';
let rounds = 0;
for (; rounds < 400; rounds++) {
  writeFileSync(probeFile, ['component TypeProbe', '  inject', ...allNames.map(n => `    ${n}: ${probeTypes[n]}`), '  view', '    text "x"', ''].join('\n'));
  writeFileSync(root, editRoot(L, allNames, flagDerives, { use: 'use TypeProbe from "./_probe.contract"', mainCall: '      TypeProbe()' }).join('\n'));
  const out = build();
  const m = out.match(/the provided `([A-Za-z_][A-Za-z0-9_]*)` is `([^`]+)`/);
  if (!m) break;
  probeTypes[m[1]] = m[2];
}
unlinkSync(probeFile);
writeFileSync(root, original);
console.error(`probe: ${rounds} rounds`);

// (2) the real component
const file = join(app, rel);
mkdirSync(dirname(file), { recursive: true });
const propLines = props.length ? ['  props', ...props.map(p => `    ${p}: number`)] : [];
const componentText = [
  `// ${comp}: one route of the root view, moved out with its values and actions injected.`,
  `component ${comp}`,
  ...propLines,
  '  inject',
  ...allNames.map(n => `    ${n}: ${probeTypes[n]}`),
  '  view',
  ...text.split('\n'),
  '',
].join('\n');
writeFileSync(file, componentText.replace(/\n{3,}/g, '\n\n'));

// (3) the root: the branch becomes a call
const relPath = './' + relative(app, file);
const rootNew = [...L.slice(0, s + 1), `            ${comp}(${props.length ? 'route=e.id' : ''})`, ...L.slice(e)];
writeFileSync(root, editRoot(rootNew, allNames, flagDerives, { use: `use ${comp} from "${relPath}"` }).join('\n'));
const out = build();
const errs = out.split('\n').filter(l => /\[(type|syntax|contract)-/.test(l));
console.log(errs.length ? errs.slice(0, 12).join('\n') : `ok: ${comp} at ${relPath} (${allNames.length} injects)`);
