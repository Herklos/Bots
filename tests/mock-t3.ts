// A stand-in for T3 Code's server, just enough of its HTTP API and Effect-RPC WebSocket
// (orchestration protocol 2) to run the gateway and the app end to end without T3 installed.
//   bun tests/mock-t3.ts [port]   (default 3773)
const TOKEN = process.env.T3_TOKEN ?? 'test-token';
const port = Number(process.argv[2] ?? 3773);
const projects: any[] = [];
const threads = new Map<string, any[]>();
// Scheduled tasks (routines), as T3's wire shapes in packages/contracts/src/rpc.ts and Threebot's web/src/lib/routines.ts.
const tasks = new Map<string, any>();
const subscribers = new Map<any, string>(); // socket -> the requestId of its scheduledTasks.subscribe stream
const publishTasks = () => { for (const [ws, id] of subscribers) ws.send(JSON.stringify({ _tag: 'Chunk', requestId: id, values: [{ tasks: [...tasks.values()] }] })); };
let seq = 0;
const item = (threadId: string, type: string, text: string, status = 'completed') => ({ id: `i${++seq}`, threadId, type, text, status, ordinal: seq, startedAt: new Date().toISOString(), updatedAt: new Date().toISOString() });
const ok = (ws: any, id: string, value: unknown) => ws.send(JSON.stringify({ _tag: 'Exit', requestId: id, exit: { _tag: 'Success', value } }));
const fail = (ws: any, id: string, error: string) => ws.send(JSON.stringify({ _tag: 'Exit', requestId: id, exit: { _tag: 'Failure', cause: [{ _tag: 'Fail', error }] } }));

Bun.serve({
  port,
  fetch(req, server) {
    const url = new URL(req.url);
    if (req.headers.get('authorization') !== `Bearer ${TOKEN}`) return new Response('unauthorized', { status: 401 });
    if (url.pathname === '/ws') {
      if (url.searchParams.get('orchestrationProtocol') !== '2') return new Response('upgrade required', { status: 426 });
      return server.upgrade(req) ? undefined : new Response('no upgrade', { status: 400 });
    }
    if (url.pathname === '/api/projects') return Response.json({ projects });
    return new Response('not found', { status: 404 });
  },
  websocket: {
    message(ws, raw) {
      const msg = JSON.parse(String(raw));
      if (msg._tag === 'Ping') return ws.send('{"_tag":"Pong"}');
      if (msg._tag !== 'Request') return;
      const { id, tag, payload } = msg;
      if (tag === 'projects.mutate' && payload.type === 'project.create') {
        projects.push({ id: payload.projectId, title: payload.title, workspaceRoot: payload.workspaceRoot });
        return ok(ws, id, {});
      }
      if (tag === 'orchestration.launchThread') { threads.set(payload.threadId, []); return ok(ws, id, {}); }
      if (tag === 'scheduledTasks.subscribe') { subscribers.set(ws, id); ws.send(JSON.stringify({ _tag: 'Chunk', requestId: id, values: [{ tasks: [...tasks.values()] }] })); return; }
      if (tag === 'scheduledTasks.upsert') {
        const task = { id: payload.id ?? `task${++seq}`, enabled: true, lastRunAt: null, lastRunStatus: null, runCount: 0, ...payload.task, ...payload };
        tasks.set(task.id, task); publishTasks(); return ok(ws, id, { task });
      }
      if (tag === 'scheduledTasks.setEnabled') {
        const task = tasks.get(payload.id); if (!task) return fail(ws, id, 'no such routine');
        Object.assign(task, { enabled: payload.enabled }); publishTasks(); return ok(ws, id, { task });
      }
      if (tag === 'scheduledTasks.runNow') {
        const task = tasks.get(payload.id); if (!task) return fail(ws, id, 'no such routine');
        Object.assign(task, { runCount: task.runCount + 1, lastRunAt: new Date().toISOString(), lastRunStatus: 'succeeded' }); publishTasks(); return ok(ws, id, { task });
      }
      if (tag === 'scheduledTasks.delete') { tasks.delete(payload.id); publishTasks(); return ok(ws, id, { id: payload.id }); }
      if (tag === 'secrets.answerRequest') {
        // The secret value is accepted and dropped; it is never stored or echoed.
        const found = [...threads.values()].flat().find((i) => i.id === payload.turnItemId);
        if (!found) return fail(ws, id, 'no such request');
        Object.assign(found, { secretStatus: payload.answer?.type === 'save' ? 'saved' : 'declined' }); return ok(ws, id, {});
      }
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
          if (payload.text === 'ask approval') items.push({ ...item(payload.threadId, 'approval_request', '', 'waiting'), id: `req${++seq}`, title: 'Run rm?' });
          const reply = item(payload.threadId, 'assistant_message', 'Thinking…', 'running');
          items.push(reply);
          setTimeout(() => Object.assign(reply, { text: `You said: ${payload.text}`, status: 'completed' }), 1200);
        }
        if (payload.type === 'runtime-request.respond') {
          const found = [...threads.values()].flat().find((i) => i.id === payload.requestId);
          if (!found) return fail(ws, id, 'no such request');
          const declined = payload.decision === 'decline' || payload.decision === 'cancel';
          Object.assign(found, { status: declined ? 'declined' : 'completed', decision: payload.decision ?? null, answers: payload.answers ?? null });
        }
        if (payload.type === 'run.interrupt') for (const items of threads.values()) for (const i of items) if (i.status === 'running') Object.assign(i, { status: 'interrupted', text: 'Stopped.' });
        return ok(ws, id, {});
      }
      fail(ws, id, `unknown rpc ${tag}`);
    },
    close(ws) { subscribers.delete(ws); },
  },
});
console.log(`mock T3 on :${port}`);
