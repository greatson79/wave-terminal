// 본부-팀 계층 자동 그룹 배정 — 단일 메인 소켓에서 계층을 표현하는 명명 규약 파서.
//
// 원천 설계 §3은 부서마다 독립 cys-dept 소켓(depts.json parent)을 전제하지만 우리는
// 다중소켓 불사용 확정이므로, 계층은 "소켓"이 아니라 ws.name의 구분자에서 뽑는다:
// 기존 GroupMeta/groupId(수동 드래그 그룹)를 그대로 재사용한다 — 자동 생성분만 auto:true로 표시해
// 수동 그룹과 구분하고, 이름이 바뀌면 재배정한다(= ws.name이 계층의 진실원).
//
// ★설계안 §3과의 의도적 편차 1건: 설계안은 "본부명 단독 → 최상위 그룹 자동 생성"도 요구하지만,
// 그러면 "non title"·"ws 1" 같은 평범한 탭이 전부 1인 그룹이 되어 사이드바가 폭발한다. 구분자가
// 있는 이름만 그룹을 만들고, 구분자 없는 이름은 같은 이름의 auto 그룹이 이미 있을 때만 합류한다.

export const HQ_SEP = "·";

export interface HierWorkspace {
  name: string;
  pending?: boolean;
  groupId?: number;
}

export interface HierGroup {
  id: number;
  name: string;
  collapsed: boolean;
  pinned: boolean;
  color?: string;
  auto?: boolean;
}

// 본부 토큰 판정 — "…본부" 또는 "…본부(별칭)". 현장 실측(2026-08-07 `cys list`)에서 소환 이름이
// 위치가 아니라 **토큰 모양**으로 본부를 찾는다. "본부장"은 본부가 아니다(끝이 '장'이라 불일치).
// ★티켓1(P1·2026-08-08 COO 발주·주인님 실관측): `부`로 끝나는 조직 단위도 본부 토큰이다 —
// 다만 모든 `부` 종결 일반어(공부·전부)를 조직으로 승격하지 않도록 `…부`는 최소 3자 구조를
// 요구한다. `본부장`은 끝이 '장'이라 여전히 불일치(팀명 오인 방지 테스트로 고정).
const HQ_TOKEN = /본부(\(.+\))?$/;
const DEPT_TOKEN = /^.{3,}부(\(.+\))?$/;
export const isHqToken = (token: string): boolean => HQ_TOKEN.test(token) || DEPT_TOKEN.test(token);

// ★B3-r2(검토 REVISE): 이름에서 **본부 토큰만** 뽑는다 — 표시용(색·배지) 단일 해소기.
//
// parseWsName 은 "본부·팀" 형태를 전제해 team 이 비면 hq=null 을 돌려준다. 그런데 실제
// 즉 진짜 본부 탭일수록 parseWsName 으로는 본부를 못 읽는다(실측 확인). 그 결과 B3 색이
// 정작 본부 탭에 안 붙고 생성순서 ws.id 색으로 되돌아갔다.
//
// 여기서는 계층 승격 판정이 아니라 **표시용 이름 해소**만 한다. 그래서 team 유무를 따지지
// 않고 세그먼트 중 본부 토큰을 그대로 집는다. buildTab 과 그룹 헤더가 이 하나를 공유해
// 두 곳이 서로 다른 키로 색을 뽑는 드리프트를 막는다.
//   "잡담"            → null        (본부 칩 없음·색은 공통 중립색)
export function hqTokenOf(name: string): string | null {
  const segs = (name ?? "").split(HQ_SEP).map((s) => s.trim());
  if (segs.some((s) => !s)) return null; // 빈 세그먼트 = 규약 밖 이름
  const hi = segs.findIndex(isHqToken);
  if (hi >= 0) return segs[hi];
  // ★병합 통합(2026-08-17): 표준 제목 시대의 본부는 "…본부" 토큰이 아니라 **권위 브랜드**다.
  // 이 함수 하나로 색 키를 뽑기 때문이다. 브랜드 집합은 닫혀 있으므로 유령 승격 위험은 없다
  // (기능명 "개발"은 토큰도 브랜드도 아니라 계속 null = 본부 칩 없음).
  const bi = segs.findIndex(isHqBrand);
  return bi >= 0 ? brandDept[segs[bi]] : null;
}

