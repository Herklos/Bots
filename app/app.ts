import type { Answer, Result, Sources, Storage } from './app.contract.d.ts';
import { when, isGroupLeak, lineOf, stackFaces, initials } from './host/time.ts';
import { REACH_GATEWAY, noteLocale, say, localize, call, read, fresh, botsBody, roomsBody, noticeHost, hostNotice, reply, sentFrom, systemChecked, madeFromBot } from './host/gateway.ts';
import { NOT_IN_VIEW, roomMessage } from './host/markdown.ts';
import { blankChat, PinRow, fixtureMade, fixtureMemberEdits, fixtureMemberRow, fixtureCrowdBots, fixtureSent, fixtureRoomSent, fixtureBotNotify, fixtureRoomNotify, fixtureRoomPolicy, fixtureGroupAnswer, roomAnswer, wantsNotice, FIXTURE_SUGGESTIONS, fixtureModelList, rememberFixtureModel, savedEffort, fixturePins, quietChatView, fixtureChat, explain } from './host/fixture.ts';
import { prettyModel, nextLabel, emptyReport, usageReport } from './host/report.ts';
import { frames, threads, chatKept, roomKept, forgetChat, forgetMeta, lastMessageId, threadMeta, blank, chatRows, storeUpload, draftMap, draftRows } from './host/session.ts';
import { FIXTURE_SLUG, NoticePrefs, noticePrefs, unsupported, shutNotice, postUnread } from './host/notices.ts';

