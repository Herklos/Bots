import { blank, fileName, fileSize, libraryPath } from './session.ts';
import { transcriptRowId } from './gateway.ts';
import { NOT_IN_VIEW } from './markdown.ts';
import type { Result } from '../app.contract.d.ts';
import { fill } from './palette.ts';
import { loadSentence, say } from './gateway.ts';
import { roomMessage, splitTables } from './markdown.ts';
import { isGroupLeak, when } from './time.ts';

export function blankChat(slug: string) {
  return { slug, rows: [] as ChatRow[], working: false, partial: false, readOnly: false, pins: [] as PinRow[], threadId: '' };
}

export type PinRow = { id: string; text: string; missing: boolean };
export type ChatRow = any;

// The contract runner has no gateway: a small conversation, a writable child and a missing pin.
// Pins are mutable so a Pin tap can toggle a row without a live gateway.
// A room the contract runner just created. Null keeps the built-in Design room.
export let fixtureMade: { title: string; description: string; color: string; shape: string; slugs: string[]; answerPolicy: string; chief: string } | null = null;
export function setFixtureMade(value: { title: string; description: string; color: string; shape: string; slugs: string[]; answerPolicy: string; chief: string } | null) {
  fixtureMade = value;
}
// Membership edits the contract runner saves without a gateway. Absent means the room's built-in roster.
export const fixtureMemberEdits = new Map<string, string[]>();
export const FIXTURE_CROWD: [string, string][] = [
  ['one', 'One'], ['two', 'Two'], ['three', 'Three'], ['four', 'Four'],
  ['five', 'Five'], ['six', 'Six'], ['seven', 'Seven'],
];

export function fixtureMemberRow(slug: string) {
  if (slug === 'bea') return { slug, name: 'Bea', line: 'Getting set up…', state: 'asleep', ready: false, working: false };
  if (slug === 'ada') return { slug, name: 'Ada', line: 'Working', state: 'working', ready: true, working: true };
  const named = FIXTURE_CROWD.find((pair) => pair[0] === slug);
  return { slug, name: slug === 'fixture' ? 'Ada' : (named?.[1] ?? slug), line: 'Ready to chat', state: 'idle', ready: true, working: false };
}

export function fixtureCrowdBots() {
  return FIXTURE_CROWD.map(([slug, name]) => ({
    slug, name, label: '', initials: name.slice(0, 1), color: fill('brand.default'), main: false, hidden: false, ready: true, unread: false, shape: 'ball', pinned: false, working: false, line: 'Ready to chat', time: '', parentSlug: '', hideFromSidebar: false, subagent: false, prompt: fixturePrompt(slug, ''), inheritParentPrompt: false, aboveLimit: false, notifications: wantsNotice(fixtureBotNotify.get(slug)),
  }));
}
// Lines the contract runner has sent. The transcript uses the same id the send returned.
export const fixtureSent: { id: string; text: string }[] = [];
export const fixtureRoomSent: { id: string; text: string }[] = [];
// Mute choices the contract runner makes. Missing means the chat still notifies.
export const fixtureBotNotify = new Map<string, boolean>();
export const fixtureBotPrompt = new Map<string, string>();
export function fixturePrompt(slug: string, fallback: string): string {
  const saved = fixtureBotPrompt.get(slug);
  return saved !== undefined ? saved : fallback;
}
export const fixtureRoomNotify = new Map<string, boolean>();
export const fixtureRoomPolicy = new Map<string, string>();
export const fixtureRoomChief = new Map<string, string>();
export function roomChief(id: string, made = ''): string {
  const saved = fixtureRoomChief.get(id);
  return saved !== undefined ? saved : made;
}
export let fixtureGroupAnswer = '';
export function setFixtureGroupAnswer(value: string) {
  fixtureGroupAnswer = value;
}
export function roomAnswer(id: string, made?: string): string {
  const saved = fixtureRoomPolicy.get(id);
  if (saved) return saved;
  if (made === 'last' || made === 'everyone' || made === 'chief' || made === 'default') return made;
  return 'default';
}
export const wantsNotice = (value: unknown): boolean => value !== false;

// The model page's fixture: five levels in the order a provider would send them.
export const FIXTURE_EFFORTS = [
  { id: 'low', label: 'Low' },
  { id: 'medium', label: 'Medium' },
  { id: 'high', label: 'High' },
  { id: 'xhigh', label: 'Extra high' },
  { id: 'max', label: 'Max' },
];

