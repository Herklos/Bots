import type { Answer, Result, Sources, Storage } from './app.contract.d.ts';
import { closeNotification, notificationStatus, setAppBadge, showNotification } from '@exact/notifications';
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
// A framed prompt that leaked into a Bot's own thread (group dispatch or Computer open).
// Same needles as the gateway transcript filter for silent turns.
function isGroupLeak(text: string): boolean {
  return text.trim().startsWith('[Group "')
    || text.includes('\n[Group "')
    || text.includes('Your reply is shared with the group')
    || text.trim().startsWith('[Computer]')
    || text.includes('\n[Computer]');
}
// The one line under a Bot's name in the inbox.
function lineOf(b: any): string {
  if (b.working === true) return 'Working…';
  const p = b.preview;
  if (p && typeof p.text === 'string' && p.text !== '' && !isGroupLeak(p.text)) return p.role === 'user' ? `You: ${p.text}` : p.text;
  return typeof b.mainThreadId === 'string' ? 'Ready to chat' : 'Getting set up…';
}
// A group's face is its members, at most three. The list stores slugs or member objects.
function stackFaces(members: unknown, directory: any[], workingSlugs: string[]): { shape: string; color: string; state: string }[] {
  const working = new Set(workingSlugs);
  const slugs = (Array.isArray(members) ? members : [])
    .map((m) => (typeof m === 'string' ? m : String((m as { slug?: unknown })?.slug ?? '')))
    .filter((slug) => slug !== '')
    .slice(0, 3);
  return slugs.map((slug) => {
    const bot = directory.find((b) => String(b.slug) === slug);
    return { shape: String(bot?.shape ?? 'ball'), color: fill(bot?.color), state: working.has(slug) ? 'working' : 'idle' };
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
  computerOpen: 'error.computerOpen',
  computerClose: 'error.computerClose',
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
// The conversation list and the unread poster both read rooms in one tick. The settled
// list stays briefly so the poster, which runs just after the roster changes, reuses it.
let roomsInflight: Promise<any> | null = null;
function roomsBody(store: any): Promise<any> {
  if (!roomsInflight) {
    const run = call(store, '/api/rooms').finally(() => {
      setTimeout(() => { if (roomsInflight === run) roomsInflight = null; }, 100);
    });
    roomsInflight = run;
  }
  return roomsInflight;
}
// The host's notification permission. A tick reads the notice resource; this remembers the last answer.
let noticeHost: { available: boolean; state: string } | null = null;
let noticeHostInflight: Promise<{ available: boolean; state: string } | null> | null = null;
async function hostNotice(): Promise<{ available: boolean; state: string } | null> {
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
const reply = (r: Answered) => (r.ok ? { ok: true, error: '' } : { ok: false, error: localize(r.error) });
// The first read on the About tab asks GitHub. Later refreshes use the stored answer.
let systemChecked = false;

// POST /api/bots returns the saved Bot. `reply` keeps only ok and error, which drops slug.
function madeFromBot(r: Answered): { ok: boolean; id: string; error: string } {
  if (!r.ok) return { ok: false, id: '', error: localize(r.error) };
  const slug = typeof r.body?.slug === 'string' ? r.body.slug : '';
  return { ok: true, id: slug, error: '' };
}

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

type TableAlign = 'left' | 'center' | 'right';
type MessagePart = { kind: 'text' | 'table'; text: string; head: string[]; rows: string[][]; align: TableAlign[] };

function textPart(text: string): MessagePart {
  return { kind: 'text', text, head: [], rows: [], align: [] };
}

function fenceMark(trimmed: string): { ch: string; n: number } | null {
  const ch = trimmed[0];
  if (ch !== '`' && ch !== '~') return null;
  let n = 0;
  while (trimmed[n] === ch) n += 1;
  if (n < 3) return null;
  return { ch, n };
}

// Block-quote depth, the same gate Exact uses: a table starts only at quote == 0.
function quoteDepth(line: string): number {
  const trimmed = line.trimStart();
  let depth = 0;
  let i = 0;
  while (trimmed[i] === '>') {
    depth += 1;
    i += 1;
    if (trimmed[i] === ' ') i += 1;
  }
  return depth;
}

function isRule(line: string | undefined): boolean {
  if (line === undefined) return false;
  const trimmed = line.trim();
  if (!trimmed.includes('-')) return false;
  for (const c of trimmed) {
    if (c !== '|' && c !== '-' && c !== ':' && c !== ' ') return false;
  }
  return true;
}

function cellsOf(line: string): string[] {
  const body = line.trim();
  const cells: string[] = [];
  let cur = '';
  let escaped = false;
  for (const c of body) {
    if (escaped) {
      if (c !== '|') cur += '\\';
      cur += c;
      escaped = false;
      continue;
    }
    if (c === '\\') {
      escaped = true;
      continue;
    }
    if (c === '|') {
      cells.push(cur);
      cur = '';
      continue;
    }
    cur += c;
  }
  if (escaped) cur += '\\';
  cells.push(cur);
  if (cells.length > 0 && cells[0]!.trim() === '' && body.startsWith('|')) cells.shift();
  if (cells.length > 0 && cells[cells.length - 1]!.trim() === '' && body.endsWith('|')) cells.pop();
  return cells.map((cell) => cell.trim());
}

function alignsOf(rule: string, width: number): TableAlign[] {
  const cells = cellsOf(rule);
  const align: TableAlign[] = [];
  for (let i = 0; i < width; i++) {
    const cell = cells[i] ?? '';
    const left = cell.startsWith(':');
    const right = cell.endsWith(':');
    align.push(left && right ? 'center' : right ? 'right' : 'left');
  }
  return align;
}

// A pipe table is a line containing `|` whose next line is a rule of `|`, `-`, `:`, and
// spaces, with at least one `-`. It ends at the first blank or pipe-less line. A header
// with no rule yet stays text, so a half-arrived message does not flash a grid.
export function splitTables(text: string): MessagePart[] {
  const lines = text.split('\n');
  const parts: MessagePart[] = [];
  const buf: string[] = [];
  let saw = false;
  let fence: { ch: string; n: number } | null = null;
  const pushText = () => {
    if (buf.length === 0) return;
    const chunk = buf.join('\n');
    buf.length = 0;
    if (chunk.trim() === '') return;
    parts.push(textPart(chunk));
  };
  for (let i = 0; i < lines.length; i++) {
    const line = lines[i] ?? '';
    const trimmed = line.trim();
    const mark = fenceMark(trimmed);
    if (fence) {
      if (mark && mark.ch === fence.ch && mark.n >= fence.n && trimmed === mark.ch.repeat(mark.n)) fence = null;
      buf.push(line);
      continue;
    }
    if (mark) {
      fence = mark;
      buf.push(line);
      continue;
    }
    const next = lines[i + 1];
    if (quoteDepth(line) === 0 && line.includes('|') && isRule(next)) {
      pushText();
      const head = cellsOf(line);
      const rows: string[][] = [];
      let j = i + 2;
      while (j < lines.length) {
        const rowLine = lines[j] ?? '';
        if (rowLine.trim() === '' || !rowLine.includes('|') || quoteDepth(rowLine) !== 0) break;
        rows.push(cellsOf(rowLine));
        j += 1;
      }
      parts.push({ kind: 'table', text: '', head, rows, align: alignsOf(next ?? '', head.length) });
      saw = true;
      i = j - 1;
      continue;
    }
    buf.push(line);
  }
  if (!saw) return [textPart(text)];
  pushText();
  return parts.length > 0 ? parts : [textPart(text)];
}

function roomMessage(m: any) {
  const from = String(m.from ?? '');
  const text = String(m.text ?? '');
  return { id: String(m.id ?? ''), name: String(m.name ?? ''), text, from, parts: from === 'user' ? [] : splitTables(text) };
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
// Mute choices the contract runner makes. Missing means the chat still notifies.
const fixtureBotNotify = new Map<string, boolean>();
const fixtureRoomNotify = new Map<string, boolean>();
const wantsNotice = (value: unknown): boolean => value !== false;

// The model page's fixture: five levels in the order a provider would send them.
const FIXTURE_EFFORTS = [
  { id: 'low', label: 'Low' },
  { id: 'medium', label: 'Medium' },
  { id: 'high', label: 'High' },
  { id: 'xhigh', label: 'Extra high' },
  { id: 'max', label: 'Max' },
];

let fixtureChoice = { instanceId: 'claudeAgent', model: 'haiku', effort: 'high', runtimeMode: 'auto' };

function fixtureProviders() {
  const efforts = () => FIXTURE_EFFORTS.map((level) => ({ ...level }));
  return [{
    instanceId: 'claudeAgent',
    name: 'Claude',
    account: '',
    models: [
      { slug: 'haiku', name: 'Haiku', recommended: true, effort: 'medium', efforts: efforts() },
      { slug: 'sonnet', name: 'Sonnet', recommended: false, effort: 'medium', efforts: efforts() },
    ],
  }];
}

function fixtureModelList() {
  const providers = fixtureProviders();
  const prov = providers.find((p) => p.instanceId === fixtureChoice.instanceId) ?? providers[0]!;
  const mod = prov.models.find((m) => m.slug === fixtureChoice.model) ?? prov.models[0]!;
  const effort = mod.efforts.some((level) => level.id === fixtureChoice.effort) ? fixtureChoice.effort : '';
  return {
    ok: true, error: '', current: mod.slug, instanceId: prov.instanceId, effort, runtimeMode: fixtureChoice.runtimeMode,
    currentName: mod.name, providerName: prov.name,
    effortLabel: mod.efforts.find((level) => level.id === effort)?.label ?? '',
    efforts: mod.efforts, providers,
  };
}

function rememberFixtureModel(instanceId: string, model: string, effort: string, mode: string): { ok: boolean; error: string } {
  const prov = fixtureProviders().find((p) => p.instanceId === instanceId);
  const mod = prov?.models.find((row) => row.slug === model);
  if (!prov || !mod) return { ok: false, error: 'That model is not available.' };
  const runtimeMode = mode === 'approval-required' || mode === 'auto' || mode === 'full-access' ? mode : fixtureChoice.runtimeMode;
  fixtureChoice = { instanceId, model, effort: mod.efforts.some((level) => level.id === effort) ? effort : '', runtimeMode };
  return { ok: true, error: '' };
}

function savedEffort(body: any): string {
  if (typeof body?.effort === 'string' && body.effort !== '') return body.effort;
  const model = body?.defaultModel;
  if (!model || typeof model !== 'object') return '';
  if (typeof model.effort === 'string' && model.effort !== '') return model.effort;
  const option = Array.isArray(model.options) ? model.options.find((row: any) => row && (row.id === 'effort' || row.id === 'reasoningEffort')) : undefined;
  return typeof option?.value === 'string' ? option.value : '';
}

const fixturePins: Record<string, PinRow[]> = {
  'fixture-thread': [
    { id: 'm1', text: 'Hello', missing: false },
    { id: 'missing', text: NOT_IN_VIEW, missing: true },
  ],
  'child-thread': [],
};

function fixtureChat(child: string) {
  const row = (r: Record<string, unknown>) => {
    const text = String(r.text ?? '');
    const mine = r.mine === true;
    return { ...blank, head: true, tail: true, mine: false, text: '', ...r, kind: 'message', parts: mine ? [] : splitTables(text) } as ChatRow;
  };
  if (child === 'child-thread') {
    return { slug: 'fixture', working: false, partial: false, readOnly: false, threadId: 'child-thread', pins: [...(fixturePins['child-thread'] ?? [])], rows: [row({ id: 'c1', text: 'Notes' })] };
  }
  if (child === 'child-readonly') {
    return { slug: 'fixture', working: false, partial: false, readOnly: true, threadId: 'child-readonly', pins: [], rows: [row({ id: 'c-ro', text: 'Finished work' })] };
  }
  return {
    slug: 'fixture', working: false, partial: true, readOnly: false, threadId: 'fixture-thread',
    pins: [...(fixturePins['fixture-thread'] ?? [])],
    rows: [
      row({ id: 'm1', mine: true, text: 'Hello', tail: false }),
      row({ id: 'm2', time: '9:00 AM', text: 'Hi there', react: '🔥', quote: 'Hello', tokens: '12 in, 3 out, 4 read from cache, 1 written to cache.', head: true }),
      row({ id: 'm3', time: '9:01 AM', text: 'No tokens here', head: false }),
      row({ id: 'm4', time: '9:02 AM', text: '| Item | Count |\n| :--- | ---: |\n| apple | 2 |', head: true }),
      row({ id: 'voice1', mine: true, files: [{ id: 'voice', name: 'voice.m4a', kind: 'audio', size: '1 KB', url: '/api/bots/fixture/attachments/voice' }] }),
      { ...blank, id: 'e1', kind: 'event', mine: false, text: 'Created routine', title: 'Morning', head: true, tail: true, variant: 'routine-created' },
      { ...blank, id: 'e2', kind: 'event', mine: false, text: 'Messaged', botName: 'Inbox', botSlug: 'inbox', botShape: 'hex', botColor: 'brand.pink', head: true, tail: true, variant: 'sent' },
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
// Reactions and pins for a thread. A poll reuses them for about ten seconds, or until the last message changes.
// A reaction or a pin clears the cache so that refresh reads them again.
const META_MS = 10_000;
const metaByThread = new Map<string, { at: number; lastId: string; meta: any }>();
function forgetMeta(): void {
  metaByThread.clear();
}
function lastMessageId(body: any): string {
  const messages = Array.isArray(body.messages) ? body.messages : [];
  let id = '';
  let ordinal = -1;
  for (const message of messages) {
    const next = Number(message?.ordinal ?? -1);
    if (id === '' || next >= ordinal) {
      ordinal = Number.isFinite(next) ? next : ordinal;
      id = String(message?.id ?? '');
    }
  }
  return id;
}
async function threadMeta(store: any, threadId: string, lastId: string, now: number): Promise<any> {
  const hit = metaByThread.get(threadId);
  if (hit && hit.lastId === lastId && now - hit.at < META_MS) return hit.meta;
  try {
    const meta = await read(store, `/api/threads/${encodeURIComponent(threadId)}/meta`);
    metaByThread.set(threadId, { at: now, lastId, meta });
    return meta;
  } catch {
    return {};
  }
}
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
const blank = { quote: '', tokens: '', react: '', key: '', time: '', title: '', files: [] as any[], botSlug: '', botName: '', botColor: 'brand.default', botShape: 'ball', variant: '', choices: [] as any[], placeholder: '', resetAt: '', parts: [] as MessagePart[] };

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
  // A framed group user message, then the Bot's answer, stays out until the next ordinary user message.
  let hideGroup = false;
  for (const item of timeline) {
    if (item._t === 'message' && item.role === 'user') hideGroup = isGroupLeak(String(item.text ?? ''));
    if (hideGroup) continue;
    const t = Date.parse(String(item.at ?? ''));
    if (now > 0 && Number.isFinite(t) && (last === 0 || t - last > GAP || dayStart(t) !== dayStart(last))) {
      rows.push({ ...blank, id: `day-${item.id}`, kind: 'day', mine: false, text: dayLabel(String(item.at), now), head: true, tail: true });
    }
    if (Number.isFinite(t)) last = t;
    if (item._t === 'event') {
      const reset = typeof item.failure?.resetAt === 'string' ? item.failure.resetAt : '';
      const resetAt = reset !== '' && Number.isFinite(Date.parse(reset)) ? clock(new Date(reset)) : '';
      rows.push({ ...blank, ...peer(item.bot), id: String(item.id), kind: 'event', mine: false, text: String(item.text ?? ''), title: String(item.title ?? ''), head: true, tail: true, variant: String(item.kind ?? ''), ...(resetAt !== '' ? { resetAt } : {}) });
      prev = null;
      continue;
    }
    const mine = item.role === 'user' && !item.from;
    const who = `${mine}:${item.from?.slug ?? ''}`;
    const files = ((item.files ?? []) as any[]).map((f) => ({
      id: String(f.id), name: String(f.name), kind: String(f.kind), size: fileSize(Number(f.bytes ?? 0)),
      url: `${API}/api/bots/${encodeURIComponent(slug)}/attachments/${encodeURIComponent(String(f.id))}`,
    }));
    const text = String(item.text ?? '');
    const row = { ...blank, ...peer(item.from), id: String(item.id), kind: 'message', mine, text, parts: mine ? [] : splitTables(text), react: ((reactions[String(item.id)] ?? []) as string[]).join(' '), quote: quoteOf(replyTo, String(item.id)), tokens: tokenText(item.tokens), files, head: !(prev && prev.who === who && t - prev.t < 5 * 60_000), tail: true,
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
const FIXTURE_SLUG = (slug: string): boolean => slug === 'fixture' || slug === 'bea' || slug === 'tabs' || slug === 'frame';

const STATUS_LINE = (line: string): boolean => line === '' || line === 'Ready to chat' || line === 'Getting set up…' || line === 'Working…';

// The last message id posted for a tag. The same id does not post again.
const noticeIds = new Map<string, string>();

type NoticePrefs = { enabled: boolean | null; sound: boolean; preview: 'full' | 'hidden'; when: 'any' | 'background' };

// A missing `enabled` is null. The poster treats that as on only when permission is already granted.
function noticePrefs(body: any): NoticePrefs {
  const note = body?.notifications;
  return {
    enabled: typeof note?.enabled === 'boolean' ? note.enabled : null,
    sound: note?.sound !== false,
    preview: note?.preview === 'hidden' ? 'hidden' : 'full',
    when: note?.when === 'background' ? 'background' : 'any',
  };
}

function unsupported(error: unknown): boolean {
  const code = (error as { code?: string; name?: string }).code ?? (error as { name?: string }).name;
  return code === 'NotSupportedError';
}

async function shutNotice(tag: string): Promise<void> {
  noticeIds.delete(tag);
  try {
    await closeNotification({ tag });
    if (tag.startsWith('unread-bot-')) await closeNotification({ tag: `unread-${tag.slice('unread-bot-'.length)}` });
  } catch {
    // The list still loads when the host cannot close a notice.
  }
}

// One notice per unread chat, when the roster changes. A subagent, a fixture, a muted
// chat, the open route, a status line and the person's own message stay quiet.
// Permission and settings are the notice and prefs resources, not another settings read.
async function postUnread(store: { get(name: string): string | null }, openSlug: string, openRoom: string, visibility: string, focused: boolean, host: { available: boolean; state: string }, prefs: NoticePrefs): Promise<void> {
  if (host.available !== true) return;
  const granted = host.state === 'granted';
  const master = prefs.enabled === null ? granted : prefs.enabled;
  const away = visibility === 'hidden' || focused === false;
  const mayPost = granted && master && (prefs.when !== 'background' || away);
  const body = await botsBody(store);
  const directory = Array.isArray(body.bots) ? body.bots : [];
  const roomsReply = await roomsBody(store);
  const rooms = roomsReply.ok && Array.isArray(roomsReply.body?.rooms) ? roomsReply.body.rooms : [];
  type Chat = { tag: string; title: string; line: string; navigate: string; id: string; close: boolean; post: boolean };
  const chats: Chat[] = [];
  let badge = 0;
  for (const bot of directory) {
    if (!bot || bot.hideFromSidebar === true || bot.role === 'subagent') continue;
    const slug = String(bot.slug ?? '');
    if (slug === '' || FIXTURE_SLUG(slug)) continue;
    const tag = `unread-bot-${slug}`;
    const line = lineOf(bot);
    const own = bot.preview?.role === 'user' || line.startsWith('You: ');
    const unread = bot.unread === true;
    const muted = !wantsNotice(bot.notifications);
    if (unread && !muted) badge += 1;
    const messageId = String(bot.preview?.id ?? bot.preview?.at ?? '');
    const close = muted || !unread || slug === openSlug;
    const quiet = own || STATUS_LINE(line) || messageId === '';
    chats.push({
      tag, title: String(bot.name || slug), line, navigate: `/bot/${slug}`, id: messageId, close,
      post: mayPost && !close && !quiet,
    });
  }
  for (const room of rooms) {
    const id = String(room?.id ?? '');
    if (id === '' || FIXTURE_SLUG(id)) continue;
    const tag = `unread-group-${id}`;
    const raw = String(room.last?.text ?? '');
    const show = Boolean(room.last) && !isGroupLeak(raw);
    const line = show ? `${String(room.last?.name ?? '')}: ${raw}`.slice(0, 120) : '';
    const own = room.last?.from === 'user';
    const unread = room.unread === true;
    const muted = !wantsNotice(room.notifications);
    if (unread && !muted) badge += 1;
    const messageId = show ? String(room.last?.id ?? `${room.last?.from ?? ''}:${room.last?.at ?? ''}`) : '';
    const close = muted || !unread || id === openRoom;
    const quiet = own || STATUS_LINE(line) || messageId === '';
    chats.push({
      tag, title: String(room.title ?? id), line, navigate: `/group/${id}`, id: messageId, close,
      post: mayPost && !close && !quiet,
    });
  }
  const live = new Set(chats.map((chat) => chat.tag));
  for (const tag of noticeIds.keys()) {
    if (!live.has(tag)) await shutNotice(tag);
  }
  for (const chat of chats) {
    if (chat.close) {
      await shutNotice(chat.tag);
      continue;
    }
    if (!chat.post || noticeIds.get(chat.tag) === chat.id) continue;
    const body = prefs.preview === 'hidden' ? say('notice.hidden') : chat.line;
    try {
      await showNotification({ title: chat.title, body, tag: chat.tag, navigate: chat.navigate, silent: !prefs.sound });
      noticeIds.set(chat.tag, chat.id);
    } catch (error) {
      if (!unsupported(error)) throw error;
    }
  }
  try {
    await setAppBadge({ contents: granted && master ? badge : 0 });
  } catch (error) {
    if (!unsupported(error)) throw error;
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
    if (focus === 'tabs' || focus === 'frame') {
      const slug = focus === 'frame' ? 'frame' : 'tabs';
      const name = focus === 'frame' ? 'Frame' : 'Tabs';
      return [
        { slug, name, label: '', initials: name.slice(0, 1), color: fill('brand.default'), main: false, hidden: false, ready: true, unread: false, shape: 'ball', pinned: false, working: false, line: 'Ready to chat', time: '', parentSlug: '', hideFromSidebar: false, subagent: false, prompt: '', inheritParentPrompt: false, notifications: wantsNotice(fixtureBotNotify.get(slug)) },
      ];
    }
    if (focus === 'fixture' || focus === 'bea') {
      return [
        { slug: 'fixture', name: 'Ada', label: '', initials: 'A', color: fill('brand.default'), main: false, hidden: false, ready: true, unread: true, shape: 'ball', pinned: false, working: false, line: 'Ready to chat', time: '', parentSlug: '', hideFromSidebar: false, subagent: false, prompt: '', inheritParentPrompt: false, notifications: wantsNotice(fixtureBotNotify.get('fixture')) },
        { slug: 'ada-junior-1', name: 'Ada junior 1', label: '', initials: 'AJ', color: fill('brand.default'), main: false, hidden: false, ready: true, unread: false, shape: 'ball', pinned: false, working: false, line: 'Ready to chat', time: '', parentSlug: 'fixture', hideFromSidebar: true, subagent: true, prompt: 'Look things up.', inheritParentPrompt: true, notifications: wantsNotice(fixtureBotNotify.get('ada-junior-1')) },
        { slug: 'bea', name: 'Bea', label: '', initials: 'B', color: fill('brand.blue'), main: false, hidden: true, ready: false, unread: true, shape: 'ball', pinned: false, working: false, line: 'Getting set up…', time: '', parentSlug: '', hideFromSidebar: false, subagent: false, prompt: '', inheritParentPrompt: false, notifications: wantsNotice(fixtureBotNotify.get('bea')) },
      ];
    }
    const body = await botsBody(store);
    if (Number(body.now) > 0) lastNow = Number(body.now);
    const at = (b: any): string => (typeof b.preview?.at === 'string' ? b.preview.at : '');
    // The main Bot first, then the most recent conversation; a Bot with none goes last.
    const sorted = [...(body.bots as any[])].sort((a, b) => (b.primary === true ? 1 : 0) - (a.primary === true ? 1 : 0) || at(b).localeCompare(at(a)));
    return sorted.map((b) => ({
      slug: String(b.slug), name: String(b.name), label: String(b.label ?? ''), initials: initials(String(b.name)),
      color: fill(b.color), shape: String(b.shape ?? 'ball'), main: b.primary === true, pinned: b.pinned === true, hidden: b.hidden === true,
      ready: typeof b.mainThreadId === 'string', working: b.working === true, unread: b.unread === true,
      line: lineOf(b), time: when(b.preview?.at, Number(body.now)),
      parentSlug: typeof b.parentSlug === 'string' ? b.parentSlug : '',
      hideFromSidebar: b.hideFromSidebar === true,
      subagent: b.role === 'subagent' || b.hideFromSidebar === true,
      prompt: typeof b.subagent?.prompt === 'string' ? b.subagent.prompt : '',
      inheritParentPrompt: b.subagent?.inheritParentPrompt === true,
      notifications: wantsNotice(b.notifications),
    }));
  },
  setBotNotifications: async ([slug, notifications], store) => {
    if (FIXTURE_SLUG(slug) || slug === 'ada-junior-1') {
      fixtureBotNotify.set(slug, notifications);
      return { ok: true, error: '' };
    }
    return reply(await call(store, `/api/bots/${encodeURIComponent(slug)}`, { method: 'PUT', body: JSON.stringify({ notifications }) }));
  },
  enableNotifications: async (_args, store) => reply(await call(store, '/api/settings', { method: 'PUT', body: JSON.stringify({ notifications: { enabled: true } }) })),
  renameBot: async ([slug, name], store) => reply(await call(store, `/api/bots/${encodeURIComponent(slug)}`, { method: 'PUT', body: JSON.stringify({ name }) })),
  recolourBot: async ([slug, color], store) => reply(await call(store, `/api/bots/${encodeURIComponent(slug)}`, { method: 'PUT', body: JSON.stringify({ color }) })),
  hideBot: async ([slug, hidden], store) => reply(await call(store, `/api/bots/${encodeURIComponent(slug)}`, { method: 'PUT', body: JSON.stringify({ hidden }) })),
  makeMainBot: async ([slug], store) => reply(await call(store, '/api/main-bot', { method: 'PUT', body: JSON.stringify({ slug }) })),
  reshapeBot: async ([slug, shape], store) => reply(await call(store, `/api/bots/${encodeURIComponent(slug)}`, { method: 'PUT', body: JSON.stringify({ shape }) })),
  reactTo: async ([slug, item, emoji], store) => {
    forgetMeta();
    return reply(await call(store, `/api/threads/${encodeURIComponent(threads[slug] ?? '')}/reactions`, { method: 'POST', body: JSON.stringify({ item, emoji }) }));
  },
  pinMessage: async ([slug, thread, item, text], store) => {
    forgetMeta();
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
    const note = noticePrefs(body);
    return {
      userName: String(body.userName ?? ''),
      theme: String(body.theme ?? 'system'),
      defaultInstance: String(body.defaultModel?.instanceId ?? ''),
      defaultModel: String(body.defaultModel?.model ?? ''),
      defaultEffort: savedEffort(body),
      defaultRuntimeMode: String(body.defaultRuntimeMode ?? (body.autoReview === false ? "full-access" : "auto")),
      noticeEnabled: note.enabled === true,
      noticeSound: note.sound,
      noticePreview: note.preview,
      noticeWhen: note.when,
      noticeEnabledMissing: note.enabled === null,
    };
  },
  loadNotice: async (): Promise<Result<'loadNotice'>> => {
    // Permission can change from Settings. Ask the host, then remember it for the poster.
    noticeHost = null;
    try {
      const status = await hostNotice();
      if (!status) return { available: false, state: 'unknown' };
      return status;
    } catch (error) {
      if (!unsupported(error)) console.log(`notification status failed: ${String(error)}`);
      return { available: false, state: 'unknown' };
    }
  },
  setNotifications: async ([enabled, sound, preview, when], store) => {
    const notifications: { sound: boolean; preview: string; when: string; enabled?: boolean } = { sound, preview, when };
    // "" leaves a never-saved master unset. "on" and "off" write it.
    if (enabled === 'on' || enabled === 'off') notifications.enabled = enabled === 'on';
    return reply(await call(store, '/api/settings', { method: 'PUT', body: JSON.stringify({ notifications }) }));
  },
  setUserName: async ([userName], store) => reply(await call(store, '/api/settings', { method: 'PUT', body: JSON.stringify({ userName }) })),
  setDefaultMode: async ([defaultRuntimeMode], store) => reply(await call(store, '/api/settings', { method: 'PUT', body: JSON.stringify({ defaultRuntimeMode }) })),
  setTheme: async ([theme], store) => reply(await call(store, '/api/settings', { method: 'PUT', body: JSON.stringify({ theme }) })),
  setPassword: async ([password], store) => reply(await call(store, '/api/access/password', { method: 'PUT', body: JSON.stringify({ password }) })),
  loadNetwork: async ([gate], store): Promise<Result<'loadNetwork'>> => {
    const blank = { mode: 'local', locked: false, restartRequired: false, canRestart: false, listening: '', urls: '', error: '', http: false };
    if (gate !== '1') return blank;
    const body = await read(store, '/api/access');
    const n = body.network ?? {};
    const text = (value: unknown): string => (typeof value === 'string' ? value : '');
    return {
      mode: text(n.mode) || 'local',
      locked: n.locked === true,
      restartRequired: n.restartRequired === true,
      canRestart: n.canRestart === true,
      listening: text(n.listening),
      urls: text(n.urls),
      error: text(n.error),
      http: n.http === true,
    };
  },
  setNetwork: async ([mode], store) => reply(await call(store, '/api/access/network', { method: 'PUT', body: JSON.stringify({ mode }) })),
  restartGateway: async (_args, store) => reply(await call(store, '/api/access/network/restart', { method: 'POST', body: '{}' })),
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
        { kind: 'bot', key: 'bot-tabs', id: 'tabs', name: 'Tabs', color: fill('brand.default'), shape: 'ball', faces: [], line: 'Ready to chat', time: '', at: '', unread: false, working: false, ready: true, notifications: wantsNotice(fixtureBotNotify.get('tabs')) },
      ];
    }
    if (focus === 'fixture' || focus === 'bea' || room === 'fixture') {
      return [
        { kind: 'bot', key: 'bot-fixture', id: 'fixture', name: 'Ada', color: fill('brand.default'), shape: 'ball', faces: [], line: 'Ready to chat', time: '', at: '', unread: true, working: false, ready: true, notifications: wantsNotice(fixtureBotNotify.get('fixture')) },
        { kind: 'group', key: 'group-fixture', id: 'fixture', name: 'Design', color: 'brand.blue', shape: 'hex', faces: [{ shape: 'ball', color: fill('brand.default'), state: 'idle' }, { shape: 'ball', color: fill('brand.blue'), state: 'idle' }], line: 'Ada: On it', time: '', at: '', unread: false, working: false, ready: true, notifications: wantsNotice(fixtureRoomNotify.get('fixture')) },
      ];
    }
    if (focus === 'fixture-busy' || room === 'fixture-busy') {
      return [
        { kind: 'group', key: 'group-fixture-busy', id: 'fixture-busy', name: 'Design', color: 'brand.blue', shape: 'hex', faces: [{ shape: 'ball', color: fill('brand.default'), state: 'working' }, { shape: 'ball', color: fill('brand.blue'), state: 'working' }], line: 'Ada: On it', time: '', at: '', unread: false, working: true, ready: true, notifications: wantsNotice(fixtureRoomNotify.get('fixture-busy')) },
      ];
    }
    const body = await botsBody(store);
    const now = Number(body.now);
    const directory = body.bots as any[];
    const at = (b: any): string => (typeof b.preview?.at === 'string' ? b.preview.at : '');
    const match = (...names: string[]): boolean => query === '' || names.some((n) => n.toLowerCase().includes(query));
    const bots = directory
      .filter((b) => b.hidden !== true && b.hideFromSidebar !== true && (query !== '' || !(b.primary === true || b.pinned === true)) && match(String(b.name), String(b.label ?? '')))
      .map((b) => ({
        kind: 'bot', key: `bot-${String(b.slug)}`, id: String(b.slug), name: String(b.name), color: fill(b.color), shape: String(b.shape ?? 'ball'), faces: [] as { shape: string; color: string; state: string }[],
        line: lineOf(b), time: when(b.preview?.at, now), at: at(b), unread: b.unread === true, working: b.working === true,
        ready: typeof b.mainThreadId === 'string', notifications: wantsNotice(b.notifications),
      }));
    // A rooms failure must not hide the Bots. An empty list is the honest fallback.
    const roomsReply = await roomsBody(store);
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
          faces: stackFaces(g.members, directory, Array.isArray(g.workingSlugs) ? g.workingSlugs.map((slug: unknown) => String(slug)) : []),
          line: last !== '' ? last : String(g.description ?? ''), time: when(said, now), at: said, unread: g.unread === true, working: g.working === true, ready: true, notifications: wantsNotice(g.notifications),
        };
      });
    // The most recent conversation first, a Bot or a group alike; one with nothing said goes last.
    return [...bots, ...groups].sort((x, y) => y.at.localeCompare(x.at));
  },
  markRoomSeen: async ([id], store) => {
    if (id === 'fixture' || id === 'fixture-busy') return { ok: true, error: '' };
    return reply(await call(store, `/api/rooms/${encodeURIComponent(id)}/seen`, { method: 'POST', body: '{}' }));
  },
  updateGroup: async ([id, field, value], store) => {
    if ((id === 'fixture' || id === 'fixture-busy') && field === 'notifications') {
      fixtureRoomNotify.set(id, value === 'true');
      return { ok: true, error: '' };
    }
    const body = field === 'members'
      ? { members: value === '' ? [] : value.split('\n') }
      : field === 'notifications'
        ? { notifications: value === 'true' }
        : { [field]: value };
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
  createBot: async ([name, color, shape, firstMessage], store) => {
    // The contract runner has no gateway. A real Bot still posts.
    // The reserved name is how that runner calls createBot. The slug in the body is what Made.id must keep.
    if (name === 'fixture') return madeFromBot({ ok: true, body: { slug: 'bea', name } });
    const r = await call(store, '/api/bots', { method: 'POST', body: JSON.stringify({ name, color, shape, ...(firstMessage !== '' ? { firstMessage } : {}) }) });
    return madeFromBot(r);
  },
  createSubagent: async ([parentSlug, name, shape, color, prompt, inheritParentPrompt], store) => reply(await call(store, '/api/bots', {
    method: 'POST',
    body: JSON.stringify({
      parentSlug,
      ...(name !== '' ? { name } : {}),
      shape,
      color,
      subagent: { prompt, inheritParentPrompt },
    }),
  })),
  saveSubagent: async ([slug, name, shape, color, prompt, inheritParentPrompt], store) => reply(await call(store, `/api/bots/${encodeURIComponent(slug)}`, {
    method: 'PUT',
    body: JSON.stringify({ name, shape, color, subagent: { prompt, inheritParentPrompt } }),
  })),
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
  loadSystem: async ([gate], store): Promise<Result<'loadSystem'>> => {
    const blank: Result<'loadSystem'> = { ok: false, installed: false, service: false, version: '', latest: '', updateAvailable: false, error: '' };
    if (gate !== '1') {
      systemChecked = false;
      return blank;
    }
    const path = systemChecked ? '/api/system' : '/api/system?check=1';
    systemChecked = true;
    const r = await call(store, path);
    if (!r.ok) return blank;
    const latest = r.body.latest;
    return {
      ok: true,
      installed: r.body.installed === true,
      service: r.body.service === true,
      version: String(r.body.version ?? ''),
      latest: latest && typeof latest === 'object' && typeof latest.version === 'string' ? latest.version : '',
      updateAvailable: r.body.updateAvailable === true,
      error: typeof r.body.error === 'string' ? r.body.error : '',
    };
  },
  startUpdate: async (_args, store): Promise<Result<'startUpdate'>> => {
    const r = await call(store, '/api/system/update', { method: 'POST', body: '{}' });
    if (!r.ok) return { ok: false, version: '', error: localize(r.error) };
    return { ok: true, version: String(r.body.version ?? ''), error: '' };
  },
  readHealth: async (_args, store): Promise<Result<'readHealth'>> => {
    const r = await call(store, '/api/health');
    if (!r.ok) return { ok: false, version: '' };
    return { ok: true, version: String(r.body.version ?? '') };
  },
  loadSuggestions: async (_args, store): Promise<Result<'loadSuggestions'>> => {
    const r = await call(store, '/api/suggestions');
    if (!r.ok) return [];
    return ((r.body.suggestions ?? []) as any[]).map((s) => ({ name: String(s.name), about: String(s.about ?? ''), color: fill(s.color), shape: String(s.shape ?? 'ball'), first: String(s.firstMessage ?? '') }));
  },
  markSeen: async ([slug], store) => reply(await call(store, `/api/bots/${encodeURIComponent(slug)}/seen`, { method: 'POST', body: '{}' })),
  noteUnread: async ([visibility, focused, available, state, enabled, enabledMissing, sound, preview, when, openSlug, openRoom], store): Promise<Result<'noteUnread'>> => {
    if (FIXTURE_SLUG(openSlug) || FIXTURE_SLUG(openRoom) || openSlug === 'fixture-busy' || openRoom === 'fixture-busy') return { ok: true, error: '' };
    const prefs: NoticePrefs = {
      enabled: enabledMissing ? null : enabled,
      sound,
      preview: preview === 'hidden' ? 'hidden' : 'full',
      when: when === 'background' ? 'background' : 'any',
    };
    try {
      await postUnread(store, openSlug, openRoom, visibility, focused, { available, state }, prefs);
    } catch {
      // A missing notification host still leaves the roster alone.
    }
    return { ok: true, error: '' };
  },
  loadChat: async ([slug, child], store): Promise<Result<'loadChat'>> => {
    if (slug === '') return blankChat('');
    if (slug === 'fixture') return fixtureChat(child);
    if (slug === 'bea') return blankChat('bea');
    if (slug === 'tabs') return blankChat('tabs');
    if (slug === 'frame') return blankChat('frame');
    const query = child !== '' ? `?child=${encodeURIComponent(child)}` : '';
    const body = await read(store, `/api/bots/${encodeURIComponent(slug)}/chat${query}`);
    const threadId = String(body.threadId ?? '');
    const now = Number(body.now);
    const meta = threadId === '' ? {} : await threadMeta(store, threadId, lastMessageId(body), now);
    if (child === '') threads[slug] = threadId;
    const rows = chatRows(slug, body, now, meta.reactions ?? {}, meta.replyTo ?? {});
    if (child === '') await shutNotice(`unread-bot-${slug}`);
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
    const none = { ok: true, error: '', current: '', instanceId: '', effort: '', runtimeMode: '', currentName: '', providerName: '', effortLabel: '', efforts: [], providers: [] };
    if (slug === '' || gate !== '1') return none;
    if (slug === 'fixture') return fixtureModelList();
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
    const runtimeMode = String(m.body.runtimeMode ?? '');
    const prov = providers.find((x: any) => x.instanceId === instanceId);
    const mod = prov?.models.find((o: any) => o.slug === current);
    return {
      ok: true, error: p.ok ? '' : explain(p.error, 'providers'), current, instanceId, effort, runtimeMode,
      currentName: mod?.name ?? prettyModel(current), providerName: prov?.name ?? instanceId,
      effortLabel: mod?.efforts.find((e: any) => e.id === effort)?.label ?? '', efforts: mod?.efforts ?? [], providers,
    };
  },
  setModel: async ([slug, instanceId, model, effort, mode], store) => {
    if (slug === 'fixture') return rememberFixtureModel(instanceId, model, effort, mode);
    return reply(await call(store, `/api/bots/${encodeURIComponent(slug)}/model`, {
      method: 'PUT', body: JSON.stringify({ instanceId, model, ...(effort !== '' ? { effort } : {}), ...(mode !== '' ? { runtimeMode: mode } : {}) }),
    }));
  },
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
          { id: 'local-1', title: 'Notes', status: 'Working', remote: false, threadId: 'child-thread' },
          { id: 'local-ro', title: 'Finished', status: 'Failed', remote: false, threadId: 'child-readonly' },
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
  loadComputer: async ([slug, gate, locale, tab, view], store): Promise<Result<'loadComputer'>> => {
    noteLocale(locale);
    const none = { ok: true, tabs: [] as { id: string; title: string; url: string }[], image: '', error: '', selected: '' };
    if (slug === '' || gate !== '1') return none;
    // Two server pages for the contract runner. A real Bot still asks the gateway.
    if (slug === 'frame') {
      return {
        ok: true,
        selected: 'only',
        image: '/9j/4AAQSkZJRgABAQEAYABgAAD/2wBDAAgGBgcGBQgHBwcJCQgKDBQNDAsLDBkSEw8UHRofHh0aHBwgJC4nICIsIxwcKDcpLDAxNDQ0Hyc5PTgyPC4zNDL/2wBDAQkJCQwLDBgNDRgyIRwhMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjL/wAARCAABAAEDASIAAhEBAxEB/8QAFQABAQAAAAAAAAAAAAAAAAAAAAj/xAAUEAEAAAAAAAAAAAAAAAAAAAAA/8QAFQEBAQAAAAAAAAAAAAAAAAAAAAX/xAAUEQEAAAAAAAAAAAAAAAAAAAAA/9oADAMBAAIRAxEAPwCfAAH/2Q==',
        error: '',
        tabs: [{ id: 'only', title: 'Only', url: 'https://only.example' }],
      };
    }
    if (slug === 'page') {
      return {
        ok: true,
        selected: 'only',
        image: '',
        error: '',
        tabs: [{ id: 'only', title: 'Only', url: 'https://only.example' }],
      };
    }
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
    const sharp = view === 'large' ? 'view=large' : '';
    const query = [since, picked, sharp].filter((part) => part !== '').join('&');
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
  openComputer: async ([slug], store) => {
    if (slug === 'frame' || slug === 'page' || slug === 'tabs') return { ok: true, error: '' };
    const r = await call(store, `/api/bots/${encodeURIComponent(slug)}/computer/open`, {
      method: 'POST', body: '{}',
    });
    if (!r.ok) return { ok: false, error: explain(r.error, 'computerOpen') };
    return { ok: true, error: '' };
  },
  closeComputer: async ([slug], store) => {
    if (slug === 'frame' || slug === 'page' || slug === 'tabs') return { ok: true, error: '' };
    const r = await call(store, `/api/bots/${encodeURIComponent(slug)}/computer/close`, {
      method: 'POST', body: '{}',
    });
    if (!r.ok) return { ok: false, error: explain(r.error, 'computerClose') };
    return { ok: true, error: '' };
  },
  computerControl: async ([slug, tabId, action], store) => {
    if (slug === 'frame' || slug === 'page' || slug === 'tabs') return { ok: true, error: '' };
    return reply(await call(store, `/api/bots/${encodeURIComponent(slug)}/computer/control`, {
      method: 'POST', body: JSON.stringify({ tabId, action }),
    }));
  },
  computerInput: async ([slug, kind, x, y, boxW, boxH, deltaY, text, keyCode, modifiers], store) => {
    if (slug === 'frame' || slug === 'page' || slug === 'tabs') return { ok: true, error: '', text: '' };
    const r = await call(store, `/api/bots/${encodeURIComponent(slug)}/computer/input`, {
      method: 'POST',
      body: JSON.stringify({
        kind, x, y, boxW, boxH, deltaY,
        text: kind === 'text' ? text : '',
        key: kind === 'key' ? text : '',
        keyCode: kind === 'key' ? keyCode : 0,
        modifiers: kind === 'key' ? modifiers : 0,
      }),
    });
    if (!r.ok) return { ok: false, error: localize(r.error), text: '' };
    return { ok: true, error: '', text: typeof r.body.text === 'string' ? r.body.text : '' };
  },
  loadRoom: async ([id], store): Promise<Result<'loadRoom'>> => {
    if (id === '') return { id: '', title: '', description: '', color: 'brand.default', shape: 'ball', members: [], messages: [], notifications: true };
    if (id === 'fixture') {
      if (fixtureMade) {
        const nameOf = (slug: string) => (slug === 'bea' ? 'Bea' : 'Ada');
        return {
          id: 'fixture', title: fixtureMade.title, description: fixtureMade.description, color: fixtureMade.color, shape: fixtureMade.shape,
          members: fixtureMade.slugs.map((slug) => ({ slug, name: nameOf(slug), line: 'Ready to chat', state: 'idle', ready: true, working: false })),
          messages: [],
          notifications: wantsNotice(fixtureRoomNotify.get('fixture')),
        };
      }
      return {
        id: 'fixture', title: 'Design', description: 'Where Ada and Bea trade drafts', color: 'brand.blue', shape: 'hex',
        members: [
          { slug: 'ada', name: 'Ada', line: 'Working', state: 'working', ready: true, working: true },
          { slug: 'bea', name: 'Bea', line: 'Getting set up…', state: 'asleep', ready: false, working: false },
        ],
        messages: [
          roomMessage({ id: 'r1', name: 'You', text: 'Hello', from: 'user' }),
          roomMessage({ id: 'r2', name: 'Ada', text: 'On it', from: 'ada' }),
        ],
        notifications: wantsNotice(fixtureRoomNotify.get('fixture')),
      };
    }
    if (id === 'fixture-busy') {
      return {
        id: 'fixture-busy', title: 'Design', description: 'Where Ada and Bea trade drafts', color: 'brand.blue', shape: 'hex',
        members: [
          { slug: 'ada', name: 'Ada', line: 'Working', state: 'working', ready: true, working: true },
          { slug: 'bea', name: 'Bea', line: 'Working', state: 'working', ready: true, working: true },
        ],
        messages: [
          roomMessage({ id: 'r1', name: 'Ada', text: 'On it', from: 'ada' }),
        ],
        notifications: wantsNotice(fixtureRoomNotify.get('fixture-busy')),
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
      messages: messages.map((m: any) => roomMessage(m)),
      notifications: wantsNotice(body.notifications),
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