// 이름을 구분자로 쪼개 본부 토큰을 찾는다. hq=본부명, team=나머지 전부(순서 보존 — 에이전트
// 이름 같은 앞머리가 행에서 사라지지 않는다). 빈 세그먼트 또는 본부 토큰 미매치는 계층으로
// 승격하지 않는다.
export function parseWsName(name: string): { hq: string | null; team: string } {
  const segs = name.split(HQ_SEP).map((s) => s.trim());
  if (segs.length < 2) return { hq: null, team: name };
  if (segs.some((s) => !s)) return { hq: null, team: name };
  const hi = segs.findIndex(isHqToken);
  if (hi >= 0) {
    // ★본부 닉 도입(2026-08-08) 후 team 규칙 정정: 본부 토큰 **뒤** 세그먼트만 팀명이다.
    // 닉은 ws 이름(hqOf)이 가져가고, 행은 사람 이름만 보여준다.
    const team = segs.slice(hi + 1).join(HQ_SEP);
    if (!segs[hi] || !team) return { hq: null, team: name };
    return { hq: segs[hi], team };
  }
  // ★폴백 전면 폐지(티켓1 근본수리·fail-closed): 종전 "첫 구분자=본부" 폴백은 토큰 미매치
  // 임의 제목마다 **없는 조직을 조용히 창작**했다 —
  // 소환 1건마다 가짜 그룹이 재현된 실피해(s166·s168·s172). 잘못된 그룹보다 그룹 없음이 안전하다:
  // 미매치는 {hq:null} = 그룹 없이 평면 배치. 리뷰어 등 비본부 노드의 정답 배치이기도 하다.
  // ★R7(주인님 실측 2026-08-17): 본부 토큰 미매치라도 **권위 브랜드**(닫힌 집합 — hqTokenOf·
  // adoptHqOf 와 동일 순서: 토큰 우선·브랜드 폴백)면 본부다. 이게 없어서 3요소 제목
  // 생기지 않는다(임의 첫-토큰 폴백의 부활이 아니다 — 티켓1 fail-closed 원칙 유지).
  const bi = segs.findIndex(isHqBrand);
  if (bi >= 0) {
    const team = segs.slice(bi + 1).join(HQ_SEP);
    if (team) return { hq: segs[bi], team };
  }
  return { hq: null, team: name };
}

// CLI 경로 브리지 — `cys new-surface --title "본부·팀"`으로 만든 surface는 ws를 만들지 않고 기존 ws의
// pane으로 입양되므로 ws.name이 안 바뀌어 계층에 잡히지 않았다(오너 실사용 결함: 본부 패널 미분리).
// 규약을 만족하는 title만 "그 이름의 ws 1개"로 승격시킨다 — 그룹 판정은 여전히 ws.name만 본다
// surface title은 그룹 판정에 쓰지 않고, ws 생성 시점의 이름 소스로만 쓴다.
// 규약 미충족 title은 null → 종전대로 기존 ws의 pane으로 입양된다.
// 이름은 원래 순서대로 두고 구분자 주변 공백만 정규화한다 — 본부를 앞으로 끌어오면 데몬 원장의
// title과 UI 이름이 어긋나기 때문(그룹 판정은 parseWsName이 위치와 무관하게 하므로 불필요하다).
export function hierarchyWsName(title: string): string | null {
  const norm = (title ?? "")
    .split(HQ_SEP)
    .map((s) => s.trim())
    .join(HQ_SEP);
  return parseWsName(norm).hq ? norm : null;
}