export let fixtureChoice = { instanceId: 'claudeAgent', model: 'haiku', effort: 'high', runtimeMode: 'auto' };
// Shown only when GET /api/suggestions cannot be read. Same words as gateway/src/suggestions.rs.
export const FIXTURE_SUGGESTIONS = [
  { name: 'Night Shift', about: 'Works overnight and preps your morning digest', color: 'orange', shape: 'hex', firstMessage: 'You are my night shift. While I sleep, go through what came in and have a short digest ready for the morning.' },
  { name: 'Inbox Triage', about: 'Sorts your email and drafts replies in your voice', color: 'pink', shape: 'cloud', firstMessage: 'Help me keep my inbox at zero. Sort what came in, and draft replies for the ones that need one.' },
  { name: 'Research Desk', about: 'Reads up on a topic and reports back with sources', color: 'blue', shape: 'egg', firstMessage: 'Be my research desk. Ask me what I want to understand, then read up and report back with sources.' },
  { name: 'Money Check', about: 'Watches payments and what is overdue', color: 'green', shape: 'square', firstMessage: 'Keep an eye on my payments. Tell me what came in, what is overdue and what needs a follow up.' },
  { name: 'Trip Planner', about: 'Plans a trip around your dates and budget', color: 'violet', shape: 'drop', firstMessage: 'Plan a trip with me. Ask for my dates and budget, then suggest a plan I can adjust.' },
];

export function fixtureProviders() {
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

export function fixtureModelList() {
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

export function rememberFixtureModel(instanceId: string, model: string, effort: string, mode: string): { ok: boolean; error: string } {
  const prov = fixtureProviders().find((p) => p.instanceId === instanceId);
  const mod = prov?.models.find((row) => row.slug === model);
  if (!prov || !mod) return { ok: false, error: 'That model is not available.' };
  const runtimeMode = mode === 'approval-required' || mode === 'auto' || mode === 'full-access' ? mode : fixtureChoice.runtimeMode;
  fixtureChoice = { instanceId, model, effort: mod.efforts.some((level) => level.id === effort) ? effort : '', runtimeMode };
  return { ok: true, error: '' };
}

export function savedEffort(body: any): string {
  if (typeof body?.effort === 'string' && body.effort !== '') return body.effort;
  const model = body?.defaultModel;
  if (!model || typeof model !== 'object') return '';
  if (typeof model.effort === 'string' && model.effort !== '') return model.effort;
  const option = Array.isArray(model.options) ? model.options.find((row: any) => row && (row.id === 'effort' || row.id === 'reasoningEffort')) : undefined;
  return typeof option?.value === 'string' ? option.value : '';
}

export const fixturePins: Record<string, PinRow[]> = {
  'fixture-thread': [
    { id: 'm1', text: 'Hello', missing: false },
    { id: 'missing', text: NOT_IN_VIEW, missing: true },
  ],
  'child-thread': [],
};

// The contract runner does not speak HTTP. This is the 304 path it can see: a second read
// of an unchanged chat returns the same answer, so the transcript list is not a new one.
export let quietChat: Result<'loadChat'> | null = null;
export function quietChatView(): Result<'loadChat'> {
  if (quietChat) return quietChat;
  const text = 'Still here';
  const row = { ...blank, id: 'q1', kind: 'message', mine: false, text, head: true, tail: true, parts: splitTables(text) } as ChatRow;
  quietChat = { slug: 'quiet', working: false, partial: false, readOnly: false, pins: [], threadId: 'quiet-thread', rows: [row] };
  return quietChat;
}

// A picture on a message. `alt` is the Markdown alt and becomes the name.
function shownImage(id: string, name: string, alt: string, url: string, bytes: number, width: number, height: number) {
  return {
    id,
    name: fileName({ name, alt }),
    kind: 'image',
    size: fileSize(bytes),
    url,
    width,
    height,
  };
}

export function fixtureChat(child: string) {
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
      row({ id: 'voice1', mine: true, files: [{ id: 'voice', name: 'voice.m4a', kind: 'audio', size: '1 KB', url: '/api/bots/fixture/attachments/voice', width: 0, height: 0 }] }),
      row({ id: 'img1', text: 'Here is the chart.', files: [shownImage('libchart', 'chart.png', 'Sales chart', libraryPath('fixture', 'library/chart.png'), 48210, 1200, 800)] }),
      row({ id: 'img2', files: [shownImage('libmissing', 'missing.png', 'Missing chart', libraryPath('fixture', 'library/missing.png'), 0, 800, 600)] }),
      ...fixtureSent.map((m) => row({ id: transcriptRowId(m.id), mine: true, text: m.text })),
      { ...blank, id: 'e1', kind: 'event', mine: false, text: 'Created routine', title: 'Morning', head: true, tail: true, variant: 'routine-created' },
      { ...blank, id: 'e2', kind: 'event', mine: false, text: 'Messaged', botName: 'Inbox', botSlug: 'inbox', botShape: 'hex', botColor: 'brand.pink', head: true, tail: true, variant: 'sent' },
    ],
  };
}

export function explain(error: string, what: string): string {
  const e = error.toLowerCase();
  if (e.includes('reach the gateway')) return loadSentence(what);
  if (e.includes('login')) return say('error.signIn');
  if (e.includes('no conversation') || e.includes('getting set up')) return say('error.settingUp');
  if (e.includes('t3')) return say('error.t3');
  if (e.includes('had a problem')) return say('error.restart');
  return error || loadSentence(what);
}
