export function ensurePaneNumber(numbers: Record<string, number>, key: string): number {
  if (!numbers[key]) numbers[key] = Math.max(0, ...Object.values(numbers)) + 1;
  return numbers[key];
}

export function paneHeading(number: number, manual: string | undefined, reported: string | null): string {
  return `${number}번 · ${manual || reported || "대기 중"}`;
}
