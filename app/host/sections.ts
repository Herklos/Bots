// Sidebar sections, the settings.sections document. The gateway is the
// authority on a live save. This check mirrors it so a bad value becomes an
// empty list instead of a sidebar the gateway would reject.

export type SectionDoc = {
  id: string;
  name: string;
  collapsed: boolean;
  sort: string;
  items: string[];
};

type SectionStore = {
  get: (key: string) => string | null;
  set: (key: string, value: string) => void;
};

const SECTION_KEY = 'section-order';
const ORDER_FIXTURE = 'fixture-order';

// The contract runner starts here: Bea then Ada, manual, so the menu test can
// see a saved order that is not the recency feed. A save replaces it in the
// session store, which survives a reload.
export const FIXTURE_SECTIONS: SectionDoc[] = [
  { id: 'chats', name: '', collapsed: false, sort: 'manual', items: ['bot:bea', 'bot:ada'] },
];

function validSlug(slug: string): boolean {
  return slug.length >= 1 && slug.length <= 63 && /^[a-z0-9][a-z0-9-]*$/.test(slug);
}

function validGroup(id: string): boolean {
  return /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/.test(id);
}

function validId(id: string): boolean {
  return id === 'chats' || id === 'hidden' || /^s[a-z0-9]{7}$/.test(id);
}

function validName(id: string, name: string): boolean {
  if (id === 'chats' || id === 'hidden') return name === '';
  if (name !== name.trim()) return false;
  let count = 0;
  for (const char of name) {
    const code = char.codePointAt(0) ?? 0;
    if (code <= 0x1f || code === 0x7f) return false;
    count += 1;
    if (count > 40) return false;
  }
  return count >= 1;
}

function validItem(item: string): boolean {
  if (item.startsWith('bot:')) return validSlug(item.slice(4));
  if (item.startsWith('group:')) return validGroup(item.slice(6));
  return false;
}

function validSort(id: string, sort: string): boolean {
  if (sort === 'manual') return true;
  return sort === 'recent' && (id === 'chats' || id === 'hidden');
}

function readOne(value: unknown): SectionDoc | null {
  if (!value || typeof value !== 'object') return null;
  const row = value as Record<string, unknown>;
  const id = typeof row.id === 'string' ? row.id : '';
  const name = typeof row.name === 'string' ? row.name : '';
  const sort = typeof row.sort === 'string' ? row.sort : '';
  const items = Array.isArray(row.items) ? row.items.filter((item): item is string => typeof item === 'string') : null;
  if (!validId(id) || !validName(id, name) || typeof row.collapsed !== 'boolean' || !validSort(id, sort) || !items) return null;
  if (id === 'hidden' && items.length > 0) return null;
  if (items.length > 200 || items.some((item) => !validItem(item))) return null;
  return { id, name, collapsed: row.collapsed, sort, items };
}

// Missing and invalid both read back as an empty list, matching the gateway.
export function readSections(value: unknown): SectionDoc[] {
  if (!Array.isArray(value)) return [];
  const out: SectionDoc[] = [];
  const seen = new Set<string>();
  let custom = 0;
  let count = 0;
  for (const row of value) {
    const section = readOne(row);
    if (!section || seen.has(section.id)) return [];
    seen.add(section.id);
    if (section.id !== 'chats' && section.id !== 'hidden') custom += 1;
    count += section.items.length;
    if (custom > 12 || count > 500) return [];
    out.push(section);
  }
  return out;
}

function decodeRow(row: string): SectionDoc | null {
  const parts = row.split('\u001f');
  if (parts.length < 4) return null;
  const [id, name, flag, sort, ...rest] = parts;
  if ((flag !== '0' && flag !== '1') || id === undefined || name === undefined || sort === undefined) return null;
  return { id, name, collapsed: flag === '1', sort, items: rest.filter((item) => item !== '') };
}

// The contract sends one encoded row per section. A row that does not survive
// the same check as a stored document is refused, and nothing is written.
export function sectionsFromRows(rows: string[]): SectionDoc[] | null {
  const decoded: SectionDoc[] = [];
  for (const row of rows) {
    const section = decodeRow(row);
    if (!section) return null;
    decoded.push(section);
  }
  const checked = readSections(decoded);
  return checked.length === decoded.length ? checked : null;
}

function remembered(store: SectionStore): SectionDoc[] | null {
  let raw = '';
  try {
    raw = store.get(SECTION_KEY) ?? '';
  } catch {
    return null;
  }
  if (raw === '') return null;
  try {
    return readSections(JSON.parse(raw));
  } catch {
    return null;
  }
}

// The runner has no gateway. A saved order wins. The order fixture supplies
// the starting document until the first save.
export function offlineSections(slug: string, store: SectionStore): SectionDoc[] | null {
  const saved = remembered(store);
  if (saved !== null) return saved;
  if (slug === ORDER_FIXTURE) return readSections(FIXTURE_SECTIONS);
  return null;
}

export function rememberSections(store: SectionStore, sections: SectionDoc[]): void {
  store.set(SECTION_KEY, JSON.stringify(sections));
}

export function forgetSections(store: SectionStore): void {
  store.set(SECTION_KEY, '');
}
