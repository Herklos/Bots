import type { Answer, Result, Sources, Storage } from './app.contract.d.ts';
import { closeNotification, notificationStatus, showNotification } from '@exact/notifications';
import en from './strings/en.json' with { type: 'json' };
import fr from './strings/fr.json' with { type: 'json' };

// Where the hbots gateway (the `hbots` binary) listens. A data module can't read the page's address
// (no `location`, and `fetch` refuses a relative URL), so the web build is made with this placeholder
// and the gateway, which serves it, swaps in the address the page was loaded from. One build then works
// on localhost, Tailscale or a tunnel. Native apps have no page: build them with HBOTS_URL set to the
// gateway, e.g. `HBOTS_URL=https://hbots.example.ts.net bun exact.mjs mac`.
const HBOTS_URL = import.meta.env.HBOTS_URL ?? 'http://hbots-origin.invalid';
const API: string = HBOTS_URL.replace(/\/$/, '');
// Any gateway the person runs: their machine, a Tailscale address or a tunnel. Plain `http:` to a host
// name on Apple also needs `appTransportSecurity.allowsArbitraryLoads` in app.json.
export const grants = 'net.fetch https:\nnet.fetch http:\nsecret.keep session\nsecret.keep drafts\nfs.read app:/tmp\nfs.write app:/tmp\ndevice.microphone purpose.microphone\ndevice.notifications purpose.notifications';

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
const DAYS = ['Sunday', 'Monday', 'Tuesday', 'Wednesday', 'Thursday', 'Friday', 'Saturday'];
const MONTHS = ['Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun', 'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec'];
const clock = (d: Date): string => `${d.getHours() % 12 || 12}:${String(d.getMinutes()).padStart(2, '0')} ${d.getHours() < 12 ? 'AM' : 'PM'}`;
const dayStart = (t: number): number => new Date(t).setHours(0, 0, 0, 0);
// A time as a list shows it: the clock today, then Yesterday, the weekday for a week, then the date.
function when(at: unknown, now: number): string {
  if (typeof at !== 'string' || at === '' || !(now > 0)) return '';
  const d = new Date(at);
  if (Number.isNaN(d.getTime())) return '';
  const days = Math.round((dayStart(now) - dayStart(d.getTime())) / 86_400_000);
  if (days <= 0) return clock(d);
  if (days === 1) return 'Yesterday';
  if (days < 7) return DAYS[d.getDay()]!;
  return `${MONTHS[d.getMonth()]} ${d.getDate()}`;
}
// A group prompt that leaked into a Bot's own thread. It belongs in the room, not under their name.
function isGroupLeak(text: string): boolean {
  return text.includes('[Group "') || text.includes('Your reply is shared with the group');
}
// The one line under a Bot's name in the inbox.
function lineOf(b: any): string {
  if (b.working === true) return 'Working…';
  const p = b.preview;
  if (p && typeof p.text === 'string' && p.text !== '' && !isGroupLeak(p.text)) return p.role === 'user' ? `You: ${p.text}` : p.text;
  return typeof b.mainThreadId === 'string' ? 'Ready to chat' : 'Getting set up…';
}
// A group's face is its members, at most three. The list stores slugs or member objects.
function stackFaces(members: unknown, directory: any[]): { shape: string; color: string; state: string }[] {
  const slugs = (Array.isArray(members) ? members : [])
    .map((m) => (typeof m === 'string' ? m : String((m as { slug?: unknown })?.slug ?? '')))
    .filter((slug) => slug !== '')
    .slice(0, 3);
  return slugs.map((slug) => {
    const bot = directory.find((b) => String(b.slug) === slug);
    return { shape: String(bot?.shape ?? 'ball'), color: fill(bot?.color), state: 'idle' };
  });
}
const initials = (name: string): string => name.trim().split(/\s+/).slice(0, 2).map((w) => w.charAt(0).toUpperCase()).join('') || '?';

type Answered = { ok: true; body: any } | { ok: false; error: string };

// Sentences the client writes. Probes in explain() stay the English wording call() returns.
// The host locale (exactTime, fr* → French) picks the table the screen shows.
const GATEWAY_PROBLEM = 'The gateway had a problem. Try again.';
const REACH_GATEWAY = 'Can’t reach the gateway. Check that `hbots` is running, then try again.';
const FILE_EMPTY = 'That file is empty.';
const FILE_TOO_BIG = 'Attach a file under 8 MB.';
const FILE_READ = 'Can’t read that file. Try another one.';
const LOAD_KEY: Record<string, string> = {
  usage: 'error.loadUsage',
  models: 'error.loadModels',
  providers: 'error.loadProviders',
  routines: 'error.routines',
  'what this Bot is doing': 'error.lineage',
};
const CLIENT_ERRORS: Record<string, string> = {
  [GATEWAY_PROBLEM]: 'error.gatewayProblem',
  [REACH_GATEWAY]: 'error.reach',
  [FILE_EMPTY]: 'error.fileEmpty',
  [FILE_TOO_BIG]: 'error.fileTooBig',
  [FILE_READ]: 'error.fileRead',
};
let hostLocale = 'en';
function noteLocale(locale: string) {
  if (locale) hostLocale = locale;
}
function say(key: string): string {
  const table: Record<string, string> = hostLocale.toLowerCase().startsWith('fr') ? fr : en;
  const text = table[key] ?? (en as Record<string, string>)[key];
  return typeof text === 'string' ? text : key;
}
function localize(error: string): string {
  const key = CLIENT_ERRORS[error];
  return key ? say(key) : error;
}
function loadSentence(what: string): string {
  const key = LOAD_KEY[what];
  return key ? say(key) : `Can’t load ${what}. Check that hbots is running, then try again.`;
}

