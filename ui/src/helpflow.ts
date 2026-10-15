export async function sendPreparedHelp(
  preview: Record<string, unknown> | null,
  prepare: () => Promise<Record<string, unknown> | null>,
  send: (payload: Record<string, unknown>) => Promise<{ id: string }>,
): Promise<string> {
  const payload = preview ?? await prepare();
  if (!payload) throw Error("보낼 내용을 준비하지 못했습니다");
  const result = await send(payload);
  if (!result.id) throw Error("접수 번호가 없습니다");
  return result.id;
}