// 본부 소환 패널 — 그룹 헤더 '+'로 그 본부에 새 팀을 만들 때 쓸 이름. 규약("본부·팀")을 자동으로
// 지켜 주므로 사용자가 구분자를 외울 필요가 없다. 이미 쓰는 이름은 건너뛴다(중복 팀명 방지).
export function nextTeamName(hq: string, existingNames: readonly string[]): string {
  const used = new Set(existingNames.map((n) => parseWsName(n)).filter((p) => p.hq === hq).map((p) => p.team));
  for (let i = 1; ; i++) {
    const team = `팀 ${i}`;
    if (!used.has(team)) return `${hq}${HQ_SEP}${team}`;
  }
}

// 사이드바 행에 표시할 이름 — 자기 본부 그룹 안에 있으면 팀명만(그룹 헤더가 본부명을 이미 보여준다).
export function tabLabel(ws: HierWorkspace, groups: readonly HierGroup[]): string {
  const { hq, team } = parseWsName(ws.name);
  if (!hq) return ws.name;
  const g = groups.find((x) => x.id === ws.groupId);
  return g?.auto && g.name === hq ? team : ws.name;
}

// 본부는 본부장부터 보여야 조직도로 읽힌다(오너 지시 2026-08-07).
// ★표준 제목 대응(2026-08-17): 제목 꼬리가 주소·모델(`…·s207·Opus5`)이 되면서 "이름의 끝"으로는
const HQ_LEAD_TOKEN = /본부장$/;
export const isHqLeadName = (name: string): boolean =>
  (name ?? "")
    .split(HQ_SEP)
    .map((s) => s.trim())
    .some((s) => HQ_LEAD_TOKEN.test(s));

// 본부 안의 팀 정렬 — 본부장 먼저, 나머지는 기존 순서 보존(사용자 드래그 순서를 뒤집지 않는다).
export function sortHqTeams<T extends { name: string }>(teams: readonly T[]): T[] {
  const lead = teams.filter((t) => isHqLeadName(t.name));
  return lead.length ? [...lead, ...teams.filter((t) => !isHqLeadName(t.name))] : [...teams];
}

// 이름 규약에 따라 그룹을 자동 생성/배정한다. workspaces의 groupId를 제자리 갱신하고 새 groups를 반환.
// 수동 그룹(auto 미표시)은 건드리지 않으며, 구분자 없는 ws의 기존 소속도 보존한다.
export function autoAssignGroups(
  workspaces: HierWorkspace[],
  groups: readonly HierGroup[],
  groupCounter: number,
): { groups: HierGroup[]; groupCounter: number } {
  const next = [...groups];
  let counter = groupCounter;
  const autoByName = new Map<string, HierGroup>();
  for (const g of next) if (g.auto) autoByName.set(g.name, g);

  for (const ws of workspaces) {
    if (ws.pending) continue; // 런칭 중 placeholder("…")는 이름이 미확정
    // ws 이름 자체가 본부면 그룹을 만들지 않는다 — 본부 패널 = ws 행 하나, 그 아래 구성원.
    // 수동 드래그 그룹과 레거시 "본부·팀" ws는 종전 경로 그대로다.
    if (isHqWsName(ws.name)) continue;
    const { hq } = parseWsName(ws.name);
    if (!hq) continue;
    let g = autoByName.get(hq);
    if (!g) {
      g = { id: counter++, name: hq, collapsed: false, pinned: false, auto: true };
      next.push(g);
      autoByName.set(hq, g);
    }
    ws.groupId = g.id; // 이름이 진실원 — rename하면 즉시 해당 본부로 이동
  }
  return { groups: next, groupCounter: counter };
}

