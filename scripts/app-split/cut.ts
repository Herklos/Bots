// Cut a file into one file per component, each starting at its leading comment block.
// usage: bun cut.ts <src> <out1> <out2> ...  (one out per component, first gets lines before the first component's comments)
import { readFileSync, writeFileSync } from 'node:fs';
const [src, ...outs] = process.argv.slice(2);
const L = readFileSync(src, 'utf8').split('\n');
const starts: number[] = [];
L.forEach((l, i) => { if (l.startsWith('component ')) { let s = i; while (s > 0 && L[s - 1].startsWith('//')) s--; starts.push(s); } });
if (starts.length !== outs.length - 0) { /* header attaches to first */ }
const header = L.slice(0, starts[0]);
const ends = [...starts.slice(1), L.length];
outs.forEach((o, k) => {
  const body = L.slice(starts[k], ends[k]);
  writeFileSync(o, (k === 0 ? header.join('\n') + '\n' : '') + body.join('\n').replace(/\n+$/, '') + '\n');
  console.log(o, body.length);
});
