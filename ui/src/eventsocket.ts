export function resolveEventSocket(
  slug: string | undefined,
  sockets: ReadonlyMap<string, string | undefined>,
): { known: boolean; socket: string | undefined } {
  if (!slug) return { known: true, socket: undefined };
  return sockets.has(slug)
    ? { known: true, socket: sockets.get(slug) }
    : { known: false, socket: undefined };
}