// 이 pane이 지금 앉아 있는 팀이 이름 규약과 어긋나는가 — 어긋나면 가야 할 ws 이름을 돌려준다.
//
// 자동입양은 title이 규약을 만족하면 전용 ws로 보내지만, 규약을 못 만족하던 시절이나 구 빌드에서
// 3층(팀원 목록)이 생기면서 그 오배정이 눈에 보이게 된 것이다 — 표시 결함이 아니라 배정 결함이다.
// ws.name이 계층의 진실원이므로(parseWsName 주석), 규약을 만족하는 title은 매 틱 제 자리로 돌려보낸다.
export function paneRehomeTarget(currentWsName: string, title: string): string | null {
  // ★R4 [2]: 입양과 **같은 판정자**(adoptHqOf — 브랜드 일치 기준)를 쓴다. 두 경로가 다른 기준을
  // 쓰면 입양이 브랜드로 보낸 pane 을 재배치가 매 틱 되돌리거나(깜박임), 반대로 잘못 앉은 pane 이
  // 영영 구조되지 않는다. 배치의 기준은 한 곳에만 둔다.
  const want = adoptHqOf(title);
  if (!want || want === currentWsName) return null;

  // 주소 없는 레거시 제목 `사람이름·본부명·직무`에서 첫 토큰만 권위 브랜드로 고친 rename은
  // 같은 본부의 기존 브랜드 ws를 재사용한다. hqOf의 레거시 `닉·본부명` 계약은 그대로 두고,
  // 실제로 다른 ws에서 들어오는 rename 재귀속에만 적용해 기존 레거시 ws를 흔들지 않는다.
  const currentDept = hqTokenOf(currentWsName);
  const renamedDept = hqTokenOf(title);
  const segs = title.split(HQ_SEP).map((s) => s.trim());
  const room = segs.slice(0, 2).join(HQ_SEP);
  const currentFirst = currentWsName.split(HQ_SEP)[0]?.trim() ?? "";
  if (currentDept && currentDept === renamedDept && !isHqBrand(currentFirst)) {
    const branded = isHqBrand(room) ? room : isHqBrand(segs[0]) ? segs[0] : null;
    if (branded && branded !== currentWsName) return branded;
  }
  return want;
}

// 표준 제목의 주소 세그먼트 — `s208` 처럼 시스템이 붙인 surface 번호. 사람이 주는 정체성과
// 달리 이 조각은 **시스템만 만들 수 있으므로**, "이 제목이 표준 조립분인가"의 지문으로 쓴다.
export const isAddressToken = (token: string): boolean => /^s\d+(\s*\[exited\])?$/.test((token ?? "").trim());

// Organization names come only from the user's local naming.json roles map.
let brandDept: Readonly<Record<string, string>> = {};
let topBrand: string | null = null;

export function configureHierarchyNaming(raw: string | null): boolean {
  let roles: Record<string, { name?: unknown; label?: unknown }>;
  try {
    const doc = JSON.parse(raw ?? "null");
    roles = doc?.roles && typeof doc.roles === "object" && !Array.isArray(doc.roles) ? doc.roles : {};
  } catch {
    roles = {};
  }
  const next: Record<string, string> = Object.create(null);
  for (const role of Object.values(roles)) {
    if (typeof role?.name !== "string" || !role.name.trim() || typeof role.label !== "string") continue;
    const dept = role.label.split(HQ_SEP).map((s) => s.trim()).find(isHqToken);
    if (dept) next[role.name.trim()] = dept;
  }
  topBrand = typeof roles.master?.name === "string" && next[roles.master.name.trim()]
    ? roles.master.name.trim() : null;
  if (topBrand) for (const role of Object.values(roles)) {
    if (role?.name !== topBrand || typeof role.label !== "string") continue;
    const parts = role.label.split(HQ_SEP).map((s) => s.trim());
    if (parts.length >= 3 && parts[0] && !isHqToken(parts[0])) next[`${topBrand}${HQ_SEP}${parts[0]}`] = "";
  }
  brandDept = next;
  return Object.keys(next).length > 0;
}

