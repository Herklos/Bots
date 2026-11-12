#!/usr/bin/env bun
// Exercise management MCP settings against an isolated in-memory sample gateway
// (scripts/preview/sample.mjs, in memory, nothing real). A separate build keeps its sample origin out of the normal web app.
import { existsSync, readFileSync } from 'node:fs';
import { resolve, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const pathFile = resolve(root, '.exact-path');
const exact = process.env.EXACT_DIR || (existsSync(pathFile)
  ? readFileSync(pathFile, 'utf8').match(/^EXACT_DIR=(.+)$/m)?.[1].trim().replace(/^['"]|['"]$/g, '') : null);
if (!exact) throw new Error('Set EXACT_DIR or configure .exact-path before running the management MCP tests.');
const chrome = '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome';
import { sample } from './preview/sample.mjs';
const headers = {
  'Access-Control-Allow-Origin': '*',
  'Access-Control-Allow-Headers': '*',
  'Access-Control-Allow-Methods': 'GET, POST, PUT, DELETE, OPTIONS',
};
let enabled = false;
let failStatus = false;
let credentials = [];
const gateway = Bun.serve({
  hostname: '127.0.0.1', port: 0,
  async fetch(req) {
    if (req.method === 'OPTIONS') return new Response(null, { status: 204, headers });
    const url = new URL(req.url);
    const body = req.method === 'GET' ? null : await req.json().catch(() => ({}));
    if (failStatus && url.pathname === '/api/management-mcp/credentials') return Response.json({ error: 'Fixture unavailable' }, { status: 503, headers });
    let out;
    if (url.pathname === '/api/settings') {
      if (body?.managementMcp) enabled = body.managementMcp.enabled;
      out = { ...(sample(req.method, url.pathname, url.searchParams, body) ?? {}), managementMcp: { enabled } };
    } else if (url.pathname === '/api/management-mcp/credentials') {
      if (req.method === 'POST') { credentials.push({ id: 'fixture', label: body.label, scopes: body.scopes }); out = { id: 'fixture', token: 'fixture-token-only' }; }
      else out = { credentials, endpoint: 'http://127.0.0.1:4321/mcp' };
    } else if (url.pathname === '/api/management-mcp/credentials/fixture' && req.method === 'DELETE') {
      credentials = []; out = { ok: true };
    } else if (req.method === 'GET' && ['/api/access', '/api/storage'].includes(url.pathname)) out = {};
    else out = sample(req.method, url.pathname, url.searchParams, body);
    if (out === undefined) return new Response('Unknown sample endpoint', { status: 404, headers });
    return Response.json(out, { headers });
  },
});
const env = {
  ...process.env,
  ...(process.env.CHROME ? {} : existsSync(chrome) ? { CHROME: chrome } : {}),
  HBOTS_URL: `http://127.0.0.1:${gateway.port}`,
  EXACT_APP_DIR: resolve(root, 'app'),
  EXACT_WEB_DIST: resolve(root, 'app/target/management-mcp-test'),
};
try {
  for (const args of [
    [resolve(exact, 'host/web/build.mjs'), 'bots-web'],
    [resolve(exact, 'scripts/agent.mjs'), 'web', '--app', 'bots', '--test', resolve(root, 'scripts/fixtures/management-mcp.test.contract')],
    [resolve(exact, 'scripts/agent.mjs'), 'web', '--app', 'bots', '--test', resolve(root, 'scripts/fixtures/management-mcp-error.test.contract')],
  ]) {
    if (args.some(arg => arg.endsWith('management-mcp-error.test.contract'))) failStatus = true;
    const child = Bun.spawn([process.execPath, ...args], { cwd: root, env, stdout: 'inherit', stderr: 'inherit' });
    const code = await child.exited;
    if (code !== 0) { process.exitCode = code; break; }
  }
} finally {
  gateway.stop(true);
}
