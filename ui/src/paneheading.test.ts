import { expect, test } from "bun:test";
import { paneHeading, paneWorkspaceLabels } from "./paneheading";

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

test("소켓 둘이면 같은 surface ID도 워크스페이스 표시 이름으로 구별", () => {
  const labels = paneWorkspaceLabels([{ name: "본부" }, { socket: "internal-socket", name: "교실" }]);
  expect(paneHeading(1, undefined, "같은 작업", labels.get(""))).toBe("본부 · 1번 · 같은 작업");
  expect(paneHeading(1, undefined, "같은 작업", labels.get("internal-socket"))).toBe("교실 · 1번 · 같은 작업");
  expect(paneWorkspaceLabels([{ name: "본부" }]).size).toBe(0);
});

test("같은 표시 이름과 내부 이름도 겹치거나 노출되지 않는다", () => {
  const labels = paneWorkspaceLabels([
    { name: "교실" }, { socket: "/tmp/internal-a", name: "교실" },
    { socket: "/tmp/internal-b", name: "dept-2" },
  ]);
  expect([...labels.values()]).toEqual(["교실", "교실 2", "워크스페이스 3"]);
  const safe = paneWorkspaceLabels([{ name: "교실 · 1번" }, { socket: "other", name: "교실" }]);
  expect([...safe.values()]).toEqual(["교실 1번", "교실"]);
});
