import type { Sources, Result } from '../app.contract.d.ts';
import { REACH_GATEWAY, say, localize, call, read, noteLocale, reply } from '../host/gateway.ts';
import { clock } from '../host/time.ts';
import { fixtureSyncBody, fixtureSyncImport } from '../host/fixture.ts';

// The Anytype Sync card's sources. The gateway owns the state and the counts; this
// layer formats times for the host locale from the strings table. The contract
// runner has no gateway, so it gets a canned snapshot instead.
const numbered = (value: unknown): number => (typeof value === 'number' && Number.isFinite(value) ? value : 0);
const text = (value: unknown): string => (typeof value === 'string' ? value : '');

function syncStatus(body: any): Result<'loadSync'> {
  return {
    ok: body.running === true,
    state: text(body.state) || 'off',
    lastSync: numbered(body.lastSync) > 0 ? clock(new Date(numbered(body.lastSync))) : '',
    pending: numbered(body.outbox),
    devices: (Array.isArray(body.devices) ? body.devices : []).map((d: any) => ({
      name: String(typeof d.hostname === 'string' && d.hostname !== '' ? d.hostname : d.slug ?? ''),
      lastSeen: seen(numbered(d.lagSecs)),
    })),
    conflicts: (Array.isArray(body.conflicts) ? body.conflicts : []).map((c: any) => ({ id: String(c.id ?? ''), path: String(c.path ?? '') })),
    needsImport: body.state === 'needs_import' || body.phase === 'needs_import',
    error: text(body.error) !== '' ? text(body.error) : text(body.reason),
  };
}

// A device's last-seen line, from the strings table: now, minutes, then hours.
function seen(secs: number): string {
  const fill = (key: string, values: Record<string, string>): string => say(key).replace(/\{(\w+)\}/g, (_match, name: string) => values[name] ?? '');
  if (secs < 90) return say('settings.sync.seenNow');
  if (secs < 5400) return fill('settings.sync.seenMinutes', { minutes: String(Math.round(secs / 60)) });
  if (secs < 129_600) return fill('settings.sync.seenHours', { hours: String(Math.round(secs / 3600)) });
  return fill('settings.sync.seenMinutes', { minutes: String(Math.round(secs / 60)) });
}

export const syncSources = {
  loadSync: async ([gate, locale], store): Promise<Result<'loadSync'>> => {
    const blank: Result<'loadSync'> = { ok: false, state: 'off', lastSync: '', pending: 0, devices: [], conflicts: [], needsImport: false, error: '' };
    if (gate !== '1') return blank;
    noteLocale(locale);
    try {
      return syncStatus(await read(store, '/api/sync/status'));
    } catch (error) {
      // The contract runner has no gateway to ask.
      if ((error as { code?: string }).code === 'bake') return syncStatus(fixtureSyncBody());
      throw error;
    }
  },
  runSyncNow: async (_args, store) => reply(await call(store, '/api/sync/run', { method: 'POST', body: '{}' })),
  importSync: async ([dry], store): Promise<Result<'importSync'>> => {
    const r = await call(store, '/api/sync/import', { method: 'POST', body: JSON.stringify({ dryRun: dry === true }) });
    if (r.ok) {
      const p = r.body?.preview ?? {};
      const secs = numbered(p.estimateSecs);
      return { ok: true, started: r.body?.started === true, push: numbered(p.push), pull: numbered(p.pull), conflict: numbered(p.conflict), minutes: secs > 0 ? Math.max(1, Math.ceil(secs / 60)) : 0, error: '' };
    }
    if (r.error === REACH_GATEWAY) return fixtureSyncImport(dry === true);
    return { ok: false, started: false, push: 0, pull: 0, conflict: 0, minutes: 0, error: localize(r.error) };
  },
  resolveConflict: async ([id, keep], store) => {
    const r = await call(store, `/api/sync/conflicts/${encodeURIComponent(id)}/resolve`, { method: 'POST', body: JSON.stringify({ keep }) });
    // The contract runner has no gateway: the canned conflict resolves.
    if (!r.ok && r.error === REACH_GATEWAY) return { ok: true, error: '' };
    return reply(r);
  },
} satisfies Pick<Sources, 'loadSync' | 'runSyncNow' | 'importSync' | 'resolveConflict'>;
