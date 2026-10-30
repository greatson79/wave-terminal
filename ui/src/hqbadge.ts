// 본부 색 배지 — 탭에 붙는 본부 식별색 결정과 미등록 진단(main.ts는 배선만 한다).
//
// [B3] 종전 탭 색은 `WS_COLORS[ws.id % 8]` 이었다. ws.id 는 생성 순서라, **같은 본부의 두 탭이
// 다른 색**이고 탭을 지웠다 만들면 **같은 본부의 색이 바뀐다**. 색이 아무것도 식별하지 못하는
// 상태다. 색의 단위를 id 가 아니라 **본부**로 옮긴다 — 같은 본부는 언제나 같은 색이다.
//
// 색의 출처는 두 단계다:
//   ① ~/.cys/naming.json 의 `colors` 맵(브랜드 명명규약 색상열) — 조직이 정한 색이 있으면 그것
//   ② 없으면 같은 맵의 _neutral 색 + 경고. 중립색도 없으면 색을 지정하지 않는다.
// 색 설정이 없을 때 ID·해시·수동색으로 조직 색을 만들어내지 않는다.

import { hqTokenOf } from "./hierarchy";

/** 본부명 → hex 색. naming.json 의 `colors` 절. */
export type HqColorMap = Readonly<Record<string, string>>;

/** #rgb / #rrggbb 만 허용 — CSS 에 그대로 꽂히는 값이라 형식을 좁게 잡는다(주입 차단). */
const HEX_RE = /^#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{6})$/;

export function isHexColor(v: unknown): v is string {
  return typeof v === "string" && HEX_RE.test(v.trim());
}

/**
 * naming.json 원문에서 색 맵을 뽑는다. 파싱 실패·형식 오류는 **조용히 빈 맵**으로 강등한다 —
 * 색 하나 때문에 사이드바가 죽으면 안 되고, 잘못된 항목만 빠지면 ②폴백이 자리를 메운다.
 * 유효하지 않은 항목 목록을 함께 돌려줘 호출측이 원하면 진단할 수 있게 한다(조용한 무시 방지).
 */
export function parseNamingColors(raw: string | null | undefined): {
  colors: HqColorMap;
  invalid: string[];
} {
  if (!raw) return { colors: {}, invalid: [] };
  let doc: unknown;
  try {
    doc = JSON.parse(raw);
  } catch {
    return { colors: {}, invalid: ["<naming.json 파싱 실패>"] };
  }
  const src = (doc as { colors?: unknown } | null)?.colors;
  if (!src || typeof src !== "object" || Array.isArray(src)) return { colors: {}, invalid: [] };
  const colors: Record<string, string> = {};
  const invalid: string[] = [];
  for (const [hq, v] of Object.entries(src as Record<string, unknown>)) {
    if (hq.startsWith("_") && hq !== "_neutral") continue; // _neutral만 색 설정, 나머지는 메타 키
    if (isHexColor(v)) colors[hq.trim()] = v.trim();
    else invalid.push(hq);
  }
  return { colors, invalid };
}

/**
 * 본부명 → 팔레트 인덱스. 순서 무관한 합이 아니라 자리값을 섞는 해시라야
 * 32bit 로 접어 음수·오버플로를 막는다(JS 비트연산은 32bit 부호형).
 */
export function hqHashIndex(hq: string, buckets: number): number {
  if (buckets <= 0) return 0;
  let h = 2166136261; // FNV-1a offset basis
  for (let i = 0; i < hq.length; i++) {
    h ^= hq.charCodeAt(i);
    h = Math.imul(h, 16777619);
  }
  return Math.abs(h % buckets);
}

// 렌더마다 같은 미등록 경고가 쌓이지 않도록 키별 한 번만 알린다.
const warnedColorKeys = new Set<string>();

/** 본부 색 해소 — 매핑 우선, 미등록은 naming.colors._neutral. 유효 색이 없으면 null. */
export function hqColor(
  hq: string | null | undefined,
  colors: HqColorMap,
  _palette: readonly string[],
): string | null {
  const key = (hq ?? "").trim();
  const mapped = key && key !== "_neutral" ? colors[key] : undefined;
  if (isHexColor(mapped)) return mapped.trim();
  const neutral = colors._neutral;
  if (!warnedColorKeys.has(key)) {
    warnedColorKeys.add(key);
    console.warn(`[hq-color] 미등록 색 키 ${JSON.stringify(key)} → ${isHexColor(neutral) ? "naming.colors._neutral" : "중립색 설정 없음; 색 미지정"}`);
  }
  return isHexColor(neutral) ? neutral.trim() : null;
}

/**
 * 칩에 넣을 짧은 표식 — 본부명의 **첫 글자**다.
 * 한글은 자소가 아니라 음절이 한 글자이고 이모지는 서로게이트 쌍이라, `[...s]` 로
 * 코드포인트 단위 분해해야 반쪽 문자가 나오지 않는다(`s[0]` 은 이모지를 깨뜨린다).
 */
export function hqInitial(hq: string | null | undefined): string {
  const key = (hq ?? "").trim();
  if (!key) return "";
  return [...key][0] ?? "";
}


/**
 * [B3-r3] 탭 하나의 **본부 표시 계산 전부**를 한 함수로 모은다 — DOM 무관.
 *
 * r2 는 hqTokenOf 를 고쳤지만 테스트가 그 함수를 **직접** 불러 검증했다. buildTab 이 무엇을
 * 쓰는지는 아무도 안 봤고, buildTab 을 구 배선(parseWsName)으로 되돌린 변이가 테스트 329건을
 * 전부 통과했다(실증된 false-green). 계산이 DOM 안에 있으면 그 배선을 잴 방법이 없다 —
 * 그래서 계산을 밖으로 꺼내고, buildTab 은 이걸 **호출만** 한다.
 *
 * 반환값은 그리기에 필요한 전부다: 본부명(없으면 null), 좌측 바·칩 색, 칩 글자.
 * 본부를 못 읽는 평면 ws 는 hq=null 이고 색은 같은 naming.colors의 중립색을 쓴다.
 */
export interface HqPresentation {
  /** 본부 토큰. 본부 규약이 아닌 이름이면 null(칩을 그리지 않는다). */
  hq: string | null;
  /** 좌측 바·칩 배경색. 본부 미등록은 중립색, 색 설정 자체가 없으면 빈 문자열. */
  color: string;
  /** 칩에 넣을 첫 글자. hq 가 null 이면 빈 문자열. */
  initial: string;
}

export function hqPresentation(
  wsName: string,
  colors: HqColorMap,
  palette: readonly string[],
  _fallbackColor: string,
): HqPresentation {
  const hq = hqTokenOf(wsName);
  const color = hqColor(hq, colors, palette) ?? "";
  return { hq, color, initial: hq ? hqInitial(hq) : "" };
}
