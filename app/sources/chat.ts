import type { Sources, Result } from '../app.contract.d.ts';
import { blankChat, PinRow, fixtureSent, fixturePins, quietChatView, fixtureChat } from '../host/fixture.ts';
import { noteLocale, call, read, fresh, reply, sentFrom } from '../host/gateway.ts';
import { NOT_IN_VIEW } from '../host/markdown.ts';
import { FIXTURE_SLUG, shutNotice } from '../host/notices.ts';
import { threads, chatKept, forgetChat, forgetMeta, lastMessageId, threadMeta, chatRows, storeUpload, draftMap, draftRows } from '../host/session.ts';

export const chatSources = {
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
  loadChat: async ([slug, child], store): Promise<Result<'loadChat'>> => {
    if (slug === '') return blankChat('');
    if (slug === 'quiet') return quietChatView();
    if (slug === 'fixture') return fixtureChat(child);
    if (slug === 'bea') return blankChat('bea');
    if (slug === 'tabs') return blankChat('tabs');
    if (slug === 'frame') return blankChat('frame');
    if (slug === 'page') return blankChat('page');
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
} satisfies Pick<Sources, 'reactTo' | 'pinMessage' | 'loadChat' | 'respondRequest' | 'sendMessage' | 'uploadAttachment' | 'stageVoice' | 'sendVoice' | 'dropVoice' | 'stopRun' | 'loadDrafts' | 'saveDrafts'>;
