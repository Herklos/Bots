// A prompt link is `/?bot=<slug>&prompt=<text>`. The text is a draft, never a send.
// The cap matches the composer maxlength: 8000 UTF-16 code units.
export const PROMPT_LIMIT = 8000;

const SLUG = /^[a-z0-9](?:[a-z0-9-]{0,62})$/;

export type PromptLink = {
  bot: string;
  prompt: string;
  present: boolean;
  bad: boolean;
};

export type PageOrigin = {
  origin: string;
  local: boolean;
};

const ABSENT: PromptLink = { bot: '', prompt: '', present: false, bad: false };

export function validSlug(slug: string): boolean {
  return SLUG.test(slug);
}

// `+` is a space. A `%` that is not two hex digits, or a sequence that is not UTF-8, is rejected.
function decodeComponent(raw: string): string | null {
  let out = '';
  for (let i = 0; i < raw.length; i++) {
    const c = raw[i];
    if (c === '+') {
      out += ' ';
      continue;
    }
    if (c === '%') {
      const hex = raw.slice(i + 1, i + 3);
      if (!/^[0-9a-fA-F]{2}$/.test(hex)) return null;
      out += `%${hex}`;
      i += 2;
      continue;
    }
    out += c;
  }
  try {
    const text = decodeURIComponent(out);
    if (text.includes('\uFFFD')) return null;
    return text;
  } catch {
    return null;
  }
}

function botOf(raw: string | null): string {
  if (raw === null || raw === '') return '';
  const decoded = decodeComponent(raw);
  if (decoded === null || !validSlug(decoded)) return '';
  return decoded;
}

// The first `bot` and the first `prompt` win. model, repo, file, mode, and anything else are ignored.
export function parsePromptLink(url: string): PromptLink {
  const q = url.indexOf('?');
  if (q < 0) return ABSENT;
  const hash = url.indexOf('#', q);
  const query = url.slice(q + 1, hash < 0 ? undefined : hash);
  let botRaw: string | null = null;
  let promptRaw: string | null = null;
  for (const part of query.split('&')) {
    if (part === '') continue;
    const eq = part.indexOf('=');
    const key = eq < 0 ? part : part.slice(0, eq);
    const value = eq < 0 ? '' : part.slice(eq + 1);
    if (key === 'bot' && botRaw === null) botRaw = value;
    else if (key === 'prompt' && promptRaw === null) promptRaw = value;
  }
  const bot = botOf(botRaw);
  if (promptRaw === null) return { ...ABSENT, bot };
  const decoded = decodeComponent(promptRaw);
  if (decoded === null) return { bot, prompt: '', present: true, bad: true };
  return { bot, prompt: decoded.slice(0, PROMPT_LIMIT), present: true, bad: false };
}

export function promptHref(origin: string, bot: string, text: string): string {
  const base = origin.endsWith('/') ? origin.slice(0, -1) : origin;
  return `${base}/?bot=${encodeURIComponent(bot)}&prompt=${encodeURIComponent(text.slice(0, PROMPT_LIMIT))}`;
}

export function localOrigin(origin: string): boolean {
  if (origin === '') return false;
  try {
    const host = new URL(origin).hostname;
    return host === 'localhost' || host === '127.0.0.1' || host === '::1' || host === '[::1]';
  } catch {
    return false;
  }
}

type LaterNative = {
  available?: boolean;
  later?: (request: Record<string, unknown>) => Promise<Record<string, unknown>>;
} | null | undefined;

export async function pageOrigin(native: LaterNative): Promise<PageOrigin> {
  if (!native?.available || !native.later) return { origin: '', local: false };
  const reply = await native.later({ op: 'origin' });
  const origin = typeof reply.origin === 'string' ? reply.origin.replace(/\/$/, '') : '';
  return { origin, local: localOrigin(origin) };
}
