import { describe, expect, test } from "bun:test";
import { updatePlan } from "./updateplan";

const base = { binCheckFailed: false, packCheckFailed: false, binaryTooOld: false };

describe("updatePlan — 옵션 2 분기 판정(문자열 핀 = 회귀 0 증명)", () => {
  test("★신설: 본체+팩 동시 + 호환 → 팩 무중단을 가리지 않는다", () => {
    const p = updatePlan({ ...base, binVersion: "0.12.57", packVersion: "0.12.58" });
    expect(p.kind).toBe("pack-and-binary");
    expect(p.badge).toBe("↻");
    expect(p.title).toBe("팩 0.12.58 무중단 적용 가능 (새 본체 0.12.57은 패치 설치)");
    expect(p.toastMsg).toContain("무중단 적용(재시작 없음)");
    expect(p.toastMsg).toContain("패치 설치"); // T5 개정(오너 2026-07-15) — 본체 인앱 패치 안내
  });

  test("본체+팩 동시 + 비호환 → 종전대로 본체 필요(가림이 정당한 케이스)", () => {
    const p = updatePlan({ ...base, binVersion: "0.12.57", packVersion: "0.12.58", binaryTooOld: true });
    // binVersion 존재가 우선 — 본체 안내(팩은 어차피 min_binary 미달).
    expect(p.kind).toBe("binary");
  });

  test("본체만 → 패치 설치 문구(T5 개정 — 오너 2026-07-15)", () => {
    const p = updatePlan({ ...base, binVersion: "0.12.57", packVersion: null });
    expect(p.kind).toBe("binary");
    expect(p.badge).toBe("!");
    expect(p.title).toBe("새 본체 버전 0.12.57 (Update 버튼으로 패치 설치)");
    expect(p.toastMsg).toBe("새 본체 0.12.57 — 상단 Update 버튼으로 패치 설치(재시작·자동 복원)");
  });

  test("팩만 + 호환 → 종전 무중단 문구 그대로(회귀 0)", () => {
    const p = updatePlan({ ...base, binVersion: null, packVersion: "0.12.58" });
    expect(p.kind).toBe("pack");
    expect(p.badge).toBe("↻");
    expect(p.title).toBe("팩 0.12.58 (무중단·세션 유지)");
    expect(p.toastMsg).toBe("팩 0.12.58 — 상단 Update(재시작 없음)");
  });

  test("팩만 + 비호환 → 종전 본체 필요 문구 그대로(회귀 0)", () => {
    const p = updatePlan({ ...base, binVersion: null, packVersion: "0.12.58", binaryTooOld: true });
    expect(p.kind).toBe("binary-required");
    expect(p.badge).toBe("!");
    expect(p.title).toBe("팩 0.12.58: 본체 업데이트 필요 (홈페이지에서 다운로드)");
    expect(p.toastMsg).toBe(
      "새 팩 0.12.58은 더 새로운 본체를 요구합니다 — 홈페이지(www.cysinsight.com)에서 본체 업데이트 후 적용됩니다.",
    );
  });

  test("업데이트 없음 + 양쪽 체크 성공 → '0' 배지(종전)", () => {
    const p = updatePlan({ ...base, binVersion: null, packVersion: null });
    expect(p.kind).toBe("none");
    expect(p.badge).toBe("0");
    expect(p.ok).toBe(true);
    expect(p.title).toBe("최신 버전 — 대기 중인 업데이트 없음");
  });

  test("업데이트 없음 + 체크 실패 → unknown = 배지 유지(fail-safe 종전)", () => {
    for (const f of [
      { binCheckFailed: true, packCheckFailed: false },
      { binCheckFailed: false, packCheckFailed: true },
      { binCheckFailed: true, packCheckFailed: true },
    ]) {
      const p = updatePlan({ ...base, ...f, binVersion: null, packVersion: null });
      expect(p.kind).toBe("unknown");
      expect(p.badge).toBe("");
    }
  });

  test("체크 실패여도 보존 상태가 있으면 그 상태로 판정(fail-safe 보존 종전)", () => {
    const p = updatePlan({ ...base, binCheckFailed: true, binVersion: "0.12.57", packVersion: null });
    expect(p.kind).toBe("binary");
  });
  // 판정_빨간느낌표_정적 상태표 · 팩 주소 404(첫 판은 팩을 안 올림): 수리(main.rs is_http_404) 전에는 팩 확인이
  // 실패(packCheckFailed)로 잡혀 unknown = 배지를 안 건드림 → 처음부터 보이던 빨간 ! 가 남았다. 수리 뒤에는
  // 404 = 확인 성공·팩 없음 → none = 회색 0 으로 바뀐다(main.ts checkForUpdate 가 unknown 아닐 때만 배지 갱신).
  test("팩 404 수리: 본체 최신 + 팩 없음 → 회색 0 (수리 전 = unknown 으로 빨간 ! 유지)", () => {
    const after = updatePlan({ ...base, binVersion: null, packVersion: null });
    expect(after.kind).toBe("none");
    expect(after.badge).toBe("0");
    expect(after.ok).toBe(true);
    const before = updatePlan({ ...base, binVersion: null, packVersion: null, packCheckFailed: true });
    expect(before.kind).toBe("unknown");
    expect(before.badge).toBe("");
  });
});
