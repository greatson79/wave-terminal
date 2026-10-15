export function paneHeading({ sid, role, model, ctx, approximate, tail }: {
  sid: number;
  role?: string | null;
  model?: string | null;
  ctx?: number | null;
  approximate?: boolean;
  tail?: string | null;
}): string {
  const roleName = role === "master" ? "마스터" : role === "worker" || role?.startsWith("worker-") ? "워커"
    : role === "cso" ? "CSO" : role?.startsWith("reviewer") ? "리뷰어" : role || "";
  const modelName = model?.replace(/^claude-(opus|sonnet|haiku|fable)-(\d+)(?:-(\d+))?.*$/i,
    (_, family: string, major: string, minor?: string) => `${family[0].toUpperCase()}${family.slice(1).toLowerCase()} ${major}${minor ? `.${minor}` : ""}`)
    .replace(/^claude-/, "") || "";
  return `${[roleName, modelName].filter(Boolean).join(" ")}${roleName || modelName ? "·" : ""}s${sid} · CTX ${ctx == null ? "—" : `${approximate ? "~" : ""}${Math.round(ctx)}%`} · ${tail ? "작업 중" : "대기 중"}`;
}
