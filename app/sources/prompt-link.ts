import type { NativeModule, Result, Sources } from '../app.contract.d.ts';
import { pageOrigin, parsePromptLink } from '../host/prompt-link.ts';

export const promptLinkSources = {
  readPromptLink: ([url]): Result<'readPromptLink'> => parsePromptLink(url),
  loadPageOrigin: async (_args, _store, _storage, native: NativeModule | null | undefined): Promise<Result<'loadPageOrigin'>> => pageOrigin(native),
} satisfies Pick<Sources, 'readPromptLink' | 'loadPageOrigin'>;
