// Regenerate `use` lines for every app contract file from a map of declarations.
// usage: bun reuse.ts <appdir> <retired files, comma separated>
import { readFileSync, writeFileSync, readdirSync, statSync } from 'node:fs';
import { relative, dirname, join } from 'node:path';
const [app, retiredArg = ''] = process.argv.slice(2);
const retired = new Set(retiredArg.split(',').filter(Boolean));
const files: string[] = [];
(function walk(d: string) {
  for (const e of readdirSync(d)) {
    if (['node_modules', 'target', '.exact'].includes(e)) continue;
    const p = join(d, e);
    if (statSync(p).isDirectory()) walk(p);
    else if (p.endsWith('.contract') && !p.endsWith('.test.contract') && !p.endsWith('app.contract.d.ts')) files.push(p);
  }
})(app);
const rel = (p: string) => relative(app, p);
const declRe = /^(style|shape|fn|keyframes|component) ([A-Za-z_][A-Za-z0-9_]*)/;
const owner: Record<string, string> = {};
const decls: Record<string, Set<string>> = {};
const texts: Record<string, string> = {};
for (const f of files) {
  texts[f] = readFileSync(f, 'utf8');
  decls[f] = new Set();
  for (const line of texts[f].split('\n')) {
    const m = line.match(declRe);
    if (m) { decls[f].add(m[2]); if (m[2] !== "Bots") owner[m[2]] = f; }
  }
}
for (const f of files) {
  const lines = texts[f].split('\n').filter(l => {
    const m = l.match(/^use .* from "([^"]+)"$/);
    if (!m) return true;
    return !retired.has(rel(join(dirname(f), m[1])));
  });
  let text = lines.join('\n');
  const needed: Record<string, string[]> = {};
  for (const [name, owned] of Object.entries(owner)) {
    if (decls[f].has(name) || owned === f) continue;
    const body = text.split('\n').filter(l => !l.startsWith('use ') && !/^\s*\/\//.test(l)).join('\n');
    if (!new RegExp(`(?<![A-Za-z0-9_.])${name}(?![A-Za-z0-9_])`).test(body)) continue;
    let p = relative(dirname(f), owned);
    if (!p.startsWith('.')) p = './' + p;
    (needed[p] ??= []).push(name);
  }
  // keep existing use lines that are still valid, and add the missing ones
  const existing = text.split('\n').filter(l => l.startsWith('use '));
  const others = text.split('\n').filter(l => !l.startsWith('use '));
  const useLines = [...existing];
  for (const [p, names] of Object.entries(needed)) {
    const have = new Set(existing.filter(l => l.endsWith(`from "${p}"`)).flatMap(l => l.slice(4, l.indexOf(' from ')).split(',').map(s => s.trim().split(' as ')[0])));
    const add = names.filter(n => !have.has(n));
    if (add.length) useLines.push(`use ${add.join(', ')} from "${p}"`);
  }
  // drop an existing use line whose names are now declared in this file
  const cleaned = useLines.filter(l => {
    const names = l.slice(4, l.indexOf(' from ')).split(',').map(s => s.trim());
    return !names.every(n => decls[f].has(n));
  });
  const outFile = [...cleaned, ...others];
  // keep the file's leading use block contiguous with the old first line's position
  const firstOther = others.findIndex(l => l.trim() !== '' || true);
  const newText = outFile.join('\n').replace(/\n{3,}/g, '\n\n');
  writeFileSync(f, newText);
}
console.log('ok', files.length);
// external (package or built-in) names declared by the root's use lines are carried into any file that names them
{
  const root = join(app, 'app.contract');
  const ext: Record<string, string> = {};
  for (const l of readFileSync(root, 'utf8').split('\n')) {
    const m = l.match(/^use (.+) from "([^"]+)"$/);
    if (m && !m[2].startsWith('.')) for (const n of m[1].split(',').map(s => s.trim())) ext[n] = m[2];
  }
  for (const f of files) {
    let text = readFileSync(f, 'utf8');
    const body = text.split('\n').filter(l => !l.startsWith('use ') && !/^\s*\/\//.test(l)).join('\n');
    const bylib: Record<string, string[]> = {};
    for (const [n, p] of Object.entries(ext)) {
      if (!new RegExp(`(?<![A-Za-z0-9_.])${n}(?![A-Za-z0-9_])`).test(body)) continue;
      if (new RegExp(`^use [^\\n]*\\b${n}\\b`, 'm').test(text)) continue;
      (bylib[p] ??= []).push(n);
    }
    const add = Object.entries(bylib).map(([p, ns]) => `use ${ns.join(', ')} from "${p}"`);
    if (add.length) writeFileSync(f, [...add, text].join('\n'));
  }
}
