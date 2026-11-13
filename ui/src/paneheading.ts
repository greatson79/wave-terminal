import { HQ_SEP, isAddressToken, isHqLeadName, isHqToken } from "./hierarchy";

export function paneHeading({ sid, role, title, label }: {
  sid: number;
  role?: string | null;
  title?: string | null;
  label?: string | null;
  model?: string | null;
  ctx?: number | null;
  approximate?: boolean;
  tail?: string | null;
}): string {
  const roleName = role === "master" ? "마스터" : role === "worker" || role?.startsWith("worker-") ? "워커"
    : role === "cso" ? "CSO" : role?.startsWith("reviewer") ? "리뷰어" : role || "";
  // Structured titles identify heads independently of the roles used by routing hooks.
  const parts = (title ?? "").split(HQ_SEP).map((part) => part.trim());
  const dept = parts.findIndex(isHqToken);
  const duty = dept >= 0 ? parts[dept + 1] : undefined;
  const name = duty?.match(/^(.+?)\s+본부장$/)?.[1].trim()
    ?? (duty === "본부장" && dept === 1 ? parts[0] : "");
  const headName = name && !isHqToken(name) && !isAddressToken(name) && !isHqLeadName(name) ? name : "";
  const seatName = duty && !isHqToken(duty) && !isAddressToken(duty) && !isHqLeadName(duty) ? duty : "";
  const seatDuty = dept >= 0 ? parts[dept + 2] : undefined;
  const namedRole = (roleName === "워커" || role === "coo" || role === "secretary") && seatName
    ? seatDuty === "워커" ? `워커 ${seatName}` : /^(COO|비서\d+)$/i.test(seatDuty ?? "") ? seatName : ""
    : "";
  if (headName || namedRole) return `${headName ? `본부장 ${headName}` : namedRole} · s${sid}`;
  return label ? roleName ? `${roleName}(${label})·s${sid}` : `${label} · s${sid}`
    : `${roleName}${roleName ? "·" : ""}s${sid}`;
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
