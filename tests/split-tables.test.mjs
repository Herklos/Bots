import assert from 'node:assert/strict';
import test from 'node:test';
import { splitTables } from '../app/host/markdown.ts';

test('a pipe table splits into a header and rows', () => {
  const parts = splitTables('| A | B |\n| --- | --- |\n| c | d |\n');
  assert.deepEqual(parts, [
    {
      kind: 'table',
      text: '',
      head: ['A', 'B'],
      rows: [['c', 'd']],
      align: ['left', 'left'],
    },
  ]);
});

test('a table with no leading blank line still splits', () => {
  const parts = splitTables('See\n| A | B |\n| :--- | ---: |\n| c | d |');
  assert.equal(parts[0].kind, 'text');
  assert.equal(parts[0].text, 'See');
  assert.equal(parts[1].kind, 'table');
  assert.deepEqual(parts[1].head, ['A', 'B']);
  assert.deepEqual(parts[1].rows, [['c', 'd']]);
  assert.deepEqual(parts[1].align, ['left', 'right']);
});

test('an escaped pipe stays inside one cell', () => {
  const parts = splitTables('| A | B |\n| --- | --- |\n| a \\| b | c |\n');
  assert.equal(parts.length, 1);
  assert.deepEqual(parts[0].rows, [['a | b', 'c']]);
});

test('a table inside a fence stays text', () => {
  const text = '```\n| A | B |\n| --- | --- |\n| c | d |\n```\n';
  const parts = splitTables(text);
  assert.deepEqual(parts, [{ kind: 'text', text, head: [], rows: [], align: [] }]);
});

test('a table at the end with no trailing newline is kept', () => {
  const parts = splitTables('| A | B |\n| --- | --- |\n| c | d |');
  assert.equal(parts.length, 1);
  assert.equal(parts[0].kind, 'table');
  assert.deepEqual(parts[0].rows, [['c', 'd']]);
});
