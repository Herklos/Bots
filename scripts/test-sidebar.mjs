#!/usr/bin/env bun
// Exercise sidebar lifetime and keyboard navigation against the sample gateway
// (scripts/preview/sample.mjs, in memory, nothing real). A separate build keeps its sample origin out of the normal web app.
import { existsSync, readFileSync } from 'node:fs';
import { resolve, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const pathFile = resolve(root, '.exact-path');
const exact = process.env.EXACT_DIR || (existsSync(pathFile)
  ? readFileSync(pathFile, 'utf8').match(/^EXACT_DIR=(.+)$/m)?.[1].trim().replace(/^['"]|['"]$/g, '') : null);
if (!exact) throw new Error('Set EXACT_DIR or configure .exact-path before running the sidebar tests.');
const chrome = '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome';
import { sample } from './preview/sample.mjs';
const headers = {
  'Access-Control-Allow-Origin': '*',
  'Access-Control-Allow-Headers': '*',
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
  EXACT_WEB_DIST: resolve(root, 'app/target/sidebar-test'),
};
try {
  for (const args of [
    [resolve(exact, 'host/web/build.mjs'), 'bots-web'],
    [resolve(exact, 'scripts/agent.mjs'), 'web', '--app', 'bots', '--test', resolve(root, 'scripts/fixtures/sidebar.test.contract')],
  ]) {
    const child = Bun.spawn([process.execPath, ...args], { cwd: root, env, stdout: 'inherit', stderr: 'inherit' });
    const code = await child.exited;
    if (code !== 0) { process.exitCode = code; break; }
  }
} finally {
  gateway.stop(true);
}