export function hqDisplayName(hq: string): string {
  const t = (hq ?? "").trim();
  const dept = brandDept[t];
  return dept ? `${t}${HQ_SEP}${dept}` : t;
}

export const isHqBrand = (token: string): boolean => Object.prototype.hasOwnProperty.call(brandDept, (token ?? "").trim());

// ★구조 정정(오너 2026-08-08): "본부 패널에 본부장과 팀원들이 ws에 함께 pane을 열고 있어야 한다".
// 즉 워크스페이스의 단위는 **팀이 아니라 본부**다. 'A본부·본부장'과 'A본부·시청로그팀'은 같은
// ws(=A본부)에서 나란히 열려야 하고, 그래야 본부 하나를 고르면 그 본부 사람들이 한 화면에 보인다.
//
// ★표준 제목 도입(오너 2026-08-17): 제목이 `{본부브랜드}·{작업명}·s{번호}·{모델}` 로 표준화되면서
//
// 그런데 "첫 세그먼트를 본부로 쓴다"는 규칙은 과거에 **없는 조직을 창작한 전력**이 있다(티켓1:
// `reviewer-gemini · page-dept` → 유령 그룹 'page'). 그래서 첫-세그먼트 규칙을 임의 이름 전체에
// 풀어주지 않고, **주소 세그먼트(`s208`)가 있는 표준 제목에만** 적용한다 — 주소는 시스템이 붙이므로
// 그 존재가 "이 제목은 조립된 표준 제목"의 결정론 증거다. 주소 없는 이름은 종전 fail-closed 경로
// (본부 토큰이 있으면 그것까지, 없으면 그룹 없음)로 남는다.
export function hqOf(title: string): string | null {
  const segs = (title ?? "").split(HQ_SEP).map((x) => x.trim());
  if (segs.length < 2) return null;
  if (segs.some((s) => !s)) return null;
  // 표준 제목: 첫 세그먼트 = 본부 브랜드명. 주소가 있을 때만 이 규칙이 열린다.
  // ★주소의 **위치**까지 본다: 주소 앞 정체성이 2조각 이상일 때만 첫 조각이 본부다.
  //   위치를 안 보면 본부를 못 받은 노드의 **작업명이 본부로 오승격**돼 작업명짜리 유령 패널이
  //   생긴다(주소만 보던 초안의 결함 — 테스트가 잡았다).
  const ai = segs.findIndex(isAddressToken);
  const room = segs.slice(0, 2).join(HQ_SEP);
  if (ai === 4 && segs.length === 5 && isHqBrand(room)) return room;
  // ★R2 [8] 수리: 첫 세그먼트를 **무조건** 브랜드로 인정하지 않는다 — 권위 브랜드 집합에
  //   있어야 본부다. 이게 없어서 기능명 "개발"이 본부 헤더로 표시됐다(주인님 화면 실관측).
  // ★프로젝트 패널 지원(발주 2026-08-24 · 주인님 직접 명령): 브랜드 닫힌 집합에 더해 **조직
  //   토큰**(`isHqToken` — …본부/…부)도 여기서 승격한다. 종전에는 이 분기만 `isHqBrand` 를 물어,
  //   같은 제목이라도 주소가 없으면 통과하고(아래 hi 분기) **주소가 붙으면 null** 이 되는 비대칭이
  //   있었다. cys 는 제목에 s번호를 자동으로 붙이므로 실사용 3경로(rename·신규 소환·콜드 재시작)가
  //   전부 이 분기를 타 조직 토큰으로 끝나는 프로젝트 패널이 파생되지 않았다.
  //   유령 조직 차단은 유지된다 — DEPT_TOKEN 이 `…부` 3자 이상을 요구해 기능명·일반어는 불통과다.
  if (ai >= 2) return isHqBrand(segs[0]) || isHqToken(segs[0]) ? segs[0] : null;
  if (ai >= 0) return null; // 표준 제목이지만 본부 없음 → 그룹 없이 평면 배치(fail-closed)
  const hi = segs.findIndex(isHqToken);
  if (hi >= 0) return segs.slice(0, hi + 1).join(HQ_SEP); // 닉(앞 세그먼트 전부) + 본부명
  // 브랜드**(닫힌 집합)면 본부다 — adoptHqOf 와 동일 순서(엄격 판정 우선·브랜드 폴백).
  // 임의 토큰은 여전히 null(티켓1 fail-closed: 유령 조직 창작 금지 — 집합이 잠금을 제공한다).
  return isHqBrand(segs[0]) ? segs[0] : null;
}

