import { localize, say } from './gateway.ts';
import { FIXTURE_SLUG } from './notices.ts';

export const LASTING_WHEN = 'Create a lasting helper when the work will outlive this turn and needs its own notes, skills, or ongoing thread. Prefer a one-off child thread for work that ends with this turn.';
export const ONCE_WHEN = 'Start a child thread for parallel or specialised work that ends with this turn. Do not create a lasting Subagent for that.';

const MODES = ['approval-required', 'auto', 'full-access'] as const;
const EFFORTS = ['low', 'medium', 'high'] as const;
const CROWD = new Set(['ada-junior-1', 'fixture-pick', 'one', 'two', 'three', 'four', 'five', 'six', 'seven']);

export interface HelpSide {
  enabled: boolean;
  cap: string;
  when: string;
  mode: string;
  effort: string;
  inherit: boolean;
  modelInstance: string;
  modelSlug: string;
  anyModel: boolean;
  modelList: string;
  modes: string;
  efforts: string;
}

export interface HelpPolicy {
  ok: boolean;
  error: string;
  lasting: HelpSide;
  once: HelpSide;
}

export interface HelpWrite {
  kind: string;
  enabled: boolean;
  cap: string;
  whenText: string;
  mode: string;
  effort: string;
  inherit: boolean;
  modelInstance: string;
  modelSlug: string;
  anyModel: boolean;
  modelList: string;
  modes: string;
  efforts: string;
}

const remembered = new Map<string, HelpPolicy>();

export function localPolicy(slug: string): boolean {
  return FIXTURE_SLUG(slug) || CROWD.has(slug);
}

export function blankSide(lasting: boolean): HelpSide {
  return {
    enabled: true,
    cap: lasting ? '8' : '4',
    when: lasting ? LASTING_WHEN : ONCE_WHEN,
    mode: '',
    effort: '',
    inherit: false,
    modelInstance: '',
    modelSlug: '',
    anyModel: true,
    modelList: '',
    modes: MODES.join(','),
    efforts: EFFORTS.join(','),
  };
}

export function blankPolicy(error = '', ok = error === ''): HelpPolicy {
  return { ok, error, lasting: blankSide(true), once: blankSide(false) };
}

export function fixturePolicy(slug: string): HelpPolicy {
  const current = remembered.get(slug);
  if (current) return current;
  const next = blankPolicy();
  remembered.set(slug, next);
  return next;
}

export function rememberPolicy(slug: string, policy: HelpPolicy) {
  remembered.set(slug, policy);
}

function text(value: unknown): string {
  return typeof value === 'string' ? value : '';
}

function ordered(allowed: readonly string[], value: unknown[]): string {
  const picked = new Set(value.filter((item): item is string => typeof item === 'string'));
  return allowed.filter((id) => picked.has(id)).join(',');
}

export function readSide(raw: unknown, lasting: boolean): HelpSide {
  const base = blankSide(lasting);
  if (!raw || typeof raw !== 'object') return base;
  const obj = raw as Record<string, unknown>;
  const defaults = obj.defaults && typeof obj.defaults === 'object' ? obj.defaults as Record<string, unknown> : {};
  const allow = obj.allowlist && typeof obj.allowlist === 'object' ? obj.allowlist as Record<string, unknown> : {};
  const model = defaults.model && typeof defaults.model === 'object' ? defaults.model as Record<string, unknown> : null;
  const models = allow.models;
  const modelList = Array.isArray(models)
    ? models.map((row) => {
      const item = row as Record<string, unknown>;
      const instanceId = text(item.instanceId);
      const name = text(item.model);
      return instanceId !== '' && name !== '' ? `${instanceId}\t${name}` : '';
    }).filter((line) => line !== '').join('\n')
    : '';
  const capValue = lasting ? obj.maxCount : obj.maxConcurrent;
  return {
    enabled: obj.enabled !== false,
    cap: typeof capValue === 'number' && Number.isInteger(capValue) ? String(capValue) : base.cap,
    when: typeof obj.when === 'string' ? obj.when : base.when,
    mode: typeof defaults.runtimeMode === 'string' ? defaults.runtimeMode : '',
    effort: typeof defaults.effort === 'string' ? defaults.effort : '',
    inherit: defaults.inheritParentPrompt === true,
    modelInstance: model ? text(model.instanceId) : '',
    modelSlug: model ? text(model.model) : '',
    anyModel: models == null,
    modelList,
    modes: Array.isArray(allow.runtimeModes) ? ordered(MODES, allow.runtimeModes) : base.modes,
    efforts: Array.isArray(allow.efforts) ? ordered(EFFORTS, allow.efforts) : base.efforts,
  };
}

export function policyFromBot(body: unknown): HelpPolicy {
  if (!body || typeof body !== 'object') return blankPolicy(say('error.helpers'));
  const bot = body as Record<string, unknown>;
  return { ok: true, error: '', lasting: readSide(bot.subagents, true), once: readSide(bot.childThreads, false) };
}

function listed(allowed: readonly string[], value: string): string | null {
  return allowed.some((id) => id === value) ? value : null;
}

export function policyBody(input: HelpWrite): { ok: true; body: Record<string, unknown> } | { ok: false; error: string } {
  const lasting = input.kind === 'lasting';
  if (!lasting && input.kind !== 'once') return { ok: false, error: say('helpers.none') };
  const max = lasting ? 32 : 16;
  const capText = input.cap.trim();
  const cap = Number(capText);
  if (!/^\d+$/.test(capText) || cap > max) return { ok: false, error: say('helpers.capInvalid') };
  if ([...input.whenText].length > 2000) return { ok: false, error: say('helpers.whenLong') };
  const runtimeModes = MODES.filter((id) => input.modes.split(',').some((part) => part === id));
  const efforts = EFFORTS.filter((id) => input.efforts.split(',').some((part) => part === id));
  const wanted = new Set(input.modelList.split('\n').filter((line) => line !== ''));
  const models = input.anyModel ? null : [...wanted].map((line) => {
    const [instanceId, model] = line.split('\t');
    return { instanceId: instanceId ?? '', model: model ?? '' };
  }).filter((row) => row.instanceId !== '' && row.model !== '');
  const picked = input.modelInstance !== '' && input.modelSlug !== ''
    ? { instanceId: input.modelInstance, model: input.modelSlug }
    : null;
  const allowedPick = picked !== null && (models === null || models.some((row) => row.instanceId === picked.instanceId && row.model === picked.model));
  const defaults: Record<string, unknown> = {
    model: allowedPick ? picked : null,
    runtimeMode: listed(runtimeModes, input.mode),
    effort: listed(efforts, input.effort),
  };
  if (lasting) defaults.inheritParentPrompt = input.inherit;
  const key = lasting ? 'subagents' : 'childThreads';
  return {
    ok: true,
    body: {
      [key]: {
        enabled: input.enabled,
        [lasting ? 'maxCount' : 'maxConcurrent']: cap,
        when: input.whenText,
        defaults,
        allowlist: { models, runtimeModes, efforts },
      },
    },
  };
}

export function applyWrite(policy: HelpPolicy, input: HelpWrite): HelpPolicy {
  const built = policyBody(input);
  if (!built.ok) return policy;
  const key = input.kind === 'lasting' ? 'subagents' : 'childThreads';
  const side = readSide(built.body[key], input.kind === 'lasting');
  return input.kind === 'lasting' ? { ...policy, lasting: side } : { ...policy, once: side };
}

export function loadError(error: string): HelpPolicy {
  return blankPolicy(localize(error), false);
}
