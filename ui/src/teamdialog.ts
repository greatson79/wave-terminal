export function teamErrorMessage(error: unknown, fallback: string): string {
  if (error && typeof error === "object") {
    const detail = error as {code?: unknown; message?: unknown};
    if (typeof detail.code === "string" && typeof detail.message === "string" && detail.message.trim()) return detail.message;
  }
  return fallback;
}

export interface TeamDialogOptions {
  team?: { id: string; name: string; folder: string; color?: string };
  suggestion?: string;
  chooseFolder: () => Promise<string | null>;
  save: (values: { name: string; folder: string }) => Promise<void>;
  remove?: () => Promise<void>;
}

export function openTeamDialog(options: TeamDialogOptions): HTMLDialogElement {
  const previousFocus = document.activeElement as HTMLElement | null;
  const dialog = document.createElement("dialog");
  dialog.className = "team-dialog";
  dialog.setAttribute("aria-label", options.team ? "팀 설정" : "팀 만들기");
  const title = document.createElement("h2");
  title.textContent = options.team ? "팀 설정" : "팀 만들기";
  dialog.append(title);
  const field = (text: string, value: string) => {
    const label = document.createElement("label");
    const input = document.createElement("input");
    input.type = "text";
    input.value = value;
    input.setAttribute("aria-label", text);
    label.append(text, input);
    dialog.append(label);
    return input;
  };
  const name = field("팀 이름", options.team?.name ?? options.suggestion ?? "");
  const folder = field("작업 폴더", options.team?.folder ?? "");
  let composing = false;
  let busy = false;
  const controls: HTMLButtonElement[] = [];
  const button = (text: string, action: () => unknown) => {
    const el = document.createElement("button");
    el.type = "button";
    el.textContent = text;
    el.addEventListener("click", action);
    dialog.append(el);
    controls.push(el);
    return el;
  };
  const error = document.createElement("p");
  error.setAttribute("role", "alert");
  const close = () => {
    if (busy) return;
    dialog.close();
    dialog.remove();
    previousFocus?.focus();
  };
  const showError = (e: unknown, fallback = "팀을 저장하지 못했습니다. 다시 시도해 주세요.") => {
    const details = e && typeof e === "object" ? e as { code?: unknown; message?: unknown } : null;
    const code = typeof details?.code === "string" ? details.code : null;
    name.setAttribute("aria-invalid", String(code === "invalid_name"));
    folder.setAttribute("aria-invalid", String(code === "invalid_folder"));
    const text = e instanceof Error ? e.message : typeof e === "string" ? e : "";
    error.textContent = teamErrorMessage(e, /folder|directory|폴더/i.test(text)
      ? "작업 폴더를 찾을 수 없습니다. 폴더를 다시 선택하세요."
      : fallback);
  };
  const run = async (action: () => Promise<void>) => {
    if (busy || composing) return;
    busy = true;
    error.textContent = "";
    for (const control of controls) control.disabled = true;
    name.disabled = folder.disabled = true;
    try {
      await action();
      busy = false;
      close();
    } catch (e) {
      showError(e);
    } finally {
      busy = false;
      for (const control of controls) control.disabled = false;
      name.disabled = folder.disabled = false;
    }
  };
  button("폴더 선택", async () => {
    if (busy) return;
    try {
      const selected = await options.chooseFolder();
      if (selected) folder.value = selected;
    } catch (e) {
      showError(e, "작업 폴더를 선택하지 못했습니다. 다시 선택하세요.");
    }
  });
  const save = () => {
    if (busy || composing) return;
    const value = name.value.trim();
    const count = Array.from(new Intl.Segmenter("ko", { granularity: "grapheme" }).segment(value)).length;
    if (!count || count > 40 || /[\u0000-\u001f\u007f-\u009f]/.test(value)) {
      error.textContent = "팀 이름을 1~40자로 입력하세요. 제어 문자는 사용할 수 없습니다.";
      return;
    }
    if (!folder.value.trim()) {
      error.textContent = "작업 폴더를 선택하세요.";
      return;
    }
    return run(() => options.save({ name: value, folder: folder.value.trim() }));
  };
  dialog.append(error);
  button(options.team ? "저장" : "만들기", save);
  button("취소", close);
  if (options.team && options.remove) {
    const warning = document.createElement("p");
    warning.textContent = "작업 폴더와 파일은 삭제되지 않습니다.";
    dialog.append(warning);
    button("팀 삭제", () => run(options.remove!));
  }
  dialog.addEventListener("compositionstart", () => { composing = true; });
  dialog.addEventListener("compositionend", () => { composing = false; });
  dialog.addEventListener("keydown", (e) => {
    e.stopPropagation();
    if (e.isComposing || e.keyCode === 229 || composing) {
      if (e.key === "Enter") e.preventDefault();
      return;
    }
    if (e.key === "Escape") { e.preventDefault(); close(); }
    if (e.key === "Enter" && (e.target as HTMLElement | null)?.tagName?.toLowerCase() !== "button") {
      e.preventDefault(); void save();
    }
  });
  dialog.addEventListener("submit", (e) => { e.preventDefault(); void save(); });
  dialog.addEventListener("cancel", (e) => { e.preventDefault(); close(); });
  dialog.addEventListener("paste", (e) => e.stopPropagation());
  document.body.append(dialog);
  dialog.showModal();
  name.focus();
  name.select();
  return dialog;
}