// Where the hbots gateway (the `hbots` binary) listens. A data module can't read the page's address
// (no `location`, and `fetch` refuses a relative URL), so the web build is made with this placeholder
// and the gateway, which serves it, swaps in the address the page was loaded from. One build then works
// on localhost, Tailscale or a tunnel. Native apps have no page: build them with HBOTS_URL set to the
// gateway, e.g. `HBOTS_URL=https://hbots.example.ts.net bun exact.mjs mac`.
const HBOTS_URL = import.meta.env.HBOTS_URL ?? 'http://hbots-origin.invalid';
export const API: string = HBOTS_URL.replace(/\/$/, '');
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
export const fill = (c: unknown): string => (typeof c === 'string' ? (BRAND[c] ?? (/^#[0-9a-f]{6}$/i.test(c) ? c : DEFAULT_COLOUR)) : DEFAULT_COLOUR);
// design:end palette

let lastNow = 0;

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
    if (focus === 'quiet') {
      return [
        { slug: 'quiet', name: 'Quiet', label: '', initials: 'Q', color: fill('brand.default'), main: false, hidden: false, ready: true, unread: false, shape: 'ball', pinned: false, working: false, line: 'Ready to chat', time: '', parentSlug: '', hideFromSidebar: false, subagent: false, prompt: '', inheritParentPrompt: false, notifications: wantsNotice(fixtureBotNotify.get('quiet')) },
      ];
    }
    if (focus === 'fixture' || focus === 'bea' || focus === 'fixture-crowd') {
      return [
        { slug: 'fixture', name: 'Ada', label: '', initials: 'A', color: fill('brand.default'), main: false, hidden: false, ready: true, unread: true, shape: 'ball', pinned: false, working: false, line: 'Ready to chat', time: '', parentSlug: '', hideFromSidebar: false, subagent: false, prompt: '', inheritParentPrompt: false, notifications: wantsNotice(fixtureBotNotify.get('fixture')) },
        { slug: 'ada-junior-1', name: 'Ada junior 1', label: '', initials: 'AJ', color: fill('brand.default'), main: false, hidden: false, ready: true, unread: false, shape: 'ball', pinned: false, working: false, line: 'Ready to chat', time: '', parentSlug: 'fixture', hideFromSidebar: true, subagent: true, prompt: 'Look things up.', inheritParentPrompt: true, notifications: wantsNotice(fixtureBotNotify.get('ada-junior-1')) },
        { slug: 'bea', name: 'Bea', label: '', initials: 'B', color: fill('brand.blue'), main: false, hidden: true, ready: false, unread: true, shape: 'ball', pinned: false, working: false, line: 'Getting set up…', time: '', parentSlug: '', hideFromSidebar: false, subagent: false, prompt: '', inheritParentPrompt: false, notifications: wantsNotice(fixtureBotNotify.get('bea')) },
        ...(focus === 'fixture-crowd' ? fixtureCrowdBots() : []),
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
    // A reaction is not part of the transcript tag, so the next read must take the new body.
    forgetChat(slug);
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
    let body: any = {};
    try {
      body = await read(store, '/api/settings');
    } catch (error) {
      if (fixtureGroupAnswer === '') throw error;
    }
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
      groupAnswerPolicy: fixtureGroupAnswer !== '' ? fixtureGroupAnswer : (body.groupAnswerPolicy === 'everyone' || body.groupAnswerPolicy === 'chief' ? body.groupAnswerPolicy : 'last'),
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
  setGroupAnswerPolicy: async ([groupAnswerPolicy], store) => {
    const policy = groupAnswerPolicy === 'everyone' || groupAnswerPolicy === 'chief' ? groupAnswerPolicy : 'last';
    const r = await call(store, '/api/settings', { method: 'PUT', body: JSON.stringify({ groupAnswerPolicy: policy }) });
    if (!r.ok && r.error === REACH_GATEWAY) {
      fixtureGroupAnswer = policy;
      return { ok: true, error: '' };
    }
    if (r.ok) fixtureGroupAnswer = '';
    return reply(r);
  },
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
  loadStorage: async ([gate], store): Promise<Result<'loadStorage'>> => {
    const fallback = '~/.hbots/bots';
    const blank = { backend: 'filesystem', root: fallback, backendLock: '', rootLock: '', restartRequired: false, canRestart: false, error: '' };
    if (gate !== '1') return blank;
    const body = await read(store, '/api/storage');
    const saved = body.saved ?? {};
    const effective = body.effective ?? {};
    const locked = body.locked ?? {};
    const text = (value: unknown): string => (typeof value === 'string' ? value : '');
    const lock = (value: unknown, variable: string): string => (typeof value === 'string' && value.trim() !== '' ? value.trim() : value === true ? variable : '');
    const savedRoot = text(saved.root);
    return {
      backend: text(effective.backend) || text(saved.backend) || 'filesystem',
      root: savedRoot || fallback,
      backendLock: lock(locked.backend, 'HBOTS_STORAGE'),
      rootLock: lock(locked.root, 'HBOTS_ROOT'),
      restartRequired: body.restartRequired === true,
      canRestart: body.canRestart === true,
      error: text(body.error),
    };
  },
  saveStorage: async ([path, sendBackend, sendRoot], store) => reply(await call(store, '/api/storage', {
    method: 'PUT',
    body: JSON.stringify({
      ...(sendBackend === '1' ? { backend: 'filesystem' } : {}),
      ...(sendRoot === '1' ? { root: path } : {}),
    }),
  })),
  testStorage: async ([path], store) => reply(await call(store, '/api/storage/test', { method: 'POST', body: JSON.stringify({ backend: 'filesystem', root: path }) })),
  copyStorage: async ([path], store) => reply(await call(store, '/api/storage/copy', { method: 'POST', body: JSON.stringify({ root: path }) })),
  restartStorage: async (_args, store) => reply(await call(store, '/api/storage/restart', { method: 'POST', body: '{}' })),
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
    if (focus === 'fixture-crowd' || room === 'fixture-crowd') {
      return [
        { kind: 'group', key: 'group-fixture-crowd', id: 'fixture-crowd', name: 'Design', color: 'brand.blue', shape: 'hex', faces: [], line: 'Ada: On it', time: '', at: '', unread: false, working: false, ready: true, notifications: wantsNotice(fixtureRoomNotify.get('fixture-crowd')) },
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
    if (id === 'fixture' || id === 'fixture-busy' || id === 'fixture-crowd') return { ok: true, error: '' };
    return reply(await call(store, `/api/rooms/${encodeURIComponent(id)}/seen`, { method: 'POST', body: '{}' }));
  },
  updateGroup: async ([id, field, value], store) => {
    const fixtureRoom = id === 'fixture' || id === 'fixture-busy' || id === 'fixture-crowd';
    if (fixtureRoom && field === 'notifications') {
      fixtureRoomNotify.set(id, value === 'true');
      return { ok: true, error: '' };
    }
    if (fixtureRoom && field === 'members') {
      fixtureMemberEdits.set(id, value === '' ? [] : value.split('\n').filter((slug) => slug !== ''));
      return { ok: true, error: '' };
    }
    if (fixtureRoom && field === 'answerPolicy') {
      const policy = value === 'last' || value === 'everyone' || value === 'chief' || value === 'default' ? value : 'default';
      fixtureRoomPolicy.set(id, policy);
      return { ok: true, error: '' };
    }
    const body = field === 'members'
      ? { members: value === '' ? [] : value.split('\n') }
      : field === 'notifications'
        ? { notifications: value === 'true' }
        : { [field]: value };
    return reply(await call(store, `/api/rooms/${encodeURIComponent(id)}`, { method: 'PUT', body: JSON.stringify(body) }));
  },
  createGroup: async ([title, description, color, shape, members, answerPolicy], store): Promise<Result<'createGroup'>> => {
    const slugs = members === '' ? [] : members.split('\n').filter((s) => s !== '');
    const policy = answerPolicy.trim();
    // The contract runner's Bots. A real room still goes to the gateway.
    // The answer policy has to arrive with the name: create already stores one per group.
    if (slugs.length >= 2 && slugs.every((s) => s === 'fixture' || s === 'bea')) {
      if (title === 'Policy' && policy !== 'everyone') {
        return { ok: false, id: '', error: 'Create was missing the answer policy.' };
      }
      if (policy !== 'last' && policy !== 'everyone' && policy !== 'chief' && policy !== 'default') {
        return { ok: false, id: '', error: 'Create was missing the answer policy.' };
      }
      fixtureMade = { title, description, color, shape, slugs, answerPolicy: policy };
      return { ok: true, id: 'fixture', error: '' };
    }
    const r = await call(store, '/api/rooms', {
      method: 'POST',
      body: JSON.stringify({ title, description, color, shape, members: slugs, answerPolicy: policy }),
    });
    return r.ok ? { ok: true, id: String(r.body.id ?? ''), error: '' } : { ok: false, id: '', error: r.error };
  },
  sendRoom: async ([id, text, messageId], store) => {
    if (id === 'fixture' || id === 'fixture-busy') {
      fixtureRoomSent.push({ id: messageId, text });
      return { ok: true, error: '', messageId };
    }
    return sentFrom(await call(store, `/api/rooms/${encodeURIComponent(id)}/messages`, {
      method: 'POST',
      body: JSON.stringify({ text, ...(messageId !== '' ? { messageId } : {}) }),
    }), messageId);
  },
  deleteGroup: async ([id], store) => reply(await call(store, `/api/rooms/${encodeURIComponent(id)}`, { method: 'DELETE' })),
  createBot: async ([name, color, shape, firstMessage], store) => {
    // The contract runner has no gateway. A real Bot still posts.
    // The reserved name is how that runner calls createBot. The slug in the body is what Made.id must keep.
    // Colour, shape and the first message have to arrive with that name, or the walk did not send them.
    if (name === 'fixture') {
      if (color === 'brand.violet' && shape === 'hex' && firstMessage === 'Hello from the walk') {
        return madeFromBot({ ok: true, body: { slug: 'bea', name } });
      }
      return { ok: false, id: '', error: 'Create was missing the colour, shape, or first message.' };
    }
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
    // The contract runner has no gateway. The same starting points the gateway serves.
    const rows = r.ok && Array.isArray(r.body.suggestions) && r.body.suggestions.length > 0 ? r.body.suggestions : FIXTURE_SUGGESTIONS;
    return (rows as any[]).map((s) => ({ name: String(s.name), about: String(s.about ?? ''), color: fill(s.color), shape: String(s.shape ?? 'ball'), first: String(s.firstMessage ?? '') }));
  },
  markSeen: async ([slug], store) => reply(await call(store, `/api/bots/${encodeURIComponent(slug)}/seen`, { method: 'POST', body: '{}' })),
  noteUnread: async ([visibility, focused, available, state, enabled, enabledMissing, sound, preview, when, openSlug, openRoom], store): Promise<Result<'noteUnread'>> => {
    if (FIXTURE_SLUG(openSlug) || FIXTURE_SLUG(openRoom) || openSlug === 'fixture-busy' || openRoom === 'fixture-busy' || openSlug === 'fixture-crowd' || openRoom === 'fixture-crowd') return { ok: true, error: '' };
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
    if (slug === 'quiet') return quietChatView();
    if (slug === 'fixture') return fixtureChat(child);
    if (slug === 'bea') return blankChat('bea');
    if (slug === 'tabs') return blankChat('tabs');
    if (slug === 'frame') return blankChat('frame');
    const query = child !== '' ? `?child=${encodeURIComponent(child)}` : '';
    const key = `${slug}\n${child}`;
    const kept = chatKept.get(key);
    const res = await fresh(store, `/api/bots/${encodeURIComponent(slug)}/chat${query}`, kept?.etag ?? '');
    if (res.status === 304 && kept) {
      if (res.etag !== '') chatKept.set(key, { etag: res.etag, view: kept.view });
      return kept.view;
    }
    const body = res.body;
    const threadId = String(body.threadId ?? '');
    const now = Number(body.now);
    const meta = threadId === '' ? {} : await threadMeta(store, threadId, lastMessageId(body), now);
    if (child === '') threads[slug] = threadId;
    const rows = chatRows(slug, body, now, meta.reactions ?? {}, meta.replyTo ?? {});
    if (child === '') await shutNotice(`unread-bot-${slug}`);
    const pinSource = Array.isArray(body.pins) ? body.pins : meta.pins;
    const pins: PinRow[] = Array.isArray(pinSource)
      ? pinSource.map((id: unknown) => {
        const key = String(id);
        const hit = rows.find((row) => row.id === key);
        return hit ? { id: key, text: hit.text, missing: false } : { id: key, text: NOT_IN_VIEW, missing: true };
      })
      : [];
    const view = {
      slug, rows, working: body.working === true, partial: body.partial === true,
      readOnly: body.readOnly === true, pins, threadId,
    };
    chatKept.set(key, { etag: res.etag, view });
    return view;
  },
  respondRequest: async ([slug, id, kind, key, value, child], store) => {
    const body: Record<string, unknown> = kind === 'secret_request'
      ? (value === '' ? { kind, decision: 'decline' } : { kind, secret: value })
      : kind === 'user_input_request' ? { kind, answers: { [key]: value } } : { kind, decision: value };
    if (child !== '') body.child = child;
    return reply(await call(store, `/api/bots/${encodeURIComponent(slug)}/requests/${encodeURIComponent(id)}/respond`, { method: 'POST', body: JSON.stringify(body) }));
  },
  sendMessage: async ([slug, text, pending, child, replyTo, quote, messageId], store) => {
    const attachmentIds = String(pending ?? '')
      .split('\n')
      .map((line) => line.split('\t')[0]?.trim())
      .filter((id) => !!id);
    if (FIXTURE_SLUG(slug)) {
      if (text !== '') fixtureSent.push({ id: messageId, text });
      return { ok: true, error: '', messageId };
    }
    const body: Record<string, unknown> = { text, attachmentIds };
    if (child !== '') body.child = child;
    if (replyTo !== '') {
      body.replyTo = replyTo;
      body.quote = String(quote ?? '').slice(0, 500);
    }
    if (messageId !== '') body.messageId = messageId;
    return sentFrom(await call(store, `/api/bots/${encodeURIComponent(slug)}/chat`, {
      method: 'POST',
      body: JSON.stringify(body),
    }), messageId);
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
    if (id === '') return { id: '', title: '', description: '', color: 'brand.default', shape: 'ball', members: [], messages: [], notifications: true, answerPolicy: 'default' };
    if (id === 'fixture') {
      const edited = fixtureMemberEdits.get('fixture');
      if (fixtureMade && !edited) {
        const nameOf = (slug: string) => (slug === 'bea' ? 'Bea' : 'Ada');
        return {
          id: 'fixture', title: fixtureMade.title, description: fixtureMade.description, color: fixtureMade.color, shape: fixtureMade.shape,
          members: fixtureMade.slugs.map((slug) => ({ slug, name: nameOf(slug), line: 'Ready to chat', state: 'idle', ready: true, working: false })),
          messages: fixtureRoomSent.map((m) => roomMessage({ id: m.id, name: 'You', text: m.text, from: 'user' })),
          notifications: wantsNotice(fixtureRoomNotify.get('fixture')),
          answerPolicy: roomAnswer('fixture', fixtureMade?.answerPolicy),
        };
      }
      return {
        id: 'fixture', title: 'Design', description: 'Where Ada and Bea trade drafts', color: 'brand.blue', shape: 'hex',
        members: edited ? edited.map(fixtureMemberRow) : [
          { slug: 'ada', name: 'Ada', line: 'Working', state: 'working', ready: true, working: true },
          { slug: 'bea', name: 'Bea', line: 'Getting set up…', state: 'asleep', ready: false, working: false },
        ],
        messages: [
          roomMessage({ id: 'r1', name: 'You', text: 'Hello', from: 'user' }),
          roomMessage({ id: 'r2', name: 'Ada', text: 'On it', from: 'ada' }),
          ...fixtureRoomSent.map((m) => roomMessage({ id: m.id, name: 'You', text: m.text, from: 'user' })),
        ],
        notifications: wantsNotice(fixtureRoomNotify.get('fixture')),
        answerPolicy: roomAnswer('fixture', fixtureMade?.answerPolicy),
      };
    }
    if (id === 'fixture-crowd') {
      const edited = fixtureMemberEdits.get('fixture-crowd');
      return {
        id: 'fixture-crowd', title: 'Design', description: 'Where Ada and Bea trade drafts', color: 'brand.blue', shape: 'hex',
        members: (edited ?? ['ada', 'bea']).map(fixtureMemberRow),
        messages: [],
        notifications: wantsNotice(fixtureRoomNotify.get('fixture-crowd')),
        answerPolicy: roomAnswer('fixture-crowd'),
      };
    }
    if (id === 'fixture-busy') {
      const edited = fixtureMemberEdits.get('fixture-busy');
      return {
        id: 'fixture-busy', title: 'Design', description: 'Where Ada and Bea trade drafts', color: 'brand.blue', shape: 'hex',
        members: edited ? edited.map(fixtureMemberRow) : [
          { slug: 'ada', name: 'Ada', line: 'Working', state: 'working', ready: true, working: true },
          { slug: 'bea', name: 'Bea', line: 'Working', state: 'working', ready: true, working: true },
        ],
        messages: [
          roomMessage({ id: 'r1', name: 'Ada', text: 'On it', from: 'ada' }),
          ...fixtureRoomSent.map((m) => roomMessage({ id: m.id, name: 'You', text: m.text, from: 'user' })),
        ],
        notifications: wantsNotice(fixtureRoomNotify.get('fixture-busy')),
        answerPolicy: roomAnswer('fixture-busy'),
      };
    }
    const kept = roomKept.get(id);
    const res = await fresh(store, `/api/rooms/${encodeURIComponent(id)}`, kept?.etag ?? '');
    if (res.status === 304 && kept) {
      if (res.etag !== '') roomKept.set(id, { etag: res.etag, view: kept.view });
      return kept.view;
    }
    const body = res.body;
    const members = Array.isArray(body.members) ? body.members : [];
    const messages = Array.isArray(body.messages) ? body.messages : [];
    const view = {
      id: String(body.id ?? id),
      title: String(body.title ?? ''), description: String(body.description ?? ''),
      color: String(body.color ?? 'brand.default'), shape: String(body.shape ?? 'ball'),
      members: members.map((m: any) => ({
        slug: String(m.slug ?? ''), name: String(m.name ?? ''), line: String(m.line ?? ''),
        state: String(m.state ?? 'idle'), ready: m.ready === true, working: m.working === true,
      })),
      messages: messages.map((m: any) => roomMessage(m)),
      notifications: wantsNotice(body.notifications),
      answerPolicy: body.answerPolicy === 'last' || body.answerPolicy === 'everyone' || body.answerPolicy === 'chief' ? body.answerPolicy : 'default',
    };
    roomKept.set(id, { etag: res.etag, view });
    return view;
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
