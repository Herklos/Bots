import type { Answer, Sources } from './app.contract.d.ts';
import { setGatewayUrl } from './host/config.ts';
import { sessionSources } from './sources/session.ts';
import { botSources } from './sources/bots.ts';
import { groupSources } from './sources/groups.ts';
import { chatSources } from './sources/chat.ts';
import { settingsSources } from './sources/settings.ts';
import { syncSources } from './sources/sync.ts';
import { librarySources } from './sources/library.ts';
import { computerSources } from './sources/computer.ts';
import { memorySources } from './sources/memory.ts';
import { promptLinkSources } from './sources/prompt-link.ts';
import { codeSources } from './sources/code.ts';

const HBOTS_URL = import.meta.env.HBOTS_URL ?? 'http://hbots-origin.invalid';
setGatewayUrl(HBOTS_URL);

// Any gateway the person runs: their machine, a Tailscale address or a tunnel. Plain `http:` to a host
// name on Apple also needs `appTransportSecurity.allowsArbitraryLoads` in app.json.
export const grants = 'net.fetch https:\nnet.fetch http:\nsecret.keep session\nsecret.keep drafts\nsecret.keep section-order\nfs.read app:/tmp\nfs.write app:/tmp\ndevice.microphone purpose.microphone\ndevice.notifications purpose.notifications';

const sources: Sources = {
  ...sessionSources,
  ...botSources,
  ...groupSources,
  ...chatSources,
  ...settingsSources,
  ...syncSources,
  ...librarySources,
  ...computerSources,
  ...memorySources,
  ...promptLinkSources,
  ...codeSources,
};
export const answer: Answer = (source, args, store, storage, native) => sources[source](args, store, storage, native);
