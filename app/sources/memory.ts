import type { Sources, Result } from '../app.contract.d.ts';
import { API } from '../host/config.ts';
import { explain } from '../host/fixture.ts';
import { GATEWAY_PROBLEM, read } from '../host/gateway.ts';

const EMPTY_REV = 'e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855';
const QUIET = { text: 'Likes tea\n', revision: 'rev-tea' };
const BEA = { text: 'Likes tea\n', revision: 'old' };
const SERVER = { text: 'Bot wrote this\n', revision: 'new' };

type Held = { text: string; revision: string };
type Store = { get: (key: string) => unknown; set: (key: string, value: string) => void };

function blank(): Result<'loadMemory'> {
  return { ok: true, path: '', text: '', revision: '', truncated: false };
}

function keyOf(slug: string): string {
  return `memory-fixture:${slug}`;
}

function held(store: Store, slug: string): Held | null {
  const raw = store.get(keyOf(slug));
  if (typeof raw !== 'string' || raw === '') return null;
  try {
    const parsed = JSON.parse(raw) as { text?: unknown; revision?: unknown };
    if (typeof parsed.text === 'string' && typeof parsed.revision === 'string') {
      return { text: parsed.text, revision: parsed.revision };
    }
  } catch {
    return null;
  }
  return null;
}

function remember(store: Store, slug: string, text: string, revision: string) {
  store.set(keyOf(slug), JSON.stringify({ text, revision }));
}

function finish(text: string): string {
  if (text === '' || text.endsWith('\n')) return text;
  return `${text}\n`;
}

function seed(slug: string): Held | null {
  if (slug === 'quiet') return QUIET;
  if (slug === 'bea') return BEA;
  if (slug === 'frame') return { text: '', revision: EMPTY_REV };
  return null;
}

function saved(ok: boolean, error: string, conflict: boolean, text: string, revision: string): Result<'saveBotMemory'> {
  return { ok, error, conflict, text, revision };
}

export const memorySources = {
  loadMemory: async ([slug, gate], store): Promise<Result<'loadMemory'>> => {
    if (slug === '' || gate !== '1') return blank();
    const fixture = seed(slug);
    if (fixture) {
      const current = held(store, slug) ?? fixture;
      return { ok: true, path: 'memory/MEMORY.md', text: current.text, revision: current.revision, truncated: false };
    }
    const body = await read(store, `/api/memory?scope=bot&slug=${encodeURIComponent(slug)}`);
    return {
      ok: true,
      path: String(body.path ?? ''),
      text: String(body.text ?? ''),
      revision: String(body.revision ?? ''),
      truncated: body.truncated === true,
    };
  },
  saveBotMemory: async ([slug, text, revision], store): Promise<Result<'saveBotMemory'>> => {
    const fixture = seed(slug);
    if (fixture) {
      const current = held(store, slug) ?? fixture;
      // The contract runner's Bea: the first save is the Bot writing first.
      if (slug === 'bea' && current.revision === BEA.revision && revision === BEA.revision) {
        remember(store, slug, SERVER.text, SERVER.revision);
        return saved(false, '', true, SERVER.text, SERVER.revision);
      }
      if (revision !== current.revision) return saved(false, '', true, current.text, current.revision);
      const next = finish(text);
      const nextRev = `saved-${next.length}`;
      remember(store, slug, next, nextRev);
      return saved(true, '', false, next, nextRev);
    }
    const session = store.get('session');
    try {
      const res = await fetch(`${API}/api/memory?scope=bot&slug=${encodeURIComponent(slug)}`, {
        method: 'PUT',
        exactTimeout: 15000,
        headers: {
          'content-type': 'application/json',
          ...(session ? { authorization: `Bearer ${session}` } : {}),
        },
        body: JSON.stringify({ text, revision }),
      });
      const body = await res.json().catch(() => ({}));
      if (res.status === 409 && body?.code === 'conflict') {
        return saved(false, '', true, String(body.text ?? ''), String(body.revision ?? ''));
      }
      if (!res.ok) {
        const message = typeof body?.message === 'string' ? body.message : typeof body?.error === 'string' ? body.error : GATEWAY_PROBLEM;
        return saved(false, explain(message, 'memory'), false, '', '');
      }
      return saved(true, '', false, String(body.text ?? ''), String(body.revision ?? ''));
    } catch (e) {
      if ((e as { code?: string }).code === 'bake') throw e;
      const message = e instanceof Error ? e.message : String(e);
      return saved(false, explain(message, 'memory'), false, '', '');
    }
  },
} satisfies Pick<Sources, 'loadMemory' | 'saveBotMemory'>;
