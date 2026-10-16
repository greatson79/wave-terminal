import { expect, test } from "bun:test";
import { paneHeading, splitPaneHeading } from "./paneheading";

test("role, model, surface ID, context, and reported status compose a fixed heading", () => {
  expect(paneHeading({ sid: 8, role: "master", model: "claude-opus-5-5", ctx: 42, tail: "홈페이지 글 고치는 중" }))
    .toBe("마스터 Opus 5.5·s8 · CTX 42% · 작업 중");
  expect(paneHeading({ sid: 11, role: "worker", model: "claude-sonnet-5-5", ctx: 18 }))
    .toBe("워커 Sonnet 5.5·s11 · CTX 18% · 대기 중");
  expect(paneHeading({ sid: 11, role: "worker-2", model: "gpt-5.6-luna", ctx: 18, approximate: true }))
    .toBe("워커 gpt-5.6-luna·s11 · CTX ~18% · 대기 중");
});

test("missing values do not invent a model or zero context", () => {
  expect(paneHeading({ sid: 11, role: "worker", ctx: 18, tail: "작업" }))
    .toBe("워커·s11 · CTX 18% · 작업 중");
  expect(paneHeading({ sid: 8, model: "claude-opus-5-5" }))
    .toBe("Opus 5.5·s8 · CTX — · 대기 중");
  expect(paneHeading({ sid: 3, role: "reviewer-1", model: "claude-unknown" }))
    .toBe("리뷰어 unknown·s3 · CTX — · 대기 중");
});

test("user label appears beside the role without changing model and status", () => {
  expect(paneHeading({ sid: 8, role: "worker", label: "숙제", model: "claude-sonnet-5-5", ctx: 42 }))
    .toBe("워커(숙제) Sonnet 5.5·s8 · CTX 42% · 대기 중");
});

test("user label without a role leads the heading", () => {
  expect(paneHeading({ sid: 8, label: "숙제", model: "claude-sonnet-5-5" }))
    .toBe("숙제 · s8 · CTX — · 대기 중");
});

test("context coloring splits at the final context marker", () => {
  const heading = paneHeading({ sid: 8, role: "worker", label: "과제 · CTX 정리", ctx: 42 });
  expect(splitPaneHeading(heading)).toEqual(["워커(과제 · CTX 정리)·s8", "42% · 대기 중"]);
});
