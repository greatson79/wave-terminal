export function paneHeading(surfaceId: number, manual: string | undefined, reported: string | null): string {
  return `${surfaceId}번 · ${manual || reported || "대기 중"}`;
}
