// A stand-in for T3 Code, the harness backend wired today. Just enough of its HTTP API and Effect-RPC WebSocket
// (orchestration protocol 2) to run the gateway and the app end to end without T3 installed.
//   bun tests/mock-t3.ts [port]   (default 3773)
const TOKEN = process.env.T3_TOKEN ?? 'test-token';
const port = Number(process.argv[2] ?? 3773);
const projects: any[] = [];
const tasks: any[] = [];
const answered: any[] = [];
const threads = new Map<string, any[]>();
const deliveries = new Map<string, any[]>();
let seq = 0;
const item = (threadId: string, type: string, text: string, status = 'completed') => ({ id: `i${++seq}`, threadId, type, text, status, ordinal: seq, startedAt: new Date().toISOString(), updatedAt: new Date().toISOString() });
const ok = (ws: any, id: string, value: unknown) => ws.send(JSON.stringify({ _tag: 'Exit', requestId: id, exit: { _tag: 'Success', value } }));
const fail = (ws: any, id: string, error: string) => ws.send(JSON.stringify({ _tag: 'Exit', requestId: id, exit: { _tag: 'Failure', cause: [{ _tag: 'Fail', error }] } }));

// Browser tabs of the Bots (preview.list) and what has been typed into each (the preview stream).
// 'busy' is controlled by another viewer; 't-app' is a browser of the app, not a server tab.
const tabs = [
  { tabId: 't-github', runtime: 'server', navStatus: { url: 'https://github.com/login', title: 'Sign in' } },
  { tabId: 't-evil', runtime: 'server', navStatus: { url: 'https://github.com.evil.io/login', title: 'Sign in' } },
  { tabId: 'busy', runtime: 'server', navStatus: { url: 'https://github.com/busy', title: 'Busy' } },
  { tabId: 't-app', runtime: 'browser', navStatus: { url: 'https://github.com/app', title: 'App' } },
];
const typed = new Map<string, string>();
const previewMessage = (ws: any, raw: unknown) => {
  const tabId: string = ws.data.tabId;
  const msg = JSON.parse(String(raw));
  if (msg.type === 'takeControl') return ws.send(JSON.stringify({ type: 'control', controller: tabId === 'busy' ? 'another-viewer' : 'you', canOperate: true }));
  if (msg.type === 'text') return typed.set(tabId, (typed.get(tabId) ?? '') + String(msg.text));
  if (msg.type === 'key' && msg.modifiers === 4 && String(msg.key).toLowerCase() === 'c') {
    return ws.send(JSON.stringify({ type: 'clipboard', text: typed.get(tabId) ?? 'copied' }));
  }
  if (msg.type === 'key' && msg.modifiers === 4 && String(msg.key).toLowerCase() === 'x') {
    const text = typed.get(tabId) ?? 'cut';
    typed.set(tabId, '');
    return ws.send(JSON.stringify({ type: 'clipboard', text }));
  }
  if (msg.type === 'probe') return ws.send(JSON.stringify({ type: 'probe', x: msg.x, y: msg.y }));
  if (msg.type === 'releaseControl') return ws.send(JSON.stringify({ type: 'control', controller: 'none', canOperate: true }));
  if (msg.type === 'ack') return;
};

const hookTokenOf = (task: any): string => {
  const path = String(task?.webhook?.path ?? '');
  const token = path.split('/').pop() ?? '';
  return token;
};

