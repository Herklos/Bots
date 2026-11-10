// Sample data for scripts/preview.mjs. It speaks the gateway's API, as the app expects it.
const now = Date.now();
const ago = (minutes) => new Date(now - minutes * 60_000).toISOString();

const bots = [
  { slug: 'ship', name: 'Ship', color: 'brand.violet', shape: 'ball', primary: true, pinned: false, hidden: false, mainThreadId: 't-ship', working: true, unread: false,
    preview: { kind: 'text', role: 'assistant', text: 'On it, pulling in today’s feedback now.', at: ago(2) } },
  { slug: 'inbox', name: 'Inbox', color: 'brand.pink', shape: 'hex', primary: false, pinned: true, hidden: false, mainThreadId: 't-inbox', working: false, unread: true,
    preview: { kind: 'text', role: 'assistant', text: 'Checking who’s paying and who isn’t this week.', at: ago(14) } },
  { slug: 'ledger', name: 'Ledger', color: 'brand.green', shape: 'square', primary: false, pinned: true, hidden: false, mainThreadId: 't-ledger', working: false, unread: false,
    preview: { kind: 'text', role: 'assistant', text: 'Stripe paid out on Tuesday: 3 payments, no disputes.', at: ago(60 * 26) } },
  { slug: 'scout', name: 'Scout', color: 'brand.blue', shape: 'cloud', primary: false, pinned: false, hidden: false, mainThreadId: 't-scout', working: false, unread: false,
    preview: { kind: 'sent', role: 'assistant', text: 'Messaged Inbox: draft ready for review', at: ago(60 * 50) } },
  { slug: 'night', name: 'Night Shift', color: 'brand.orange', shape: 'drop', primary: false, pinned: false, hidden: false, mainThreadId: 't-night', working: false, unread: false,
    preview: { kind: 'attachment', role: 'assistant', text: 'Attachment', at: ago(60 * 24 * 5) } },
  { slug: 'quiet', name: 'Archive', color: 'brand.gray', shape: 'pill', primary: false, pinned: false, hidden: true, mainThreadId: 't-quiet', working: false, unread: false, preview: null },
].map((b) => ({ label: '', ...b }));

const state = { bots, settings: { userName: 'Alex', theme: 'system', beta: { projects: false } } };

const inbox = { slug: 'inbox', name: 'Inbox', color: 'brand.pink', shape: 'hex' };
const scout = { slug: 'scout', name: 'Scout', color: 'brand.blue', shape: 'cloud' };
const m = (id, ordinal, role, text, minutes, extra = {}) => ({ id, ordinal, role, text, at: ago(minutes), files: [], attachments: [], streaming: false, from: null, ...extra });
const chats = {
  ship: {
    messages: [
      m('m1', 1, 'user', 'Morning. Go through today’s feedback.', 60 * 26),
      m('m2', 2, 'bot', 'On it, pulling in today’s feedback now.', 60 * 26 - 1),
      m('m3', 4, 'bot', 'Done. Here is what came in overnight:\n\n- **12** new items, 3 flagged as bugs\n- Most common ask: *dark mode for the sidebar*\n- One refund request, already routed to `Ledger`\n\n```\nfeedback.csv  12 rows\n```\nWant me to draft replies?', 60 * 25),
      m('m4', 5, 'user', 'Yes, draft them. And send me the summary as a file.', 60 * 24),
      m('m5', 7, 'bot', 'Here is the summary.', 60 * 24 - 2, { files: [{ id: 'f1', name: 'Feedback summary.pdf', mime: 'application/pdf', bytes: 1258291, kind: 'file' }, { id: 'f2', name: 'chart.png', mime: 'image/png', bytes: 83000, kind: 'image' }] }),
      m('m6', 9, 'bot', 'Inbox has the drafts if you want a look.', 8, { from: null }),
      m('m7', 11, 'user', 'Morning. Go through today’s feedback.', 4),
      m('m8', 12, 'bot', 'On it, pulling in today’s feedback now.', 3),
    ],
    events: [
      { id: 'event-e1', ordinal: 3, at: ago(60 * 25 + 1), kind: 'sent', text: 'Messaged', title: '', bot: inbox },
      { id: 'event-e2', ordinal: 6, at: ago(60 * 24 - 1), kind: 'routine', text: 'Created routine', title: 'Morning briefing', bot: null },
      { id: 'event-e3', ordinal: 8, at: ago(10), kind: 'from', text: 'Message from', title: '', bot: scout },
      { id: 'event-e4', ordinal: 10, at: ago(6), kind: 'ran', text: 'Routine ran', title: '', bot: null },
    ],
    requests: [{ id: 'req1', itemId: 'i1', kind: 'approval_request', title: 'Run `git status` in the Feedback folder?', reason: 'The Bot wants to read the folder’s state.', requestKind: 'command',
      options: [{ decision: 'accept', label: 'Allow' }, { decision: 'decline', label: 'Deny' }] }],
    working: true,
  },
};
const chatFor = (slug) => chats[slug] ?? { messages: [], events: [], requests: [], working: false };

