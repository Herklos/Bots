import { expect, test } from 'bun:test';
import { localOrigin, pageOrigin, parsePromptLink, promptHref, validSlug } from './prompt-link.ts';

test('a slug matches the gateway rule', () => {
  expect(validSlug('ada-2')).toBe(true);
  expect(validSlug('2bot')).toBe(true);
  expect(validSlug('a')).toBe(true);
  expect(validSlug('a'.repeat(63))).toBe(true);
  expect(validSlug('')).toBe(false);
  expect(validSlug('a'.repeat(64))).toBe(false);
  expect(validSlug('-ada')).toBe(false);
  expect(validSlug('Ada')).toBe(false);
  expect(validSlug('../x')).toBe(false);
  expect(validSlug('a_b')).toBe(false);
});

test('a prompt is capped, decoded, and kept as text', () => {
  const parsed = parsePromptLink(`/?bot=fixture&prompt=${'a'.repeat(8001)}`);
  expect(parsed.prompt.length).toBe(8000);
  expect(parsed.bad).toBe(false);
  expect(parsed.present).toBe(true);
  expect(parsePromptLink('/?bot=fixture&prompt=a+b').prompt).toBe('a b');
  expect(parsePromptLink('/?bot=fixture&prompt=a%20b').prompt).toBe('a b');
  expect(parsePromptLink('/?bot=fixture&prompt=%3Cscript%3Ealert%3C%2Fscript%3E').prompt).toBe('<script>alert</script>');
  expect(parsePromptLink('/?bot=fixture&prompt=%3Cb%3Ehi%3C%2Fb%3E').prompt).toBe('<b>hi</b>');
  expect(parsePromptLink('/?bot=fixture&prompt=%26lt%3Bb%26gt%3B').prompt).toBe('&lt;b&gt;');
});

test('a bad encoding does not invent text', () => {
  for (const prompt of ['%ZZ', '%C3%28', '%FF', '%']) {
    const parsed = parsePromptLink(`/?bot=fixture&prompt=${prompt}`);
    expect(parsed.present).toBe(true);
    expect(parsed.bad).toBe(true);
    expect(parsed.prompt).toBe('');
    expect(parsed.bot).toBe('fixture');
  }
  expect(parsePromptLink('/?bot=%ZZ&prompt=Hi')).toEqual({ bot: '', prompt: 'Hi', present: true, bad: false });
  expect(parsePromptLink('/?bot=../x&prompt=Hi').bot).toBe('');
  expect(parsePromptLink('/?bot=-ada&prompt=Hi').bot).toBe('');
});

test('only bot and prompt are read, and the first of each wins', () => {
  expect(parsePromptLink('/?bot=ada&prompt=Hi&mode=send&model=opus&repo=x&file=%ZZ')).toEqual({
    bot: 'ada', prompt: 'Hi', present: true, bad: false,
  });
  expect(parsePromptLink('/?bot=ada&prompt=Hi&mode=draft').prompt).toBe('Hi');
  expect(parsePromptLink('/?prompt=one&prompt=two&bot=fixture&bot=bea')).toEqual({
    bot: 'fixture', prompt: 'one', present: true, bad: false,
  });
  expect(parsePromptLink('/?bot=fixture&prompt=Hi#later').prompt).toBe('Hi');
  expect(parsePromptLink('/?bot=fixture&prompt=').prompt).toBe('');
  expect(parsePromptLink('/?bot=fixture&prompt=').present).toBe(true);
  expect(parsePromptLink('/?bot=fixture').present).toBe(false);
  expect(parsePromptLink('/bot/fixture').present).toBe(false);
});

test('a copied link round-trips and a loopback origin is local', () => {
  const href = promptHref('https://hbots.example', 'fixture', 'Hello there');
  expect(href).toBe('https://hbots.example/?bot=fixture&prompt=Hello%20there');
  expect(parsePromptLink(href).prompt).toBe('Hello there');
  expect(promptHref('https://hbots.example/', 'fixture', 'Hi')).toBe('https://hbots.example/?bot=fixture&prompt=Hi');
  expect(promptHref('', 'fixture', 'Hi')).toBe('/?bot=fixture&prompt=Hi');
  expect(promptHref('https://hbots.example', 'fixture', 'a'.repeat(8001)).length).toBeLessThan(9000);
  expect(parsePromptLink(promptHref('https://hbots.example', 'fixture', 'a'.repeat(8001))).prompt.length).toBe(8000);
  expect(localOrigin('http://localhost:3780')).toBe(true);
  expect(localOrigin('http://127.0.0.1:8765')).toBe(true);
  expect(localOrigin('http://[::1]:3780')).toBe(true);
  expect(localOrigin('https://hbots.example')).toBe(false);
  expect(localOrigin('http://localhost.example.com')).toBe(false);
  expect(localOrigin('')).toBe(false);
  expect(localOrigin('not a url')).toBe(false);
});

test('the page origin comes from the host module', async () => {
  expect(await pageOrigin(null)).toEqual({ origin: '', local: false });
  expect(await pageOrigin({ available: false })).toEqual({ origin: '', local: false });
  const got = await pageOrigin({
    available: true,
    later: async () => ({ origin: 'http://127.0.0.1:3780/' }),
  });
  expect(got).toEqual({ origin: 'http://127.0.0.1:3780', local: true });
  const remote = await pageOrigin({
    available: true,
    later: async () => ({ origin: 'https://hbots.example' }),
  });
  expect(remote).toEqual({ origin: 'https://hbots.example', local: false });
});