// The hook needs the URL token only. Do not require the T3 bearer here.
const hookFetch = async (req: Request, url: URL): Promise<Response> => {
  const parts = url.pathname.split('/').filter(Boolean);
  if (parts.length !== 4) return Response.json({ error: 'not_found', taskId: 'hidden' }, { status: 404 });
  if (req.method !== 'POST' && req.method !== 'PUT' && req.method !== 'PATCH') {
    return Response.json({ error: 'not_found' }, { status: 404 });
  }
  const raw = new Uint8Array(await req.arrayBuffer());
  if (raw.byteLength > 1_048_576) {
    return Response.json({ error: 'body_too_large', taskId: parts[2] }, { status: 413 });
  }
  const token = parts[3];
  const task = tasks.find((item) => hookTokenOf(item) === token);
  if (!task) {
    return new Response(JSON.stringify({ error: 'not_found', taskId: parts[2] }), {
      status: 404,
      headers: { 'content-type': 'application/json', 'x-t3-hook-outcome': 'not_found' },
    });
  }
  const row = {
    id: `delivery:${++seq}`,
    taskId: task.id,
    receivedAt: new Date().toISOString(),
    method: req.method,
    contentType: req.headers.get('content-type'),
    bodyBytes: raw.byteLength,
    outcome: 'accepted',
    signatureVerified: false,
    pendingWindow: false,
    missingFields: [],
    error: null,
  };
  const list = deliveries.get(task.id) ?? [];
  list.unshift(row);
  deliveries.set(task.id, list.slice(0, 50));
  return new Response(JSON.stringify({ outcome: 'accepted', taskId: task.id }), {
    status: 202,
    headers: { 'content-type': 'application/json', 'x-t3-hook-outcome': 'accepted' },
  });
};

