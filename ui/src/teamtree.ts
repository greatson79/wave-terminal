// 사이드바 3단 계층의 3층 — 팀(ws) 아래 팀원(pane) 행 계산.
//
// 계층은 본부(그룹) > 팀(ws) > 팀원(pane)이다. 1층·2층은 hierarchy.ts(ws.name 규약)가 이미
// 소유하고, 이 모듈은 "한 팀의 pane들을 그 팀 아래에만 묶어 보여주는" 3층의 순수 계산만 한다.
// 팀원은 ws.tree에서 온 sid 목록이므로 타 팀과 섞일 수 없다(묶임 불변식의 근거 = 트리 소유권).
//
// 라벨은 `{이름}, {직무}` 형식이다. 이름 해소는 제목에서만 한다. 표준 제목에서는 본부 토큰
// 다음의 `이름 직무` 토큰을 첫 공백에서 나눈다 — 코드에 role→이름 매핑을 박지 않는다.
// raw s번호·모델명·퍼센트는 워커 행에 노출하지 않는다.

// ★R2 [3] 수리: 표시 모델에서 **상태 신호를 전부 걷어냈다**(ctxPct·state·agentAlive·dot·active).
// dot 을 지운 뒤에도 `active` 가 `.dim`·opacity 로 idle/dead 를 계속 시각화해서, 정본 사양
// ("탭/사이드바 부가지표 제거")이 형태만 바꾼 채 유지되고 있었다(검토 지적). 표시 모델이 상태를
// **모르게** 만드는 것이 근본 수리다 — 데이터가 남아 있으면 다음 사람이 다시 그린다.
export interface TeamMemberInput {
  sid: number;
  role?: string | null;
  title?: string | null;
}

export interface TeamMemberRow {
  sid: number;
  label: string;
}

const isAutoTitle = (t: string | null | undefined) => !t || /^surface \d+$/.test(t.trim());

// ★R7: 브랜드·본부·주소 판정은 hierarchy.ts가 단일 출처 — 제목 소비자마다 문법을 재구현하지 않는다.
import { isAddressToken, isHqBrand, isHqToken } from "./hierarchy";

// 모델명 세그먼트(`Opus5`·`Sonnet4.6`·`GPT-5`) — 라틴 문자로 시작해 숫자로 끝나는 꼬리.
// `COO`·`reviewer` 같은 직함(숫자 없음)은 매치되지 않는다 — 레거시 닉 제목의 직함 보존.
const MODEL_TOKEN = /^[A-Za-z][A-Za-z\-]*\d[\d.]*$/;

// 직무 토큰 자체에 실린 괄호 애칭 — 직무와 표시 이름을 분리한다.
const DUTY_NICK = /^(.+?)\((.+)\)$/;

// ★확정 SOT(2026-08-24): 제목 = `{본부닉}·{본부}·{이름 직무}·{s번호}`. `[exited]`와 주소는
// 표시에서 제외하고, 본부 다음 토큰을 첫 공백에서 나눠 `{이름}, {직무}`로 만든다. 공백이 없으면
// 그 토큰만 표시한다. 본부닉을 사람 이름으로 대체하지 않으며, 레거시 `…본부장`은 `본부장` 유지.
export function memberLabel(m: TeamMemberInput): string {
  const title = (m.title ?? "").trim();
  const role = (m.role ?? "").trim();
  const segs = title.split("·").map((token) => token.trim()).filter(Boolean);
  if (isAutoTitle(title)) return role || `s${m.sid}`;

  const hqIdx = segs.findIndex(isHqToken);
  if (hqIdx >= 0) {
    const identity = segs.slice(hqIdx + 1).find((token) =>
      token !== "[exited]" && !isAddressToken(token) && !MODEL_TOKEN.test(token)
    );
    if (!identity) return role || `s${m.sid}`;
    if (identity === "본부장") return "본부장";

    const firstSpace = identity.search(/\s/);
    if (firstSpace > 0) {
      const name = identity.slice(0, firstSpace).trim();
      const duty = identity.slice(firstSpace).trim();
      if (name && duty) return `${name}, ${duty}`;
    }
    return identity;
  }

  const ai = segs.findIndex(isAddressToken);
  if (ai >= 1) {
    // 표준 제목 — 주소 바로 앞 세그먼트가 직무직함이다(4세그: 닉·본부·직함·s번호 → 직함).
    // 주소 앞 정체성이 1조각이면 그것은 작업명이므로 역시 그 조각(같은 식: ai-1).
    // 단 그 조각이 본부명·브랜드면(직함 누락 제목) 행에 본부를 반복하지 않는다 — role 폴백.
    const cand = segs[ai - 1];
    if (!cand || isHqToken(cand) || isHqBrand(cand)) return role || `s${m.sid}`;

    // 닫힌 브랜드 표에서만 인정해 임의 두 번째 조각을 실명으로 승격하지 않는다.
    if (ai >= 4 && isHqBrand(`${segs[0]}·${segs[1]}`)) return `${segs[ai - 2]}, ${cand}`;

    const paren = cand.match(DUTY_NICK);
    const duty = paren?.[1].trim();
    const nick = paren?.[2].trim();
    if (duty && nick) return `${nick}, ${duty}`;

    return cand;
  }
  // 레거시 제목 — 종전 규칙 보존(본부장은 직책으로 통일 · 그 외는 마지막 의미 토큰).
  if (/본부장$/.test(title)) return "본부장";
  const tail = [...segs];
  while (tail.length > 1 && MODEL_TOKEN.test(tail[tail.length - 1])) tail.pop();
  // 첫 세그먼트가 권위 브랜드면 표준 조립분 — 브랜드(닉)는 패널 헤더가 보여주므로 뺀다.
  // 종전 규칙(마지막 의미 토큰)을 유지한다 — 브랜드 닉이 이 분기를 오발동하면 안 된다.
  if (tail.length >= 2 && isHqBrand(tail[0]) && !tail.some(isHqToken)) return tail.slice(1).join("·");
  // ★R1 minor 수리: nullish(`??`)가 아니라 **falsy(`||`)** 폴백이어야 한다. `role` 이 null 이 아니라
  // **빈 문자열**이면 `??` 는 그것을 통과시켜 `s{sid}` 폴백에 도달하지 못하고, 사이드바 팀원 이름이
  // **빈 값**으로 렌더된다. segs 도 같은 함정이다 — 제목이 `'·'`·`'··'`·공백뿐이면 segs 가 비어
  // `at(-1)` 이 undefined 이므로, 세 단계 전부 falsy 기준으로 내려가야 이름이 반드시 남는다.
  const lastSeg = tail.at(-1);
  return lastSeg || role || `s${m.sid}`;
}

export function teamMemberRows(members: readonly TeamMemberInput[]): TeamMemberRow[] {
  return members.map((m) => ({ sid: m.sid, label: memberLabel(m) }));
}
