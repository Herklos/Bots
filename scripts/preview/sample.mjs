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

const state = { bots, settings: { userName: 'Alex', theme: 'system', beta: { projects: false } }, chats: {} };

// Overridden per step as the transcript grows richer.
const chatFor = (slug) => state.chats[slug] ?? {
  messages: [
    { id: 'm1', role: 'user', text: 'Morning. Go through today’s feedback.', at: ago(4) },
    { id: 'm2', role: 'bot', text: 'On it, pulling in today’s feedback now.', at: ago(3) },
  ],
  requests: [], working: slug === 'ship',
};

export function sample(method, path, params, body) {
  if (path === '/api/session') return { authenticated: true, passwordRequired: false };
  if (path === '/api/config') return { botsRoot: '/sample', version: '0.0.0', harness: { name: 'T3 Code', connected: true }, appUrl: '' };
  if (path === '/api/bots' && method === 'GET') return { bots: state.bots };
  if (path === '/api/settings') return state.settings;
  if (path === '/api/vault') return { items: [] };
  if (path === '/api/projects') return { beta: true, items: [] };
  const m = path.match(/^\/api\/bots\/([^/]+)\/(.+)$/);
  if (m) {
    const [, slug, rest] = m;
    if (rest === 'chat' && method === 'GET') return chatFor(slug);
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
