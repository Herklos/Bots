#!/usr/bin/env bun
// Parse and type check the Exact contract files. Exits 1 on any error.
//   bun scripts/check-contract.mjs
// Uses EXACT_DIR or .exact-path. Builds the `contract` CLI there when it is missing.

import { spawnSync } from 'node:child_process';
import { existsSync, readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const pathFile = resolve(root, '.exact-path');
const exact = process.env.EXACT_DIR || (existsSync(pathFile)
  ? readFileSync(pathFile, 'utf8').match(/^EXACT_DIR=(.+)$/m)?.[1].trim().replace(/^['"]|['"]$/g, '')
  : '');
if (!exact || !existsSync(resolve(exact, 'Cargo.toml'))) {
  console.error('check-contract: set EXACT_DIR or .exact-path to the Exact checkout');
  process.exit(1);
}

const find = () => ['debug', 'release'].map((p) => resolve(exact, 'target', p, 'contract')).find(existsSync);
let bin = find();
if (!bin) {
  console.log('check-contract: building the contract CLI');
  const built = spawnSync('cargo', ['build', '-p', 'contract', '--bin', 'contract', '--manifest-path', resolve(exact, 'Cargo.toml')], { stdio: 'inherit' });
  if (built.status !== 0) process.exit(built.status ?? 1);
  bin = find();
}

const run = spawnSync(bin, ['build', 'app/app.contract', '--errors-only'], { cwd: root, encoding: 'utf8' });
// A clean build prints a size summary on stdout; only failures are worth showing.
if (run.status !== 0) process.stderr.write(run.stdout + run.stderr);
process.exit(run.status ?? 1);
