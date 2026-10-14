type WorkspaceLabelSource = { socket?: string; name: string };

export function paneWorkspaceLabels(workspaces: WorkspaceLabelSource[]): Map<string, string> {
  const bySocket = new Map<string, WorkspaceLabelSource>();
  for (const ws of workspaces) {
    const key = ws.socket ?? "";
    if (!bySocket.has(key)) bySocket.set(key, ws);
  }
  if (bySocket.size < 2) return new Map();
  const labels = new Map<string, string>();
  const used = new Set<string>();
  let index = 0;
  for (const [socket, ws] of bySocket) {
    index++;
    const name = ws.name.replace(/·/g, " ").replace(/\s+/g, " ").trim();
    const internal = !name || name === "non title" || name === "…" || /^dept-\d+$/.test(name)
      || /^ws \d+$/.test(name) || /[/\\]/.test(name) || (!!socket && name.includes(socket));
    const base = internal ? `워크스페이스 ${index}` : name;
    let label = base;
    for (let n = 2; used.has(label); n++) label = `${base} ${n}`;
    used.add(label);
    labels.set(socket, label);
  }
  return labels;
}

export function paneHeadingPrefix(surfaceId: number, workspaceLabel?: string): string {
  return `${workspaceLabel ? `${workspaceLabel} · ` : ""}${surfaceId}번 · `;
}

export function paneHeading(surfaceId: number, manual: string | undefined, reported: string | null, workspaceLabel?: string): string {
  return `${paneHeadingPrefix(surfaceId, workspaceLabel)}${manual || reported || "대기 중"}`;
}
