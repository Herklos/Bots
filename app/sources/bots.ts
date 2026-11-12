import type { Sources, Result } from '../app.contract.d.ts';
import { fixtureCrowdBots, fixtureBotNotify, fixtureBotPrompt, fixturePrompt, wantsNotice, FIXTURE_SUGGESTIONS } from '../host/fixture.ts';
import { call, botsBody, reply, madeFromBot } from '../host/gateway.ts';
import { FIXTURE_SLUG } from '../host/notices.ts';
import { fill } from '../host/palette.ts';
import { when, lineOf, initials, setLastNow } from '../host/time.ts';

export const botSources = {
  loadBots: async ([focus], store): Promise<Result<'loadBots'>> => {
    // Reserved slugs for the contract runner, which has no gateway. A real inbox still fetches.
    // `bea` is the second fixture Bot, so a draft test can open her and come back without a gateway.
    if (focus === 'tabs' || focus === 'frame') {
      const slug = focus === 'frame' ? 'frame' : 'tabs';
      const name = focus === 'frame' ? 'Frame' : 'Tabs';
      return [
        { slug, name, label: '', initials: name.slice(0, 1), color: fill('brand.default'), main: false, hidden: false, ready: true, unread: false, shape: 'ball', pinned: false, working: false, line: 'Ready to chat', time: '', parentSlug: '', hideFromSidebar: false, subagent: false, prompt: fixturePrompt(slug, ''), inheritParentPrompt: false, aboveLimit: false, notifications: wantsNotice(fixtureBotNotify.get(slug)) },
      ];
    }
    if (focus === 'quiet') {
      return [
        { slug: 'quiet', name: 'Quiet', label: '', initials: 'Q', color: fill('brand.default'), main: false, hidden: false, ready: true, unread: false, shape: 'ball', pinned: false, working: false, line: 'Ready to chat', time: '', parentSlug: '', hideFromSidebar: false, subagent: false, prompt: fixturePrompt('quiet', ''), inheritParentPrompt: false, aboveLimit: false, notifications: wantsNotice(fixtureBotNotify.get('quiet')) },
      ];
    }
    if (focus === 'fixture' || focus === 'bea' || focus === 'fixture-crowd') {
      return [
        { slug: 'fixture', name: 'Ada', label: '', initials: 'A', color: fill('brand.default'), main: false, hidden: false, ready: true, unread: true, shape: 'ball', pinned: false, working: false, line: 'Ready to chat', time: '', parentSlug: '', hideFromSidebar: false, subagent: false, prompt: fixturePrompt('fixture', ''), inheritParentPrompt: false, aboveLimit: false, notifications: wantsNotice(fixtureBotNotify.get('fixture')) },
        { slug: 'ada-junior-1', name: 'Ada junior 1', label: '', initials: 'AJ', color: fill('brand.default'), main: false, hidden: false, ready: true, unread: false, shape: 'ball', pinned: false, working: false, line: 'Ready to chat', time: '', parentSlug: 'fixture', hideFromSidebar: true, subagent: true, prompt: fixturePrompt('ada-junior-1', 'Look things up.'), inheritParentPrompt: true, aboveLimit: false, notifications: wantsNotice(fixtureBotNotify.get('ada-junior-1')) },
        { slug: 'bea', name: 'Bea', label: '', initials: 'B', color: fill('brand.blue'), main: false, hidden: true, ready: false, unread: true, shape: 'ball', pinned: false, working: false, line: 'Getting set up…', time: '', parentSlug: '', hideFromSidebar: false, subagent: false, prompt: fixturePrompt('bea', ''), inheritParentPrompt: false, aboveLimit: false, notifications: wantsNotice(fixtureBotNotify.get('bea')) },
        ...(focus === 'fixture-crowd' ? fixtureCrowdBots() : []),
      ];
    }
    const body = await botsBody(store);
    setLastNow(Number(body.now));
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
      prompt: (b.role === 'subagent' || b.hideFromSidebar === true)
        ? (typeof b.subagent?.prompt === 'string' ? b.subagent.prompt : '')
        : (typeof b.prompt === 'string' ? b.prompt : ''),
      inheritParentPrompt: b.subagent?.inheritParentPrompt === true,
      aboveLimit: b.aboveLimit === true,
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
  renameBot: async ([slug, name], store) => reply(await call(store, `/api/bots/${encodeURIComponent(slug)}`, { method: 'PUT', body: JSON.stringify({ name }) })),
  saveBotPrompt: async ([slug, prompt], store) => {
    if (FIXTURE_SLUG(slug) || slug === 'ada-junior-1') {
      fixtureBotPrompt.set(slug, prompt);
      return { ok: true, error: '' };
    }
    return reply(await call(store, `/api/bots/${encodeURIComponent(slug)}`, { method: 'PUT', body: JSON.stringify({ prompt }) }));
  },
  recolourBot: async ([slug, color], store) => reply(await call(store, `/api/bots/${encodeURIComponent(slug)}`, { method: 'PUT', body: JSON.stringify({ color }) })),
  hideBot: async ([slug, hidden], store) => reply(await call(store, `/api/bots/${encodeURIComponent(slug)}`, { method: 'PUT', body: JSON.stringify({ hidden }) })),
  makeMainBot: async ([slug], store) => reply(await call(store, '/api/main-bot', { method: 'PUT', body: JSON.stringify({ slug }) })),
  reshapeBot: async ([slug, shape], store) => reply(await call(store, `/api/bots/${encodeURIComponent(slug)}`, { method: 'PUT', body: JSON.stringify({ shape }) })),
  pinBot: async ([slug, pinned], store) => reply(await call(store, `/api/bots/${encodeURIComponent(slug)}`, { method: 'PUT', body: JSON.stringify({ pinned }) })),
  deleteBot: async ([slug], store) => reply(await call(store, `/api/bots/${encodeURIComponent(slug)}`, { method: 'DELETE' })),
  createBot: async ([name, color, shape, firstMessage, prompt], store) => {
    // The contract runner has no gateway. A real Bot still posts.
    // The reserved name is how that runner calls createBot. The slug in the body is what Made.id must keep.
    // Colour, shape and the first message have to arrive with that name, or the walk did not send them.
    if (name === 'fixture') {
      if (color === 'brand.violet' && shape === 'hex' && firstMessage === 'Hello from the walk') {
        fixtureBotPrompt.set('bea', prompt);
        return madeFromBot({ ok: true, body: { slug: 'bea', name } });
      }
      return { ok: false, id: '', error: 'Create was missing the colour, shape, or first message.' };
    }
    const r = await call(store, '/api/bots', { method: 'POST', body: JSON.stringify({ name, color, shape, prompt, ...(firstMessage !== '' ? { firstMessage } : {}) }) });
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
  markSeen: async ([slug], store) => reply(await call(store, `/api/bots/${encodeURIComponent(slug)}/seen`, { method: 'POST', body: '{}' })),
  loadSuggestions: async (_args, store): Promise<Result<'loadSuggestions'>> => {
    const r = await call(store, '/api/suggestions');
    // The contract runner has no gateway. The same starting points the gateway serves.
    const rows = r.ok && Array.isArray(r.body.suggestions) && r.body.suggestions.length > 0 ? r.body.suggestions : FIXTURE_SUGGESTIONS;
    return (rows as any[]).map((s) => ({ name: String(s.name), about: String(s.about ?? ''), color: fill(s.color), shape: String(s.shape ?? 'ball'), first: String(s.firstMessage ?? '') }));
  },
} satisfies Pick<Sources, 'loadBots' | 'setBotNotifications' | 'renameBot' | 'saveBotPrompt' | 'recolourBot' | 'hideBot' | 'makeMainBot' | 'reshapeBot' | 'pinBot' | 'deleteBot' | 'createBot' | 'createSubagent' | 'saveSubagent' | 'markSeen' | 'loadSuggestions'>;
