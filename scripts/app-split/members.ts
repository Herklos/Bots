// Move named root members of `component Bots` into features/<f>/model.contract.
// usage: bun members.ts <appdir> <feature> <name,name,...>
// Each member is its declaration line, its body (more-indented lines), and the comment
// block directly above it. Moved text is copied unchanged. A `members "..."` line is
// added before `  view`, and the members file is written with `members Bots`.
import { readFileSync, writeFileSync, mkdirSync, existsSync } from 'node:fs';
import { join } from 'node:path';
const [app, feature, namesArg] = process.argv.slice(2);
const want = new Set(namesArg.split(',').filter(Boolean));
const file = join(app, 'app.contract');
const L = readFileSync(file, 'utf8').split('\n');
const start = L.findIndex(l => l.startsWith('component Bots'));
const view = L.findIndex((l, i) => i > start && l === '  view');
if (start < 0 || view < 0) throw new Error('no Bots/view');
const declRe = /^  (state|derive|resource|mutation|action|task|sequence) ([A-Za-z_][A-Za-z0-9_]*)/;
type Item = { name: string; from: number; to: number }; // [from, to) line indexes incl. comments
const items: Item[] = [];
for (let i = start + 1; i < view; i++) {
  const m = L[i].match(declRe);
  if (!m) continue;
  let from = i;
  while (from - 1 > start && L[from - 1].startsWith('  //')) from--;
  let to = i + 1;
  while (to < view && (L[to].startsWith('   ') || (L[to].trim() === '' && to + 1 < view && L[to + 1].startsWith('   ')))) to++;
  items.push({ name: m[2], from, to });
}
// nested `task` / `action` items may be followed by blank lines; trim trailing blanks
const picked = items.filter(it => want.has(it.name));
const missing = [...want].filter(n => !picked.some(p => p.name === n));
if (missing.length) { console.error('not found: ' + missing.join(', ')); process.exit(1); }
const dir = join(app, feature);
mkdirSync(dir, { recursive: true });
const out = join(dir, 'model.contract');
const body: string[] = ['members Bots'];
for (const it of picked) {
  const lines = L.slice(it.from, it.to);
  while (lines.length && lines[lines.length - 1].trim() === '') lines.pop();
  body.push(...lines);
  body.push('');
}
writeFileSync(out, body.join('\n').replace(/\n+$/, '') + '\n');
// remove picked lines (from highest index down), keep the rest
const drop = new Set<number>();
for (const it of picked) for (let k = it.from; k < it.from + (L.slice(it.from, it.to).filter(() => true).length); k++) drop.add(k);
const keep: string[] = [];
for (let i = 0; i < L.length; i++) {
  if (drop.has(i)) continue;
  if (i === view) keep.push(`  members "./${feature}/model.contract"`);
  keep.push(L[i]);
}
writeFileSync(file, keep.join('\n'));
console.log(`moved ${picked.length} members to ${feature}/model.contract`);