// ws 이름이 "본부 ws"인가 — 더미 조각을 하나 붙여도 ws 이름이 자기 자신으로 나오면 그렇다.
// hqOf가 폴백을 타 "page"를 돌려주기 때문이다(실측 — 유령 그룹 'page'가 남았다).
// 더미를 붙여 실제 소환 제목과 같은 형태로 만들면 판정이 일치한다. 표준 제목 시대에는 ws 이름이
// 한 조각(브랜드명)이라 **주소 더미(`s0`)** 로도 물어봐야 한다 — 두 형식을 모두 인정한다.
export function isHqWsName(name: string): boolean {
  return isHqBrand(name) || hqOf(`${name}${HQ_SEP}s0`) === name || hqOf(`${name}${HQ_SEP}x`) === name;
}

// ws 이름 → 본부명. ws 단위가 본부이므로 **구분자 없는 ws 이름은 그 자체가 본부**다
// (표준 제목의 첫 세그먼트가 ws 이름이 되므로 — hqOf). 구 "닉·본부명" 이름은 parseWsName 이
// 계속 읽는다. 탭 색·본부 배지가 ws 이름에서 본부를 얻는 통로다(B3 는 "…본부" 토큰에 의존했고,
// 표준 제목에는 그 토큰이 없어 색·배지가 통째로 사라졌다).
export function hqOfWsName(name: string): string | null {
  const n = (name ?? "").trim();
  if (!n) return null;
  if (isHqWsName(n)) return n;
  const legacy = parseWsName(n).hq;
  if (legacy) return legacy;
  // 공백이 있으면 사람이 손으로 붙인 잡탭이므로 본부로 승격하지 않는다(유령 본부 방지 —
  // "non title" 이 본부 배지를 달던 초안의 결함. 티켓1 정신을 이름 판정에도 적용).
  // ★R2 [8] 수리: 종전에는 "구분자·공백 없는 한 낱말"이면 무엇이든 본부로 승격해서 ws 이름
  //   "개발"(기능명)이 본부 헤더가 됐다. 닫힌 집합으로 바꿔 유령 본부를 끊는다.
  return !n.includes(HQ_SEP) && isHqBrand(n) ? n : null;
}

