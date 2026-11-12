import { DAYS, MONTHS, clock, dayStart, isGroupLeak, when } from './time.ts';
import type { MessagePart } from './markdown.ts';
import { FILE_EMPTY, FILE_READ, FILE_TOO_BIG, localize } from './gateway.ts';
import type { Result, Storage } from '../app.contract.d.ts';
import { API } from './config.ts';
import { fill } from './palette.ts';
import { call, read } from './gateway.ts';
import { marksOf, quoteOf, splitTables, tokenText } from './markdown.ts';

// What the gateway last said the time was, and each Bot's last computer frame. A data module may keep
// values like these between reads. The session token and the draft map live in the secret store.
export const frames: Record<string, string> = {};
export const threads: Record<string, string> = {};
// The last chat or group answer, kept when the gateway says nothing changed.
export const chatKept = new Map<string, { etag: string; view: Result<'loadChat'> }>();
export const roomKept = new Map<string, { etag: string; view: Result<'loadRoom'> }>();
export function forgetChat(slug: string): void {
  for (const key of [...chatKept.keys()]) {
    if (key.startsWith(`${slug}\n`)) chatKept.delete(key);
  }
}
// Reactions and pins for a thread. A poll reuses them for about ten seconds, or until the last message changes.
// A reaction or a pin clears the cache so that refresh reads them again.
export const META_MS = 10_000;
export const metaByThread = new Map<string, { at: number; lastId: string; meta: any }>();
export function forgetMeta(): void {
  metaByThread.clear();
}
export function lastMessageId(body: any): string {
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
export async function threadMeta(store: any, threadId: string, lastId: string, now: number): Promise<any> {
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
export const TYPES: Record<string, string> = { png: 'image/png', jpg: 'image/jpeg', jpeg: 'image/jpeg', gif: 'image/gif', webp: 'image/webp', pdf: 'application/pdf', txt: 'text/plain', md: 'text/markdown', csv: 'text/csv', json: 'application/json', mp4: 'video/mp4', m4a: 'audio/mp4' };
export const guessType = (name: string): string => TYPES[name.split('.').pop()?.toLowerCase() ?? ''] ?? 'application/octet-stream';
export const GAP = 60 * 60_000;
export const fileSize = (n: number): string => (n < 1024 ? `${n} B` : n < 1024 * 1024 ? `${(n / 1024).toFixed(1)} KB` : `${(n / 1024 / 1024).toFixed(1)} MB`);
// A day mark: the clock today, then "Yesterday 9:41 AM", the weekday for a week, then the date.
export function dayLabel(at: string, now: number): string {
  const d = new Date(at);
  const days = Math.round((dayStart(now) - dayStart(d.getTime())) / 86_400_000);
  const day = days <= 0 ? 'Today' : days === 1 ? 'Yesterday' : days < 7 ? DAYS[d.getDay()]! : `${MONTHS[d.getMonth()]} ${d.getDate()}`;
  return `${day} ${clock(d)}`;
}
export const blank = { quote: '', tokens: '', react: '', key: '', time: '', title: '', files: [] as any[], botSlug: '', botName: '', botColor: 'brand.default', botShape: 'ball', variant: '', choices: [] as any[], placeholder: '', resetAt: '', parts: [] as MessagePart[] };

// A library picture is served from the Bot's folder. Any other file stays an attachment.
// A path that is not `library/...` must not become a URL the page would fetch.
export function fileUrl(slug: string, file: { id?: unknown; path?: unknown }): string {
  const path = typeof file.path === 'string' ? file.path : '';
  const parts = path.split('/');
  const local = parts[0] === 'library' && parts.length > 1 && !path.includes('\\') && !path.includes('://')
    && parts.every((part) => part !== '' && part !== '.' && part !== '..' && !part.startsWith('.'));
  if (local) return `${API}${libraryPath(slug, path)}`;
  return `${API}/api/bots/${encodeURIComponent(slug)}/attachments/${encodeURIComponent(String(file.id ?? ''))}`;
}

// The library route, without the gateway host. The fixture uses this so the picture stays on the app origin.
export function libraryPath(slug: string, path: string): string {
  return `/api/bots/${encodeURIComponent(slug)}/library/file?path=${encodeURIComponent(path)}`;
}

// The name a screen reader hears. The Markdown alt wins when the reply gave one.
export function fileName(file: { name?: unknown; alt?: unknown }): string {
  const alt = typeof file.alt === 'string' ? file.alt.trim() : '';
  if (alt !== '') return alt;
  return String(file.name ?? '');
}

// Pixels for the image box. Missing or unusable sizes stay 0, which keeps the old layout.
export function fileDim(value: unknown): number {
  const n = typeof value === 'number' ? value : Number(value);
  if (!Number.isFinite(n) || n <= 0) return 0;
  return Math.round(n);
}

// The transcript as rows: day marks, messages in runs, the lines between them, what the Bot is
// waiting on, and the working line. A run is one sender's messages with no break between them.
export function chatRows(slug: string, body: any, now: number, reactions: Record<string, string[]> = {}, replyTo: unknown = {}): any[] {
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
      id: String(f.id), name: fileName(f), kind: String(f.kind), size: fileSize(Number(f.bytes ?? 0)),
      url: fileUrl(slug, f), width: fileDim(f.width), height: fileDim(f.height),
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

export async function storeUpload(store: any, storage: Storage, slug: string, path: string, rawName: string, rawType: string) {
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
export const DRAFT_SEP = '\u001f';
export const FILE_SEP = '\\t';
export type DraftFile = { id: string; name: string };
export type DraftVoice = { hold: string; id: string; preview: string; seconds: number };
export type DraftEntry = { text: string; files: DraftFile[]; voice?: DraftVoice };

export function draftEntry(map: Record<string, DraftEntry>, key: string): DraftEntry {
  return (map[key] ??= { text: '', files: [] });
}

export function draftMap(texts: string[], files: string[], voices: string[]): Record<string, DraftEntry> {
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

export function draftRows(raw: string | null): { stamp: string; texts: string[]; files: string[]; voices: string[] } {
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
