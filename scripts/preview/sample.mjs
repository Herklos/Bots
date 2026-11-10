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

export function sample(method, path, params, body) {
  if (path === '/api/session') return { authenticated: true, passwordRequired: false };
  if (path === '/api/config') return { botsRoot: '/sample', version: '0.0.0', harness: { name: 'T3 Code', connected: true }, appUrl: '' };
  if (path === '/api/bots' && method === 'GET') return { bots: state.bots, now };
  if (path === '/api/settings') return state.settings;
  if (path === '/api/vault') return { items: [] };
  if (path === '/api/projects') return { beta: true, items: [] };
  const m = path.match(/^\/api\/bots\/([^/]+)\/(.+)$/);
  if (m) {
    const [, slug, rest] = m;
    if (rest === 'chat' && method === 'GET') return { ...chatFor(slug), now };
    if (rest === 'seen') return { ok: true };
    if (rest === 'model') return { current: 'claude-haiku-5-5', instanceId: 'claudeAgent', choices: [{ instanceId: 'claudeAgent', model: 'claude-haiku-5-5', label: 'Haiku' }] };
    if (rest === 'routines') return { tasks: [], error: '' };
    if (rest === 'lineage') return { rows: [], error: '' };
    if (rest === 'library') return { ok: true, entries: [], connectors: [] };
    if (rest === 'computer') return { ok: true, tabs: [], image: '', error: '' };
    if (rest === 'usage') return { ok: true, error: '', messages: 12, attachments: 2, turns: 9, inputTokens: 120000, outputTokens: 8000, cachedInputTokens: 90000, cacheCreationTokens: 4000, tokensKnown: true };
  }
  if (path === '/api/usage') return { messages: 40, attachments: 5, vaultWrites: 1, iconFetches: 2 };
  if (method !== 'GET') return { ok: true };
  return undefined;
}
