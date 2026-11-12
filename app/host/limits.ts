import { REACH_GATEWAY, call, hostLocale, say } from './gateway.ts';
import { explain } from './fixture.ts';
import { hostNow, resetWhen } from './time.ts';

const STATES = ['ok', 'signed-out', 'not-installed', 'unavailable'];

export type LimitWindow = {
  id: string;
  label: string;
  usedPercent: number;
  percent: string;
  resetLabel: string;
};

export type ProviderLimit = {
  instanceId: string;
  label: string;
  state: string;
  note: string;
  hint: string;
  windows: LimitWindow[];
};

export type LimitList = {
  providers: ProviderLimit[];
  sample: boolean;
  error: string;
  at: string;
};

const fill = (text: string, values: Record<string, string>): string =>
  text.replace(/\{(\w+)\}/g, (_match, key: string) => values[key] ?? '');

const clamp = (value: number): number => Math.max(0, Math.min(100, value));

function worst(windows: LimitWindow[]): LimitWindow | undefined {
  return windows.reduce<LimitWindow | undefined>((best, window) => (
    !best || window.usedPercent > best.usedPercent ? window : best
  ), undefined);
}

function hintOf(label: string, windows: LimitWindow[], state: string): string {
  const window = worst(windows);
  if (window) {
    const values = { name: label, percent: window.percent, time: window.resetLabel };
    return window.resetLabel !== '' ? fill(say('limits.hint'), values) : fill(say('limits.hintNow'), values);
  }
  if (state === 'signed-out') return say('limits.signedOut');
  if (state === 'not-installed') return say('limits.notInstalled');
  if (state === 'unavailable') return say('limits.unavailable');
  return say('limits.quiet');
}

function windowOf(raw: any, now: number, locale: string): LimitWindow | undefined {
  const used = Number(raw?.usedPercent);
  if (!Number.isFinite(used)) return undefined;
  const id = String(raw.id ?? '');
  if (id === '') return undefined;
  const clamped = clamp(used);
  return {
    id,
    label: String(raw.label ?? id),
    usedPercent: clamped,
    percent: String(Math.round(clamped)),
    resetLabel: resetWhen(raw.resetsAt, now, locale),
  };
}

function providerOf(raw: any, now: number, locale: string): ProviderLimit | undefined {
  const instanceId = String(raw?.instanceId ?? '');
  if (instanceId === '') return undefined;
  const label = String(raw.label ?? instanceId);
  const state = STATES.includes(String(raw.state)) ? String(raw.state) : 'unavailable';
  const windows = (Array.isArray(raw.windows) ? raw.windows : [])
    .map((window: any) => windowOf(window, now, locale))
    .filter((window: LimitWindow | undefined): window is LimitWindow => window !== undefined);
  const note = raw.note === 'Could not refresh' ? say('limits.couldNotRefresh') : '';
  return { instanceId, label, state, note, hint: hintOf(label, windows, state), windows };
}

export function emptyLimits(): LimitList {
  return { providers: [], sample: false, error: '', at: '' };
}

// Shown when the contract runner, or a browser, cannot reach hbots. The sentence says so.
export function sampleLimits(sample: boolean): LimitList {
  const claude: ProviderLimit = {
    instanceId: 'claudeAgent',
    label: 'Claude',
    state: 'ok',
    note: '',
    hint: '',
    windows: [
      { id: '5h', label: '5 hour', usedPercent: 96, percent: '96', resetLabel: '5:00 PM' },
      { id: 'week', label: 'Weekly', usedPercent: 62, percent: '62', resetLabel: 'Monday' },
      { id: 'month', label: 'Monthly', usedPercent: 88, percent: '88', resetLabel: '' },
    ],
  };
  claude.hint = hintOf(claude.label, claude.windows, claude.state);
  const quiet: ProviderLimit = {
    instanceId: 'quiet', label: 'Quiet', state: 'ok', note: '', hint: '', windows: [],
  };
  quiet.hint = hintOf(quiet.label, quiet.windows, quiet.state);
  const away: ProviderLimit = {
    instanceId: 'away', label: 'Away', state: 'signed-out', note: '', hint: '', windows: [],
  };
  away.hint = hintOf(away.label, away.windows, away.state);
  return { providers: [claude, quiet, away], sample, error: '', at: '' };
}

export async function fetchLimits(gate: string, store: any, locale: string): Promise<LimitList> {
  if (gate === '0') return emptyLimits();
  if (gate === 'fixture') return sampleLimits(false);
  const r = await call(store, '/api/providers/limits');
  if (!r.ok) {
    if (r.error === REACH_GATEWAY) return sampleLimits(true);
    return { ...emptyLimits(), error: explain(r.error, 'limits') };
  }
  const now = hostNow() || Date.parse(String(r.body?.at ?? '')) || 0;
  const providers = (Array.isArray(r.body?.providers) ? r.body.providers : [])
    .map((provider: any) => providerOf(provider, now, locale))
    .filter((provider: ProviderLimit | undefined): provider is ProviderLimit => provider !== undefined);
  return { providers, sample: false, error: '', at: String(r.body?.at ?? '') };
}

export async function refreshLimitBars(store: any): Promise<{ ok: boolean; error: string; cooling: boolean }> {
  const r = await call(store, '/api/providers/refresh', { method: 'POST', body: '{}' });
  if (!r.ok) {
    if (r.error === REACH_GATEWAY) return { ok: false, error: '', cooling: true };
    return { ok: false, error: explain(r.error, 'limits'), cooling: false };
  }
  return { ok: true, error: '', cooling: r.body?.cooling === true };
}

// The chip shows the stored sentence. Localize it before the rows are built.
export function paintLimitEvents(body: any, now: number): void {
  const events = Array.isArray(body?.events) ? body.events : [];
  for (const event of events) {
    if (event?.kind !== 'provider-near-limit') continue;
    const name = String(event.label ?? '');
    const percent = String(event.percent ?? '');
    const time = resetWhen(event.resetsAt, now, hostLocale);
    const values = { name, percent, time };
    event.text = time !== '' ? fill(say('limits.near'), values) : fill(say('limits.nearNow'), values);
  }
}
