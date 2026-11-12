// Fold every `  provide` block of app.contract's root component into the first one.
import { readFileSync, writeFileSync } from 'node:fs';
const f = process.argv[2];
const L = readFileSync(f, 'utf8').split('\n');
const blocks: string[] = [];
for (let i = 0; i < L.length; i++) {
  if (L[i] !== '  provide') continue;
  let j = i + 1; while (j < L.length && L[j].startsWith('    ')) { blocks.push(L[j]); j++; }
  L.splice(i, j - i); i--;
}
if (blocks.length === 0) process.exit(0);
const first = L.indexOf('  view');
L.splice(first, 0, '  provide', ...blocks);
writeFileSync(f, L.join('\n'));
console.log('provide entries', blocks.length);
