import { expect, test } from "bun:test";
import { paneHeading } from "./paneheading";

test("번호는 데몬이 발급한 surface_id: 두 패인, 닫고 새 패인, 같은 ID 복원", () => {
  expect(paneHeading(7, undefined, null)).toBe("7번 · 대기 중");
  expect(paneHeading(8, undefined, null)).toBe("8번 · 대기 중");
  expect(paneHeading(9, undefined, null)).toBe("9번 · 대기 중");
  expect(paneHeading(7, undefined, "복원된 작업")).toBe("7번 · 복원된 작업");
});

test("사람 작업 내용 우선, 좌석 신고 반영, 빈 작업은 대기 중", () => {
  expect(paneHeading(3, undefined, "홈페이지 글 고치는 중")).toBe("3번 · 홈페이지 글 고치는 중");
  expect(paneHeading(3, "사람 수정", "좌석 신고")).toBe("3번 · 사람 수정");
  expect(paneHeading(3, undefined, null)).toBe("3번 · 대기 중");
});
