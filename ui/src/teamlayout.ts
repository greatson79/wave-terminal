export const LEGACY_LAYOUT_KEY = "cys-layout-v2";
export const TEAM_LAYOUT_KEY = "cys-layout-v3";
export const LAYOUT_BACKUP_KEY = "cys-layout-v2-backup";
export const LAYOUT_VERSION_KEY = "cys-layout-version";

type LayoutStorage = Pick<Storage, "getItem" | "setItem">;

export function loadTeamLayout(storage: Pick<LayoutStorage, "getItem">): unknown {
  return JSON.parse(storage.getItem(TEAM_LAYOUT_KEY) ?? storage.getItem(LEGACY_LAYOUT_KEY) ?? "null");
}

export function saveTeamLayout(storage: LayoutStorage, payload: unknown): void {
  const encoded = JSON.stringify(payload);
  if (encoded === undefined) throw new Error("저장할 작업 공간을 확인해 주세요.");
  const original = storage.getItem(LEGACY_LAYOUT_KEY);
  if (original !== null && storage.getItem(LAYOUT_BACKUP_KEY) === null)
    storage.setItem(LAYOUT_BACKUP_KEY, original);
  storage.setItem(TEAM_LAYOUT_KEY, encoded);
  storage.setItem(LAYOUT_VERSION_KEY, "3");
}
