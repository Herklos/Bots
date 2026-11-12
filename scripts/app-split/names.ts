// usage: bun names.ts <appdir> <headerLine> <endLine>  -> member names used by that root-view branch
import { readFileSync, readdirSync, statSync } from 'node:fs';
import { join } from 'node:path';
const [app, h, en] = process.argv.slice(2);
const walk = (d: string): string[] => readdirSync(d).flatMap(x => { if (['node_modules','target','.exact'].includes(x)) return []; const p = join(d, x); return statSync(p).isDirectory() ? walk(p) : p.endsWith('.contract') && !p.endsWith('.test.contract') ? [p] : []; });
const decl = new Set<string>();
for (const f of walk(app).filter(f => f.endsWith("/app.contract") || /^members Bots/m.test(readFileSync(f, "utf8")))) for (const l of readFileSync(f, "utf8").split("\n")) { const m = l.match(/^  (state|derive|resource|mutation|action|task) ([A-Za-z_][A-Za-z0-9_]*)/); if (m) decl.add(m[2]); }
const L = readFileSync(join(app, 'app.contract'), 'utf8').split('\n');
const s = L.findIndex(l => l === h); const e = L.findIndex((l, i) => i > s && l === en);
const body = L.slice(s, e).join('\n');
const used = [...decl].filter(n => new RegExp(`(?<![A-Za-z0-9_.])${n}(?![A-Za-z0-9_])`).test(body));
console.log(used.join(','));
