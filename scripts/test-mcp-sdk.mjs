#!/usr/bin/env bun
// Real SDK and gateway, temporary home, no real harness/account access.
import { mkdtemp, copyFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { resolve, join } from 'node:path';
const root = resolve(import.meta.dir, '..');
const scratch = await mkdtemp(join(tmpdir(), 'hbots-mcp-sdk-'));
const gateways = [];
try {
  const build = Bun.spawn(['cargo', 'build', '-q'], { cwd: root, stdout: 'inherit', stderr: 'inherit' });
  if (await build.exited !== 0) throw new Error('Gateway build failed');
  for (const file of ['package.json', 'bun.lock', 'client.mjs']) await copyFile(join(root, 'scripts/fixtures/mcp-sdk', file), join(scratch, file));
  const install = Bun.spawn([process.execPath, 'install', '--frozen-lockfile', '--ignore-scripts'], { cwd: scratch, stdout: 'inherit', stderr: 'inherit' });
  if (await install.exited !== 0) throw new Error('SDK dependency install failed');
  const password = crypto.randomUUID();
  async function boot(binary = join(root, 'target/debug/hbots')) {
    const reserve = Bun.serve({ hostname: '127.0.0.1', port: 0, fetch: () => new Response() });
    const port = reserve.port;
    reserve.stop(true);
    const gateway = Bun.spawn([binary, 'serve'], {
      cwd: scratch, env: { PATH: process.env.PATH, HBOTS_HOME: join(scratch, 'home'), PORT: String(port), HOST: '127.0.0.1', HBOTS_PASSWORD: password, HBOTS_NETWORK_ACCESS: 'local', T3_URL: 'http://127.0.0.1:9', T3_TOKEN: 'sdk-fixture-only' }, stdout: 'ignore', stderr: 'pipe',
    });
    gateways.push(gateway);
    const origin = `http://127.0.0.1:${port}`;
    const deadline = Date.now() + 15000;
    while (true) {
      try { if ((await fetch(`${origin}/api/health`)).ok) return origin; } catch {}
      if (Date.now() > deadline || gateway.exitCode !== null) throw new Error('Temporary gateway did not start');
      await Bun.sleep(100);
    }
  }
  const origin = await boot();
  const secondOrigin = await boot();
  const oldOrigin = process.env.HBOTS_MCP_COMPAT_BIN ? await boot(resolve(process.env.HBOTS_MCP_COMPAT_BIN)) : '';
  const client = Bun.spawn([process.execPath, join(scratch, 'client.mjs')], { cwd: scratch, env: { ...process.env, MCP_TEST_ORIGIN: origin, MCP_TEST_SECOND_ORIGIN: secondOrigin, MCP_TEST_OLD_ORIGIN: oldOrigin, MCP_TEST_PASSWORD: password }, stdout: 'inherit', stderr: 'inherit' });
  if (await client.exited !== 0) throw new Error('MCP SDK integration failed');
} finally {
  for (const gateway of gateways) gateway.kill();
  await Promise.all(gateways.map(gateway => gateway.exited));
  await rm(scratch, { recursive: true, force: true });
}
