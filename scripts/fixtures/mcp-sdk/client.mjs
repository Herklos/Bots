import assert from 'node:assert/strict';
import { Client } from '@modelcontextprotocol/sdk/client/index.js';
import { StreamableHTTPClientTransport } from '@modelcontextprotocol/sdk/client/streamableHttp.js';
const origin = process.env.MCP_TEST_ORIGIN;
const password = process.env.MCP_TEST_PASSWORD;
const login = await fetch(`${origin}/api/login`, { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify({ password }) });
assert.equal(login.status, 200);
const { token: session } = await login.json();
async function api(path, method = 'GET', body) {
  const response = await fetch(`${origin}${path}`, { method, headers: { authorization: `Bearer ${session}`, 'content-type': 'application/json' }, body: body === undefined ? undefined : JSON.stringify(body) });
  assert.equal(response.ok, true, `API ${method} ${path}: ${response.status}`);
  return response.json();
}
await api('/api/settings', 'PUT', { managementMcp: { enabled: true } });
if (process.env.MCP_TEST_OLD_ORIGIN) {
  const oldOrigin = process.env.MCP_TEST_OLD_ORIGIN;
  const oldHeaders = { authorization: `Bearer ${session}`, 'content-type': 'application/json' };
  const unsupported = await fetch(`${oldOrigin}/api/management-mcp/credentials`, { headers: oldHeaders });
  assert.equal(unsupported.status, 404, 'Baseline binary must predate management MCP');
  const oldWrite = await fetch(`${oldOrigin}/api/settings`, { method: 'PUT', headers: oldHeaders, body: JSON.stringify({ userName: 'Legacy writer fixture' }) });
  assert.equal(oldWrite.status, 200);
  assert.equal((await oldWrite.json()).managementMcp.enabled, true);
  assert.equal((await api('/api/settings')).managementMcp.enabled, true);
  console.log('PASS: pre-MCP binary accepts shared sessions and preserves the management flag when saving settings');
}
async function connect(scopes, endpointOrigin = origin) {
  assert.equal((await api('/api/management-mcp/credentials')).endpoint, `${origin}/mcp`);
  const issued = await api('/api/management-mcp/credentials', 'POST', { label: 'SDK interoperability fixture', scopes });
  const client = new Client({ name: 'hbots-integration-test', version: '1.0.0' });
  await client.connect(new StreamableHTTPClientTransport(new URL(`${endpointOrigin}/mcp`), { requestInit: { headers: { authorization: `Bearer ${issued.token}` } } }));
  return { client, id: issued.id };
}
const { client } = await connect(['admin']);
let reader;
async function call(name, args = {}) {
  const result = await client.callTool({ name, arguments: args });
  assert.notEqual(result.isError, true, `Tool ${name} failed: ${JSON.stringify(result.content)}`);
  assert.equal(typeof result.structuredContent?.status, 'number');
  return result.structuredContent.body;
}
try {
  const catalog = await client.listTools();
  assert.equal(catalog.tools.length, 112);
  assert(catalog.tools.find(t => t.name === 'hbots_create_bot').inputSchema.properties.body.properties.name);
  await client.ping();
  const ada = await call('hbots_create_bot', { body: { name: 'SDK Ada' } });
  const bob = await call('hbots_create_bot', { body: { name: 'SDK Bob' } });
  const carol = await call('hbots_create_bot', { body: { name: 'SDK Carol' } });
  const renamed = await call('hbots_save_bot', { path: { slug: ada.slug }, body: { name: 'Ada Lovelace' } });
  assert.equal(renamed.name, 'Ada Lovelace');
  const group = await call('hbots_rooms_create', { body: { title: 'SDK group', members: [ada.slug, bob.slug] } });
  const updatedGroup = await call('hbots_room_update', { path: { id: group.id }, body: { title: 'SDK research', members: [ada.slug, carol.slug] } });
  assert(updatedGroup.members.some(member => member.slug === carol.slug));
  assert(!updatedGroup.members.some(member => member.slug === bob.slug));
  const groups = await call('hbots_rooms_list');
  assert(groups.rooms.some(g => g.id === group.id && g.title === 'SDK research'));
  const attachment = await call('hbots_create', { path: { slug: ada.slug }, body: { name: 'sdk.txt', mediaType: 'text/plain', data: Buffer.from('SDK round trip').toString('base64') } });
  const file = await call('hbots_download', { path: { slug: ada.slug, id: attachment.id } });
  assert.equal(Buffer.from(file.bodyBase64, 'base64').toString(), 'SDK round trip');
  const jsonFile = await call('hbots_create', { path: { slug: ada.slug }, body: { name: 'sdk.json', mediaType: 'application/json', data: Buffer.from('{"fixture":true}').toString('base64') } });
  const jsonDownload = await call('hbots_download', { path: { slug: ada.slug, id: jsonFile.id } });
  assert.equal(Buffer.from(jsonDownload.bodyBase64, 'base64').toString(), '{"fixture":true}');
  assert.equal(typeof jsonDownload.contentDisposition, 'string');
  const vault = await call('hbots_vault_create' , { body: { type: 'login', title: 'SDK login', website: 'https://fixture.invalid', username: 'fixture', password: 'fixture-password' } });
  const revealed = await call('hbots_vault_get', { path: { id: vault.id }, query: { reveal: '1' } });
  assert.equal(revealed.password, 'fixture-password');
  const csv = await call('hbots_export');
  const imported = await call('hbots_import_csv', { body: Buffer.from(csv.bodyBase64, 'base64').toString() });
  assert.equal(typeof imported.skipped, 'number');
  const memory = await call('hbots_memory_get', { query: { scope: 'shared' } });
  await call('hbots_memory_put', { query: { scope: 'shared' }, body: { text: 'SDK shared memory', revision: memory.revision } });
  reader = await connect(['read'], process.env.MCP_TEST_SECOND_ORIGIN ?? origin);
  const readCatalog = await reader.client.listTools();
  assert(readCatalog.tools.some(t => t.name === 'hbots_list_bots'));
  assert(!readCatalog.tools.some(t => t.name === 'hbots_create_bot'));
  const denied = await reader.client.callTool({ name: 'hbots_create_bot', arguments: { body: { name: 'Forbidden' } } });
  assert.equal(denied.isError, true);
  await Promise.allSettled([reader.client.listTools(), api(`/api/management-mcp/credentials/${reader.id}`, 'DELETE')]);
  await assert.rejects(() => reader.client.listTools());
  await call('hbots_room_delete', { path: { id: group.id } });
  await call('hbots_delete_bot', { path: { slug: bob.slug } });
  await call('hbots_delete_bot', { path: { slug: carol.slug } });
  await call('hbots_delete_bot', { path: { slug: ada.slug } });
  await call('hbots_put_settings', { body: { managementMcp: { enabled: false } } });
  await assert.rejects(() => client.listTools());
  await api('/api/settings', 'PUT', { managementMcp: { enabled: true } });
  assert.equal((await client.listTools()).tools.length, 112);
  console.log('PASS: official MCP SDK initialization, discovery, Bot/group CRUD, binary files, Vault/CSV, memory, permissions, cross-process revocation and disablement');
} finally {
  await reader?.client.close();
  await client.close();
}
