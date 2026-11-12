import { expect, mock, test } from 'bun:test';

mock.module('./gateway.ts', () => ({
  say: (key: string) => key,
  localize: (text: string) => text,
}));
mock.module('./notices.ts', () => ({
  FIXTURE_SLUG: (slug: string) => slug === 'fixture',
}));

const { applyWrite, blankPolicy, policyBody, policyFromBot, readSide, LASTING_WHEN, ONCE_WHEN } = await import('./helpers.ts');

test('a missing policy reads as the product defaults', () => {
  const policy = policyFromBot({});
  expect(policy.ok).toBe(true);
  expect(policy.lasting.enabled).toBe(true);
  expect(policy.lasting.cap).toBe('8');
  expect(policy.lasting.when).toBe(LASTING_WHEN);
  expect(policy.lasting.anyModel).toBe(true);
  expect(policy.lasting.modes).toBe('approval-required,auto,full-access');
  expect(policy.lasting.efforts).toBe('low,medium,high');
  expect(policy.once.cap).toBe('4');
  expect(policy.once.when).toBe(ONCE_WHEN);
  expect(policy.once.inherit).toBe(false);
});

test('a save keeps the ceiling inside the stored shape', () => {
  const built = policyBody({
    kind: 'lasting',
    enabled: false,
    cap: '3',
    whenText: 'Only for research.',
    mode: 'auto',
    effort: 'high',
    inherit: true,
    modelInstance: 'claudeAgent',
    modelSlug: 'haiku',
    anyModel: false,
    modelList: 'claudeAgent\thaiku\nclaudeAgent\tsonnet',
    modes: 'full-access,auto',
    efforts: 'high,low',
  });
  expect(built.ok).toBe(true);
  if (!built.ok) return;
  const stored = built.body.subagents as Record<string, unknown>;
  expect(stored.enabled).toBe(false);
  expect(stored.maxCount).toBe(3);
  expect(stored.when).toBe('Only for research.');
  const defaults = stored.defaults as Record<string, unknown>;
  expect(defaults.runtimeMode).toBe('auto');
  expect(defaults.effort).toBe('high');
  expect(defaults.inheritParentPrompt).toBe(true);
  expect(defaults.model).toEqual({ instanceId: 'claudeAgent', model: 'haiku' });
  const allow = stored.allowlist as Record<string, unknown>;
  expect(allow.runtimeModes).toEqual(['auto', 'full-access']);
  expect(allow.efforts).toEqual(['low', 'high']);
  expect(allow.models).toEqual([
    { instanceId: 'claudeAgent', model: 'haiku' },
    { instanceId: 'claudeAgent', model: 'sonnet' },
  ]);
  const side = readSide(stored, true);
  expect(side.enabled).toBe(false);
  expect(side.cap).toBe('3');
  expect(side.modelSlug).toBe('haiku');
  expect(side.anyModel).toBe(false);
});

test('an open model ceiling is stored as null and a bad cap is refused', () => {
  const open = policyBody({
    kind: 'once',
    enabled: true,
    cap: '4',
    whenText: ONCE_WHEN,
    mode: 'nope',
    effort: '',
    inherit: true,
    modelInstance: '',
    modelSlug: '',
    anyModel: true,
    modelList: '',
    modes: 'approval-required,auto,full-access',
    efforts: 'low,medium,high',
  });
  expect(open.ok).toBe(true);
  if (!open.ok) return;
  const threads = open.body.childThreads as Record<string, unknown>;
  expect(threads.maxConcurrent).toBe(4);
  expect((threads.allowlist as Record<string, unknown>).models).toBeNull();
  expect((threads.defaults as Record<string, unknown>).runtimeMode).toBeNull();
  expect((threads.defaults as Record<string, unknown>).inheritParentPrompt).toBeUndefined();
  const refused = policyBody({ ...blankWrite(), cap: '40' });
  expect(refused.ok).toBe(false);
  const next = applyWrite(blankPolicy(), { ...blankWrite(), enabled: false, cap: '1' });
  expect(next.lasting.enabled).toBe(false);
  expect(next.lasting.cap).toBe('1');
  expect(next.once.cap).toBe('4');
});

function blankWrite() {
  return {
    kind: 'lasting',
    enabled: true,
    cap: '8',
    whenText: LASTING_WHEN,
    mode: '',
    effort: '',
    inherit: false,
    modelInstance: '',
    modelSlug: '',
    anyModel: true,
    modelList: '',
    modes: 'approval-required,auto,full-access',
    efforts: 'low,medium,high',
  };
}
