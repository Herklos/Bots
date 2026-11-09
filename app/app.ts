import type { Answer, Result, Sources } from './app.contract.d.ts';

// Where the hbots gateway (the `hbots` binary) listens. A data module can't read the page's address
// (no `location`, and `fetch` refuses a relative URL), so the web build is made with this placeholder
// and the gateway, which serves it, swaps in the address the page was loaded from. One build then works
// on localhost, Tailscale or a tunnel. Native apps have no page: build them with HBOTS_URL set to the
// gateway, e.g. `HBOTS_URL=https://hbots.example.ts.net bun exact.mjs mac`.
const HBOTS_URL = import.meta.env.HBOTS_URL ?? 'http://hbots-origin.invalid';
const API: string = HBOTS_URL.replace(/\/$/, '');
// Any gateway the person runs: their machine, a Tailscale address or a tunnel. Plain `http:` to a host
// name on Apple also needs `appTransportSecurity.allowsArbitraryLoads` in app.json.
export const grants = 'net.fetch https:\nnet.fetch http:\nsecret.keep session';

// hbots avatar palette (server/avatar-config.json), so a Bot looks the same in both apps.
const COLORS: Record<string, string> = {
  black: '#000000', white: '#f2f2f0', brown: '#936439', red: '#ff263c', orange: '#ff6700', amber: '#ffaf38',
  yellow: '#ff9800', green: '#00c972', teal: '#1cc3b0', cyan: '#00bca6', blue: '#1084fe', purple: '#a97efe',
  violet: '#9159fe', pink: '#ff5eb1', magenta: '#ff309b', gray: '#777777',
};
export const SWATCHES = ['teal', 'blue', 'violet', 'pink', 'orange', 'green'];
const fill = (c: unknown): string => (typeof c === 'string' ? (COLORS[c] ?? (/^#[0-9a-f]{6}$/i.test(c) ? c : COLORS.teal)) : COLORS.teal);
const initials = (name: string): string => name.trim().split(/\s+/).slice(0, 2).map((w) => w.charAt(0).toUpperCase()).join('') || '?';

type Answered = { ok: true; body: any } | { ok: false; error: string };
async function call(store: any, path: string, init: RequestInit = {}): Promise<Answered> {
  const session = store.get('session');
  try {
    const res = await fetch(`${API}${path}`, { ...init, exactTimeout: 15000,
      headers: { 'content-type': 'application/json', ...(session ? { authorization: `Bearer ${session}` } : {}) } });
    const body = await res.json().catch(() => ({}));
    if (res.ok) return { ok: true, body };
    return { ok: false, error: typeof body.error === 'string' ? body.error : 'The gateway had a problem. Try again.' };
  } catch (e) {
    if ((e as { code?: string }).code === 'bake') throw e; // there is no gateway at build time
    console.log(`gateway request failed: ${path}: ${String((e as { code?: string }).code ?? '')} ${String(e)}`);
    return { ok: false, error: 'Can’t reach the gateway. Check that `hbots` is running, then try again.' };
  }
}
// A read that fails fails its resource, so Exact keeps the last answer while it retries.
async function read(store: any, path: string): Promise<any> {
  const r = await call(store, path);
  if (!r.ok) throw new Error(r.error);
  return r.body;
}
const reply = (r: Answered) => (r.ok ? { ok: true, error: '' } : { ok: false, error: r.error });

const sources: Sources = {
  loadSession: async (_args, store): Promise<Result<'loadSession'>> => {
    const r = await call(store, '/api/session');
    if (!r.ok) throw new Error(r.error);
    return { authenticated: r.body.authenticated === true, passwordRequired: r.body.passwordRequired === true };
  },
  signIn: async ([password], store) => {
    const r = await call(store, '/api/login', { method: 'POST', body: JSON.stringify({ password }) });
    if (r.ok && typeof r.body.token === 'string') store.set('session', r.body.token);
    return reply(r);
  },
  signOut: async (_args, store) => { store.set('session', ''); return { ok: true, error: '' }; },
  loadBots: async (_args, store): Promise<Result<'loadBots'>> => {
    const body = await read(store, '/api/bots');
    return (body.bots as any[]).filter((b) => !b.hidden).map((b) => ({
      slug: String(b.slug), name: String(b.name), label: String(b.label ?? ''), initials: initials(String(b.name)),
      color: fill(b.color), main: b.primary === true, ready: typeof b.mainThreadId === 'string',
    }));
  },
  createBot: async ([name, color], store) => reply(await call(store, '/api/bots', { method: 'POST', body: JSON.stringify({ name, color }) })),
  loadChat: async ([slug], store): Promise<Result<'loadChat'>> => {
    if (slug === '') return { slug, rows: [], working: false }; // no chat is open: nothing to read
    const body = await read(store, `/api/bots/${encodeURIComponent(slug)}/chat`);
    const rows = [
      ...(body.messages as any[]).map((m) => ({ id: String(m.id), kind: 'message', mine: m.role === 'user', text: String(m.text) })),
      ...(body.requests as any[]).map((r) => ({ id: String(r.id), kind: 'request', mine: false, text: String(r.title) })),
      ...(body.working === true ? [{ id: 'working', kind: 'working', mine: false, text: '' }] : []),
    ];
    return { slug, rows, working: body.working === true };
  },
  sendMessage: async ([slug, text], store) => reply(await call(store, `/api/bots/${encodeURIComponent(slug)}/chat`, { method: 'POST', body: JSON.stringify({ text }) })),
  stopRun: async ([slug], store) => reply(await call(store, `/api/bots/${encodeURIComponent(slug)}/stop`, { method: 'POST', body: '{}' })),
};
export const answer: Answer = (source, args, store, storage, native) => sources[source](args, store, storage, native);
