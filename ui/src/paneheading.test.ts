import { expect, test } from "bun:test";
import { ensurePaneNumber, paneHeading } from "./paneheading";

test("닫힌 패인을 지워도 새 패인은 살아 있는 번호와 겹치지 않는다", () => {
  const numbers: Record<string, number> = {};
  expect(ensurePaneNumber(numbers, "a#4")).toBe(1);
  expect(ensurePaneNumber(numbers, "b#4")).toBe(2);
  expect(ensurePaneNumber(numbers, "a#5")).toBe(3);
  expect(ensurePaneNumber(numbers, "a#4")).toBe(1);
});

test("사람 작업 내용 우선, 좌석 신고 반영, 빈 작업은 대기 중", () => {
  expect(paneHeading(3, undefined, "홈페이지 글 고치는 중")).toBe("3번 · 홈페이지 글 고치는 중");
  expect(paneHeading(3, "사람 수정", "좌석 신고")).toBe("3번 · 사람 수정");
  expect(paneHeading(3, undefined, null)).toBe("3번 · 대기 중");
});
