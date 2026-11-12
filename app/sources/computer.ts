import type { Sources, Result } from '../app.contract.d.ts';
import { explain } from '../host/fixture.ts';
import { noteLocale, say, localize, call, reply } from '../host/gateway.ts';
import { frames } from '../host/session.ts';

export const computerSources = {
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
} satisfies Pick<Sources, 'loadComputer' | 'openComputer' | 'closeComputer' | 'computerControl' | 'computerInput'>;
