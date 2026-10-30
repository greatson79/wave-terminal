export function paneRenameErrorDetail(error: unknown): string {
  const detail = String(error);
  return detail.includes("rename denied") || detail.includes("not attributed to a surface")
    ? `이름 변경 권한을 확인할 수 없습니다. 앱과 데몬을 업데이트한 뒤 다시 시도하세요. (${detail})`
    : detail;
}

export async function commitPaneRename(
  rename: () => Promise<unknown>,
  refresh: () => Promise<unknown>,
  showError: (detail: string) => void,
): Promise<boolean> {
  let ok = false;
  try {
    await rename();
    ok = true;
  } catch (error) {
    showError(paneRenameErrorDetail(error));
  } finally {
    await refresh();
  }
  return ok;
}
