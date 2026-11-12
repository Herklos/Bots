// The screen says what failed. A stopped gateway, an old hbots, a signed-out
// session, and a quiet T3 are not the same problem.
export const NOT_IN_VIEW = 'This message is not in view.';

export function tokenText(tokens: unknown): string {
  if (!tokens || typeof tokens !== 'object') return '';
  const n = (key: string): number => {
    const value = (tokens as Record<string, unknown>)[key];
    return typeof value === 'number' ? value : 0;
  };
  return `${n('inputTokens')} in, ${n('outputTokens')} out, ${n('cachedInputTokens')} read from cache, ${n('cacheCreationTokens')} written to cache.`;
}

export function marksOf(reactions: unknown, id: string): string {
  if (!reactions || typeof reactions !== 'object') return '';
  const list = (reactions as Record<string, unknown>)[id];
  if (!Array.isArray(list)) return '';
  return list.map((mark) => String(mark)).filter(Boolean).join('');
}

export function quoteOf(replyTo: unknown, id: string): string {
  if (!replyTo || typeof replyTo !== 'object') return '';
  const entry = (replyTo as Record<string, unknown>)[id];
  if (!entry || typeof entry !== 'object') return '';
  return String((entry as { text?: unknown }).text ?? '');
}

export type TableAlign = 'left' | 'center' | 'right';
export type MessagePart = { kind: 'text' | 'table'; text: string; head: string[]; rows: string[][]; align: TableAlign[] };

export function textPart(text: string): MessagePart {
  return { kind: 'text', text, head: [], rows: [], align: [] };
}

export function fenceMark(trimmed: string): { ch: string; n: number } | null {
  const ch = trimmed[0];
  if (ch !== '`' && ch !== '~') return null;
  let n = 0;
  while (trimmed[n] === ch) n += 1;
  if (n < 3) return null;
  return { ch, n };
}

// Block-quote depth, the same gate Exact uses: a table starts only at quote == 0.
export function quoteDepth(line: string): number {
  const trimmed = line.trimStart();
  let depth = 0;
  let i = 0;
  while (trimmed[i] === '>') {
    depth += 1;
    i += 1;
    if (trimmed[i] === ' ') i += 1;
  }
  return depth;
}

export function isRule(line: string | undefined): boolean {
  if (line === undefined) return false;
  const trimmed = line.trim();
  if (!trimmed.includes('-')) return false;
  for (const c of trimmed) {
    if (c !== '|' && c !== '-' && c !== ':' && c !== ' ') return false;
  }
  return true;
}

export function cellsOf(line: string): string[] {
  const body = line.trim();
  const cells: string[] = [];
  let cur = '';
  let escaped = false;
  for (const c of body) {
    if (escaped) {
      if (c !== '|') cur += '\\';
      cur += c;
      escaped = false;
      continue;
    }
    if (c === '\\') {
      escaped = true;
      continue;
    }
    if (c === '|') {
      cells.push(cur);
      cur = '';
      continue;
    }
    cur += c;
  }
  if (escaped) cur += '\\';
  cells.push(cur);
  if (cells.length > 0 && cells[0]!.trim() === '' && body.startsWith('|')) cells.shift();
  if (cells.length > 0 && cells[cells.length - 1]!.trim() === '' && body.endsWith('|')) cells.pop();
  return cells.map((cell) => cell.trim());
}

export function alignsOf(rule: string, width: number): TableAlign[] {
  const cells = cellsOf(rule);
  const align: TableAlign[] = [];
  for (let i = 0; i < width; i++) {
    const cell = cells[i] ?? '';
    const left = cell.startsWith(':');
    const right = cell.endsWith(':');
    align.push(left && right ? 'center' : right ? 'right' : 'left');
  }
  return align;
}

// A pipe table is a line containing `|` whose next line is a rule of `|`, `-`, `:`, and
// spaces, with at least one `-`. It ends at the first blank or pipe-less line. A header
// with no rule yet stays text, so a half-arrived message does not flash a grid.
export function splitTables(text: string): MessagePart[] {
  const lines = text.split('\n');
  const parts: MessagePart[] = [];
  const buf: string[] = [];
  let saw = false;
  let fence: { ch: string; n: number } | null = null;
  const pushText = () => {
    if (buf.length === 0) return;
    const chunk = buf.join('\n');
    buf.length = 0;
    if (chunk.trim() === '') return;
    parts.push(textPart(chunk));
  };
  for (let i = 0; i < lines.length; i++) {
    const line = lines[i] ?? '';
    const trimmed = line.trim();
    const mark = fenceMark(trimmed);
    if (fence) {
      if (mark && mark.ch === fence.ch && mark.n >= fence.n && trimmed === mark.ch.repeat(mark.n)) fence = null;
      buf.push(line);
      continue;
    }
    if (mark) {
      fence = mark;
      buf.push(line);
      continue;
    }
    const next = lines[i + 1];
    if (quoteDepth(line) === 0 && line.includes('|') && isRule(next)) {
      pushText();
      const head = cellsOf(line);
      const rows: string[][] = [];
      let j = i + 2;
      while (j < lines.length) {
        const rowLine = lines[j] ?? '';
        if (rowLine.trim() === '' || !rowLine.includes('|') || quoteDepth(rowLine) !== 0) break;
        rows.push(cellsOf(rowLine));
        j += 1;
      }
      parts.push({ kind: 'table', text: '', head, rows, align: alignsOf(next ?? '', head.length) });
      saw = true;
      i = j - 1;
      continue;
    }
    buf.push(line);
  }
  if (!saw) return [textPart(text)];
  pushText();
  return parts.length > 0 ? parts : [textPart(text)];
}

export function roomMessage(m: any) {
  const from = String(m.from ?? '');
  const text = String(m.text ?? '');
  return { id: String(m.id ?? ''), name: String(m.name ?? ''), text, from, parts: from === 'user' ? [] : splitTables(text) };
}
