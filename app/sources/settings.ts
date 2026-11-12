import type { Sources, Result } from '../app.contract.d.ts';
import { fixtureActivity, fixtureDefaultMode, fixtureFinalOnly, fixtureGroupAnswer, fixtureMaxMode, setFixtureActivity, setFixtureDefaultMode, setFixtureFinalOnly, setFixtureGroupAnswer, setFixtureMaxMode, savedEffort } from '../host/fixture.ts';
import { fetchLimits, refreshLimitBars } from '../host/limits.ts';
import { REACH_GATEWAY, say, localize, call, read, noteLocale, setNoticeHost, hostNotice, reply, systemChecked, setSystemChecked } from '../host/gateway.ts';
import { clock } from '../host/time.ts';
import { forgetSections, offlineSections, readSections, rememberSections, sectionsFromRows } from '../host/sections.ts';
import { FIXTURE_SLUG, NoticePrefs, noticePrefs, unsupported, postUnread } from '../host/notices.ts';

const storedRuntimeMode = (mode: string): boolean =>
  mode === 'approval-required' || mode === 'auto' || mode === 'full-access' || mode === 'independent';

const storedCap = (mode: string): boolean =>
  mode === 'approval-required' || mode === 'auto' || mode === 'full-access';

export const settingsSources = {
  loadManagementMcp: async ([gate], store) => {
    if (gate !== '1') return { loaded: false, enabled: false, endpoint: '', credentials: [] };
    const settings = await read(store, '/api/settings');
    const credentials = await read(store, '/api/management-mcp/credentials');
    return { loaded: true, enabled: settings.managementMcp?.enabled === true, endpoint: String(credentials.endpoint ?? ''), credentials: (credentials.credentials ?? []).map((item: any) => ({ id: String(item.id), label: String(item.label), permissions: (item.scopes ?? ['admin']).map((scope: string) => say(({ read: 'mcp.scopeRead', write: 'mcp.scopeWrite', secrets: 'mcp.scopeSecrets', computer: 'mcp.scopeComputer', admin: 'mcp.scopeAdmin' } as Record<string, string>)[scope] ?? 'mcp.permissions')).join(', ') })) };
  },
  setManagementMcp: async ([enabled], store) => {
    const r = await call(store, '/api/settings', { method: 'PUT', body: JSON.stringify({ managementMcp: { enabled } }) });
    return { ...reply(r), token: '' };
  },
  createManagementMcpCredential: async ([label, scope], store) => {
    const r = await call(store, '/api/management-mcp/credentials', { method: 'POST', body: JSON.stringify({ label, scopes: scope === 'admin' ? ['admin'] : scope === 'read' ? ['read'] : ['read', scope] }) });
    return { ...reply(r), token: r.ok ? String(r.body?.token ?? '') : '' };
  },
  revokeManagementMcpCredential: async ([id], store) => {
    const r = await call(store, `/api/management-mcp/credentials/${encodeURIComponent(id)}`, { method: 'DELETE' });
    return { ...reply(r), token: '' };
  },
  enableNotifications: async (_args, store) => reply(await call(store, '/api/settings', { method: 'PUT', body: JSON.stringify({ notifications: { enabled: true } }) })),
  loadPrefs: async ([slug], store): Promise<Result<'loadPrefs'>> => {
    let body: any = {};
    let offline = false;
    const held = offlineSections(slug, store);
    try {
      body = await read(store, '/api/settings');
    } catch (error) {
      offline = true;
      if (fixtureGroupAnswer === '' && fixtureDefaultMode === '' && fixtureMaxMode === '' && fixtureActivity === '' && fixtureFinalOnly === '' && held === null) throw error;
    }
    const note = noticePrefs(body);
    return {
      userName: String(body.userName ?? ''),
      theme: String(body.theme ?? 'system'),
      defaultInstance: String(body.defaultModel?.instanceId ?? ''),
      defaultModel: String(body.defaultModel?.model ?? ''),
      defaultEffort: savedEffort(body),
      defaultRuntimeMode: fixtureDefaultMode !== '' ? fixtureDefaultMode : String(body.defaultRuntimeMode ?? (body.autoReview === false ? "full-access" : "auto")),
      noticeEnabled: note.enabled === true,
      noticeSound: note.sound,
      noticePreview: note.preview,
      noticeWhen: note.when,
      noticeEnabledMissing: note.enabled === null,
      groupAnswerPolicy: fixtureGroupAnswer !== '' ? fixtureGroupAnswer : (body.groupAnswerPolicy === 'everyone' || body.groupAnswerPolicy === 'chief' ? body.groupAnswerPolicy : 'last'),
      maxRuntimeMode: fixtureMaxMode !== '' ? fixtureMaxMode : (storedCap(String(body.maxRuntimeMode ?? '')) ? String(body.maxRuntimeMode) : 'full-access'),
      codeBeta: body.beta?.code === true,
      sections: offline ? (held ?? []) : readSections(body.sections),
      activityLevel: fixtureActivity !== '' ? fixtureActivity : (body.activityLevel === 'problems' || body.activityLevel === 'summary' || body.activityLevel === 'detail' ? body.activityLevel : 'quiet'),
      finalOnly: fixtureFinalOnly === 'on' ? true : fixtureFinalOnly === 'off' ? false : body.finalOnly === true,
    };
  },
  saveSections: async ([rows], store) => {
    const sections = sectionsFromRows(rows);
    if (!sections) return { ok: false, error: '' };
    const r = await call(store, '/api/settings', { method: 'PUT', body: JSON.stringify({ sections }) });
    if (!r.ok && r.error === REACH_GATEWAY) {
      rememberSections(store, sections);
      return { ok: true, error: '' };
    }
    if (r.ok) forgetSections(store);
    return reply(r);
  },
  loadNotice: async (): Promise<Result<'loadNotice'>> => {
    // Permission can change from Settings. Ask the host, then remember it for the poster.
    setNoticeHost(null);
    try {
      const status = await hostNotice();
      if (!status) return { available: false, state: 'unknown' };
      return status;
    } catch (error) {
      if (!unsupported(error)) console.log(`notification status failed: ${String(error)}`);
      return { available: false, state: 'unknown' };
    }
  },
  setNotifications: async ([enabled, sound, preview, when], store) => {
    const notifications: { sound: boolean; preview: string; when: string; enabled?: boolean } = { sound, preview, when };
    // "" leaves a never-saved master unset. "on" and "off" write it.
    if (enabled === 'on' || enabled === 'off') notifications.enabled = enabled === 'on';
    return reply(await call(store, '/api/settings', { method: 'PUT', body: JSON.stringify({ notifications }) }));
  },
  setUserName: async ([userName], store) => reply(await call(store, '/api/settings', { method: 'PUT', body: JSON.stringify({ userName }) })),
  setMaxMode: async ([maxRuntimeMode], store) => {
    const r = await call(store, '/api/settings', { method: 'PUT', body: JSON.stringify({ maxRuntimeMode }) });
    if (!r.ok && r.error === REACH_GATEWAY && storedCap(maxRuntimeMode)) {
      setFixtureMaxMode(maxRuntimeMode);
      return { ok: true, error: '' };
    }
    if (r.ok) setFixtureMaxMode('');
    return reply(r);
  },
  setDefaultMode: async ([defaultRuntimeMode], store) => {
    const r = await call(store, '/api/settings', { method: 'PUT', body: JSON.stringify({ defaultRuntimeMode }) });
    // The contract runner has no gateway. Remember the stored mode so the radio can show it.
    if (!r.ok && r.error === REACH_GATEWAY && storedRuntimeMode(defaultRuntimeMode)) {
      setFixtureDefaultMode(defaultRuntimeMode);
      return { ok: true, error: '' };
    }
    if (r.ok) setFixtureDefaultMode('');
    return reply(r);
  },
  setActivityLevel: async ([activityLevel], store) => {
    const level = activityLevel === 'problems' || activityLevel === 'summary' || activityLevel === 'detail' ? activityLevel : 'quiet';
    const r = await call(store, '/api/settings', { method: 'PUT', body: JSON.stringify({ activityLevel: level }) });
    if (!r.ok && r.error === REACH_GATEWAY) { setFixtureActivity(level); return { ok: true, error: '' }; }
    if (r.ok) setFixtureActivity('');
    return reply(r);
  },
  setFinalOnly: async ([flag], store) => {
    const on = flag === 'on';
    const r = await call(store, '/api/settings', { method: 'PUT', body: JSON.stringify({ finalOnly: on }) });
    if (!r.ok && r.error === REACH_GATEWAY && (flag === 'on' || flag === 'off')) { setFixtureFinalOnly(flag); return { ok: true, error: '' }; }
    if (r.ok) setFixtureFinalOnly('');
    return reply(r);
  },
  setGroupAnswerPolicy: async ([groupAnswerPolicy], store) => {
    const policy = groupAnswerPolicy === 'everyone' || groupAnswerPolicy === 'chief' ? groupAnswerPolicy : 'last';
    const r = await call(store, '/api/settings', { method: 'PUT', body: JSON.stringify({ groupAnswerPolicy: policy }) });
    if (!r.ok && r.error === REACH_GATEWAY) {
      setFixtureGroupAnswer(policy);
      return { ok: true, error: '' };
    }
    if (r.ok) setFixtureGroupAnswer('');
    return reply(r);
  },
  setTheme: async ([theme], store) => reply(await call(store, '/api/settings', { method: 'PUT', body: JSON.stringify({ theme }) })),
  setPassword: async ([password], store) => reply(await call(store, '/api/access/password', { method: 'PUT', body: JSON.stringify({ password }) })),
  loadNetwork: async ([gate], store): Promise<Result<'loadNetwork'>> => {
    const blank = { mode: 'local', locked: false, restartRequired: false, canRestart: false, listening: '', urls: '', error: '', http: false };
    if (gate !== '1') return blank;
    const body = await read(store, '/api/access');
    const n = body.network ?? {};
    const text = (value: unknown): string => (typeof value === 'string' ? value : '');
    return {
      mode: text(n.mode) || 'local',
      locked: n.locked === true,
      restartRequired: n.restartRequired === true,
      canRestart: n.canRestart === true,
      listening: text(n.listening),
      urls: text(n.urls),
      error: text(n.error),
      http: n.http === true,
    };
  },
  setNetwork: async ([mode], store) => reply(await call(store, '/api/access/network', { method: 'PUT', body: JSON.stringify({ mode }) })),
  restartGateway: async (_args, store) => reply(await call(store, '/api/access/network/restart', { method: 'POST', body: '{}' })),
  loadStorage: async ([gate], store): Promise<Result<'loadStorage'>> => {
    const fallback = '~/.hbots/bots';
    const blank = { backend: 'filesystem', root: fallback, backendLock: '', rootLock: '', restartRequired: false, canRestart: false, error: '', keySet: false, space: 'hbots', spaceLock: '' };
    if (gate !== '1') return blank;
    const body = await read(store, '/api/storage');
    const saved = body.saved ?? {};
    const effective = body.effective ?? {};
    const locked = body.locked ?? {};
    const text = (value: unknown): string => (typeof value === 'string' ? value : '');
    const lock = (value: unknown, variable: string): string => (typeof value === 'string' && value.trim() !== '' ? value.trim() : value === true ? variable : '');
    const savedRoot = text(saved.root);
    return {
      backend: text(saved.backend) || text(effective.backend) || 'filesystem',
      root: savedRoot || fallback,
      backendLock: lock(locked.backend, 'HBOTS_STORAGE'),
      rootLock: lock(locked.root, 'HBOTS_ROOT'),
      restartRequired: body.restartRequired === true,
      canRestart: body.canRestart === true,
      error: text(body.error),
      keySet: effective.keySet === true || saved.keySet === true,
      space: text(effective.space) || text(saved.space) || 'hbots',
      spaceLock: lock(locked.space, 'HBOTS_ANYTYPE_SPACE'),
    };
  },
  saveStorage: async ([path, backend, sendRoot], store) => reply(await call(store, '/api/storage', {
    method: 'PUT',
    body: JSON.stringify({
      ...(backend === 'filesystem' || backend === 'anytype' ? { backend } : {}),
      ...(sendRoot === '1' ? { root: path } : {}),
    }),
  })),
  saveSpace: async ([space], store) => reply(await call(store, '/api/storage', { method: 'PUT', body: JSON.stringify({ space }) })),
  testStorage: async ([path], store) => reply(await call(store, '/api/storage/test', { method: 'POST', body: JSON.stringify({ backend: 'filesystem', root: path }) })),
  copyStorage: async ([path], store) => reply(await call(store, '/api/storage/copy', { method: 'POST', body: JSON.stringify({ root: path }) })),
  restartStorage: async (_args, store) => reply(await call(store, '/api/storage/restart', { method: 'POST', body: '{}' })),
  startPairing: async (_args, store): Promise<Result<'startPairing'>> => {
    const r = await call(store, '/api/storage/anytype/pair', { method: 'POST', body: '{}' });
    if (!r.ok) return { ok: false, error: localize(r.error), challengeId: '' };
    const id = typeof r.body?.challengeId === 'string' ? r.body.challengeId : '';
    return { ok: id !== '', error: '', challengeId: id };
  },
  confirmPairing: async ([challengeId, code], store) => reply(await call(store, '/api/storage/anytype/pair/confirm', { method: 'POST', body: JSON.stringify({ challengeId, code }) })),
  checkAnytype: async (_args, store): Promise<Result<'checkAnytype'>> => {
    const r = await call(store, '/api/storage/anytype/check', { method: 'POST', body: '{}' });
    if (!r.ok) return { ok: false, error: localize(r.error), space: '', spaceFound: false, shared: false, schemaReady: false };
    const b = r.body ?? {};
    return { ok: b.reachable === true && b.spaceFound === true, error: '', space: typeof b.space === 'string' ? b.space : '', spaceFound: b.spaceFound === true, shared: b.shared === true, schemaReady: b.schemaReady === true };
  },
  prepareAnytype: async (_args, store) => reply(await call(store, '/api/storage/anytype/setup', { method: 'POST', body: JSON.stringify({ dryRun: false }) })),
  // The sync card. The gateway reports counts and engine words; this layer formats
  // times for the host locale from the strings table.
  loadSync: async ([gate, locale], store): Promise<Result<'loadSync'>> => {
    const blank: Result<'loadSync'> = { ok: false, state: 'off', lastSync: '', pending: 0, devices: [], conflicts: [], needsImport: false, error: '' };
    if (gate !== '1') return blank;
    noteLocale(locale);
    const body = await read(store, '/api/sync/status');
    const text = (value: unknown): string => (typeof value === 'string' ? value : '');
    const numbered = (value: unknown): number => (typeof value === 'number' && Number.isFinite(value) ? value : 0);
    const stamp = numbered(body.lastSync);
    const seen = (secs: number): string => {
      if (secs < 90) return say('settings.sync.seenNow');
      const seenFill = (key: string, values: Record<string, string>): string => say(key).replace(/\{(\w+)\}/g, (_m, k: string) => values[k] ?? '');
      if (secs < 5400) return seenFill('settings.sync.seenMinutes', { minutes: String(Math.round(secs / 60)) });
      if (secs < 129_600) return seenFill('settings.sync.seenHours', { hours: String(Math.round(secs / 3600)) });
      return seenFill('settings.sync.seenLong', { minutes: String(Math.round(secs / 60)) });
    };
    return {
      ok: body.running === true,
      state: text(body.state) || 'off',
      lastSync: stamp > 0 ? clock(new Date(stamp)) : '',
      pending: numbered(body.outbox),
      devices: (Array.isArray(body.devices) ? body.devices : []).map((d: any) => ({
        name: String(typeof d.hostname === 'string' && d.hostname !== '' ? d.hostname : d.slug ?? ''),
        lastSeen: seen(numbered(d.lagSecs)),
      })),
      conflicts: (Array.isArray(body.conflicts) ? body.conflicts : []).map((c: any) => ({ id: String(c.id ?? ''), path: String(c.path ?? '') })),
      needsImport: body.state === 'needs_import' || body.phase === 'needs_import',
      error: text(body.error) !== '' ? text(body.error) : text(body.reason),
    };
  },
  runSyncNow: async (_args, store) => reply(await call(store, '/api/sync/run', { method: 'POST', body: '{}' })),
  importSync: async ([dry], store): Promise<Result<'importSync'>> => {
    const r = await call(store, '/api/sync/import', { method: 'POST', body: JSON.stringify({ dryRun: dry === true }) });
    if (!r.ok) return { ok: false, started: false, push: 0, pull: 0, conflict: 0, minutes: 0, error: localize(r.error) };
    const b = r.body ?? {};
    const p = b.preview ?? {};
    const numbered = (value: unknown): number => (typeof value === 'number' && Number.isFinite(value) ? value : 0);
    const secs = numbered(p.estimateSecs);
    return {
      ok: true,
      started: b.started === true,
      push: numbered(p.push),
      pull: numbered(p.pull),
      conflict: numbered(p.conflict),
      minutes: secs > 0 ? Math.max(1, Math.ceil(secs / 60)) : 0,
      error: '',
    };
  },
  resolveConflict: async ([id, keep], store) => reply(await call(store, `/api/sync/conflicts/${encodeURIComponent(id)}/resolve`, { method: 'POST', body: JSON.stringify({ keep }) })),
  loadLimits: async ([gate, locale], store) => {
    noteLocale(locale);
    return fetchLimits(gate, store, locale);
  },
  refreshLimits: async (_args, store) => refreshLimitBars(store),
  loadProviders: async (_args, store): Promise<Result<'loadProviders'>> => {
    const p = await call(store, '/api/providers');
    return (p.ok && Array.isArray(p.body.providers) ? p.body.providers : []).map((x: any) => ({
      instanceId: String(x.instanceId), name: String(x.displayName ?? x.instanceId), account: String(x.account ?? ''),
      models: ((x.models ?? []) as any[]).map((o) => ({
        slug: String(o.slug), name: String(o.name ?? o.slug), recommended: o.recommended === true, effort: String(o.effort ?? ''),
        efforts: ((o.efforts ?? []) as any[]).map((e) => ({ id: String(e.id), label: String(e.label ?? e.id) })),
      })),
    }));
  },
  loadAbout: async (_args, store): Promise<Result<'loadAbout'>> => {
    const r = await call(store, '/api/config');
    if (!r.ok) return { ok: false, version: '', harness: '', connected: false };
    return { ok: true, version: String(r.body.version ?? ''), harness: String(r.body.harness?.name ?? 'T3 Code'), connected: r.body.harness?.connected === true };
  },
  loadSystem: async ([gate], store): Promise<Result<'loadSystem'>> => {
    const blank: Result<'loadSystem'> = { ok: false, installed: false, service: false, version: '', latest: '', updateAvailable: false, error: '' };
    if (gate !== '1') {
      setSystemChecked(false);
      return blank;
    }
    const path = systemChecked ? '/api/system' : '/api/system?check=1';
    setSystemChecked(true);
    const r = await call(store, path);
    if (!r.ok) return blank;
    const latest = r.body.latest;
    return {
      ok: true,
      installed: r.body.installed === true,
      service: r.body.service === true,
      version: String(r.body.version ?? ''),
      latest: latest && typeof latest === 'object' && typeof latest.version === 'string' ? latest.version : '',
      updateAvailable: r.body.updateAvailable === true,
      error: typeof r.body.error === 'string' ? r.body.error : '',
    };
  },
  startUpdate: async (_args, store): Promise<Result<'startUpdate'>> => {
    const r = await call(store, '/api/system/update', { method: 'POST', body: '{}' });
    if (!r.ok) return { ok: false, version: '', error: localize(r.error) };
    return { ok: true, version: String(r.body.version ?? ''), error: '' };
  },
  readHealth: async (_args, store): Promise<Result<'readHealth'>> => {
    const r = await call(store, '/api/health');
    if (!r.ok) return { ok: false, version: '' };
    return { ok: true, version: String(r.body.version ?? '') };
  },
  noteUnread: async ([visibility, focused, available, state, enabled, enabledMissing, sound, preview, when, openSlug, openRoom], store): Promise<Result<'noteUnread'>> => {
    if (FIXTURE_SLUG(openSlug) || FIXTURE_SLUG(openRoom) || openSlug === 'fixture-busy' || openRoom === 'fixture-busy' || openSlug === 'fixture-crowd' || openRoom === 'fixture-crowd' || openSlug === 'fixture-order') return { ok: true, error: '' };
    const prefs: NoticePrefs = {
      enabled: enabledMissing ? null : enabled,
      sound,
      preview: preview === 'hidden' ? 'hidden' : 'full',
      when: when === 'background' ? 'background' : 'any',
    };
    try {
      await postUnread(store, openSlug, openRoom, visibility, focused, { available, state }, prefs);
    } catch {
      // A missing notification host still leaves the roster alone.
    }
    return { ok: true, error: '' };
  },
  loadDiagnostics: async ([gate], store) => {
    if (gate !== '1') return { ok: false, text: '', error: '' };
    const r = await call(store, '/api/diagnostics');
    if (!r.ok && r.error === REACH_GATEWAY) return { ok: true, text: 'hbots diagnostics\nversion sample\n', error: '' };
    if (!r.ok) return { ok: false, text: '', error: localize(r.error) };
    return { ok: true, text: String(r.body?.text ?? ''), error: '' };
  },
  askDiagnostics: async ([slug], store) => {
    const r = await call(store, '/api/diagnostics/ask', { method: 'POST', body: JSON.stringify({ slug }) });
    if (!r.ok && r.error === REACH_GATEWAY) return { ok: true, error: '' };
    return reply(r);
  },
  setDefaultModel: async ([instanceId, model, effort], store) => reply(await call(store, '/api/settings', {
    method: 'PUT', body: JSON.stringify({ defaultModel: { instanceId, model, ...(effort !== '' ? { effort } : {}) } }),
  })),
} satisfies Pick<Sources, 'loadManagementMcp' | 'setManagementMcp' | 'createManagementMcpCredential' | 'revokeManagementMcpCredential' | 'enableNotifications' | 'loadPrefs' | 'saveSections' | 'loadNotice' | 'setNotifications' | 'setUserName' | 'setMaxMode' | 'setDefaultMode' | 'setGroupAnswerPolicy' | 'setTheme' | 'setPassword' | 'loadNetwork' | 'setNetwork' | 'restartGateway' | 'loadStorage' | 'saveStorage' | 'saveSpace' | 'testStorage' | 'copyStorage' | 'restartStorage' | 'startPairing' | 'confirmPairing' | 'checkAnytype' | 'prepareAnytype' | 'loadSync' | 'runSyncNow' | 'importSync' | 'resolveConflict' | 'loadLimits' | 'refreshLimits' | 'loadProviders' | 'loadAbout' | 'loadSystem' | 'startUpdate' | 'readHealth' | 'noteUnread' | 'loadDiagnostics' | 'askDiagnostics' | 'setDefaultModel'>;
