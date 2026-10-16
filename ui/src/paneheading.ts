export function paneHeading({ sid, role, label, model, ctx, approximate, tail }: {
  sid: number;
  role?: string | null;
  label?: string | null;
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
  const prefix = label ? roleName ? `${roleName}(${label})${modelName ? ` ${modelName}` : ""}·s${sid}`
    : `${label} · s${sid}`
    : `${[roleName, modelName].filter(Boolean).join(" ")}${roleName || modelName ? "·" : ""}s${sid}`;
  return `${prefix} · CTX ${ctx == null ? "—" : `${approximate ? "~" : ""}${Math.round(ctx)}%`} · ${tail ? "작업 중" : "대기 중"}`;
}

export function splitPaneHeading(heading: string): [string, string] {
  const marker = " · CTX ";
  const at = heading.lastIndexOf(marker);
  return at < 0 ? [heading, ""] : [heading.slice(0, at), heading.slice(at + marker.length)];
}
