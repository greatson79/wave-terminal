import { expect, test } from "bun:test";
import { safeHelpText } from "./help";

test("도움 글 가림: 메일, 토큰, 홈 경로, 제어 문자", () => {
  const text = "me@example.com Bearer abc123 /Users/alice/private KEY=secret\x00\nls";
  expect(safeHelpText(text)).toBe("<EMAIL> <TOKEN> /Users/<USER>/private KEY=<SECRET> ls");
});

test("전송 글은 500자를 넘지 않는다", () => {
  expect(safeHelpText("가".repeat(501)).length).toBe(500);
});

test("현재 로그인 이름을 단독 텍스트에서도 가린다", () => {
  expect(safeHelpText("alice 작업입니다", "alice")).toBe("<USER> 작업입니다");
});
