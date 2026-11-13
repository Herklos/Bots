import type { Sources, Result } from '../app.contract.d.ts';
import { FIXTURE_HOOK_ROTATED, FIXTURE_HOOK_URL, FIXTURE_SCHEDULE_ID, fixtureAddedHook, explain, fixtureDeliveries, fixtureDelivery } from '../host/fixture.ts';
import { noteLocale, say, call } from '../host/gateway.ts';
import { hostNow, when } from '../host/time.ts';

const blankHook = (id: string) => ({ ok: false, error: '', id, url: '', source: '', stable: false });
const blankMade = (error = '') => ({ ok: false, error, id: '', url: '', source: '', stable: false });
const blankList = { ok: true, error: '', rows: [] as Result<'loadDeliveries'>['rows'] };
const blankDetail = { ok: true, error: '', id: '', at: '', outcome: '', prompt: '', body: '', truncated: false, headers: [] as Result<'loadDelivery'>['headers'] };

function atOf(value: unknown): string {
  const stamp = String(value ?? '');
  return when(stamp, hostNow()) || stamp.slice(0, 16);
}

function rowsOf(body: any) {
  const rows = Array.isArray(body?.deliveries) ? body.deliveries : [];
  return rows.map((row: any) => ({
    id: String(row.id ?? ''),
    at: atOf(row.receivedAt),
    method: String(row.method ?? ''),
    bytes: String(row.bodyBytes ?? 0),
    outcome: String(row.outcome ?? ''),
  }));
}

function headersOf(value: unknown) {
  if (!value || typeof value !== 'object' || Array.isArray(value)) return [];
  return Object.entries(value as Record<string, unknown>).map(([name, text]) => ({
    name,
    value: String(text ?? ''),
  }));
}

function detailOf(row: any) {
  return {
    ok: true,
    error: '',
    id: String(row.id ?? ''),
    at: atOf(row.receivedAt),
    outcome: String(row.outcome ?? ''),
    prompt: String(row.renderedPrompt ?? ''),
    body: String(row.body ?? ''),
    truncated: row.bodyTruncated === true,
    headers: headersOf(row.headers),
  };
}

