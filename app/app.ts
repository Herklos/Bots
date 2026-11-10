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
export const grants = 'net.fetch https:\nnet.fetch http:\nsecret.keep session\nfs.read app:/tmp';

// The avatar palette: the names a Bot may carry, and the app tokens they map to. Its values
// come from design/tokens.json: change them there.
// design:begin palette
const BRAND: Record<string, string> = {
  black: 'brand.black', '#000000': 'brand.black', 'brand.black': 'brand.black',
  white: 'brand.white', '#f2f2f0': 'brand.white', 'brand.white': 'brand.white',
  brown: 'brand.brown', '#936439': 'brand.brown', 'brand.brown': 'brand.brown',
  red: 'brand.red', '#ff263c': 'brand.red', 'brand.red': 'brand.red',
  orange: 'brand.orange', '#ff6700': 'brand.orange', 'brand.orange': 'brand.orange',
  amber: 'brand.amber', '#ffaf38': 'brand.amber', 'brand.amber': 'brand.amber',
  yellow: 'brand.yellow', '#ff9800': 'brand.yellow', 'brand.yellow': 'brand.yellow',
  green: 'brand.green', '#00c972': 'brand.green', 'brand.green': 'brand.green',
  teal: 'brand.default', '#1cc3b0': 'brand.default', 'brand.default': 'brand.default',
  cyan: 'brand.cyan', '#00bca6': 'brand.cyan', 'brand.cyan': 'brand.cyan',
  blue: 'brand.blue', '#1084fe': 'brand.blue', 'brand.blue': 'brand.blue',
  purple: 'brand.purple', '#a97efe': 'brand.purple', 'brand.purple': 'brand.purple',
  violet: 'brand.violet', '#9159fe': 'brand.violet', 'brand.violet': 'brand.violet',
  pink: 'brand.pink', '#ff5eb1': 'brand.pink', 'brand.pink': 'brand.pink',
  magenta: 'brand.magenta', '#ff309b': 'brand.magenta', 'brand.magenta': 'brand.magenta',
  gray: 'brand.gray', '#777777': 'brand.gray', 'brand.gray': 'brand.gray',
};
export const SWATCHES = ['brand.brown', 'brand.red', 'brand.orange', 'brand.amber', 'brand.green', 'brand.default', 'brand.blue', 'brand.violet', 'brand.pink', 'brand.gray'];
const DEFAULT_COLOUR = 'brand.default';
const fill = (c: unknown): string => (typeof c === 'string' ? (BRAND[c] ?? (/^#[0-9a-f]{6}$/i.test(c) ? c : DEFAULT_COLOUR)) : DEFAULT_COLOUR);
// design:end palette
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

// The screen says what failed. A stopped gateway, an old hbots, a signed-out
// session, and a quiet T3 are not the same problem.
const NOT_IN_VIEW = 'This message is not in view.';

function tokenText(tokens: unknown): string {
  if (!tokens || typeof tokens !== 'object') return '';
  const n = (key: string): number => {
    const value = (tokens as Record<string, unknown>)[key];
    return typeof value === 'number' ? value : 0;
  };
  return `${n('inputTokens')} in, ${n('outputTokens')} out, ${n('cachedInputTokens')} read from cache, ${n('cacheCreationTokens')} written to cache.`;
}

function marksOf(reactions: unknown, id: string): string {
  if (!reactions || typeof reactions !== 'object') return '';
  const list = (reactions as Record<string, unknown>)[id];
  if (!Array.isArray(list)) return '';
  return list.map((mark) => String(mark)).filter(Boolean).join('');
}

function quoteOf(replyTo: unknown, id: string): string {
  if (!replyTo || typeof replyTo !== 'object') return '';
  const entry = (replyTo as Record<string, unknown>)[id];
  if (!entry || typeof entry !== 'object') return '';
  return String((entry as { text?: unknown }).text ?? '');
}

function blankChat(slug: string) {
  return { slug, rows: [] as ChatRow[], working: false, partial: false, readOnly: false, pins: [] as PinRow[], threadId: '' };
}

type ChatRow = {
  id: string; kind: string; mine: boolean; text: string; files: string;
  head: boolean; tail: boolean; marks: string; quote: string; tokens: string;
};
type PinRow = { id: string; text: string; missing: boolean };

function withRuns(raw: Omit<ChatRow, 'head' | 'tail'>[]): ChatRow[] {
  return raw.map((row, i) => {
    const prev = raw[i - 1];
    const next = raw[i + 1];
    const samePrev = !!prev && prev.kind === 'message' && row.kind === 'message' && prev.mine === row.mine;
    const sameNext = !!next && next.kind === 'message' && row.kind === 'message' && next.mine === row.mine;
    return { ...row, head: !samePrev, tail: !sameNext };
  });
}

function explain(error: string, what: string): string {
  const e = error.toLowerCase();
  if (e.includes('reach the gateway')) return `Can’t load ${what}. Check that hbots is running, then try again.`;
  if (e.includes('login')) return 'Sign in from Settings, then try again.';
  if (e.includes('no conversation') || e.includes('getting set up')) return 'This Bot is still getting set up.';
  if (e.includes('t3')) return 'T3 Code didn’t answer. Open T3 Code, then try again.';
  if (e.includes('had a problem')) return 'Restart hbots so this screen can load, then try again.';
  return error || `Can’t load ${what}. Check that hbots is running, then try again.`;
}

function libraryEntry(slug: string, e: any) {
  const id = String(e.id ?? '');
  const prefix = `file:${slug}/`;
  const path = id.startsWith(prefix) ? id.slice(prefix.length) : '';
  const detail = String(e.detail ?? '');
  const open = path !== ''
    ? `${API}/api/bots/${encodeURIComponent(slug)}/library/file?path=${encodeURIComponent(path)}`
    : (detail.startsWith('http://') || detail.startsWith('https://') ? detail : '');
  return {
    id,
    kind: String(e.kind ?? ''),
    title: String(e.title ?? ''),
    detail,
    group: String(e.group ?? ''),
    status: String(e.status ?? ''),
    body: String(e.body ?? ''),
    path,
    open,
  };
}

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
  loadBots: async ([focus], store): Promise<Result<'loadBots'>> => {
    // Reserved slug for the contract runner, which has no gateway. A real inbox still fetches.
    if (focus === 'fixture') {
      return [
        { slug: 'fixture', name: 'Ada', label: '', initials: 'A', color: fill('brand.default'), main: false, hidden: false, ready: true, unread: true },
        { slug: 'bea', name: 'Bea', label: '', initials: 'B', color: fill('brand.blue'), main: false, hidden: true, ready: false, unread: true },
      ];
    }
    const body = await read(store, '/api/bots');
    const listed = body.bots as any[];
    return Promise.all(listed.map(async (b) => {
      const thread = typeof b.mainThreadId === 'string' ? b.mainThreadId : '';
      let unread = b.unread === true;
      if (thread !== '') {
        try {
          const meta = await read(store, `/api/threads/${encodeURIComponent(thread)}/meta`);
          const latest = String(meta.latest ?? '');
          const stored = String(meta.lastRead ?? '');
          unread = latest !== '' && stored !== latest;
        } catch {
          // The list's own flag stands when this thread has no meta yet.
        }
      }
      return {
        slug: String(b.slug), name: String(b.name), label: String(b.label ?? ''), initials: initials(String(b.name)),
        color: fill(b.color), main: b.primary === true, hidden: b.hidden === true, ready: thread !== '', unread,
      };
    }));
  },
  renameBot: async ([slug, name], store) => reply(await call(store, `/api/bots/${encodeURIComponent(slug)}`, { method: 'PUT', body: JSON.stringify({ name }) })),
  recolourBot: async ([slug, color], store) => reply(await call(store, `/api/bots/${encodeURIComponent(slug)}`, { method: 'PUT', body: JSON.stringify({ color }) })),
  hideBot: async ([slug, hidden], store) => reply(await call(store, `/api/bots/${encodeURIComponent(slug)}`, { method: 'PUT', body: JSON.stringify({ hidden }) })),
  makeMainBot: async ([slug], store) => reply(await call(store, '/api/main-bot', { method: 'PUT', body: JSON.stringify({ slug }) })),
  deleteBot: async ([slug], store) => reply(await call(store, `/api/bots/${encodeURIComponent(slug)}`, { method: 'DELETE' })),
  loadPrefs: async (_args, store): Promise<Result<'loadPrefs'>> => {
    const body = await read(store, '/api/settings');
    return { userName: String(body.userName ?? ''), theme: String(body.theme ?? 'system'), projects: body.beta?.projects === true };
  },
  setUserName: async ([userName], store) => reply(await call(store, '/api/settings', { method: 'PUT', body: JSON.stringify({ userName }) })),
  setTheme: async ([theme], store) => reply(await call(store, '/api/settings', { method: 'PUT', body: JSON.stringify({ theme }) })),
  setProjects: async ([on], store) => reply(await call(store, '/api/settings', { method: 'PUT', body: JSON.stringify({ beta: { projects: on } }) })),
  setPassword: async ([password], store) => reply(await call(store, '/api/access/password', { method: 'PUT', body: JSON.stringify({ password }) })),
  loadVault: async (_args, store): Promise<Result<'loadVault'>> => {
    const body = await read(store, '/api/vault');
    return (body.items as any[]).filter((i) => i.type === 'login').map((i) => ({
      id: String(i.id), title: String(i.title ?? ''), website: String(i.website ?? ''), username: String(i.username ?? ''),
      hasPassword: i.hasPassword === true, hints: String(i.hints ?? ''),
    }));
  },
  revealPassword: async ([id], store): Promise<Result<'revealPassword'>> => {
    const r = await call(store, `/api/vault/${encodeURIComponent(id)}?reveal=1`);
    if (!r.ok) return { ok: false, id, password: '', error: r.error };
    return { ok: true, id, password: String(r.body.password ?? ''), error: '' };
  },
  saveLogin: async ([id, title, website, username, password, hints], store) => {
    // A blank password keeps the saved one; the gateway drops an empty password on update.
    const fields = { type: 'login', title, website, username, hints, ...(password !== '' ? { password } : {}) };
    const r = id === ''
      ? await call(store, '/api/vault', { method: 'POST', body: JSON.stringify(fields) })
      : await call(store, `/api/vault/${encodeURIComponent(id)}`, { method: 'PUT', body: JSON.stringify(fields) });
    return reply(r);
  },
  deleteLogin: async ([id], store) => reply(await call(store, `/api/vault/${encodeURIComponent(id)}`, { method: 'DELETE' })),
  loadProjects: async (_args, store): Promise<Result<'loadProjects'>> => {
    const body = await read(store, '/api/projects');
    // The gateway answers an empty board with beta false while the Projects beta is off; the page shows that as its off state.
    if (body.beta === false) throw new Error('Projects beta is off');
    return (body.items as any[]).map((p) => ({
      id: String(p.id), kind: String(p.kind), title: String(p.title ?? ''), status: String(p.status ?? ''),
      area: String(p.area ?? ''), body: String(p.body ?? ''),
    }));
  },
  saveProject: async ([id, kind, title, status, area, body], store) => {
    const item = { ...(id !== '' ? { id } : { kind }), title, status, area, body };
    return reply(await call(store, '/api/projects/items', { method: 'POST', body: JSON.stringify({ items: [item] }) }));
  },
  deleteProject: async ([id], store) => reply(await call(store, '/api/projects/delete', { method: 'POST', body: JSON.stringify({ ids: [id] }) })),
  createBot: async ([name, color], store) => reply(await call(store, '/api/bots', { method: 'POST', body: JSON.stringify({ name, color }) })),
  loadChat: async ([slug, child], store): Promise<Result<'loadChat'>> => {
    if (slug === '') return blankChat('');
    if (slug === 'fixture' && child === 'child-thread') {
      return {
        slug, working: false, partial: false, readOnly: true, threadId: 'child-thread', pins: [],
        rows: withRuns([{ id: 'c1', kind: 'message', mine: false, text: 'Notes', files: '', marks: '', quote: '', tokens: '' }]),
      };
    }
    if (slug === 'fixture') {
      return {
        slug, working: false, partial: true, readOnly: false, threadId: '',
        pins: [
          { id: 'm1', text: 'Hello', missing: false },
          { id: 'missing', text: NOT_IN_VIEW, missing: true },
        ],
        rows: withRuns([
          { id: 'm1', kind: 'message', mine: true, text: 'Hello', files: '', marks: '', quote: '', tokens: '' },
          { id: 'm2', kind: 'message', mine: false, text: 'Hi there', files: '', marks: '🔥', quote: 'Hello', tokens: '12 in, 3 out, 4 read from cache, 1 written to cache.' },
          { id: 'm3', kind: 'message', mine: false, text: 'No tokens here', files: '', marks: '', quote: '', tokens: '' },
          { id: 'voice1', kind: 'message', mine: true, text: '', files: 'voice.m4a', marks: '', quote: '', tokens: '' },
        ]),
      };
    }
    const query = child !== '' ? `?child=${encodeURIComponent(child)}` : '';
    const body = await read(store, `/api/bots/${encodeURIComponent(slug)}/chat${query}`);
    const threadId = String(body.threadId ?? '');
    let meta: any = {};
    if (threadId !== '') {
      try {
        meta = await read(store, `/api/threads/${encodeURIComponent(threadId)}/meta`);
      } catch {
        meta = {};
      }
    }
    const fileLabel = (m: any): string => {
      const list = Array.isArray(m.attachments) ? m.attachments : [];
      const names = list.map((a: any) => String(a?.name ?? a?.fileName ?? '').trim()).filter(Boolean);
      return names.length === 0 ? '' : names.join(', ');
    };
    const raw = [
      ...(body.messages as any[]).map((m) => ({
        id: String(m.id), kind: 'message', mine: m.role === 'user', text: String(m.text ?? ''), files: fileLabel(m),
        marks: marksOf(meta.reactions, String(m.id)), quote: quoteOf(meta.replyTo, String(m.id)), tokens: tokenText(m.tokens),
      })),
      ...(body.requests as any[]).map((r) => ({ id: String(r.id), kind: 'request', mine: false, text: String(r.title), files: '', marks: '', quote: '', tokens: '' })),
      ...(body.working === true ? [{ id: 'working', kind: 'working', mine: false, text: '', files: '', marks: '', quote: '', tokens: '' }] : []),
    ];
    const rows = withRuns(raw);
    const pins: PinRow[] = Array.isArray(meta.pins)
      ? meta.pins.map((id: unknown) => {
        const key = String(id);
        const hit = rows.find((row) => row.id === key);
        return hit ? { id: key, text: hit.text, missing: false } : { id: key, text: NOT_IN_VIEW, missing: true };
      })
      : [];
    if (threadId !== '' && body.readOnly !== true) {
      const latest = [...rows].reverse().find((row) => row.kind === 'message')?.id ?? '';
      if (latest !== '') {
        await call(store, `/api/threads/${encodeURIComponent(threadId)}/read`, {
          method: 'POST', body: JSON.stringify({ latest, open: true, hidden: false }),
        });
      }
    }
    return {
      slug, rows, working: body.working === true, partial: body.partial === true,
      readOnly: body.readOnly === true, pins, threadId,
    };
  },
  sendMessage: async ([slug, text, pending], store) => {
    const attachmentIds = String(pending ?? '')
      .split('\n')
      .map((line) => line.split('\t')[0]?.trim())
      .filter((id) => !!id);
    return reply(await call(store, `/api/bots/${encodeURIComponent(slug)}/chat`, {
      method: 'POST',
      body: JSON.stringify({ text, attachmentIds }),
    }));
  },
  uploadAttachment: async ([slug, path, name, mediaType], store, storage): Promise<Result<'uploadAttachment'>> => {
    try {
      const bytes = new Uint8Array(await storage.fs.readFile(String(path)));
      if (bytes.byteLength === 0) return { ok: false, id: '', name: '', error: 'That file is empty.' };
      if (bytes.byteLength > 8 << 20) return { ok: false, id: '', name: '', error: 'Attach a file under 8 MB.' };
      let binary = '';
      for (let i = 0; i < bytes.length; i++) binary += String.fromCharCode(bytes[i]!);
      const data = btoa(binary);
      const r = await call(store, `/api/bots/${encodeURIComponent(slug)}/attachments`, {
        method: 'POST',
        body: JSON.stringify({
          name: String(name || 'file'),
          mediaType: String(mediaType || 'application/octet-stream'),
          data,
        }),
      });
      if (!r.ok) return { ok: false, id: '', name: '', error: r.error };
      return {
        ok: true,
        id: String(r.body.id ?? r.body.attachmentId ?? ''),
        name: String(r.body.name ?? name ?? 'file'),
        error: '',
      };
    } catch {
      return { ok: false, id: '', name: '', error: 'Can’t read that file. Try another one.' };
    }
  },
  stopRun: async ([slug], store) => reply(await call(store, `/api/bots/${encodeURIComponent(slug)}/stop`, { method: 'POST', body: '{}' })),
  loadUsage: async ([gate], store): Promise<Result<'loadUsage'>> => {
    const blank = { ok: false, error: '', messages: 0, attachments: 0, vaultWrites: 0, iconFetches: 0 };
    if (gate !== '1') return blank;
    try {
      const body = await read(store, '/api/usage');
      const totals = body.totals ?? {};
      const n = (key: string): number => (typeof totals[key] === 'number' ? totals[key] : 0);
      return { ok: true, error: '', messages: n('messages'), attachments: n('attachments'), vaultWrites: n('vaultWrites'), iconFetches: n('iconFetches') };
    } catch (e) {
      return { ...blank, error: explain(e instanceof Error ? e.message : String(e), 'usage') };
    }
  },
  loadBotUsage: async ([slug, gate], store): Promise<Result<'loadBotUsage'>> => {
    const blank = { ok: true, error: '', messages: 0, attachments: 0, turns: 0, inputTokens: 0, outputTokens: 0, cachedInputTokens: 0, cacheCreationTokens: 0, tokensKnown: false };
    if (slug === '' || gate !== '1') return blank;
    const r = await call(store, `/api/bots/${encodeURIComponent(slug)}/usage`);
    if (!r.ok) return { ...blank, ok: false, error: explain(r.error, 'usage') };
    const n = (key: string): number => (typeof r.body[key] === 'number' ? r.body[key] : 0);
    return {
      ok: true,
      error: String(r.body.error ?? ''),
      messages: n('messages'),
      attachments: n('attachments'),
      turns: n('turns'),
      inputTokens: n('inputTokens'),
      outputTokens: n('outputTokens'),
      cachedInputTokens: n('cachedInputTokens'),
      cacheCreationTokens: n('cacheCreationTokens'),
      tokensKnown: r.body.tokensKnown === true,
    };
  },
  loadModels: async ([slug, gate], store): Promise<Result<'loadModels'>> => {
    if (slug === '' || gate !== '1') return { ok: true, error: '', current: '', instanceId: '', choices: [] };
    const r = await call(store, `/api/bots/${encodeURIComponent(slug)}/model`);
    if (!r.ok) return { ok: false, error: explain(r.error, 'models'), current: '', instanceId: '', choices: [] };
    const choices = Array.isArray(r.body.choices) ? r.body.choices : [];
    return {
      ok: true,
      error: '',
      current: String(r.body.current ?? ''),
      instanceId: String(r.body.instanceId ?? ''),
      choices: choices.map((c: any) => ({
        instanceId: String(c.instanceId ?? ''),
        model: String(c.model ?? ''),
        label: String(c.label ?? c.model ?? ''),
      })),
    };
  },
  setModel: async ([slug, instanceId, model], store) => reply(await call(store, `/api/bots/${encodeURIComponent(slug)}/model`, {
    method: 'PUT', body: JSON.stringify({ instanceId, model }),
  })),
  loadRoutines: async ([slug, gate], store): Promise<Result<'loadRoutines'>> => {
    if (slug === '' || gate !== '1') return { ok: true, tasks: [], error: '' };
    const r = await call(store, `/api/bots/${encodeURIComponent(slug)}/routines`);
    if (!r.ok) {
      if (r.error.includes('no conversation') || r.error.includes('getting set up')) {
        return { ok: true, tasks: [], error: 'This Bot is still getting set up.' };
      }
      return { ok: false, tasks: [], error: explain(r.error, 'routines') };
    }
    const tasks = Array.isArray(r.body.tasks) ? r.body.tasks : [];
    return {
      ok: true,
      error: String(r.body.error ?? ''),
      tasks: tasks.map((t: any) => ({
        id: String(t.id ?? ''),
        title: String(t.title ?? ''),
        enabled: t.enabled === true,
        schedule: String(t.schedule ?? ''),
      })),
    };
  },
  saveRoutine: async ([slug, title, kind], store) => reply(await call(store, `/api/bots/${encodeURIComponent(slug)}/routines`, {
    method: 'POST', body: JSON.stringify({ title, kind }),
  })),
  setRoutineEnabled: async ([slug, id, enabled], store) => reply(await call(store, `/api/bots/${encodeURIComponent(slug)}/routines/enabled`, {
    method: 'POST', body: JSON.stringify({ id, enabled }),
  })),
  deleteRoutine: async ([slug, id], store) => reply(await call(store, `/api/bots/${encodeURIComponent(slug)}/routines/delete`, {
    method: 'POST', body: JSON.stringify({ id }),
  })),
  runRoutine: async ([slug, id], store) => reply(await call(store, `/api/bots/${encodeURIComponent(slug)}/routines/run`, {
    method: 'POST', body: JSON.stringify({ id }),
  })),
  loadLineage: async ([slug, gate], store): Promise<Result<'loadLineage'>> => {
    if (slug === '' || gate !== '1') return { ok: true, rows: [], error: '' };
    if (slug === 'fixture') {
      return {
        ok: true, error: '',
        rows: [
          { id: 'local-1', title: 'Notes', status: 'Done', remote: false, threadId: 'child-thread' },
          { id: 'remote-1', title: 'Away', status: 'Working', remote: true, threadId: 'remote-thread' },
        ],
      };
    }
    const r = await call(store, `/api/bots/${encodeURIComponent(slug)}/lineage`);
    if (!r.ok) {
      if (r.error.includes('no conversation') || r.error.includes('getting set up')) {
        return { ok: true, rows: [], error: 'This Bot is still getting set up.' };
      }
      return { ok: false, rows: [], error: explain(r.error, 'what this Bot is doing') };
    }
    const rows = Array.isArray(r.body.rows) ? r.body.rows : [];
    return {
      ok: true,
      error: '',
      rows: rows.map((row: any) => ({
        id: String(row.id ?? ''),
        title: String(row.title ?? ''),
        status: String(row.status ?? ''),
        remote: row.remote === true,
        threadId: String(row.threadId ?? ''),
      })),
    };
  },
  loadLibrary: async ([slug, gate], store): Promise<Result<'loadLibrary'>> => {
    if (slug === '' || gate !== '1') return { ok: true, entries: [], connectors: [] };
    const body = await read(store, `/api/bots/${encodeURIComponent(slug)}/library`);
    const entries = Array.isArray(body.entries) ? body.entries : [];
    const connectors = Array.isArray(body.connectors) ? body.connectors : [];
    return {
      ok: body.ok !== false,
      entries: entries.map((e: any) => libraryEntry(slug, e)),
      connectors: connectors.map((c: any) => ({
        name: String(c.name ?? ''),
        url: String(c.url ?? ''),
        enabled: c.enabled === true,
        locked: c.locked === true,
        hasSecret: c.hasSecret === true,
      })),
    };
  },
  saveLibraryItem: async ([slug, id, kind, title, status, body], store) => reply(await call(store, `/api/bots/${encodeURIComponent(slug)}/library`, {
    method: 'POST',
    body: JSON.stringify({ ...(id !== '' ? { id } : {}), kind, title, status, body }),
  })),
  deleteLibraryItem: async ([slug, id], store) => reply(await call(store, `/api/bots/${encodeURIComponent(slug)}/library/items/${encodeURIComponent(id)}`, { method: 'DELETE' })),
  saveConnector: async ([slug, name, url, enabled, secret], store) => reply(await call(store, `/api/bots/${encodeURIComponent(slug)}/connectors`, {
    method: 'POST', body: JSON.stringify({ name, url, enabled, ...(secret !== '' ? { secret } : {}) }),
  })),
  deleteConnector: async ([slug, name], store) => reply(await call(store, `/api/bots/${encodeURIComponent(slug)}/connectors`, {
    method: 'DELETE', body: JSON.stringify({ name }),
  })),
  loadComputer: async ([slug, gate], store): Promise<Result<'loadComputer'>> => {
    if (slug === '' || gate !== '1') return { ok: true, tabs: [], image: '', error: '' };
    const r = await call(store, `/api/bots/${encodeURIComponent(slug)}/computer`);
    if (!r.ok) {
      if (r.error.includes('no conversation') || r.error.includes('getting set up')) {
        return { ok: true, tabs: [], image: '', error: 'This Bot is still getting set up.' };
      }
      throw new Error(r.error);
    }
    const tabs = Array.isArray(r.body.tabs) ? r.body.tabs : [];
    return {
      ok: true,
      image: String(r.body.image ?? ''),
      error: String(r.body.error ?? ''),
      tabs: tabs.map((t: any) => ({ id: String(t.id ?? ''), title: String(t.title ?? ''), url: String(t.url ?? '') })),
    };
  },
  computerControl: async ([slug, tabId, action], store) => reply(await call(store, `/api/bots/${encodeURIComponent(slug)}/computer/control`, {
    method: 'POST', body: JSON.stringify({ tabId, action }),
  })),
  loadRoom: async ([id], store): Promise<Result<'loadRoom'>> => {
    if (id === '') return { id: '', title: '', members: [], messages: [] };
    if (id === 'fixture') {
      return {
        id: 'fixture', title: 'Design',
        members: [
          { slug: 'ada', name: 'Ada', line: 'Working', state: 'working', ready: true, working: true },
          { slug: 'bea', name: 'Bea', line: 'Getting set up…', state: 'asleep', ready: false, working: false },
        ],
        messages: [
          { id: 'r1', name: 'You', text: 'Hello' },
          { id: 'r2', name: 'Ada', text: 'On it' },
        ],
      };
    }
    const body = await read(store, `/api/rooms/${encodeURIComponent(id)}`);
    const members = Array.isArray(body.members) ? body.members : [];
    const messages = Array.isArray(body.messages) ? body.messages : [];
    return {
      id: String(body.id ?? id),
      title: String(body.title ?? ''),
      members: members.map((m: any) => ({
        slug: String(m.slug ?? ''), name: String(m.name ?? ''), line: String(m.line ?? ''),
        state: String(m.state ?? 'idle'), ready: m.ready === true, working: m.working === true,
      })),
      messages: messages.map((m: any) => ({ id: String(m.id ?? ''), name: String(m.name ?? ''), text: String(m.text ?? '') })),
    };
  },
};
export const answer: Answer = (source, args, store, storage, native) => sources[source](args, store, storage, native);
