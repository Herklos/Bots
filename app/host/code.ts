// The Code tab's read. A closed gate never asks the gateway. Fixture slugs are
// the contract runner, which has no gateway.

export type CodeFile = { path: string; insertions: number; deletions: number };
export type CodePr = {
  number: number;
  title: string;
  url: string;
  draft: boolean;
  checks: string;
  review: string;
  updated: string;
};
export type CodeView = {
  enabled: boolean;
  signal: boolean;
  loaded: boolean;
  error: string;
  branch: string;
  defaultBranch: boolean;
  ahead: number;
  behind: number;
  hasRemote: boolean;
  provider: string;
  baseUrl: string;
  insertions: number;
  deletions: number;
  fileTotal: number;
  files: CodeFile[];
  linkedNumber: number;
  linkedTitle: string;
  linkedUrl: string;
  prs: CodePr[];
};

const FILE_CAP = 50;
const PR_CAP = 20;

export function blankCode(): CodeView {
  return {
    enabled: false,
    signal: false,
    loaded: false,
    error: '',
    branch: '',
    defaultBranch: false,
    ahead: 0,
    behind: 0,
    hasRemote: false,
    provider: '',
    baseUrl: '',
    insertions: 0,
    deletions: 0,
    fileTotal: 0,
    files: [],
    linkedNumber: 0,
    linkedTitle: '',
    linkedUrl: '',
    prs: [],
  };
}

/// `/api/bots/{slug}/code`, or null when the tab is not asking.
export function codePath(slug: string, gate: string, open: string): string | null {
  if (gate !== '1' || slug === '') return null;
  const base = `/api/bots/${encodeURIComponent(slug)}/code`;
  return open === '1' ? `${base}?open=1` : base;
}

/// Drop `user:pass@`. A URL with no password stays as written.
export function publicUrl(raw: string): string {
  const at = raw.indexOf('://');
  if (at < 0) return raw;
  const rest = raw.slice(at + 3);
  const mark = rest.indexOf('@');
  if (mark < 0) return raw;
  const info = rest.slice(0, mark);
  if (info.includes('/') || !info.includes(':')) return raw;
  return raw.slice(0, at + 3) + rest.slice(mark + 1);
}

function num(value: unknown): number {
  return typeof value === 'number' && Number.isFinite(value) ? value : 0;
}

function filesOf(body: any): CodeFile[] {
  const rows = Array.isArray(body?.repo?.files) ? body.repo.files : [];
  return rows.slice(0, FILE_CAP).map((row: any) => ({
    path: String(row?.path ?? ''),
    insertions: num(row?.insertions),
    deletions: num(row?.deletions),
  })).filter((row: CodeFile) => row.path !== '');
}

function prsOf(body: any): CodePr[] {
  const rows = Array.isArray(body?.prs) ? body.prs : [];
  return rows.slice(0, PR_CAP).map((row: any) => ({
    number: num(row?.number),
    title: String(row?.title ?? ''),
    url: publicUrl(String(row?.url ?? '')),
    draft: row?.draft === true,
    checks: String(row?.checks ?? ''),
    review: String(row?.review ?? ''),
    updated: String(row?.updated ?? ''),
  }));
}

/// `open` is the tab's full read. A signal read only needs `signal`.
export function viewFrom(body: any, open: boolean): CodeView {
  const view = blankCode();
  view.enabled = body?.enabled === true;
  view.signal = body?.signal === true;
  if (!open) return view;
  const repo = body?.repo;
  view.loaded = true;
  view.error = String(body?.error ?? '');
  if (repo && typeof repo === 'object') {
    view.branch = String(repo.branch ?? '');
    view.defaultBranch = repo.defaultBranch === true;
    view.ahead = num(repo.ahead);
    view.behind = num(repo.behind);
    view.hasRemote = repo.hasRemote === true;
    view.provider = String(repo.provider ?? '');
    view.baseUrl = publicUrl(String(repo.baseUrl ?? ''));
    view.insertions = num(repo.insertions);
    view.deletions = num(repo.deletions);
    view.fileTotal = num(repo.fileTotal);
    view.files = filesOf(body);
    const linked = repo.linked;
    if (linked && typeof linked === 'object') {
      view.linkedNumber = num(linked.number);
      view.linkedTitle = String(linked.title ?? '');
      view.linkedUrl = publicUrl(String(linked.url ?? ''));
    }
  }
  view.prs = prsOf(body);
  return view;
}

function fixture(slug: string, open: boolean): CodeView {
  const view = blankCode();
  view.enabled = true;
  view.signal = true;
  if (!open) return view;
  view.loaded = true;
  if (slug === 'code-empty') return view;
  if (slug === 'code-down') {
    view.error = 'offline';
    return view;
  }
  if (slug === 'code-remote') {
    view.error = 'no-remote';
    view.branch = 'main';
    view.defaultBranch = true;
    view.files = [{ path: 'notes.md', insertions: 1, deletions: 0 }];
    view.fileTotal = 1;
    view.insertions = 1;
    return view;
  }
  view.branch = 'feature';
  view.ahead = 2;
  view.behind = 1;
  view.hasRemote = true;
  view.provider = 'GitHub';
  view.baseUrl = 'https://github.com';
  view.insertions = 4;
  view.deletions = 1;
  view.fileTotal = 1;
  view.files = [{ path: 'main.rs', insertions: 4, deletions: 1 }];
  view.linkedNumber = 12;
  view.linkedTitle = 'Fix the gate';
  view.linkedUrl = 'https://github.com/org/repo/pull/12';
  view.prs = [{
    number: 12,
    title: 'Fix the gate',
    url: 'https://github.com/org/repo/pull/12',
    draft: true,
    checks: 'failing',
    review: 'approved',
    updated: '2026-01-02T00:00:00.000Z',
  }];
  return view;
}

const FIXTURES = new Set(['code', 'code-empty', 'code-down', 'code-remote']);

export function codeFixture(slug: string, gate: string, open: string): CodeView | null {
  if (gate !== '1' || slug === '') return blankCode();
  if (!FIXTURES.has(slug)) return null;
  return fixture(slug, open === '1');
}
