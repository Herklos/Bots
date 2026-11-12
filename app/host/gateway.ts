import { unsupported } from './notices.ts';
import { notificationStatus } from '@exact/notifications';
import en from '../strings/en.json' with { type: 'json' };
import fr from '../strings/fr.json' with { type: 'json' };
import { API } from '../app.ts';

export type Answered = { ok: true; body: any } | { ok: false; error: string };

// Sentences the client writes. Probes in explain() stay the English wording call() returns.
// The host locale (exactTime, fr* → French) picks the table the screen shows.
export const GATEWAY_PROBLEM = 'The gateway had a problem. Try again.';
export const REACH_GATEWAY = 'Can’t reach the gateway. Check that `hbots` is running, then try again.';
export const FILE_EMPTY = 'That file is empty.';
export const FILE_TOO_BIG = 'Attach a file under 8 MB.';
export const FILE_READ = 'Can’t read that file. Try another one.';
export const LOAD_KEY: Record<string, string> = {
  computerOpen: 'error.computerOpen',
  computerClose: 'error.computerClose',
  usage: 'error.loadUsage',
  models: 'error.loadModels',
  providers: 'error.loadProviders',
  routines: 'error.routines',
  'what this Bot is doing': 'error.lineage',
};
export const CLIENT_ERRORS: Record<string, string> = {
  [GATEWAY_PROBLEM]: 'error.gatewayProblem',
  [REACH_GATEWAY]: 'error.reach',
  [FILE_EMPTY]: 'error.fileEmpty',
  [FILE_TOO_BIG]: 'error.fileTooBig',
  [FILE_READ]: 'error.fileRead',
};
export let hostLocale = 'en';
export function noteLocale(locale: string) {
  if (locale) hostLocale = locale;
}
export function say(key: string): string {
  const table: Record<string, string> = hostLocale.toLowerCase().startsWith('fr') ? fr : en;
  const text = table[key] ?? (en as Record<string, string>)[key];
  return typeof text === 'string' ? text : key;
}
export function localize(error: string): string {
  const key = CLIENT_ERRORS[error];
  return key ? say(key) : error;
}
export function loadSentence(what: string): string {
  const key = LOAD_KEY[what];
  return key ? say(key) : `Can’t load ${what}. Check that hbots is running, then try again.`;
}

export async function call(store: any, path: string, init: RequestInit = {}): Promise<Answered> {
  const session = store.get('session');
  try {
    const res = await fetch(`${API}${path}`, { ...init, exactTimeout: 15000,
      headers: { 'content-type': 'application/json', ...(session ? { authorization: `Bearer ${session}` } : {}) } });
    const body = await res.json().catch(() => ({}));
    if (res.ok) return { ok: true, body };
    return { ok: false, error: typeof body.error === 'string' ? body.error : GATEWAY_PROBLEM };
  } catch (e) {
    if ((e as { code?: string }).code === 'bake') throw e; // there is no gateway at build time
    console.log(`gateway request failed: ${path}: ${String((e as { code?: string }).code ?? '')} ${String(e)}`);
    return { ok: false, error: REACH_GATEWAY };
  }
}
// A read that fails fails its resource, so Exact keeps the last answer while it retries.
export async function read(store: any, path: string): Promise<any> {
  const r = await call(store, path);
  if (!r.ok) throw new Error(r.error);
  return r.body;
}
// A chat or group read may be held for about 25 seconds. 304 keeps the previous answer.
export const CHAT_HOLD_MS = 30_000;
export async function fresh(store: any, path: string, etag: string): Promise<{ status: number; body: any; etag: string }> {
  const session = store.get('session');
  try {
    const res = await fetch(`${API}${path}`, {
      exactTimeout: CHAT_HOLD_MS,
      headers: {
        ...(session ? { authorization: `Bearer ${session}` } : {}),
        ...(etag !== '' ? { 'if-none-match': etag } : {}),
      },
    });
    const next = res.headers.get('etag') ?? etag;
    if (res.status === 304) return { status: 304, body: null, etag: next };
    const body = await res.json().catch(() => ({}));
    if (!res.ok) throw new Error(typeof body.error === 'string' ? body.error : GATEWAY_PROBLEM);
    return { status: res.status, body, etag: next };
  } catch (e) {
    if ((e as { code?: string }).code === 'bake') throw e;
    if (e instanceof Error && (e as { code?: string }).code === undefined) throw e;
    console.log(`gateway request failed: ${path}: ${String((e as { code?: string }).code ?? '')} ${String(e)}`);
    throw new Error(REACH_GATEWAY);
  }
}
// Two lists read the Bots in one tick; the second shares the first's request.
export let botsInflight: Promise<any> | null = null;
export function botsBody(store: any): Promise<any> {
  if (!botsInflight) botsInflight = read(store, '/api/bots').finally(() => { botsInflight = null; });
  return botsInflight;
}
// The conversation list and the unread poster share a rooms read that is still in flight.
export let roomsInflight: Promise<any> | null = null;
export function roomsBody(store: any): Promise<any> {
  if (!roomsInflight) {
    // No timer: Exact forbids setTimeout in a resource. Clear when the read settles.
    const run: Promise<Answered> = call(store, '/api/rooms').finally(() => {
      if (roomsInflight === run) roomsInflight = null;
    });
    roomsInflight = run;
  }
  return roomsInflight;
}
// The host's notification permission. A tick reads the notice resource; this remembers the last answer.
export let noticeHost: { available: boolean; state: string } | null = null;
export let noticeHostInflight: Promise<{ available: boolean; state: string } | null> | null = null;
export async function hostNotice(): Promise<{ available: boolean; state: string } | null> {
  if (noticeHost) return noticeHost;
  if (!noticeHostInflight) {
    noticeHostInflight = notificationStatus({}).then((status) => {
      const next = { available: status?.available === true, state: String(status?.state ?? 'unknown') };
      noticeHost = next;
      return next;
    }).catch((error: unknown) => {
      if (unsupported(error)) return null;
      throw error;
    }).finally(() => { noticeHostInflight = null; });
  }
  return noticeHostInflight;
}
export const reply = (r: Answered) => (r.ok ? { ok: true, error: '' } : { ok: false, error: localize(r.error) });
// The transcript row for a command id is `turn-item:message:` plus that id.
export const transcriptRowId = (id: string): string => (id === '' || id.startsWith('turn-item:') ? id : `turn-item:message:${id}`);
export function sentFrom(r: Answered, fallback: string): { ok: boolean; error: string; messageId: string } {
  if (!r.ok) return { ok: false, error: localize(r.error), messageId: '' };
  const id = typeof r.body?.messageId === 'string' ? r.body.messageId : '';
  return { ok: true, error: '', messageId: id !== '' ? id : fallback };
}
// The first read on the About tab asks GitHub. Later refreshes use the stored answer.
export let systemChecked = false;

// POST /api/bots returns the saved Bot. `reply` keeps only ok and error, which drops slug.
export function madeFromBot(r: Answered): { ok: boolean; id: string; error: string } {
  if (!r.ok) return { ok: false, id: '', error: localize(r.error) };
  const slug = typeof r.body?.slug === 'string' ? r.body.slug : '';
  return { ok: true, id: slug, error: '' };
}
