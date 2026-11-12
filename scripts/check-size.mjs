#!/usr/bin/env bun
// Fail when an app contract or TypeScript file is longer than 300 lines.
// scripts/size-baseline.json lists files that are still too long, with their current
// length. Each may not grow, and it must leave the list once it is 300 lines or fewer.
//   bun scripts/check-size.mjs
import { readdirSync, readFileSync, existsSync } from 'node:fs';
import { resolve, relative, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const LIMIT = 300;
const SKIP = new Set(['node_modules', 'target', '.exact']);
const baselineFile = resolve(root, 'scripts/size-baseline.json');
const baseline = existsSync(baselineFile) ? JSON.parse(readFileSync(baselineFile, 'utf8')) : {};

const files = [];
(function walk(dir) {
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    if (SKIP.has(entry.name)) continue;
    const path = resolve(dir, entry.name);
    if (entry.isDirectory()) walk(path);
    else if (/\.(contract|ts)$/.test(entry.name)) files.push(path);
  }
})(resolve(root, 'app'));

let bad = false;
const seen = new Set();
for (const file of files) {
  const text = readFileSync(file, 'utf8');
  if (text.slice(0, 300).includes('@generated')) continue;
  const rel = relative(root, file);
  seen.add(rel);
  const lines = text.split('\n').length - (text.endsWith('\n') ? 1 : 0);
  const limit = baseline[rel] ?? LIMIT;
  if (lines > limit) {
    console.error(`size: ${rel} has ${lines} lines (limit ${limit})`);
    bad = true;
  } else if (rel in baseline && lines <= LIMIT) {
    console.error(`size: ${rel} is ${lines} lines; remove it from scripts/size-baseline.json`);
    bad = true;
  }
}
for (const rel of Object.keys(baseline)) {
  if (!seen.has(rel)) {
    console.error(`size: ${rel} is in the baseline but no longer exists or is generated; remove it`);
    bad = true;
  }
}
process.exit(bad ? 1 : 0);
