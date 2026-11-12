import type { Sources, Result } from '../app.contract.d.ts';
import { API } from '../host/config.ts';
import { fixtureModelList, rememberFixtureModel, explain } from '../host/fixture.ts';
import { noteLocale, say, call, read, reply } from '../host/gateway.ts';
import { prettyModel, nextLabel, emptyReport, usageReport } from '../host/report.ts';
import { hostNow } from '../host/time.ts';

export const librarySources = {
  loadUsage: async ([gate, locale], store): Promise<Result<'loadUsage'>> => {
    noteLocale(locale);
    const blank = { ok: false, error: '', messages: 0, attachments: 0, vaultWrites: 0, iconFetches: 0 };
    if (gate !== '1') return blank;
    try {
      const body = await read(store, '/api/usage');
      const totals = body.totals ?? {};
      const n = (key: string): number => (typeof totals[key] === 'number' ? totals[key] : 0);
      return { ok: true, error: '', messages: n('messages'), attachments: n('attachments'), vaultWrites: n('vaultWrites'), iconFetches: n('iconFetches') };
    } catch (e) {
      return { ...blank, error: explain(e instanceof Error ? e.message : String(e), 'usage') };
    }
  },
  loadModels: async ([slug, gate, locale], store): Promise<Result<'loadModels'>> => {
    noteLocale(locale);
    const none = { ok: true, error: '', current: '', instanceId: '', effort: '', runtimeMode: '', currentName: '', providerName: '', effortLabel: '', efforts: [], providers: [] };
    if (slug === '' || gate !== '1') return none;
    if (slug === 'fixture') return fixtureModelList();
    const [m, p] = await Promise.all([call(store, `/api/bots/${encodeURIComponent(slug)}/model`), call(store, '/api/providers')]);
    if (!m.ok) return { ...none, ok: false, error: explain(m.error, 'models') };
    const providers = (p.ok && Array.isArray(p.body.providers) ? p.body.providers : []).map((x: any) => ({
      instanceId: String(x.instanceId), name: String(x.displayName ?? x.instanceId), account: String(x.account ?? ''),
      models: ((x.models ?? []) as any[]).map((o) => ({
        slug: String(o.slug), name: String(o.name ?? o.slug), recommended: o.recommended === true, effort: String(o.effort ?? ''),
        efforts: ((o.efforts ?? []) as any[]).map((e) => ({ id: String(e.id), label: String(e.label ?? e.id) })),
      })),
    }));
    const instanceId = String(m.body.instanceId ?? '');
    const current = String(m.body.current ?? '');
    const effort = String(m.body.effort ?? '');
    const runtimeMode = String(m.body.runtimeMode ?? '');
    const prov = providers.find((x: any) => x.instanceId === instanceId);
    const mod = prov?.models.find((o: any) => o.slug === current);
    return {
      ok: true, error: p.ok ? '' : explain(p.error, 'providers'), current, instanceId, effort, runtimeMode,
      currentName: mod?.name ?? prettyModel(current), providerName: prov?.name ?? instanceId,
      effortLabel: mod?.efforts.find((e: any) => e.id === effort)?.label ?? '', efforts: mod?.efforts ?? [], providers,
    };
  },
  setModel: async ([slug, instanceId, model, effort, mode, confirm], store) => {
    if (slug === 'fixture') return rememberFixtureModel(instanceId, model, effort, mode);
    return reply(await call(store, `/api/bots/${encodeURIComponent(slug)}/model`, {
      method: 'PUT', body: JSON.stringify({ instanceId, model, ...(effort !== '' ? { effort } : {}), ...(mode !== '' ? { runtimeMode: mode } : {}), ...(confirm === '1' ? { confirmSwitch: true } : {}) }),
    }));
  },
  loadRoutines: async ([slug, gate, locale], store): Promise<Result<'loadRoutines'>> => {
    noteLocale(locale);
    if (slug === '' || gate !== '1') return { ok: true, tasks: [], error: '' };
    const r = await call(store, `/api/bots/${encodeURIComponent(slug)}/routines`);
    if (!r.ok) {
      if (r.error.includes('no conversation') || r.error.includes('getting set up')) {
        return { ok: true, tasks: [], error: say('error.settingUp') };
      }
      return { ok: false, tasks: [], error: explain(r.error, 'routines') };
    }
    const tasks = Array.isArray(r.body.tasks) ? r.body.tasks : [];
    const now = hostNow();
    return {
      ok: true,
      error: String(r.body.error ?? ''),
      tasks: tasks.map((t: any) => ({
        id: String(t.id ?? ''),
        title: String(t.title ?? ''),
        paused: t.paused === true,
        schedule: String(t.schedule ?? ''),
        next: t.paused === true ? 'Paused' : nextLabel(t.nextRunAt, now),
        failed: t.lastStatus === 'failed',
      })),
    };
  },
  loadLineage: async ([slug, gate, locale], store): Promise<Result<'loadLineage'>> => {
    noteLocale(locale);
    if (slug === '' || gate !== '1') return { ok: true, rows: [], error: '' };
    if (slug === 'fixture') {
      return {
        ok: true, error: '',
        rows: [
          { id: 'local-1', title: 'Notes', status: 'Working', remote: false, threadId: 'child-thread' },
          { id: 'local-ro', title: 'Finished', status: 'Failed', remote: false, threadId: 'child-readonly' },
          { id: 'remote-1', title: 'Away', status: 'Working', remote: true, threadId: 'remote-thread' },
        ],
      };
    }
    const r = await call(store, `/api/bots/${encodeURIComponent(slug)}/lineage`);
    if (!r.ok) {
      if (r.error.includes('no conversation') || r.error.includes('getting set up')) {
        return { ok: true, rows: [], error: say('error.settingUp') };
      }
      return { ok: false, rows: [], error: explain(r.error, 'what this Bot is doing') };
    }
    const rows = Array.isArray(r.body.rows) ? r.body.rows : [];
    return {
      ok: true,
      error: '',
      rows: rows.map((row: any) => ({
        id: String(row.id ?? ''),
        title: String(row.title ?? ''),
        status: String(row.status ?? ''),
        remote: row.remote === true,
        threadId: String(row.threadId ?? ''),
      })),
    };
  },
  loadShelf: async ([slug, gate], store): Promise<Result<'loadShelf'>> => {
    const none = { ok: true, total: 0, pictures: [], files: [], links: [] };
    if (slug === '' || gate !== '1') return none;
    const body = await read(store, `/api/bots/${encodeURIComponent(slug)}/shelf`);
    const item = (x: any) => ({ id: String(x.id), title: String(x.title ?? ''), detail: String(x.detail ?? ''), url: String(x.url ?? '').startsWith('/') ? `${API}${x.url}` : String(x.url ?? '') });
    const list = (k: string) => ((body[k] ?? []) as any[]).map(item);
    return { ok: true, total: Number(body.counts?.pictures ?? 0) + Number(body.counts?.files ?? 0) + Number(body.counts?.links ?? 0), pictures: list('pictures'), files: list('files'), links: list('links') };
  },
  loadUsageReport: async ([slug, period, gate, locale], store): Promise<Result<'loadUsageReport'>> => {
    noteLocale(locale);
    if (gate !== '1') return emptyReport('');
    const now = hostNow();
    const offset = now > 0 ? -new Date(now).getTimezoneOffset() : 0;
    const r = await call(store, `/api/usage/report?period=${encodeURIComponent(period)}&bot=${encodeURIComponent(slug === '' ? 'all' : slug)}&offset=${offset}`);
    if (!r.ok) return emptyReport(explain(r.error, 'usage'));
    return usageReport(r.body);
  },
} satisfies Pick<Sources, 'loadUsage' | 'loadModels' | 'setModel' | 'loadRoutines' | 'loadLineage' | 'loadShelf' | 'loadUsageReport'>;
