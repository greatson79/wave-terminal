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

export function paneRenameKeyAction(e: Pick<KeyboardEvent, "key" | "keyCode" | "isComposing">): "save" | "cancel" | null {
  if (e.isComposing || e.keyCode === 229) return null;
  return e.key === "Enter" ? "save" : e.key === "Escape" ? "cancel" : null;
}

export function bindPaneRename(titleEl: HTMLElement, finish: (cancelled: boolean) => void): void {
  let cancelled = false;
  const onKey = (ke: KeyboardEvent) => {
    ke.stopPropagation();
    const action = paneRenameKeyAction(ke);
    if (action === "cancel") cancelled = true;
    if (action) { ke.preventDefault(); titleEl.blur(); }
  };
  titleEl.addEventListener("keydown", onKey);
  titleEl.addEventListener("blur", () => {
    titleEl.removeEventListener("keydown", onKey);
    titleEl.contentEditable = "false";
    finish(cancelled);
  }, { once: true });
}
