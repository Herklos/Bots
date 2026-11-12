// One tool-call row, packed into the fields a transcript row already has.
// Counts ride in `key`. Calls ride in `title`, one per line.
export function activityRow(blank: any, item: any) {
  const counts = item.counts && typeof item.counts === 'object' ? item.counts : {};
  const key = ['commands', 'edits', 'searches', 'browser', 'memory', 'vault', 'tools']
    .map((name) => String(counts[name] ?? 0))
    .join('\u001f');
  const calls = Array.isArray(item.calls) ? item.calls : [];
  const title = calls.map((call: any) => [call?.id, call?.label, call?.status, call?.target]
    .map((part) => String(part ?? '').replace(/[\n\u001f]/g, ' '))
    .join('\u001f')).join('\n');
  return {
    ...blank,
    id: String(item.id ?? ''),
    kind: 'activity',
    mine: false,
    text: '',
    title,
    head: true,
    tail: true,
    variant: String(item.status ?? ''),
    key,
    placeholder: item.detail === true ? 'detail' : '',
  };
}