// ★R4 [2] — **배치(placement) 전용** 본부 해소자. 오너 원사양: "배치는 현재 화면 기준이 아니라
// 브랜드 일치 기준으로 패널을 찾고, 브랜드가 다르면 새 패널이 정본이다."
//
// 왜 `hqOf` 를 그대로 쓰지 않는가(두 판정의 목적이 다르다):
//  · `hqOf` = "이 제목이 **조립된 표준 제목**인가"까지 함께 재는 엄격 판정자다. 주소(`s208`)를
//    요구하는 게이트가 그 증거고, 임의 이름이 조직으로 승격되던 티켓1(유령 'page')·R2 [8]('개발')
//    회귀를 막는 잠금이다 — 느슨하게 만들면 안 된다(그 계약은 시험으로 박제돼 있다).
//  · 반면 **배치**는 "이 노드를 어느 패널에 둘까"만 정한다. 제목이 표준을 덜 갖췄어도 첫 조각이
//    **권위 브랜드 집합**에 있으면 소속은 명백하다 — 그 노드를 남의 본부 패널에 넣을 이유가 없다.
//
// 안전성: 승격 근거가 **닫힌 집합**(로컬 naming.json)이라 임의 토큰은 통과하지 못한다. 즉 유령 패널이
// 생기지 않는다는 보장은 주소 게이트가 아니라 이 집합이 제공한다.
//
// 이 판정은 **독립 2중 방어**다 — 어느 한쪽이 다시 깨져도 배치는 산다.
export function adoptHqOf(title: string): string | null {
  const strict = hqOf(title);
  if (strict) return strict;
  const first = (title ?? "").split(HQ_SEP)[0]?.trim() ?? "";
  // ★프로젝트 패널: 배치 판정도 조직 토큰을 인정해야 `paneRehomeTarget`(= 이 함수를 쓴다)이
  //   엄격 판정(hqOf)과 같은 기준이어야 입양과 재배치가 매 틱 서로를 되돌리지 않는다(R4 [2] 계약).
  return isHqBrand(first) || isHqToken(first) ? first : null;
}

// 본부를 모르는 surface(제목에 본부 세그먼트가 없음)를 어느 ws 에 붙일지 고른다.
//
// ★배치 원칙(오너 확정 2026-08-17): "기본값 = 무패널 · 전부 같은 화면 split. 새 패널은 본부가
// 다를 때만." 본부를 아는 노드는 그 본부 ws 로 가고(없으면 그때만 새 ws), 본부를 **모르는** 노드는
// 새 패널을 만들지 않는다 — 여기서 붙일 곳만 고른다.
//
// 순서의 근거(★R1 수리로 ①②를 뒤집었다): **활성 ws 가 최우선**이다. 초안은 평면 ws 를 먼저
// 골랐는데("남의 본부 패널을 오염시키지 않는다"), 그러면 흔한 `non title` 평면 탭 하나만 있어도
// 사용자가 활성 본부에서 소환한 워커가 **엉뚱한 화면**에 붙는다(검토 변이 실증). 사용자가 보고
// 있는 화면이 곧 소환 문맥이므로 그것이 가장 예측 가능한 기본값이다.
// ②활성이 후보에 없으면(다른 소켓) 평면 ws — 특정 본부 패널을 무작정 오염시키지 않는다.
// ③그마저 없으면 첫 ws. 어느 갈래도 **새 ws 를 만들지 않는다**("기본값 = 무패널").
//
// 덧붙여, 본부를 아는 노드는 애초에 이 함수에 오지 않는다(hqOf 가 본부 ws 를 직접 찾는다) —
// 관통 배선(run_skill → --title)이 제목에 본부를 실어 보내므로 이 경로는 예외 처리에 가깝다.
export function pickAdoptWs<T extends { name: string }>(
  candidates: readonly T[],
  active: T | undefined,
): T | undefined {
  if (active && candidates.includes(active)) return active;
  const flat = candidates.find((w) => hqOfWsName(w.name) === null);
  if (flat) return flat;
  return candidates[0];
}

// 본부 ws 안에서 사람 순서 — 본부장 먼저(조직도), 나머지는 기존 순서 보존.
export function sortHqMembers<T extends { title: string }>(members: readonly T[]): T[] {
  const lead = members.filter((m) => isHqLeadName(m.title));
  return lead.length ? [...lead, ...members.filter((m) => !isHqLeadName(m.title))] : [...members];
}

// 유지된다. 랭크 표를 늘리지 않는다 — 그 외 본부의 순서는 사용자의 드래그가 진실원이다.

export function sortHqWorkspaces<T extends { name: string }>(list: readonly T[]): T[] {
  const top = topBrand ? list.filter((w) => w.name === topBrand || w.name === hqDisplayName(topBrand)) : [];
  return top.length ? [...top, ...list.filter((w) => !top.includes(w))] : [...list];
}
