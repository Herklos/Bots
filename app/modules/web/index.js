// The page origin for a copied prompt link. A data module cannot read location.
export async function later(request) {
  if (!request || request.op !== 'origin') return { origin: '' };
  const location = globalThis.location;
  const origin = location && typeof location.origin === 'string' ? location.origin : '';
  return { origin };
}
