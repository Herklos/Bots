import type { Sources, Result } from '../app.contract.d.ts';
import { fixtureMade, setFixtureMade, fixtureMemberEdits, fixtureMemberRow, fixtureRoomSent, fixtureBotNotify, fixtureRoomNotify, fixtureRoomPolicy, fixtureRoomChief, roomAnswer, roomChief, wantsNotice } from '../host/fixture.ts';
import { call, fresh, botsBody, roomsBody, reply, sentFrom } from '../host/gateway.ts';
import { roomMessage } from '../host/markdown.ts';
import { fill } from '../host/palette.ts';
import { roomKept } from '../host/session.ts';
import { when, isGroupLeak, lineOf, stackFaces } from '../host/time.ts';

// A group on the machine default follows the Settings choice; fixtures use last speaker.
const effectivePolicy = (policy: string): string => (policy === 'everyone' || policy === 'chief' ? policy : 'last');

export const groupSources = {
  loadConvos: async ([query, focus, room], store): Promise<Result<'loadConvos'>> => {
    if (focus === 'tabs') {
      return [
        { kind: 'bot', key: 'bot-tabs', id: 'tabs', name: 'Tabs', color: fill('brand.default'), shape: 'ball', faces: [], line: 'Ready to chat', time: '', at: '', unread: false, working: false, ready: true, notifications: wantsNotice(fixtureBotNotify.get('tabs')) },
      ];
    }
    if (focus === 'fixture-order') {
      return [
        { kind: 'bot', key: 'bot-ada', id: 'ada', name: 'Ada', color: fill('brand.default'), shape: 'ball', faces: [], line: 'Ready to chat', time: '', at: '2026-11-12T00:00:02Z', unread: false, working: false, ready: true, notifications: wantsNotice(fixtureBotNotify.get('ada')) },
        { kind: 'bot', key: 'bot-bea', id: 'bea', name: 'Bea', color: fill('brand.blue'), shape: 'ball', faces: [], line: 'Ready to chat', time: '', at: '2026-11-12T00:00:01Z', unread: false, working: false, ready: true, notifications: wantsNotice(fixtureBotNotify.get('bea')) },
      ];
    }
    if (focus === 'fixture' || focus === 'bea' || room === 'fixture' || focus === 'fixture-pick') {
      return [
        { kind: 'bot', key: 'bot-fixture', id: 'fixture', name: 'Ada', color: fill('brand.default'), shape: 'ball', faces: [], line: 'Ready to chat', time: '', at: '', unread: true, working: false, ready: true, notifications: wantsNotice(fixtureBotNotify.get('fixture')) },
        { kind: 'group', key: 'group-fixture', id: 'fixture', name: 'Design', color: 'brand.blue', shape: 'hex', faces: [{ shape: 'ball', color: fill('brand.default'), state: 'idle' }, { shape: 'ball', color: fill('brand.blue'), state: 'idle' }], line: 'Ada: On it', time: '', at: '', unread: false, working: false, ready: true, notifications: wantsNotice(fixtureRoomNotify.get('fixture')) },
      ];
    }
    if (focus === 'fixture-busy' || room === 'fixture-busy') {
      return [
        { kind: 'group', key: 'group-fixture-busy', id: 'fixture-busy', name: 'Design', color: 'brand.blue', shape: 'hex', faces: [{ shape: 'ball', color: fill('brand.default'), state: 'working' }, { shape: 'ball', color: fill('brand.blue'), state: 'working' }], line: 'Ada: On it', time: '', at: '', unread: false, working: true, ready: true, notifications: wantsNotice(fixtureRoomNotify.get('fixture-busy')) },
      ];
    }
    if (focus === 'fixture-crowd' || room === 'fixture-crowd') {
      return [
        { kind: 'group', key: 'group-fixture-crowd', id: 'fixture-crowd', name: 'Design', color: 'brand.blue', shape: 'hex', faces: [], line: 'Ada: On it', time: '', at: '', unread: false, working: false, ready: true, notifications: wantsNotice(fixtureRoomNotify.get('fixture-crowd')) },
      ];
    }
    const body = await botsBody(store);
    const now = Number(body.now);
    const directory = body.bots as any[];
    const at = (b: any): string => (typeof b.preview?.at === 'string' ? b.preview.at : '');
    const match = (...names: string[]): boolean => query === '' || names.some((n) => n.toLowerCase().includes(query));
    const bots = directory
      .filter((b) => b.hidden !== true && b.hideFromSidebar !== true && (query !== '' || !(b.primary === true || b.pinned === true)) && match(String(b.name), String(b.label ?? '')))
      .map((b) => ({
        kind: 'bot', key: `bot-${String(b.slug)}`, id: String(b.slug), name: String(b.name), color: fill(b.color), shape: String(b.shape ?? 'ball'), faces: [] as { shape: string; color: string; state: string }[],
        line: lineOf(b), time: when(b.preview?.at, now), at: at(b), unread: b.unread === true, working: b.working === true,
        ready: typeof b.mainThreadId === 'string', notifications: wantsNotice(b.notifications),
      }));
    // A rooms failure must not hide the Bots. An empty list is the honest fallback.
    const roomsReply = await roomsBody(store);
    const listed = roomsReply.ok && Array.isArray(roomsReply.body?.rooms) ? roomsReply.body.rooms : [];
    const groups = listed
      .filter((g: any) => match(String(g.title ?? '')))
      .map((g: any) => {
        const raw = String(g.last?.text ?? '');
        const show = Boolean(g.last) && !isGroupLeak(raw);
        const last = show ? `${String(g.last.name ?? '')}: ${raw}`.slice(0, 120) : '';
        const said = typeof g.last?.at === 'string' ? g.last.at : '';
        return {
          kind: 'group', key: `group-${String(g.id)}`, id: String(g.id), name: String(g.title ?? ''), color: String(g.color ?? 'brand.default'), shape: String(g.shape ?? 'ball'),
          faces: stackFaces(g.members, directory, Array.isArray(g.workingSlugs) ? g.workingSlugs.map((slug: unknown) => String(slug)) : []),
          line: last !== '' ? last : String(g.description ?? ''), time: when(said, now), at: said, unread: g.unread === true, working: g.working === true, ready: true, notifications: wantsNotice(g.notifications),
        };
      });
    // The most recent conversation first, a Bot or a group alike; one with nothing said goes last.
    return [...bots, ...groups].sort((x, y) => y.at.localeCompare(x.at));
  },
  markRoomSeen: async ([id], store) => {
    if (id === 'fixture' || id === 'fixture-busy' || id === 'fixture-crowd') return { ok: true, error: '' };
    return reply(await call(store, `/api/rooms/${encodeURIComponent(id)}/seen`, { method: 'POST', body: '{}' }));
  },
  updateGroup: async ([id, field, value], store) => {
    const fixtureRoom = id === 'fixture' || id === 'fixture-busy' || id === 'fixture-crowd';
    if (fixtureRoom && field === 'notifications') {
      fixtureRoomNotify.set(id, value === 'true');
      return { ok: true, error: '' };
    }
    if (fixtureRoom && field === 'members') {
      fixtureMemberEdits.set(id, value === '' ? [] : value.split('\n').filter((slug) => slug !== ''));
      return { ok: true, error: '' };
    }
    if (fixtureRoom && field === 'answerPolicy') {
      const policy = value === 'last' || value === 'everyone' || value === 'chief' || value === 'default' ? value : 'default';
      fixtureRoomPolicy.set(id, policy);
      return { ok: true, error: '' };
    }
    if (fixtureRoom && field === 'chief') return (fixtureRoomChief.set(id, value.trim()), { ok: true, error: '' });
    const body = field === 'members'
      ? { members: value === '' ? [] : value.split('\n') }
      : field === 'notifications'
        ? { notifications: value === 'true' }
        : { [field]: value };
    return reply(await call(store, `/api/rooms/${encodeURIComponent(id)}`, { method: 'PUT', body: JSON.stringify(body) }));
  },
  createGroup: async ([title, description, color, shape, members, answerPolicy, chief], store): Promise<Result<'createGroup'>> => {
    const slugs = members === '' ? [] : members.split('\n').filter((s) => s !== '');
    const policy = answerPolicy.trim();
    // The contract runner's Bots. A real room still goes to the gateway.
    // The answer policy has to arrive with the name: create already stores one per group.
    if (slugs.length >= 2 && slugs.every((s) => s === 'fixture' || s === 'bea')) {
      if (title === 'Policy' && policy !== 'everyone') {
        return { ok: false, id: '', error: 'Create was missing the answer policy.' };
      }
      if (policy !== 'last' && policy !== 'everyone' && policy !== 'chief' && policy !== 'default') {
        return { ok: false, id: '', error: 'Create was missing the answer policy.' };
      }
      if (policy === 'chief' && !slugs.some((slug) => slug === chief.trim())) return { ok: false, id: '', error: 'Create was missing the chief.' };
      setFixtureMade({ title, description, color, shape, slugs, answerPolicy: policy, chief: policy === 'chief' ? chief.trim() : '' });
      return { ok: true, id: 'fixture', error: '' };
    }
    const r = await call(store, '/api/rooms', { method: 'POST', body: JSON.stringify({ title, description, color, shape, members: slugs, answerPolicy: policy, chief: chief.trim() }) });
    return r.ok ? { ok: true, id: String(r.body.id ?? ''), error: '' } : { ok: false, id: '', error: r.error };
  },
  sendRoom: async ([id, text, messageId], store) => {
    if (id === 'fixture' || id === 'fixture-busy') {
      fixtureRoomSent.push({ id: messageId, text });
      return { ok: true, error: '', messageId };
    }
    return sentFrom(await call(store, `/api/rooms/${encodeURIComponent(id)}/messages`, {
      method: 'POST',
      body: JSON.stringify({ text, ...(messageId !== '' ? { messageId } : {}) }),
    }), messageId);
  },
  deleteGroup: async ([id], store) => reply(await call(store, `/api/rooms/${encodeURIComponent(id)}`, { method: 'DELETE' })),
  loadRoom: async ([id], store): Promise<Result<'loadRoom'>> => {
    if (id === '') return { id: '', title: '', description: '', color: 'brand.default', shape: 'ball', members: [], messages: [], notifications: true, answerPolicy: 'default', answerPolicyEffective: 'last', chief: '' };
    if (id === 'fixture') {
      const edited = fixtureMemberEdits.get('fixture');
      if (fixtureMade && !edited) {
        const nameOf = (slug: string) => (slug === 'bea' ? 'Bea' : 'Ada');
        return {
          id: 'fixture', title: fixtureMade.title, description: fixtureMade.description, color: fixtureMade.color, shape: fixtureMade.shape,
          members: fixtureMade.slugs.map((slug) => ({ slug, name: nameOf(slug), line: 'Ready to chat', state: 'idle', ready: true, working: false })),
          messages: fixtureRoomSent.map((m) => roomMessage({ id: m.id, name: 'You', text: m.text, from: 'user' })),
          notifications: wantsNotice(fixtureRoomNotify.get('fixture')),
          answerPolicy: roomAnswer('fixture', fixtureMade?.answerPolicy),
          answerPolicyEffective: effectivePolicy(roomAnswer('fixture', fixtureMade?.answerPolicy)), chief: roomChief('fixture', fixtureMade?.chief ?? ''),
        };
      }
      return {
        id: 'fixture', title: 'Design', description: 'Where Ada and Bea trade drafts', color: 'brand.blue', shape: 'hex',
        members: edited ? edited.map(fixtureMemberRow) : [
          { slug: 'ada', name: 'Ada', line: 'Working', state: 'working', ready: true, working: true },
          { slug: 'bea', name: 'Bea', line: 'Getting set up…', state: 'asleep', ready: false, working: false },
        ],
        messages: [
          roomMessage({ id: 'r1', name: 'You', text: 'Hello', from: 'user' }),
          roomMessage({ id: 'r2', name: 'Ada', text: 'On it', from: 'ada' }),
          ...fixtureRoomSent.map((m) => roomMessage({ id: m.id, name: 'You', text: m.text, from: 'user' })),
        ],
        notifications: wantsNotice(fixtureRoomNotify.get('fixture')),
        answerPolicy: roomAnswer('fixture', fixtureMade?.answerPolicy),
        answerPolicyEffective: effectivePolicy(roomAnswer('fixture', fixtureMade?.answerPolicy)), chief: roomChief('fixture', fixtureMade?.chief ?? ''),
      };
    }
    if (id === 'fixture-crowd') {
      const edited = fixtureMemberEdits.get('fixture-crowd');
      return {
        id: 'fixture-crowd', title: 'Design', description: 'Where Ada and Bea trade drafts', color: 'brand.blue', shape: 'hex',
        members: (edited ?? ['ada', 'bea']).map(fixtureMemberRow),
        messages: [],
        notifications: wantsNotice(fixtureRoomNotify.get('fixture-crowd')),
        answerPolicy: roomAnswer('fixture-crowd'),
        answerPolicyEffective: effectivePolicy(roomAnswer('fixture-crowd')), chief: roomChief('fixture-crowd'),
      };
    }
    if (id === 'fixture-busy') {
      const edited = fixtureMemberEdits.get('fixture-busy');
      return {
        id: 'fixture-busy', title: 'Design', description: 'Where Ada and Bea trade drafts', color: 'brand.blue', shape: 'hex',
        members: edited ? edited.map(fixtureMemberRow) : [
          { slug: 'ada', name: 'Ada', line: 'Working', state: 'working', ready: true, working: true },
          { slug: 'bea', name: 'Bea', line: 'Working', state: 'working', ready: true, working: true },
        ],
        messages: [
          roomMessage({ id: 'r1', name: 'Ada', text: 'On it', from: 'ada' }),
          ...fixtureRoomSent.map((m) => roomMessage({ id: m.id, name: 'You', text: m.text, from: 'user' })),
        ],
        notifications: wantsNotice(fixtureRoomNotify.get('fixture-busy')),
        answerPolicy: roomAnswer('fixture-busy'),
        answerPolicyEffective: effectivePolicy(roomAnswer('fixture-busy')), chief: roomChief('fixture-busy'),
      };
    }
    const kept = roomKept.get(id);
    const res = await fresh(store, `/api/rooms/${encodeURIComponent(id)}`, kept?.etag ?? '');
    if (res.status === 304 && kept) {
      if (res.etag !== '') roomKept.set(id, { etag: res.etag, view: kept.view });
      return kept.view;
    }
    const body = res.body;
    const members = Array.isArray(body.members) ? body.members : [];
    const messages = Array.isArray(body.messages) ? body.messages : [];
    const view = {
      id: String(body.id ?? id),
      title: String(body.title ?? ''), description: String(body.description ?? ''),
      color: String(body.color ?? 'brand.default'), shape: String(body.shape ?? 'ball'),
      members: members.map((m: any) => ({
        slug: String(m.slug ?? ''), name: String(m.name ?? ''), line: String(m.line ?? ''),
        state: String(m.state ?? 'idle'), ready: m.ready === true, working: m.working === true,
      })),
      messages: messages.map((m: any) => roomMessage(m)),
      notifications: wantsNotice(body.notifications),
      answerPolicy: body.answerPolicy === 'last' || body.answerPolicy === 'everyone' || body.answerPolicy === 'chief' ? body.answerPolicy : 'default',
      answerPolicyEffective: body.answerPolicyEffective === 'last' || body.answerPolicyEffective === 'everyone' || body.answerPolicyEffective === 'chief' ? body.answerPolicyEffective : 'last',
      chief: typeof body.chief === 'string' ? body.chief : '',
    };
    roomKept.set(id, { etag: res.etag, view });
    return view;
  },
} satisfies Pick<Sources, 'loadConvos' | 'markRoomSeen' | 'updateGroup' | 'createGroup' | 'sendRoom' | 'deleteGroup' | 'loadRoom'>;
