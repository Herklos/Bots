#!/usr/bin/env bun
// Local dev stack.
//   bun scripts/dev.mjs
//   bun scripts/dev.mjs --lan  (set HBOTS_DEV_HOST to choose a LAN address)
//
// Gateway (debug, restarts after a Rust change): http://127.0.0.1:3780
// Frontend (Exact dev loop, rebuilds and reloads the page): http://127.0.0.1:8765
//
// The page calls the gateway at HBOTS_URL. An hbots process already bound to
// 3780 is stopped once the new debug binary is ready. T3 Code on 3773 is left
// alone. Ctrl-C stops both.

import { spawn, execFileSync } from 'node:child_process';
import { watch, existsSync, readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { networkInterfaces } from 'node:os';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const gatewayPort = 3780;
const webPort = 8765;
const lan = process.argv.slice(2).includes('--lan');
const unknown = process.argv.slice(2).filter((arg) => arg !== '--lan');
if (unknown.length) {
  console.error(`Usage: bun scripts/dev.mjs [--lan]`);
  process.exit(1);
}
const lanAddress = process.env.HBOTS_DEV_HOST || Object.values(networkInterfaces()).flat().find((a) => a?.family === 'IPv4' && !a.internal)?.address;
if (lan && !lanAddress) {
  console.error('No LAN IPv4 address found; set HBOTS_DEV_HOST to this machine’s reachable address');
  process.exit(1);
}
const gatewayUrl = `http://${lan ? lanAddress : '127.0.0.1'}:${gatewayPort}`;
const pathFile = resolve(root, '.exact-path');
const exact = process.env.EXACT_DIR || (existsSync(pathFile)
  ? readFileSync(pathFile, 'utf8').match(/^EXACT_DIR=(.+)$/m)?.[1].trim().replace(/^['"]|['"]$/g, '')
  : '');
if (!exact || !existsSync(resolve(exact, 'host/web/dev.mjs'))) {
  console.error('scripts/dev.mjs: set EXACT_DIR or .exact-path to the Exact checkout');
  process.exit(1);
}

const children = new Set();
let gateway = null;
let web = null;
let building = false;
let again = false;
let timer = null;
let stopping = false;

function prefix(child, name) {
  let buf = '';
  const on = (chunk) => {
    buf += chunk.toString();
    const lines = buf.split('\n');
    buf = lines.pop() ?? '';
    for (const line of lines) if (line.trim()) console.log(`[${name}] ${line}`);
  };
  child.stdout?.on('data', on);
  child.stderr?.on('data', on);
  child.on('exit', () => { if (buf.trim()) console.log(`[${name}] ${buf.trim()}`); });
}

function listeners(port) {
  try {
    return execFileSync('lsof', ['-nP', `-iTCP:${port}`, '-sTCP:LISTEN', '-t'], { encoding: 'utf8' })
      .split('\n').map((s) => s.trim()).filter(Boolean);
  } catch {
    return [];
  }
}

function command(pid) {
  try { return execFileSync('ps', ['-p', String(pid), '-o', 'command='], { encoding: 'utf8' }).trim(); }
  catch { return ''; }
}

function waitDead(pid, ms = 4000) {
  const start = Date.now();
  return new Promise((ok) => {
    const tick = () => {
      try { process.kill(pid, 0); }
      catch { return ok(); }
      if (Date.now() - start > ms) {
        try { process.kill(pid, 'SIGKILL'); } catch { /* already gone */ }
        return ok();
      }
      setTimeout(tick, 100);
    };
    tick();
  });
}

function spawnGroup(cmd, args, env) {
  const child = spawn(cmd, args, { cwd: root, env, stdio: ['ignore', 'pipe', 'pipe'] });
  children.add(child);
  child.on('exit', () => children.delete(child));
  return child;
}

async function stopGroup(child) {
  if (!child || child.exitCode !== null) return;
  try { child.kill('SIGTERM'); } catch { /* gone */ }
  await waitDead(child.pid);
}

async function freeGatewayPort() {
  for (const pid of listeners(gatewayPort)) {
    if (gateway && Number(pid) === gateway.pid) continue;
    const cmd = command(pid);
    if (!cmd.includes('hbots')) throw new Error(`port ${gatewayPort} is in use (${cmd || pid})`);
    console.log(`[gateway] stopping ${pid} ${cmd}`);
    try { process.kill(Number(pid), 'SIGTERM'); } catch { /* gone */ }
    await waitDead(Number(pid));
  }
}

function cargoBuild() {
  return new Promise((ok) => {
    const child = spawnGroup('cargo', ['build', '--bin', 'hbots'], process.env);
    prefix(child, 'cargo');
    child.on('exit', (code) => ok(code ?? 1));
  });
}

function startGateway() {
  const bin = resolve(root, 'target/debug/hbots');
  gateway = spawnGroup(bin, ['serve'], lan ? { ...process.env, HOST: '0.0.0.0', HBOTS_DEV_ALLOW_ANY_ORIGIN: '1' } : process.env);
  prefix(gateway, 'gateway');
  const started = gateway;
  started.on('exit', (code, signal) => {
    if (!stopping && gateway === started) console.log(`[gateway] exited ${signal || code}`);
  });
  console.log(`[gateway] ${gatewayUrl}`);
}

async function cycle() {
  if (stopping) return;
  if (building) { again = true; return; }
  building = true;
  try {
    do {
      again = false;
      const code = await cargoBuild();
      if (stopping) return;
      if (code !== 0) {
        console.error('[cargo] build failed; the gateway process was left as it is');
        break;
      }
      if (again) continue;
      await freeGatewayPort();
      await stopGroup(gateway);
      gateway = null;
      startGateway();
    } while (again && !stopping);
  } catch (err) {
    console.error(`[gateway] ${err.message}`);
  } finally {
    building = false;
    if (again && !stopping) cycle();
  }
}

function schedule() {
  clearTimeout(timer);
  timer = setTimeout(cycle, 400);
}

function arm(rel, recursive) {
  const path = resolve(root, rel);
  if (!existsSync(path)) return;
  watch(path, recursive ? { recursive: true } : {}, schedule);
}

const webEnv = {
  ...process.env,
  EXACT_APP_DIR: resolve(root, 'app'),
  EXACT_WEB_DIST: resolve(root, 'app/target/web-dev'),
  HBOTS_URL: gatewayUrl,
  PATH: `${process.env.HOME}/.t3/bin:${process.env.PATH ?? ''}`,
};
const taken = listeners(webPort);
if (taken.length) {
  console.error(`port ${webPort} is in use (${command(taken[0]) || taken[0]})`);
  process.exit(1);
}
web = spawnGroup(process.execPath, [resolve(exact, 'host/web/dev.mjs'), '--app', resolve(root, 'app'), '--port', String(webPort), ...(lan ? ['--lan'] : [])], webEnv);
prefix(web, 'web');
web.on('exit', (code, signal) => {
  if (stopping) return;
  console.error(`[web] dev loop exited ${signal || code}`);
  shutdown();
});

arm('gateway/src', true);
arm('design/src', true);
arm('gateway/Cargo.toml', false);
arm('design/Cargo.toml', false);
arm('Cargo.toml', false);

console.log(`[web] http://${lan ? lanAddress : '127.0.0.1'}:${webPort}`);
if (lan) console.warn('[dev] LAN mode allows requests from any browser origin; use only on a trusted network.');
cycle();

async function shutdown() {
  if (stopping) return;
  stopping = true;
  clearTimeout(timer);
  await stopGroup(web);
  await stopGroup(gateway);
  for (const child of children) await stopGroup(child);
  process.exit(0);
}
process.on('SIGINT', shutdown);
process.on('SIGTERM', shutdown);
