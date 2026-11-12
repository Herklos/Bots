import { fill } from './palette.ts';

export const DAYS = ['Sunday', 'Monday', 'Tuesday', 'Wednesday', 'Thursday', 'Friday', 'Saturday'];
export const MONTHS = ['Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun', 'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec'];
export const clock = (d: Date): string => `${d.getHours() % 12 || 12}:${String(d.getMinutes()).padStart(2, '0')} ${d.getHours() < 12 ? 'AM' : 'PM'}`;
export const dayStart = (t: number): number => new Date(t).setHours(0, 0, 0, 0);
// A time as a list shows it: the clock today, then Yesterday, the weekday for a week, then the date.
export function when(at: unknown, now: number): string {
  if (typeof at !== 'string' || at === '' || !(now > 0)) return '';
  const d = new Date(at);
  if (Number.isNaN(d.getTime())) return '';
  const days = Math.round((dayStart(now) - dayStart(d.getTime())) / 86_400_000);
  if (days <= 0) return clock(d);
  if (days === 1) return 'Yesterday';
  if (days < 7) return DAYS[d.getDay()]!;
  return `${MONTHS[d.getMonth()]} ${d.getDate()}`;
}
// A framed prompt that leaked into a Bot's own thread (group dispatch or Computer open).
// Same needles as the gateway transcript filter for silent turns.
export function isGroupLeak(text: string): boolean {
  return text.trim().startsWith('[Group "')
    || text.includes('\n[Group "')
    || text.includes('Your reply is shared with the group')
    || text.trim().startsWith('[Computer]')
    || text.includes('\n[Computer]');
}
// The one line under a Bot's name in the inbox.
export function lineOf(b: any): string {
  if (b.working === true) return 'Working…';
  const p = b.preview;
  if (p && typeof p.text === 'string' && p.text !== '' && !isGroupLeak(p.text)) return p.role === 'user' ? `You: ${p.text}` : p.text;
  return typeof b.mainThreadId === 'string' ? 'Ready to chat' : 'Getting set up…';
}
// A group's face is its members, at most three. The list stores slugs or member objects.
export function stackFaces(members: unknown, directory: any[], workingSlugs: string[]): { shape: string; color: string; state: string }[] {
  const working = new Set(workingSlugs);
  const slugs = (Array.isArray(members) ? members : [])
    .map((m) => (typeof m === 'string' ? m : String((m as { slug?: unknown })?.slug ?? '')))
    .filter((slug) => slug !== '')
    .slice(0, 3);
  return slugs.map((slug) => {
    const bot = directory.find((b) => String(b.slug) === slug);
    return { shape: String(bot?.shape ?? 'ball'), color: fill(bot?.color), state: working.has(slug) ? 'working' : 'idle' };
  });
}
export const initials = (name: string): string => name.trim().split(/\s+/).slice(0, 2).map((w) => w.charAt(0).toUpperCase()).join('') || '?';

// The gateway's clock, remembered from the bot list and read by routines and usage.
let lastNow = 0;
export function setLastNow(now: number) {
  if (now > 0) lastNow = now;
}
export function hostNow(): number {
  return lastNow;
}

// When a provider limit window resets, in the host locale. Same day is a clock time.
export function resetWhen(at: unknown, now: number, locale: string): string {
  if (typeof at !== 'string' || at === '' || !(now > 0)) return '';
  const d = new Date(at);
  if (Number.isNaN(d.getTime())) return '';
  const loc = locale.toLowerCase().startsWith('fr') ? 'fr-FR' : 'en-US';
  const time = d.toLocaleTimeString(loc, { hour: 'numeric', minute: '2-digit' });
  if (dayStart(d.getTime()) === dayStart(now)) return time;
  return `${d.toLocaleDateString(loc, { month: 'short', day: 'numeric' })} ${time}`;
}