export const webhookSources: Pick<Sources, 'readHook' | 'rotateHook' | 'loadDeliveries' | 'loadDelivery' | 'saveRoutine' | 'saveWebhookRoutine' | 'testWebhookRoutine'> = {
  readHook: async ([slug, id, locale], store): Promise<Result<'readHook'>> => {
    noteLocale(locale);
    if (slug === 'fixture') {
      return id === 'hook'
        ? { ok: true, error: '', id, url: FIXTURE_HOOK_URL, source: 'publicBase', stable: true }
        : { ...blankHook(id), error: say('hook.missing') };
    }
    const r = await call(store, `/api/bots/${encodeURIComponent(slug)}/routines/${encodeURIComponent(id)}/hook`);
    if (!r.ok) return { ...blankHook(id), error: explain(r.error, 'routines') };
    const url = String(r.body.url ?? '');
    if (url === '') return { ...blankHook(id), error: say('hook.missing') };
    return { ok: true, error: '', id, url, source: String(r.body.source ?? ''), stable: r.body.stable === true };
  },
  rotateHook: async ([slug, id, locale], store): Promise<Result<'rotateHook'>> => {
    noteLocale(locale);
    if (slug === 'fixture') {
      return id === 'hook'
        ? { ok: true, error: '', id, url: FIXTURE_HOOK_ROTATED, source: 'publicBase', stable: true }
        : { ...blankHook(id), error: say('hook.missing') };
    }
    const r = await call(store, `/api/bots/${encodeURIComponent(slug)}/routines/${encodeURIComponent(id)}/rotate`, { method: 'POST' });
    if (!r.ok) return { ...blankHook(id), error: explain(r.error, 'routines') };
    const url = String(r.body.url ?? '');
    if (url === '') return { ...blankHook(id), error: say('hook.missing') };
    return { ok: true, error: '', id, url, source: String(r.body.source ?? ''), stable: r.body.stable === true };
  },
  loadDeliveries: async ([slug, id, gate, locale], store): Promise<Result<'loadDeliveries'>> => {
    noteLocale(locale);
    if (slug === '' || id === '' || gate !== '1') return blankList;
    if (slug === 'fixture') return { ok: true, error: '', rows: rowsOf({ deliveries: fixtureDeliveries() }) };
    const r = await call(store, `/api/bots/${encodeURIComponent(slug)}/routines/${encodeURIComponent(id)}/deliveries`);
    if (!r.ok) return { ...blankList, ok: false, error: explain(r.error, 'deliveries') };
    return { ok: true, error: '', rows: rowsOf(r.body) };
  },
  loadDelivery: async ([slug, id, deliveryId, gate, locale], store): Promise<Result<'loadDelivery'>> => {
    noteLocale(locale);
    if (slug === '' || id === '' || deliveryId === '' || gate !== '1') return blankDetail;
    if (slug === 'fixture') {
      const row = fixtureDelivery(deliveryId);
      return row ? detailOf(row) : { ...blankDetail, ok: false, error: say('hook.detailMissing') };
    }
    const path = `/api/bots/${encodeURIComponent(slug)}/routines/${encodeURIComponent(id)}/deliveries/${encodeURIComponent(deliveryId)}`;
    const r = await call(store, path);
    if (!r.ok) return { ...blankDetail, ok: false, error: explain(r.error, 'deliveries') };
    return detailOf(r.body.delivery ?? r.body);
  },
  saveRoutine: async ([slug, kind, title], store): Promise<Result<'saveRoutine'>> => {
    if (slug === 'fixture') return { ok: true, error: '', id: FIXTURE_SCHEDULE_ID, url: '', source: '', stable: false };
    const r = await call(store, `/api/bots/${encodeURIComponent(slug)}/routines`, {
      method: 'POST',
      body: JSON.stringify({ kind, title }),
    });
    if (!r.ok) return blankMade(explain(r.error, 'routines'));
    return {
      ok: true,
      error: '',
      id: String(r.body.task?.id ?? ''),
      url: '',
      source: '',
      stable: false,
    };
  },
  saveWebhookRoutine: async ([slug, title, prompt, preset, secret, f1Path, f1Op, f1Value, f2Path, f2Op, f2Value, replay, age, enable], store): Promise<Result<'saveWebhookRoutine'>> => {
    if (slug === 'fixture') return fixtureAddedHook();
    const filters = [] as Record<string, unknown>[];
    if (f1Path !== '') filters.push({ path: f1Path, op: f1Op, ...(f1Op === 'exists' ? {} : { value: f1Value }) });
    if (f2Path !== '') filters.push({ path: f2Path, op: f2Op, ...(f2Op === 'exists' ? {} : { value: f2Value }) });
    const body: Record<string, unknown> = {
      kind: 'webhook',
      title,
      prompt,
      preset,
      ...(secret !== '' ? { secret } : {}),
      ...(filters.length > 0 ? { filters } : {}),
      ...(replay !== '' ? { dedupe: { replayKey: replay } } : {}),
      ...(age !== '' ? { maxAgeMinutes: Number(age) } : {}),
      enable,
    };
    const r = await call(store, `/api/bots/${encodeURIComponent(slug)}/routines`, {
      method: 'POST',
      body: JSON.stringify(body),
    });
    if (!r.ok) return blankMade(explain(r.error, 'routines'));
    // The address is returned only once, here. The polling list never has it.
    return {
      ok: true,
      error: '',
      id: String(r.body.task?.id ?? ''),
      url: String(r.body.hook?.url ?? ''),
      source: String(r.body.hook?.source ?? ''),
      stable: r.body.hook?.stable === true,
    };
  },
  testWebhookRoutine: async ([slug, id, body, secret], store): Promise<Result<'testWebhookRoutine'>> => {
    if (slug === 'fixture') return { ok: true, error: '', outcome: 'accepted' };
    const r = await call(store, `/api/bots/${encodeURIComponent(slug)}/routines/${encodeURIComponent(id)}/test`, {
      method: 'POST',
      // The secret is signed into this one request and dropped. It is not stored.
      body: JSON.stringify({ body, ...(secret !== '' ? { secret } : {}) }),
    });
    if (!r.ok) return { ok: false, error: explain(r.error, 'routines'), outcome: '' };
    return { ok: r.body.ok === true, error: '', outcome: String(r.body.outcome ?? '') };
  },
};