async function call(store: any, path: string, init: RequestInit = {}): Promise<Answered> {
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
async function read(store: any, path: string): Promise<any> {
  const r = await call(store, path);
  if (!r.ok) throw new Error(r.error);
  return r.body;
}
// Two lists read the Bots in one tick; the second shares the first's request.
let botsInflight: Promise<any> | null = null;
function botsBody(store: any): Promise<any> {
  if (!botsInflight) botsInflight = read(store, '/api/bots').finally(() => { botsInflight = null; });
  return botsInflight;
}
const reply = (r: Answered) => (r.ok ? { ok: true, error: '' } : { ok: false, error: localize(r.error) });

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

type PinRow = { id: string; text: string; missing: boolean };
type ChatRow = any;

// The contract runner has no gateway: a small conversation, a writable child and a missing pin.
// Pins are mutable so a Pin tap can toggle a row without a live gateway.
// A room the contract runner just created. Null keeps the built-in Design room.
let fixtureMade: { title: string; description: string; color: string; shape: string; slugs: string[] } | null = null;

const fixturePins: Record<string, PinRow[]> = {
  'fixture-thread': [
    { id: 'm1', text: 'Hello', missing: false },
    { id: 'missing', text: NOT_IN_VIEW, missing: true },
  ],
  'child-thread': [],
};

function fixtureChat(child: string) {
  const row = (r: Record<string, unknown>) => ({ ...blank, head: true, tail: true, mine: false, text: '', ...r, kind: 'message' }) as ChatRow;
  if (child === 'child-thread') {
    return { slug: 'fixture', working: false, partial: false, readOnly: false, threadId: 'child-thread', pins: [...(fixturePins['child-thread'] ?? [])], rows: [row({ id: 'c1', text: 'Notes' })] };
  }
  return {
    slug: 'fixture', working: false, partial: true, readOnly: false, threadId: 'fixture-thread',
    pins: [...(fixturePins['fixture-thread'] ?? [])],
    rows: [
      row({ id: 'm1', mine: true, text: 'Hello', tail: false }),
      row({ id: 'm2', time: '9:00 AM', text: 'Hi there', react: '🔥', quote: 'Hello', tokens: '12 in, 3 out, 4 read from cache, 1 written to cache.', head: true }),
      row({ id: 'm3', time: '9:01 AM', text: 'No tokens here', head: false }),
      row({ id: 'voice1', mine: true, files: [{ id: 'voice', name: 'voice.m4a', kind: 'audio', size: '1 KB', url: '/api/bots/fixture/attachments/voice' }] }),
    ],
  };
}

function explain(error: string, what: string): string {
  const e = error.toLowerCase();
  if (e.includes('reach the gateway')) return loadSentence(what);
  if (e.includes('login')) return say('error.signIn');
  if (e.includes('no conversation') || e.includes('getting set up')) return say('error.settingUp');
  if (e.includes('t3')) return say('error.t3');
  if (e.includes('had a problem')) return say('error.restart');
  return error || loadSentence(what);
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


const compact = (n: number): string => (n >= 1e6 ? `${(n / 1e6).toFixed(n >= 1e7 ? 0 : 1)}M` : n >= 1e3 ? `${(n / 1e3).toFixed(n >= 1e4 ? 0 : 1)}k` : String(n)).replace('.0', '');
// "claude-opus-5-5" as "Claude Opus 5.5" when T3 gave no name.
const prettyModel = (slug: string): string => slug.split('-').filter(Boolean).map((w, i, all) => (/^\d+$/.test(w) && /^\d+$/.test(all[i + 1] ?? '') ? `${w}.` : w.charAt(0).toUpperCase() + w.slice(1))).join(' ').replace(/\. /g, '.');
const dayOf = (day: string): Date => new Date(Number(day.slice(0, 4)), Number(day.slice(5, 7)) - 1, Number(day.slice(8, 10)));
// When a routine runs next: the clock today or tomorrow, then the weekday.
function nextLabel(at: unknown, now: number): string {
  if (typeof at !== 'string' || at === '' || !(now > 0)) return '';
  const d = new Date(at);
  if (Number.isNaN(d.getTime())) return '';
  const days = Math.round((dayStart(d.getTime()) - dayStart(now)) / 86_400_000);
  const day = days <= 0 ? 'Today' : days === 1 ? 'Tomorrow' : days < 7 ? DAYS[d.getDay()]! : `${MONTHS[d.getMonth()]} ${d.getDate()}`;
  return `Next: ${day} ${clock(d)}`;
}
const PERIODS: Record<string, string> = { today: 'Today', '7d': '7 days', '30d': '30 days', all: 'All time' };
function emptyReport(error: string) {
  return { ok: error === '', error, period: '', periodLabel: '', messages: '0', attachments: '0', turns: '0', tokens: '0', input: '0', output: '0', cached: '0', cacheWrite: '0', hit: '', sentence: '', days: [] as any[], rows: [] as any[], models: [] as any[], bots: [] as any[], empty: true, first: '', last: '' };
}
// The gateway's usage report as the panel and Settings show it: figures as words, bars as heights.
function usageReport(b: any) {
  const t = b.totals ?? {};
  const series = ((b.series ?? []) as any[]);
  const peak = Math.max(1, ...series.map((d) => Number(d.input) + Number(d.output)));
  const days = series.map((d) => {
    const tokens = Number(d.input) + Number(d.output);
    const date = dayOf(String(d.day));
    return { day: String(d.day), label: `${DAYS[date.getDay()]!.slice(0, 3)} ${date.getDate()}`, short: String(date.getDate()), tokens: compact(tokens), input: compact(Number(d.input)), output: compact(Number(d.output)), cached: compact(Number(d.cached)), cacheWrite: compact(Number(d.cacheWrite)),
      turns: String(d.turns), messages: String(d.messages), px: tokens === 0 ? 2 : Math.max(4, Math.round((tokens / peak) * 88)), spx: tokens === 0 ? 2 : Math.max(3, Math.round((tokens / peak) * 28)) };
  });
  const models = ((b.byModel ?? []) as any[]);
  const top = Math.max(1, ...models.map((m) => Number(m.tokens)));
  const bots = ((b.byBot ?? []) as any[]).filter((x) => Number(x.tokens) + Number(x.messages) > 0);
  const botTop = Math.max(1, ...bots.map((x) => Number(x.tokens)));
  const input = Number(t.input ?? 0);
  return {
    ok: true, error: String(b.error ?? ''), period: String(b.period ?? ''), periodLabel: PERIODS[String(b.period)] ?? '',
    messages: compact(Number(t.messages ?? 0)), attachments: compact(Number(t.attachments ?? 0)), turns: compact(Number(t.turns ?? 0)),
    tokens: compact(Number(t.tokens ?? 0)), input: compact(input), output: compact(Number(t.output ?? 0)), cached: compact(Number(t.cached ?? 0)), cacheWrite: compact(Number(t.cacheWrite ?? 0)),
    hit: input > 0 ? `${Math.round((Number(t.cached ?? 0) / input) * 100)}%` : '',
    sentence: input > 0 ? `${Math.round((Number(t.cached ?? 0) / input) * 100)}% of input came from cache.` : (Number(t.messages ?? 0) > 0 ? '' : 'Nothing counted in this period.'),
    days, empty: Number(t.tokens ?? 0) === 0 && Number(t.messages ?? 0) === 0,
    first: days[0]?.label ?? '', last: days[days.length - 1]?.label ?? '',
    rows: [...days].reverse().filter((d) => d.tokens !== '0' || d.messages !== '0'),
    models: models.map((m) => ({ name: prettyModel(String(m.model || 'Unknown')), tokens: compact(Number(m.tokens)), turns: String(m.turns), share: Math.max(4, Math.round((Number(m.tokens) / top) * 100)) })),
    bots: bots.map((x) => ({ slug: String(x.slug), name: String(x.name), color: fill(x.color), shape: String(x.shape ?? 'ball'), tokens: compact(Number(x.tokens)), turns: String(x.turns), messages: String(x.messages), share: Math.max(4, Math.round((Number(x.tokens) / botTop) * 100)) })),
  };
}

// What the gateway last said the time was, and each Bot's last computer frame. A data module may keep
// values like these between reads. The session token and the draft map live in the secret store.
let lastNow = 0;
const frames: Record<string, string> = {};
const threads: Record<string, string> = {};
const TYPES: Record<string, string> = { png: 'image/png', jpg: 'image/jpeg', jpeg: 'image/jpeg', gif: 'image/gif', webp: 'image/webp', pdf: 'application/pdf', txt: 'text/plain', md: 'text/markdown', csv: 'text/csv', json: 'application/json', mp4: 'video/mp4', m4a: 'audio/mp4' };
const guessType = (name: string): string => TYPES[name.split('.').pop()?.toLowerCase() ?? ''] ?? 'application/octet-stream';
const GAP = 60 * 60_000;
const fileSize = (n: number): string => (n < 1024 ? `${n} B` : n < 1024 * 1024 ? `${(n / 1024).toFixed(1)} KB` : `${(n / 1024 / 1024).toFixed(1)} MB`);
// A day mark: the clock today, then "Yesterday 9:41 AM", the weekday for a week, then the date.
function dayLabel(at: string, now: number): string {
  const d = new Date(at);
  const days = Math.round((dayStart(now) - dayStart(d.getTime())) / 86_400_000);
  const day = days <= 0 ? 'Today' : days === 1 ? 'Yesterday' : days < 7 ? DAYS[d.getDay()]! : `${MONTHS[d.getMonth()]} ${d.getDate()}`;
  return `${day} ${clock(d)}`;
}
const blank = { quote: '', tokens: '', react: '', key: '', time: '', title: '', files: [] as any[], botSlug: '', botName: '', botColor: 'brand.default', botShape: 'ball', variant: '', choices: [] as any[], placeholder: '' };

// The transcript as rows: day marks, messages in runs, the lines between them, what the Bot is
// waiting on, and the working line. A run is one sender's messages with no break between them.
function chatRows(slug: string, body: any, now: number, reactions: Record<string, string[]> = {}, replyTo: unknown = {}): any[] {
  const peer = (b: any) => (b && typeof b === 'object'
    ? { botSlug: String(b.slug ?? ''), botName: String(b.name ?? ''), botColor: fill(b.color), botShape: String(b.shape ?? 'ball') } : {});
  const timeline = [
    ...(body.messages as any[]).map((m) => ({ ...m, _t: 'message' })),
    ...((body.events ?? []) as any[]).map((e) => ({ ...e, _t: 'event' })),
  ].sort((a, b) => (a.ordinal ?? 0) - (b.ordinal ?? 0));
  const rows: any[] = [];
  let prev: any = null;
  let last = 0;
  for (const item of timeline) {
    const t = Date.parse(String(item.at ?? ''));
    if (now > 0 && Number.isFinite(t) && (last === 0 || t - last > GAP || dayStart(t) !== dayStart(last))) {
      rows.push({ ...blank, id: `day-${item.id}`, kind: 'day', mine: false, text: dayLabel(String(item.at), now), head: true, tail: true });
    }
    if (Number.isFinite(t)) last = t;
    if (item._t === 'event') {
      rows.push({ ...blank, ...peer(item.bot), id: String(item.id), kind: 'event', mine: false, text: String(item.text ?? ''), title: String(item.title ?? ''), head: true, tail: true, variant: String(item.kind ?? '') });
      prev = null;
      continue;
    }
    const mine = item.role === 'user' && !item.from;
    const who = `${mine}:${item.from?.slug ?? ''}`;
    const files = ((item.files ?? []) as any[]).map((f) => ({
      id: String(f.id), name: String(f.name), kind: String(f.kind), size: fileSize(Number(f.bytes ?? 0)),
      url: `${API}/api/bots/${encodeURIComponent(slug)}/attachments/${encodeURIComponent(String(f.id))}`,
    }));
    const row = { ...blank, ...peer(item.from), id: String(item.id), kind: 'message', mine, text: String(item.text ?? ''), react: ((reactions[String(item.id)] ?? []) as string[]).join(' '), quote: quoteOf(replyTo, String(item.id)), tokens: tokenText(item.tokens), files, head: !(prev && prev.who === who && t - prev.t < 5 * 60_000), tail: true,
      time: Number.isFinite(t) ? clock(new Date(t)) : '' };
    if (prev && !row.head) rows[prev.index].tail = false;
    rows.push(row);
    prev = { who, t, index: rows.length - 1 };
  }
  for (const r of (body.requests ?? []) as any[]) {
    const kind = String(r.kind);
    const choices = kind === 'approval_request'
      ? ((r.options ?? []) as any[]).map((o) => ({ label: String(o.label ?? o.decision), value: String(o.decision), key: '', warn: String(o.warning ?? '') }))
      : kind === 'user_input_request'
        ? (((r.questions ?? [])[0]?.options ?? []) as any[]).map((o) => ({ label: String(o.label), value: String(o.value ?? o.label), key: String(r.questions[0].id), warn: '' }))
        : [];
    rows.push({ ...blank, id: String(r.id), kind: 'request', mine: false, text: String(r.title ?? ''), title: String(r.reason ?? ''), head: true, tail: true, variant: kind, key: String((r.questions ?? [])[0]?.id ?? ''), choices,
      placeholder: String(r.placeholder ?? ((r.questions ?? [])[0]?.allowCustomAnswer ? 'Type an answer' : '')) });
  }
  if (body.working === true) rows.push({ ...blank, id: 'working', kind: 'working', mine: false, text: '', head: true, tail: true });
  return rows;
}

async function storeUpload(store: any, storage: Storage, slug: string, path: string, rawName: string, rawType: string) {
  const name = rawName !== '' ? rawName : decodeURIComponent(String(path).split('/').pop() ?? 'file');
  const mediaType = rawType !== '' ? rawType : guessType(name);
  const fail = (error: string) => ({ ok: false, id: '', name: '', url: '', error: localize(error) });
  try {
    const bytes = new Uint8Array(await storage.fs.readFile(String(path)));
    if (bytes.byteLength === 0) return fail(FILE_EMPTY);
    if (bytes.byteLength > 8 << 20) return fail(FILE_TOO_BIG);
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
    if (!r.ok) return fail(r.error);
    const id = String(r.body.id ?? r.body.attachmentId ?? '');
    return { ok: true, id, name: String(r.body.name ?? name ?? 'file'), url: `${API}/api/bots/${encodeURIComponent(slug)}/attachments/${encodeURIComponent(id)}`, error: '' };
  } catch {
    return fail(FILE_READ);
  }
}

// Drafts live in the session store under `drafts`: one record per Bot, and one per child
// (`slug/thread`). Text is capped at 8000. A room is never a key.
const DRAFT_SEP = '\u001f';
const FILE_SEP = '\\t';
type DraftFile = { id: string; name: string };
type DraftVoice = { hold: string; id: string; preview: string; seconds: number };
type DraftEntry = { text: string; files: DraftFile[]; voice?: DraftVoice };

function draftEntry(map: Record<string, DraftEntry>, key: string): DraftEntry {
  return (map[key] ??= { text: '', files: [] });
}

function draftMap(texts: string[], files: string[], voices: string[]): Record<string, DraftEntry> {
  const map: Record<string, DraftEntry> = {};
  for (const row of texts) {
    const cut = row.indexOf(DRAFT_SEP);
    if (cut <= 0) continue;
    draftEntry(map, row.slice(0, cut)).text = row.slice(cut + 1).slice(0, 8000);
  }
  for (const row of files) {
    const cut = row.indexOf(DRAFT_SEP);
    if (cut <= 0) continue;
    const body = row.slice(cut + 1);
    const nameAt = body.indexOf(FILE_SEP);
    const id = (nameAt < 0 ? body : body.slice(0, nameAt)).trim();
    if (!id) continue;
    draftEntry(map, row.slice(0, cut)).files.push({ id, name: nameAt < 0 ? '' : body.slice(nameAt + FILE_SEP.length) });
  }
  for (const row of voices) {
    const parts = row.split(DRAFT_SEP);
    const key = parts[0] ?? '';
    if (!key) continue;
    const hold = parts[1] ?? '';
    const id = parts[2] ?? '';
    if (!hold && !id) continue;
    const seconds = Number(parts[4] ?? 0);
    draftEntry(map, key).voice = { hold, id, preview: parts[3] ?? '', seconds: Number.isFinite(seconds) ? seconds : 0 };
  }
  for (const key of Object.keys(map)) {
    const entry = map[key]!;
    if (entry.text === '' && entry.files.length === 0 && !entry.voice) delete map[key];
  }
  return map;
}

function draftRows(raw: string | null): { stamp: string; texts: string[]; files: string[]; voices: string[] } {
  const texts: string[] = [];
  const files: string[] = [];
  const voices: string[] = [];
  if (!raw) return { stamp: '', texts, files, voices };
  let map: Record<string, DraftEntry> = {};
  try {
    const parsed = JSON.parse(raw) as unknown;
    if (!parsed || typeof parsed !== 'object') return { stamp: '', texts, files, voices };
    map = parsed as Record<string, DraftEntry>;
  } catch {
    return { stamp: '', texts, files, voices };
  }
  for (const [key, value] of Object.entries(map)) {
    if (!key || !value || typeof value !== 'object') continue;
    const text = typeof value.text === 'string' ? value.text.slice(0, 8000) : '';
    if (text) texts.push(`${key}${DRAFT_SEP}${text}`);
    for (const file of Array.isArray(value.files) ? value.files : []) {
      const id = String(file?.id ?? '').trim();
      if (!id) continue;
      files.push(`${key}${DRAFT_SEP}${id}${FILE_SEP}${String(file?.name ?? '')}`);
    }
    const voice = value.voice;
    if (voice && (voice.hold || voice.id)) {
      const seconds = Number(voice.seconds);
      voices.push(`${key}${DRAFT_SEP}${voice.hold ?? ''}${DRAFT_SEP}${voice.id ?? ''}${DRAFT_SEP}${voice.preview ?? ''}${DRAFT_SEP}${Number.isFinite(seconds) ? seconds : 0}`);
    }
  }
  return { stamp: raw, texts, files, voices };
}

// The contract runner's slugs. A real Bot still notifies.
const FIXTURE_SLUG = (slug: string): boolean => slug === 'fixture' || slug === 'bea' || slug === 'tabs';

async function noticeGranted(): Promise<boolean> {
  try {
    const status = await notificationStatus({});
    return status?.state === 'granted';
  } catch {
    return false;
  }
}

// Closes the one notice for this Bot. A host with no notification centre still opens the chat.
async function closeNotice(slug: string): Promise<void> {
  if (slug === '' || FIXTURE_SLUG(slug)) return;
  try {
    await closeNotification({ tag: `unread-${slug}` });
  } catch {
    // The page keeps working when the host cannot close a notice.
  }
}

// One local notice per hidden unread Bot. The open chat is closed instead.
// Nothing is posted until the person has allowed notifications, so a poll never asks.
async function noteHidden(bots: any[], openSlug: string): Promise<void> {
  if (FIXTURE_SLUG(openSlug)) return;
  const hidden = bots.filter((b) => b && b.hidden === true && b.notifications !== false && !FIXTURE_SLUG(String(b.slug ?? '')));
  if (hidden.length === 0) return;
  if (!(await noticeGranted())) return;
  for (const bot of hidden) {
    const slug = String(bot.slug);
    const tag = `unread-${slug}`;
    if (slug === openSlug || bot.unread !== true) {
      await closeNotification({ tag });
      continue;
    }
    const line = lineOf(bot);
    const body = line === 'Ready to chat' || line === 'Getting set up…' || line === 'Working…' ? say('notice.hidden') : line;
    await showNotification({ title: String(bot.name || slug), body, tag, navigate: `/bot/${slug}` });
  }
}

const sources: Sources = {
  loadSession: async ([locale], store): Promise<Result<'loadSession'>> => {
    noteLocale(locale);
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
    // Reserved slugs for the contract runner, which has no gateway. A real inbox still fetches.
    // `bea` is the second fixture Bot, so a draft test can open her and come back without a gateway.
    if (focus === 'tabs') {
      return [
        { slug: 'tabs', name: 'Tabs', label: '', initials: 'T', color: fill('brand.default'), main: false, hidden: false, ready: true, unread: false, shape: 'ball', pinned: false, working: false, line: 'Ready to chat', time: '' },
      ];
    }
    if (focus === 'fixture' || focus === 'bea') {
      return [
        { slug: 'fixture', name: 'Ada', label: '', initials: 'A', color: fill('brand.default'), main: false, hidden: false, ready: true, unread: true, shape: 'ball', pinned: false, working: false, line: 'Ready to chat', time: '' },
        { slug: 'bea', name: 'Bea', label: '', initials: 'B', color: fill('brand.blue'), main: false, hidden: true, ready: false, unread: true, shape: 'ball', pinned: false, working: false, line: 'Getting set up…', time: '' },
      ];
    }
    const body = await botsBody(store);
    if (Number(body.now) > 0) lastNow = Number(body.now);
    const at = (b: any): string => (typeof b.preview?.at === 'string' ? b.preview.at : '');
    // The main Bot first, then the most recent conversation; a Bot with none goes last.
    const sorted = [...(body.bots as any[])].sort((a, b) => (b.primary === true ? 1 : 0) - (a.primary === true ? 1 : 0) || at(b).localeCompare(at(a)));
    try {
      await noteHidden(sorted, focus);
    } catch {
      // A missing notification host still lists the Bots.
    }
    return sorted.map((b) => ({
      slug: String(b.slug), name: String(b.name), label: String(b.label ?? ''), initials: initials(String(b.name)),
      color: fill(b.color), shape: String(b.shape ?? 'ball'), main: b.primary === true, pinned: b.pinned === true, hidden: b.hidden === true,
      ready: typeof b.mainThreadId === 'string', working: b.working === true, unread: b.unread === true,
      line: lineOf(b), time: when(b.preview?.at, Number(body.now)),
    }));
  },
  renameBot: async ([slug, name], store) => reply(await call(store, `/api/bots/${encodeURIComponent(slug)}`, { method: 'PUT', body: JSON.stringify({ name }) })),
  recolourBot: async ([slug, color], store) => reply(await call(store, `/api/bots/${encodeURIComponent(slug)}`, { method: 'PUT', body: JSON.stringify({ color }) })),
  hideBot: async ([slug, hidden], store) => reply(await call(store, `/api/bots/${encodeURIComponent(slug)}`, { method: 'PUT', body: JSON.stringify({ hidden }) })),
  makeMainBot: async ([slug], store) => reply(await call(store, '/api/main-bot', { method: 'PUT', body: JSON.stringify({ slug }) })),
  reshapeBot: async ([slug, shape], store) => reply(await call(store, `/api/bots/${encodeURIComponent(slug)}`, { method: 'PUT', body: JSON.stringify({ shape }) })),
  reactTo: async ([slug, item, emoji], store) => reply(await call(store, `/api/threads/${encodeURIComponent(threads[slug] ?? '')}/reactions`, { method: 'POST', body: JSON.stringify({ item, emoji }) })),
  pinMessage: async ([slug, thread, item, text], store) => {
    if (slug === 'fixture') {
      const key = thread || 'fixture-thread';
      const list = fixturePins[key] ?? (fixturePins[key] = []);
      const at = list.findIndex((row) => row.id === item);
      if (at >= 0) list.splice(at, 1);
      else list.push({ id: String(item), text: String(text || NOT_IN_VIEW), missing: false });
      return { ok: true, error: '' };
    }
    return reply(await call(store, `/api/threads/${encodeURIComponent(thread)}/pins`, {
      method: 'POST',
      body: JSON.stringify({ item }),
    }));
  },
  pinBot: async ([slug, pinned], store) => reply(await call(store, `/api/bots/${encodeURIComponent(slug)}`, { method: 'PUT', body: JSON.stringify({ pinned }) })),
  deleteBot: async ([slug], store) => reply(await call(store, `/api/bots/${encodeURIComponent(slug)}`, { method: 'DELETE' })),
  loadPrefs: async (_args, store): Promise<Result<'loadPrefs'>> => {
    const body = await read(store, '/api/settings');
    return { userName: String(body.userName ?? ''), theme: String(body.theme ?? 'system'), defaultInstance: String(body.defaultModel?.instanceId ?? ''), defaultModel: String(body.defaultModel?.model ?? '') };
  },
  setUserName: async ([userName], store) => reply(await call(store, '/api/settings', { method: 'PUT', body: JSON.stringify({ userName }) })),
  setTheme: async ([theme], store) => reply(await call(store, '/api/settings', { method: 'PUT', body: JSON.stringify({ theme }) })),
  setPassword: async ([password], store) => reply(await call(store, '/api/access/password', { method: 'PUT', body: JSON.stringify({ password }) })),
  loadVault: async (_args, store): Promise<Result<'loadVault'>> => {
    const body = await read(store, '/api/vault');
    return (body.items as any[]).filter((i) => i.type === 'login').map((i) => ({
      id: String(i.id), title: String(i.title ?? ''), website: String(i.website ?? ''), username: String(i.username ?? ''),
      hasPassword: i.hasPassword === true, hints: String(i.hints ?? ''),
      icon: ((h) => (h ? `${API}/api/icons/${h}` : ''))(/^(?:https?:\/\/)?([^/:?#\s]+\.[^/:?#\s]+)/i.exec(String(i.website ?? ''))?.[1] ?? ''),
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
  loadConvos: async ([query, focus, room], store): Promise<Result<'loadConvos'>> => {
    if (focus === 'tabs') {
      return [
        { kind: 'bot', key: 'bot-tabs', id: 'tabs', name: 'Tabs', color: fill('brand.default'), shape: 'ball', faces: [], line: 'Ready to chat', time: '', at: '', unread: false, working: false, ready: true },
      ];
    }
    if (focus === 'fixture' || focus === 'bea' || room === 'fixture') {
      return [
        { kind: 'bot', key: 'bot-fixture', id: 'fixture', name: 'Ada', color: fill('brand.default'), shape: 'ball', faces: [], line: 'Ready to chat', time: '', at: '', unread: true, working: false, ready: true },
        { kind: 'group', key: 'group-fixture', id: 'fixture', name: 'Design', color: 'brand.blue', shape: 'hex', faces: [{ shape: 'ball', color: fill('brand.default'), state: 'idle' }, { shape: 'ball', color: fill('brand.blue'), state: 'idle' }], line: 'Ada: On it', time: '', at: '', unread: false, working: false, ready: true },
      ];
    }
    const body = await botsBody(store);
    const now = Number(body.now);
    const directory = body.bots as any[];
    const at = (b: any): string => (typeof b.preview?.at === 'string' ? b.preview.at : '');
    const match = (...names: string[]): boolean => query === '' || names.some((n) => n.toLowerCase().includes(query));
    const bots = directory
      .filter((b) => b.hidden !== true && (query !== '' || !(b.primary === true || b.pinned === true)) && match(String(b.name), String(b.label ?? '')))
      .map((b) => ({
        kind: 'bot', key: `bot-${String(b.slug)}`, id: String(b.slug), name: String(b.name), color: fill(b.color), shape: String(b.shape ?? 'ball'), faces: [] as { shape: string; color: string; state: string }[],
        line: lineOf(b), time: when(b.preview?.at, now), at: at(b), unread: b.unread === true, working: b.working === true,
        ready: typeof b.mainThreadId === 'string',
      }));
    // A rooms failure must not hide the Bots. An empty list is the honest fallback.
    const roomsReply = await call(store, '/api/rooms');
    const listed = roomsReply.ok && Array.isArray(roomsReply.body?.rooms) ? roomsReply.body.rooms : [];
    const groups = listed
      .filter((g: any) => match(String(g.title ?? '')))
      .map((g: any) => {
        const raw = String(g.last?.text ?? '');
        const show = Boolean(g.last) && !isGroupLeak(raw);
        const last = show ? `${String(g.last.name ?? '')}: ${raw}`.slice(0, 120) : '';
        const said = typeof g.last?.at === 'string' ? g.last.at : '';
        return {
          kind: 'group', key: `group-${String(g.id)}`, id: String(g.id), name: String(g.title ?? ''), color: String(g.color ?? 'brand.default'), shape: String(g.shape ?? 'ball'),
          faces: stackFaces(g.members, directory),
          line: last !== '' ? last : String(g.description ?? ''), time: when(said, now), at: said, unread: false, working: false, ready: true,
        };
      });
    // The most recent conversation first, a Bot or a group alike; one with nothing said goes last.
    return [...bots, ...groups].sort((x, y) => y.at.localeCompare(x.at));
  },
  updateGroup: async ([id, field, value], store) => {
    const body = field === 'members' ? { members: value === '' ? [] : value.split('\n') } : { [field]: value };
    return reply(await call(store, `/api/rooms/${encodeURIComponent(id)}`, { method: 'PUT', body: JSON.stringify(body) }));
  },
  createGroup: async ([title, description, color, shape, members], store): Promise<Result<'createGroup'>> => {
    const slugs = members === '' ? [] : members.split('\n').filter((s) => s !== '');
    // The contract runner's Bots. A real room still goes to the gateway.
    if (slugs.length >= 2 && slugs.every((s) => s === 'fixture' || s === 'bea')) {
      fixtureMade = { title, description, color, shape, slugs };
      return { ok: true, id: 'fixture', error: '' };
    }
    const r = await call(store, '/api/rooms', { method: 'POST', body: JSON.stringify({ title, description, color, shape, members: slugs }) });
    return r.ok ? { ok: true, id: String(r.body.id ?? ''), error: '' } : { ok: false, id: '', error: r.error };
  },
  sendRoom: async ([id, text], store) => reply(await call(store, `/api/rooms/${encodeURIComponent(id)}/messages`, { method: 'POST', body: JSON.stringify({ text }) })),
  deleteGroup: async ([id], store) => reply(await call(store, `/api/rooms/${encodeURIComponent(id)}`, { method: 'DELETE' })),
  createBot: async ([name, color, shape, firstMessage], store) => reply(await call(store, '/api/bots', { method: 'POST', body: JSON.stringify({ name, color, shape, ...(firstMessage !== '' ? { firstMessage } : {}) }) })),
  loadProviders: async (_args, store): Promise<Result<'loadProviders'>> => {
    const p = await call(store, '/api/providers');
    return (p.ok && Array.isArray(p.body.providers) ? p.body.providers : []).map((x: any) => ({
      instanceId: String(x.instanceId), name: String(x.displayName ?? x.instanceId), account: String(x.account ?? ''),
      models: ((x.models ?? []) as any[]).map((o) => ({
        slug: String(o.slug), name: String(o.name ?? o.slug), recommended: o.recommended === true, effort: String(o.effort ?? ''),
        efforts: ((o.efforts ?? []) as any[]).map((e) => ({ id: String(e.id), label: String(e.label ?? e.id) })),
      })),
    }));
  },
  loadAbout: async (_args, store): Promise<Result<'loadAbout'>> => {
    const r = await call(store, '/api/config');
    if (!r.ok) return { ok: false, version: '', harness: '', connected: false };
    return { ok: true, version: String(r.body.version ?? ''), harness: String(r.body.harness?.name ?? 'T3 Code'), connected: r.body.harness?.connected === true };
  },
  loadSuggestions: async (_args, store): Promise<Result<'loadSuggestions'>> => {
    const r = await call(store, '/api/suggestions');
    if (!r.ok) return [];
    return ((r.body.suggestions ?? []) as any[]).map((s) => ({ name: String(s.name), about: String(s.about ?? ''), color: fill(s.color), shape: String(s.shape ?? 'ball'), first: String(s.firstMessage ?? '') }));
  },
  markSeen: async ([slug], store) => reply(await call(store, `/api/bots/${encodeURIComponent(slug)}/seen`, { method: 'POST', body: '{}' })),
  loadChat: async ([slug, child], store): Promise<Result<'loadChat'>> => {
    if (slug === '') return blankChat('');
    if (slug === 'fixture') return fixtureChat(child);
    if (slug === 'bea') return blankChat('bea');
    if (slug === 'tabs') return blankChat('tabs');
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
    if (child === '') threads[slug] = threadId;
    const rows = chatRows(slug, body, Number(body.now), meta.reactions ?? {}, meta.replyTo ?? {});
    if (child === '') await closeNotice(slug);
    const pins: PinRow[] = Array.isArray(meta.pins)
      ? meta.pins.map((id: unknown) => {
        const key = String(id);
        const hit = rows.find((row) => row.id === key);
        return hit ? { id: key, text: hit.text, missing: false } : { id: key, text: NOT_IN_VIEW, missing: true };
      })
      : [];
    return {
      slug, rows, working: body.working === true, partial: body.partial === true,
      readOnly: body.readOnly === true, pins, threadId,
    };
  },
  respondRequest: async ([slug, id, kind, key, value, child], store) => {
    const body: Record<string, unknown> = kind === 'secret_request'
      ? (value === '' ? { kind, decision: 'decline' } : { kind, secret: value })
      : kind === 'user_input_request' ? { kind, answers: { [key]: value } } : { kind, decision: value };
    if (child !== '') body.child = child;
    return reply(await call(store, `/api/bots/${encodeURIComponent(slug)}/requests/${encodeURIComponent(id)}/respond`, { method: 'POST', body: JSON.stringify(body) }));
  },
  sendMessage: async ([slug, text, pending, child, replyTo, quote], store) => {
    const attachmentIds = String(pending ?? '')
      .split('\n')
      .map((line) => line.split('\t')[0]?.trim())
      .filter((id) => !!id);
    const body: Record<string, unknown> = { text, attachmentIds };
    if (child !== '') body.child = child;
    if (replyTo !== '') {
      body.replyTo = replyTo;
      body.quote = String(quote ?? '').slice(0, 500);
    }
    return reply(await call(store, `/api/bots/${encodeURIComponent(slug)}/chat`, {
      method: 'POST',
      body: JSON.stringify(body),
    }));
  },
  uploadAttachment: async ([slug, path, rawName, rawType, locale], store, storage): Promise<Result<'uploadAttachment'>> => {
    noteLocale(locale);
    const saved = await storeUpload(store, storage, slug, path, rawName, rawType);
    return { ok: saved.ok, id: saved.id, name: saved.name, error: saved.error };
  },
  stageVoice: async ([slug, path, mediaType, seconds], store, storage) => {
    const saved = await storeUpload(store, storage, slug, path, '', mediaType);
    const secs = Number(seconds);
    if (!saved.ok) return { ok: false, id: '', name: '', url: '', seconds: 0, error: saved.error };
    return { ok: true, id: saved.id, name: saved.name, url: saved.url, seconds: Number.isFinite(secs) ? secs : 0, error: '' };
  },
  sendVoice: async ([slug, seconds, attachmentId, child], store) => {
    const body: Record<string, unknown> = { seconds: Number(seconds), attachmentId };
    if (child !== '') body.child = child;
    return reply(await call(store, `/api/bots/${encodeURIComponent(slug)}/voice`, {
      method: 'POST',
      body: JSON.stringify(body),
    }));
  },
  dropVoice: async ([slug, id], store) => reply(await call(store, `/api/bots/${encodeURIComponent(slug)}/attachments/${encodeURIComponent(id)}`, { method: 'DELETE' })),
  stopRun: async ([slug, child], store) => {
    const body: Record<string, unknown> = {};
    if (child !== '') body.child = child;
    return reply(await call(store, `/api/bots/${encodeURIComponent(slug)}/stop`, { method: 'POST', body: JSON.stringify(body) }));
  },
  loadUsage: async ([gate, locale], store): Promise<Result<'loadUsage'>> => {
    noteLocale(locale);
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
  loadModels: async ([slug, gate, locale], store): Promise<Result<'loadModels'>> => {
    noteLocale(locale);
    const none = { ok: true, error: '', current: '', instanceId: '', effort: '', currentName: '', providerName: '', effortLabel: '', efforts: [], providers: [] };
    if (slug === '' || gate !== '1') return none;
    const [m, p] = await Promise.all([call(store, `/api/bots/${encodeURIComponent(slug)}/model`), call(store, '/api/providers')]);
    if (!m.ok) return { ...none, ok: false, error: explain(m.error, 'models') };
    const providers = (p.ok && Array.isArray(p.body.providers) ? p.body.providers : []).map((x: any) => ({
      instanceId: String(x.instanceId), name: String(x.displayName ?? x.instanceId), account: String(x.account ?? ''),
      models: ((x.models ?? []) as any[]).map((o) => ({
        slug: String(o.slug), name: String(o.name ?? o.slug), recommended: o.recommended === true, effort: String(o.effort ?? ''),
        efforts: ((o.efforts ?? []) as any[]).map((e) => ({ id: String(e.id), label: String(e.label ?? e.id) })),
      })),
    }));
    const instanceId = String(m.body.instanceId ?? '');
    const current = String(m.body.current ?? '');
    const effort = String(m.body.effort ?? '');
    const prov = providers.find((x: any) => x.instanceId === instanceId);
    const mod = prov?.models.find((o: any) => o.slug === current);
    return {
      ok: true, error: p.ok ? '' : explain(p.error, 'providers'), current, instanceId, effort,
      currentName: mod?.name ?? prettyModel(current), providerName: prov?.name ?? instanceId,
      effortLabel: mod?.efforts.find((e: any) => e.id === effort)?.label ?? '', efforts: mod?.efforts ?? [], providers,
    };
  },
  setModel: async ([slug, instanceId, model, effort], store) => reply(await call(store, `/api/bots/${encodeURIComponent(slug)}/model`, {
    method: 'PUT', body: JSON.stringify({ instanceId, model, ...(effort !== '' ? { effort } : {}) }),
  })),
  setDefaultModel: async ([instanceId, model, effort], store) => reply(await call(store, '/api/settings', {
    method: 'PUT', body: JSON.stringify({ defaultModel: { instanceId, model, ...(effort !== '' ? { effort } : {}) } }),
  })),
  loadRoutines: async ([slug, gate, locale], store): Promise<Result<'loadRoutines'>> => {
    noteLocale(locale);
    if (slug === '' || gate !== '1') return { ok: true, tasks: [], error: '' };
    const r = await call(store, `/api/bots/${encodeURIComponent(slug)}/routines`);
    if (!r.ok) {
      if (r.error.includes('no conversation') || r.error.includes('getting set up')) {
        return { ok: true, tasks: [], error: say('error.settingUp') };
      }
      return { ok: false, tasks: [], error: explain(r.error, 'routines') };
    }
    const tasks = Array.isArray(r.body.tasks) ? r.body.tasks : [];
    const now = lastNow;
    return {
      ok: true,
      error: String(r.body.error ?? ''),
      tasks: tasks.map((t: any) => ({
        id: String(t.id ?? ''),
        title: String(t.title ?? ''),
        paused: t.paused === true,
        schedule: String(t.schedule ?? ''),
        next: t.paused === true ? 'Paused' : nextLabel(t.nextRunAt, now),
        failed: t.lastStatus === 'failed',
      })),
    };
  },
  loadLineage: async ([slug, gate, locale], store): Promise<Result<'loadLineage'>> => {
    noteLocale(locale);
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
        return { ok: true, rows: [], error: say('error.settingUp') };
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
  loadShelf: async ([slug, gate], store): Promise<Result<'loadShelf'>> => {
    const none = { ok: true, total: 0, pictures: [], files: [], links: [] };
    if (slug === '' || gate !== '1') return none;
    const body = await read(store, `/api/bots/${encodeURIComponent(slug)}/shelf`);
    const item = (x: any) => ({ id: String(x.id), title: String(x.title ?? ''), detail: String(x.detail ?? ''), url: String(x.url ?? '').startsWith('/') ? `${API}${x.url}` : String(x.url ?? '') });
    const list = (k: string) => ((body[k] ?? []) as any[]).map(item);
    return { ok: true, total: Number(body.counts?.pictures ?? 0) + Number(body.counts?.files ?? 0) + Number(body.counts?.links ?? 0), pictures: list('pictures'), files: list('files'), links: list('links') };
  },
  loadComputer: async ([slug, gate, locale, tab], store): Promise<Result<'loadComputer'>> => {
    noteLocale(locale);
    const none = { ok: true, tabs: [] as { id: string; title: string; url: string }[], image: '', error: '', selected: '' };
    if (slug === '' || gate !== '1') return none;
    // Two server pages for the contract runner. A real Bot still asks the gateway.
    if (slug === 'tabs') {
      const selected = tab === 'two' ? 'two' : 'one';
      return {
        ok: true,
        selected,
        image: '',
        error: '',
        tabs: [
          { id: 'one', title: 'One', url: 'https://one.example' },
          { id: 'two', title: 'Two', url: 'https://two.example' },
        ],
      };
    }
    const kept = frames[slug] ?? '';
    const [tag, image] = kept === '' ? ['', ''] : [kept.slice(0, kept.indexOf(':')), kept.slice(kept.indexOf(':') + 1)];
    const since = tag !== '' ? `since=${encodeURIComponent(tag)}` : '';
    const picked = tab !== '' ? `tab=${encodeURIComponent(tab)}` : '';
    const query = [since, picked].filter((part) => part !== '').join('&');
    const r = await call(store, `/api/bots/${encodeURIComponent(slug)}/computer${query !== '' ? `?${query}` : ''}`);
    if (!r.ok) {
      if (r.error.includes('no conversation') || r.error.includes('getting set up')) {
        return { ok: true, tabs: [], image: '', error: say('error.settingUp'), selected: '' };
      }
      throw new Error(r.error);
    }
    const tabs = Array.isArray(r.body.tabs) ? r.body.tabs : [];
    // An unchanged frame is not sent again: keep the one the app has.
    const shown = r.body.unchanged === true ? image : String(r.body.image ?? '');
    if (typeof r.body.etag === 'string' && shown !== '') frames[slug] = `${r.body.etag}:${shown}`;
    const listed = tabs.map((t: any) => ({ id: String(t.id ?? ''), title: String(t.title ?? ''), url: String(t.url ?? '') }));
    return {
      ok: true,
      image: shown,
      error: String(r.body.error ?? ''),
      tabs: listed,
      selected: String(r.body.selected ?? listed[0]?.id ?? ''),
    };
  },
  loadUsageReport: async ([slug, period, gate, locale], store): Promise<Result<'loadUsageReport'>> => {
    noteLocale(locale);
    if (gate !== '1') return emptyReport('');
    const now = lastNow;
    const offset = now > 0 ? -new Date(now).getTimezoneOffset() : 0;
    const r = await call(store, `/api/usage/report?period=${encodeURIComponent(period)}&bot=${encodeURIComponent(slug === '' ? 'all' : slug)}&offset=${offset}`);
    if (!r.ok) return emptyReport(explain(r.error, 'usage'));
    return usageReport(r.body);
  },
  computerControl: async ([slug, tabId, action], store) => reply(await call(store, `/api/bots/${encodeURIComponent(slug)}/computer/control`, {
    method: 'POST', body: JSON.stringify({ tabId, action }),
  })),
  loadRoom: async ([id], store): Promise<Result<'loadRoom'>> => {
    if (id === '') return { id: '', title: '', description: '', color: 'brand.default', shape: 'ball', members: [], messages: [] };
    if (id === 'fixture') {
      if (fixtureMade) {
        const nameOf = (slug: string) => (slug === 'bea' ? 'Bea' : 'Ada');
        return {
          id: 'fixture', title: fixtureMade.title, description: fixtureMade.description, color: fixtureMade.color, shape: fixtureMade.shape,
          members: fixtureMade.slugs.map((slug) => ({ slug, name: nameOf(slug), line: 'Ready to chat', state: 'idle', ready: true, working: false })),
          messages: [],
        };
      }
      return {
        id: 'fixture', title: 'Design', description: 'Where Ada and Bea trade drafts', color: 'brand.blue', shape: 'hex',
        members: [
          { slug: 'ada', name: 'Ada', line: 'Working', state: 'working', ready: true, working: true },
          { slug: 'bea', name: 'Bea', line: 'Getting set up…', state: 'asleep', ready: false, working: false },
        ],
        messages: [
          { id: 'r1', name: 'You', text: 'Hello', from: 'user' },
          { id: 'r2', name: 'Ada', text: 'On it', from: 'ada' },
        ],
      };
    }
    const body = await read(store, `/api/rooms/${encodeURIComponent(id)}`);
    const members = Array.isArray(body.members) ? body.members : [];
    const messages = Array.isArray(body.messages) ? body.messages : [];
    return {
      id: String(body.id ?? id),
      title: String(body.title ?? ''), description: String(body.description ?? ''),
      color: String(body.color ?? 'brand.default'), shape: String(body.shape ?? 'ball'),
      members: members.map((m: any) => ({
        slug: String(m.slug ?? ''), name: String(m.name ?? ''), line: String(m.line ?? ''),
        state: String(m.state ?? 'idle'), ready: m.ready === true, working: m.working === true,
      })),
      messages: messages.map((m: any) => ({ id: String(m.id ?? ''), name: String(m.name ?? ''), text: String(m.text ?? ''), from: String(m.from ?? '') })),
    };
  },
  loadDrafts: async (_args, store) => {
    try {
      return draftRows(store.get('drafts'));
    } catch {
      return { stamp: '', texts: [], files: [], voices: [] };
    }
  },
  saveDrafts: async ([texts, files, voices], store) => {
    try {
      store.set('drafts', JSON.stringify(draftMap(texts, files, voices)));
    } catch {
      // The screen still holds the draft when the secret store is missing.
    }
    return { ok: true, error: '' };
  },
};
export const answer: Answer = (source, args, store, storage, native) => sources[source](args, store, storage, native);
