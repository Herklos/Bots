import { expect, test } from 'bun:test';
import { codePath, publicUrl, viewFrom } from './code.ts';

test('a closed gate does not name a code route', () => {
  expect(codePath('ada', '0', '0')).toBeNull();
  expect(codePath('ada', '0', '1')).toBeNull();
  expect(codePath('', '1', '1')).toBeNull();
});

test('an open tab asks for the remote read', () => {
  expect(codePath('ada', '1', '0')).toBe('/api/bots/ada/code');
  expect(codePath('ada', '1', '1')).toBe('/api/bots/ada/code?open=1');
  expect(codePath('a b', '1', '1')).toBe('/api/bots/a%20b/code?open=1');
});

test('a credential in a repository url is not shown', () => {
  expect(publicUrl('https://user:token@github.com/org/repo')).toBe('https://github.com/org/repo');
  expect(publicUrl('https://github.com/org/repo')).toBe('https://github.com/org/repo');
  const view = viewFrom({
    enabled: true,
    signal: true,
    error: '',
    repo: { branch: 'main', baseUrl: 'https://user:token@github.com', files: [] },
    prs: [{ number: 1, title: 'One', url: 'https://user:pass@github.com/org/repo/pull/1', draft: false, checks: '', review: '', updated: '' }],
  }, true);
  expect(view.baseUrl).toBe('https://github.com');
  expect(view.prs[0]?.url).toBe('https://github.com/org/repo/pull/1');
  expect(view.loaded).toBe(true);
});
