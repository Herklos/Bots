import type { Sources, Result } from '../app.contract.d.ts';
import { read } from '../host/gateway.ts';
import { blankCode, codeFixture, codePath, viewFrom } from '../host/code.ts';

export const codeSources = {
  loadCode: async ([slug, gate, open], store): Promise<Result<'loadCode'>> => {
    const fixture = codeFixture(slug, gate, open);
    if (fixture) return fixture;
    const path = codePath(slug, gate, open);
    if (!path) return blankCode();
    try {
      return viewFrom(await read(store, path), open === '1');
    } catch {
      if (open !== '1') return blankCode();
      return { ...blankCode(), enabled: true, signal: true, loaded: true, error: 'offline' };
    }
  },
} satisfies Pick<Sources, 'loadCode'>;
