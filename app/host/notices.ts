import { lineOf } from './time.ts';
import { closeNotification, setAppBadge, showNotification } from '@exact/notifications';
import { botsBody, roomsBody, say } from './gateway.ts';
import { wantsNotice } from './fixture.ts';
import { isGroupLeak } from './time.ts';

// The contract runner's slugs. A real Bot still notifies.
export const FIXTURE_SLUG = (slug: string): boolean => slug === 'fixture' || slug === 'bea' || slug === 'tabs' || slug === 'frame' || slug === 'quiet';

export const STATUS_LINE = (line: string): boolean => line === '' || line === 'Ready to chat' || line === 'Getting set up…' || line === 'Working…';

// The last message id posted for a tag. The same id does not post again.
export const noticeIds = new Map<string, string>();

export type NoticePrefs = { enabled: boolean | null; sound: boolean; preview: 'full' | 'hidden'; when: 'any' | 'background' };

// A missing `enabled` is null. The poster treats that as on only when permission is already granted.
export function noticePrefs(body: any): NoticePrefs {
  const note = body?.notifications;
  return {
    enabled: typeof note?.enabled === 'boolean' ? note.enabled : null,
    sound: note?.sound !== false,
    preview: note?.preview === 'hidden' ? 'hidden' : 'full',
    when: note?.when === 'background' ? 'background' : 'any',
  };
}

export function unsupported(error: unknown): boolean {
  const code = (error as { code?: string; name?: string }).code ?? (error as { name?: string }).name;
  return code === 'NotSupportedError';
}

export async function shutNotice(tag: string): Promise<void> {
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
export async function postUnread(store: { get(name: string): string | null }, openSlug: string, openRoom: string, visibility: string, focused: boolean, host: { available: boolean; state: string }, prefs: NoticePrefs): Promise<void> {
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