const efforts = (def) => ['low', 'medium', 'high', 'xhigh'].map((id) => ({ id, label: { low: 'Low', medium: 'Medium', high: 'High', xhigh: 'Extra High' }[id], isDefault: id === def }));
const providers = [
  { instanceId: 'claudeAgent', driver: 'claudeAgent', displayName: 'Claude Code', account: 'Claude Pro Subscription', models: [
    { slug: 'claude-haiku-5-5', name: 'Claude Haiku 5.5', recommended: true, effort: 'medium', efforts: efforts('medium') },
    { slug: 'claude-sonnet-5-5', name: 'Claude Sonnet 5.5', recommended: false, effort: 'high', efforts: efforts('high') },
    { slug: 'claude-opus-5-5', name: 'Claude Opus 5.5', recommended: false, effort: 'medium', efforts: efforts('medium') },
  ] },
  { instanceId: 'codex', driver: 'codex', displayName: 'Codex', account: 'ChatGPT Plus Subscription', models: [
    { slug: 'gpt-5.5', name: 'GPT-5.5', recommended: false, effort: 'medium', efforts: efforts('medium') },
  ] },
  { instanceId: 'grok', driver: 'grok', displayName: 'Grok', account: 'Grok account', models: [{ slug: 'grok-4', name: 'Grok 4', recommended: false, effort: '', efforts: [] }] },
];
let model = { instanceId: 'claudeAgent', current: 'claude-haiku-5-5', effort: 'medium' };
const day = (back) => new Date(now - back * 86_400_000).toISOString().slice(0, 10);
const tokens = [38000, 120000, 64000, 0, 210000, 90000, 150000];
const report = (period) => {
  const n = period === 'today' ? 1 : period === '30d' ? 30 : period === 'all' ? 45 : 7;
  const series = Array.from({ length: n }, (_, i) => {
    const t = tokens[i % 7] * (period === '30d' || period === 'all' ? 1.3 : 1);
    return { day: day(n - 1 - i), turns: t ? Math.ceil(t / 20000) : 0, input: Math.round(t * 0.96), output: Math.round(t * 0.04), cached: Math.round(t * 0.7), cacheWrite: Math.round(t * 0.08), messages: t ? 3 : 0, attachments: 0 };
  });
  const sum = (k) => series.reduce((a, d) => a + d[k], 0);
  return { ok: true, period, error: '', series,
    totals: { messages: sum('messages'), attachments: 2, turns: sum('turns'), input: sum('input'), output: sum('output'), cached: sum('cached'), cacheWrite: sum('cacheWrite'), tokens: sum('input') + sum('output') },
    byModel: [{ model: 'claude-haiku-5-5', turns: 40, input: 500000, output: 20000, cached: 400000, cacheWrite: 30000, tokens: 520000 }, { model: 'claude-opus-5-5', turns: 6, input: 90000, output: 4000, cached: 0, cacheWrite: 0, tokens: 94000 }],
    byBot: state.bots.slice(0, 4).map((b, i) => ({ slug: b.slug, name: b.name, color: b.color, shape: b.shape, tokens: [520000, 90000, 40000, 8000][i], turns: 10 - i, messages: 20 - i * 4 })) };
};
const routines = [
  { id: 'r1', title: 'Morning briefing', paused: false, schedule: 'Weekdays at 8:00 AM', nextRunAt: new Date(now + 20 * 3600_000).toISOString(), lastStatus: 'succeeded' },
  { id: 'r2', title: 'Inbox cleanup', paused: false, schedule: 'Every day at 6:00 PM', nextRunAt: new Date(now + 5 * 3600_000).toISOString(), lastStatus: 'failed' },
  { id: 'r3', title: 'Weekly team update', paused: true, schedule: 'Mon at 9:00 AM', nextRunAt: '', lastStatus: 'never' },
];
const shelf = {
  ok: true, counts: { pictures: 4, files: 2, links: 2 },
  pictures: ['a', 'b', 'c', 'd'].map((id) => ({ id: `p${id}`, title: `Picture ${id}.png`, detail: 'image/png', url: `/pic/${id}` })),
  files: [{ id: 'f1', title: 'Kickoff Agenda.pdf', detail: 'Workspace file', url: '/f/1' }, { id: 'f2', title: 'Notes today.html', detail: 'Private', url: '/f/2' }],
  links: [{ id: 'https://example.com/a', title: 'example.com', detail: 'https://example.com/a', url: 'https://example.com/a' }, { id: 'https://t3.codes', title: 't3.codes', detail: 'https://t3.codes', url: 'https://t3.codes' }],
};

