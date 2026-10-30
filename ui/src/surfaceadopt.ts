// refreshPaneTitles의 자동입양 게이트 — role 없어도 title이 있으면 입양한다(2026-08-07 결함 수정).
// role도 title도 없는 진짜 무명 빈 셸만 계속 제외(사이드바에 잡셸이 넘치는 것을 막기 위한 원래 취지 보존).
export function shouldAdoptSurface(s: { exited: boolean; role: string | null; title: string }): boolean {
  if (s.exited) return false;
  return Boolean(s.role) || Boolean(s.title);
}
