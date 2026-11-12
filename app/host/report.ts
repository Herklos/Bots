import { DAYS, MONTHS, clock, dayStart } from './time.ts';
import { API } from './config.ts';
import { fill } from './palette.ts';
import { say } from './gateway.ts';
import { when } from './time.ts';

export function libraryEntry(slug: string, e: any) {
  const id = String(e.id ?? '');
  const prefix = `file:${slug}/`;
  const path = id.startsWith(prefix) ? id.slice(prefix.length) : '';
  const detail = String(e.detail ?? '');
  const open = path !== ''
    ? `${API}/api/bots/${encodeURIComponent(slug)}/library/file?path=${encodeURIComponent(path)}`
    : (detail.startsWith('http://') || detail.startsWith('https://') ? detail : '');
  return {
    id,
    kind: String(e.kind ?? ''),
    title: String(e.title ?? ''),
    detail,
    group: String(e.group ?? ''),
    status: String(e.status ?? ''),
    body: String(e.body ?? ''),
    path,
    open,
  };
}

export const compact = (n: number): string => (n >= 1e6 ? `${(n / 1e6).toFixed(n >= 1e7 ? 0 : 1)}M` : n >= 1e3 ? `${(n / 1e3).toFixed(n >= 1e4 ? 0 : 1)}k` : String(n)).replace('.0', '');
// "claude-opus-5-5" as "Claude Opus 5.5" when T3 gave no name.
export const prettyModel = (slug: string): string => slug.split('-').filter(Boolean).map((w, i, all) => (/^\d+$/.test(w) && /^\d+$/.test(all[i + 1] ?? '') ? `${w}.` : w.charAt(0).toUpperCase() + w.slice(1))).join(' ').replace(/\. /g, '.');
export const dayOf = (day: string): Date => new Date(Number(day.slice(0, 4)), Number(day.slice(5, 7)) - 1, Number(day.slice(8, 10)));
// When a routine runs next: the clock today or tomorrow, then the weekday.
export function nextLabel(at: unknown, now: number): string {
  if (typeof at !== 'string' || at === '' || !(now > 0)) return '';
  const d = new Date(at);
  if (Number.isNaN(d.getTime())) return '';
  const days = Math.round((dayStart(d.getTime()) - dayStart(now)) / 86_400_000);
  const day = days <= 0 ? 'Today' : days === 1 ? 'Tomorrow' : days < 7 ? DAYS[d.getDay()]! : `${MONTHS[d.getMonth()]} ${d.getDate()}`;
  return `Next: ${day} ${clock(d)}`;
}
export const PERIODS: Record<string, string> = { today: 'Today', '7d': '7 days', '30d': '30 days', all: 'All time' };
export function emptyReport(error: string) {
  return { ok: error === '', error, period: '', periodLabel: '', messages: '0', attachments: '0', turns: '0', tokens: '0', input: '0', output: '0', cached: '0', cacheWrite: '0', hit: '', sentence: '', days: [] as any[], rows: [] as any[], models: [] as any[], bots: [] as any[], empty: true, first: '', last: '' };
}
// The gateway's usage report as the panel and Settings show it: figures as words, bars as heights.
export function usageReport(b: any) {
  const t = b.totals ?? {};
  const series = ((b.series ?? []) as any[]);
  const peak = Math.max(1, ...series.map((d) => Number(d.input) + Number(d.output)));
  const days = series.map((d) => {
    const tokens = Number(d.input) + Number(d.output);
    const date = dayOf(String(d.day));
    return { day: String(d.day), label: `${DAYS[date.getDay()]!.slice(0, 3)} ${date.getDate()}`, short: String(date.getDate()), tokens: compact(tokens), input: compact(Number(d.input)), output: compact(Number(d.output)), cached: compact(Number(d.cached)), cacheWrite: compact(Number(d.cacheWrite)),
      turns: String(d.turns), messages: String(d.messages), px: tokens === 0 ? 2 : Math.max(4, Math.round((tokens / peak) * 88)), spx: tokens === 0 ? 2 : Math.max(3, Math.round((tokens / peak) * 28)) };
  });
  const models = ((b.byModel ?? []) as any[]);
  const top = Math.max(1, ...models.map((m) => Number(m.tokens)));
  const bots = ((b.byBot ?? []) as any[]).filter((x) => Number(x.tokens) + Number(x.messages) > 0);
  const botTop = Math.max(1, ...bots.map((x) => Number(x.tokens)));
  const input = Number(t.input ?? 0);
  return {
    ok: true, error: String(b.error ?? ''), period: String(b.period ?? ''), periodLabel: PERIODS[String(b.period)] ?? '',
    messages: compact(Number(t.messages ?? 0)), attachments: compact(Number(t.attachments ?? 0)), turns: compact(Number(t.turns ?? 0)),
    tokens: compact(Number(t.tokens ?? 0)), input: compact(input), output: compact(Number(t.output ?? 0)), cached: compact(Number(t.cached ?? 0)), cacheWrite: compact(Number(t.cacheWrite ?? 0)),
    hit: input > 0 ? `${Math.round((Number(t.cached ?? 0) / input) * 100)}%` : '',
    sentence: input > 0 ? `${Math.round((Number(t.cached ?? 0) / input) * 100)}% of input came from cache.` : (Number(t.messages ?? 0) > 0 ? '' : 'Nothing counted in this period.'),
    days, empty: Number(t.tokens ?? 0) === 0 && Number(t.messages ?? 0) === 0,
    first: days[0]?.label ?? '', last: days[days.length - 1]?.label ?? '',
    rows: [...days].reverse().filter((d) => d.tokens !== '0' || d.messages !== '0'),
    models: models.map((m) => ({ name: prettyModel(String(m.model || 'Unknown')), tokens: compact(Number(m.tokens)), turns: String(m.turns), share: Math.max(4, Math.round((Number(m.tokens) / top) * 100)) })),
    bots: bots.map((x) => ({ slug: String(x.slug), name: String(x.name), color: fill(x.color), shape: String(x.shape ?? 'ball'), tokens: compact(Number(x.tokens)), turns: String(x.turns), messages: String(x.messages), share: Math.max(4, Math.round((Number(x.tokens) / botTop) * 100)) })),
  };
}
