// Only terminal-scoped, ordered numbered lines establish visible movement.
export function classifyViewport(before, after) {
  const valid = value => !value?.error && !value?.helperError && !value?.captureError && value?.source === 'terminal-visible-markers' &&
    Array.isArray(value.markers) && value.markers.length >= 2 &&
    value.markers.every((n,i,a) => Number.isSafeInteger(n) && n > 0 && (i === 0 || n === a[i-1]+1));
  if (!valid(before) || !valid(after)) return 'NOT_PROVEN';
  const delta = after.markers[0] - before.markers[0];
  return delta < 0 ? 'MOVED_UP' : delta > 0 ? 'MOVED_DOWN' : 'STATIONARY';
}

// Streaming may advance markers; zero response means no older history at either sample.
export function hasNoOlderResponse(before, immediate, after) {
 const states=[classifyViewport(before,immediate),classifyViewport(before,after)];
 return states.includes('NOT_PROVEN') ? null : !states.includes('MOVED_UP');
}
