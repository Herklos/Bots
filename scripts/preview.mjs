#!/usr/bin/env bun
// Look at the app with rich sample data, without a gateway or T3 Code.
//   bun scripts/preview.mjs [--size 1280x860] "tap bot-ada" "clock +600" "screenshot out.png" …
// The ops are the Exact agent's (tree, tap, type, clock, screenshot, prefer …). It starts a sample
// gateway (scripts/preview/sample.mjs), builds the web app against it into app/target/preview,
// and drives it. Nothing here touches real Bots.
import { existsSync, readFileSync } from 'node:fs';
import { resolve, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { sample } from './preview/sample.mjs';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const pathFile = resolve(root, '.exact-path');
const exact = process.env.EXACT_DIR || (existsSync(pathFile)
  ? readFileSync(pathFile, 'utf8').match(/^EXACT_DIR=(.+)$/m)?.[1].trim().replace(/^['"]|['"]$/g, '') : null);
if (!exact) throw new Error('Set EXACT_DIR or configure .exact-path.');
const chrome = '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome';
const args = process.argv.slice(2);
const size = args[0] === '--size' ? args.splice(0, 2)[1] : '1280x860';
const headers = {
  'Access-Control-Allow-Origin': '*', 'Access-Control-Allow-Headers': '*',
  'Access-Control-Allow-Methods': 'GET, POST, PUT, DELETE, OPTIONS',
};
const gateway = Bun.serve({
  hostname: '127.0.0.1', port: 0,
  async fetch(req) {
    if (req.method === 'OPTIONS') return new Response(null, { status: 204, headers });
    const url = new URL(req.url);
    const body = req.method === 'GET' ? null : await req.json().catch(() => ({}));
    const out = sample(req.method, url.pathname, url.searchParams, body);
    if (out === undefined) return new Response('Unknown sample endpoint', { status: 404, headers });
    return Response.json(out, { headers });
  },
});
const env = {
  ...process.env,
  ...(process.env.CHROME ? {} : existsSync(chrome) ? { CHROME: chrome } : {}),
  HBOTS_URL: `http://127.0.0.1:${gateway.port}`,
  EXACT_APP_DIR: resolve(root, 'app'),
  EXACT_WEB_DIST: resolve(root, 'app/target/preview'),
};
try {
  const build = Bun.spawn([process.execPath, resolve(exact, 'host/web/build.mjs'), 'bots-web'], { cwd: root, env, stdout: 'ignore', stderr: 'inherit' });
  if ((await build.exited) !== 0) { process.exitCode = 1; throw new Error('build failed'); }
  const run = Bun.spawn([process.execPath, resolve(exact, 'scripts/agent.mjs'), 'web', '--app', 'bots', '--size', size, '--epoch', 'now', ...args], { cwd: root, env, stdout: 'inherit', stderr: 'inherit' });
  process.exitCode = await run.exited;
} finally {
  gateway.stop(true);
}