export function sample(method, path, params, body) {
  if (path === '/api/session') return { authenticated: true, passwordRequired: false };
  if (path === '/api/config') return { botsRoot: '/sample', version: '0.0.0', harness: { name: 'T3 Code', connected: true }, appUrl: '' };
  if (path === '/api/bots' && method === 'GET') return { bots: state.bots, now };
  if (path === '/api/settings') return { ...state.settings, defaultModel: { instanceId: 'claudeAgent', model: 'claude-haiku-5-5', options: [] } };
  if (path === '/api/suggestions') return { suggestions: [{ name: 'Night Shift', about: 'Works overnight and preps your morning digest', color: 'orange', shape: 'hex', firstMessage: 'Hi' }, { name: 'Inbox Triage', about: 'Sorts your email and drafts replies in your voice', color: 'pink', shape: 'cloud', firstMessage: 'Hi' }] };
  if (path === '/api/providers') return { ok: true, providers };
  if (path === '/api/usage/report') return report(params.get('period') ?? '7d');
  if (path === '/api/vault') return { items: [] };
  const m = path.match(/^\/api\/bots\/([^/]+)\/(.+)$/);
  if (m) {
    const [, slug, rest] = m;
    if (rest === 'chat' && method === 'GET') return { ...chatFor(slug), now };
    if (rest === 'seen') return { ok: true };
    if (rest === 'model') {
      if (method === 'PUT') { model = { instanceId: body.instanceId, current: body.model, effort: body.effort ?? '' }; return { ok: true }; }
      return { ok: true, ...model, choices: [] };
    }
    if (rest === 'routines') return { ok: true, tasks: routines, error: '' };
    if (rest === 'lineage') return { rows: [{ id: 'l1', title: 'Scout', status: 'Working', remote: false, threadId: 't-scout' }, { id: 'l2', title: 'Research on the other Mac', status: 'Done', remote: true, threadId: '' }], error: '' };
    if (rest === 'shelf') return shelf;
    if (rest === 'computer') return { ok: true, tabs: [{ id: 'tab1', title: 'calendar.google.com', url: 'https://calendar.google.com' }], image: '', error: '', etag: 'x', unchanged: false };
  }
  if (path === '/api/usage') return { totals: { messages: 40, attachments: 5, vaultWrites: 1, iconFetches: 2 } };
  if (method !== 'GET') return { ok: true };
  return undefined;
}