Bun.serve({
  port,
  fetch(req, server) {
    const url = new URL(req.url);
    if (url.pathname.startsWith('/api/hooks/')) return hookFetch(req, url);
    if (req.headers.get('authorization') !== `Bearer ${TOKEN}`) return new Response('unauthorized', { status: 401 });
    if (url.pathname === '/ws') {
      if (url.searchParams.get('orchestrationProtocol') !== '2') return new Response('upgrade required', { status: 426 });
      return server.upgrade(req) ? undefined : new Response('no upgrade', { status: 400 });
    }
    if (url.pathname === '/api/preview-stream/ws') {
      const tabId = url.searchParams.get('tabId') ?? '';
      if (!tabs.some((t) => t.tabId === tabId && t.runtime === 'server')) return new Response('no such tab', { status: 404 });
      return server.upgrade(req, { data: { tabId } }) ? undefined : new Response('no upgrade', { status: 400 });
    }
    if (url.pathname === '/mock/typed') return Response.json({ text: typed.get(url.searchParams.get('tabId') ?? '') ?? '' });
    if (url.pathname === '/api/projects') return Response.json({ projects });
    if (url.pathname === '/mock/answers') return Response.json({ answered });
    return new Response('not found', { status: 404 });
  },
  websocket: {
    message(ws, raw) {
      if ((ws.data as any)?.tabId !== undefined) return previewMessage(ws, raw);
      const msg = JSON.parse(String(raw));
      if (msg._tag === 'Ping') return ws.send('{"_tag":"Pong"}');
      if (msg._tag !== 'Request') return;
      const { id, tag, payload } = msg;
      if (tag === 'preview.list') return ok(ws, id, { sessions: tabs });
      if (tag === 'projects.mutate' && payload.type === 'project.create') {
        projects.push({ id: payload.projectId, title: payload.title, workspaceRoot: payload.workspaceRoot });
        return ok(ws, id, {});
      }
      if (tag === 'projects.mutate' && payload.type === 'project.update') {
        const project = projects.find((p) => p.id === payload.projectId);
        if (project) {
          project.title = payload.title;
          project.faviconPath = payload.faviconPath;
          project.projectIcon = payload.projectIcon;
        }
        return ok(ws, id, {});
      }
      if (tag === 'orchestration.launchThread') { threads.set(payload.threadId, []); return ok(ws, id, {}); }
      if (tag === 'orchestration.getThreadProjection') {
        const items = threads.get(payload.threadId);
        if (!items) return fail(ws, id, 'no such thread');
        const live = items.some((i) => i.status === 'running');
        return ok(ws, id, { turnItems: items, runs: live ? [{ id: 'run1', status: 'running' }] : [] });
      }
      if (tag === 'orchestration.dispatchCommand') {
        if (payload.type === 'message.dispatch') {
          const items = threads.get(payload.threadId);
          if (!items) return fail(ws, id, 'no such thread');
          items.push(item(payload.threadId, 'user_message', payload.text));
          const reply = item(payload.threadId, 'assistant_message', 'Thinking…', 'running');
          items.push(reply);
          setTimeout(() => Object.assign(reply, { text: `You said: ${payload.text}`, status: 'completed' }), 1200);
        }
        if (payload.type === 'run.interrupt') for (const items of threads.values()) for (const i of items) if (i.status === 'running') Object.assign(i, { status: 'interrupted', text: 'Stopped.' });
        if (payload.type === 'runtime-request.respond') {
          answered.push({ tag: payload.type, requestId: payload.requestId, decision: payload.decision ?? null, answers: payload.answers ?? null });
          return ok(ws, id, { storedEvents: [] });
        }
        return ok(ws, id, {});
      }
      if (tag === 'scheduledTasks.subscribe') {
        ws.send(JSON.stringify({ _tag: 'Chunk', requestId: id, values: [{ tasks }] }));
        return ok(ws, id, { tasks });
      }
      if (tag === 'scheduledTasks.upsert') {
        const task: any = { id: payload.id ?? `task${++seq}`, title: payload.title, prompt: payload.prompt, enabled: payload.enabled !== false, runCount: 0, lastRunStatus: 'never', schedule: payload.schedule ?? null, threadId: payload.threadId ?? null };
        const at = tasks.findIndex((t) => t.id === task.id);
        const prev = at >= 0 ? tasks[at] : null;
        if (payload.schedule?.type === 'webhook') {
          const token = hookTokenOf(prev) || `tok${++seq}`;
          const hasSecret = Boolean(payload.schedule?.signature?.secret) || prev?.webhook?.hasSecret === true;
          task.webhook = { path: `/api/hooks/${encodeURIComponent(task.id)}/${token}`, url: null, hasSecret };
          if (task.schedule?.signature) delete task.schedule.signature.secret;
        } else if (prev?.webhook) task.webhook = prev.webhook;
        if (at >= 0) tasks[at] = { ...prev, ...task, runCount: prev.runCount };
        else tasks.push(task);
        return ok(ws, id, { task: tasks.find((t) => t.id === task.id) });
      }
      if (tag === 'scheduledTasks.rotateWebhookToken') {
        const task = tasks.find((t) => t.id === payload.id);
        if (!task?.webhook) return fail(ws, id, 'no such task');
        const token = `tok${++seq}`;
        task.webhook = { ...task.webhook, path: `/api/hooks/${encodeURIComponent(task.id)}/${token}` };
        return ok(ws, id, { task });
      }
      if (tag === 'scheduledTasks.listWebhookDeliveries') {
        return ok(ws, id, { deliveries: deliveries.get(payload.id) ?? [] });
      }
      if (tag === 'scheduledTasks.getWebhookDelivery') {
        const row = (deliveries.get(payload.id) ?? []).find((item) => item.id === payload.deliveryId);
        if (!row) return fail(ws, id, 'no such delivery');
        return ok(ws, id, { delivery: { ...row, query: {}, headers: {}, body: '', bodyTruncated: false, renderedPrompt: '' } });
      }
      if (tag === 'scheduledTasks.setEnabled' || tag === 'scheduledTasks.runNow' || tag === 'scheduledTasks.delete') {
        const at = tasks.findIndex((t) => t.id === payload.id);
        if (at < 0) return fail(ws, id, 'no such task');
        if (tag === 'scheduledTasks.delete') {
          const [gone] = tasks.splice(at, 1);
          return ok(ws, id, { id: gone.id });
        }
        if (tag === 'scheduledTasks.setEnabled') tasks[at].enabled = payload.enabled === true;
        if (tag === 'scheduledTasks.runNow') { tasks[at].runCount += 1; tasks[at].lastRunStatus = 'succeeded'; }
        return ok(ws, id, { task: tasks[at] });
      }
      if (tag === 'secrets.answerRequest') {
        answered.push({ tag, turnItemId: payload.turnItemId, answerType: payload.answer?.type ?? null });
        return ok(ws, id, {});
      }
      if (tag === 'server.getConfig') {
        return ok(ws, id, {
          providers: [{ instanceId: 'codex', enabled: true, installed: true, status: 'ready',
            models: [{ slug: 'claude-haiku-5-5', name: 'Haiku 5.5', capabilities: { optionDescriptors: [] } }] }],
          settings: {},
        });
      }
      fail(ws, id, `unknown rpc ${tag}`);
    },
  },
});
console.log(`mock T3 on :${port}`);
