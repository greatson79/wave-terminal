// 워크스페이스 탭의 **본부 표시 그리기** — DOM 구현과 분리된 순수 로직(주입식 페인터).
//
// [B3-r4] r3 는 변이 오라클을 **소스 문자열 needle** 로 걸었다. 그건 두 방향으로 약하다:
//   ⑴ 정확 substring 만 잡아서 공백·줄바꿈·별칭이 다른 **동치 구배선**은 그대로 통과한다.
//   ⑵ positive 단언 `toContain("hqPresentation(")` 은 주석이나 다른 자리의 호출로도 충족돼,
//      실제 hq/color 계산을 들어내도 초록이 유지된다.
// 즉 "무엇을 그렸는가"가 아니라 "소스에 어떤 글자가 있는가"를 재고 있었다.
//
// 그래서 그리기를 **주입 가능한 페인터**로 뒤집는다. 테스트는 기록용 페인터를 넣어
// 실제로 어떤 색·글자·툴팁이 적용됐는지 **결과로** 단언한다 — 계산을 어떻게 바꾸든
// 결과가 달라지면 잡힌다(동치 변이 포함). DOM 라이브러리 의존은 0이다.

import { hqPresentation, type HqColorMap } from "./hqbadge";
import { wcagContrast } from "./contrast";

/** 본부 칩에 실제로 실리는 값 전부. DOM 이 아니라 **값**이라 그대로 단언할 수 있다. */
export interface HqChip {
  text: string;
  title: string;
  ariaLabel: string;
  background: string;
  foreground: string;
}

/** 탭에 가하는 조작. main.ts 가 DOM 구현을, 테스트가 기록 구현을 넣는다. */
export interface TabPaint {
  setBorderColor(color: string): void;
  addHqChip(chip: HqChip): void;
}

/**
 * 탭 하나의 본부 표시를 그린다. 본부를 못 읽는 평면 ws 는 **칩을 만들지 않고** 폴백색만 칠한다.
 *
 * 계산은 hqPresentation(순수·테스트됨) 한 번으로 끝내고 {hq, color, initial} 을 전부 소비한다 —
 * 같은 값을 두 번 계산하면 두 계산이 갈릴 자리가 생긴다(r3 에서 chip.initial 이 따로 계산되던
 * 형태가 정확히 그 위험이었다).
 */
export function paintHqTab(
  wsName: string,
  colors: HqColorMap,
  palette: readonly string[],
  fallbackColor: string,
  paint: TabPaint,
): void {
  const { hq, color, initial } = hqPresentation(wsName, colors, palette, fallbackColor);
  paint.setBorderColor(color);
  if (!hq) return; // 본부가 아니면 칩 없음 — 색만 폴백으로 칠한다
  paint.addHqChip({
    text: initial,
    title: `본부: ${hq}`,
    ariaLabel: `본부 ${hq}`,
    background: color,
    foreground: wcagContrast(color, "#ffffff") >= 4.5 ? "#ffffff" : "#0d1117",
  });
}

/** 테스트·진단용 기록 페인터 — 적용된 조작을 그대로 남긴다. */
export function recordingPaint(): TabPaint & { borderColor: string | null; chips: HqChip[] } {
  const rec = {
    borderColor: null as string | null,
    chips: [] as HqChip[],
    setBorderColor(color: string) {
      rec.borderColor = color;
    },
    addHqChip(chip: HqChip) {
      rec.chips.push(chip);
    },
  };
  return rec;
}

// ── [B3-r5] 탭 엘리먼트 생성 자체를 실호출 가능한 함수로 ────────────────────────
//
// r4 까지는 페인터(값)만 쟀다. 같은 계열로 세 번 반려됐으므로 이제 **실제로 엘리먼트를 만드는
// 코드**를 시험 대상으로 삼는다. main.ts 의 buildTab 은 심볼 15개(workspaces·groups·panes…)에
// 묶여 있어 테스트에서 import 할 수 없다 — 그래서 **본부 표시를 만드는 부분을 여기로 꺼내고**
// buildTab 은 이 함수를 호출한 뒤 나머지(서브라인·드래그·닫기)를 이어 붙인다.
//
// 테스트는 최소 DOM 스텁을 주입해 **실제로 적용된 엘리먼트 속성**(borderLeftColor·칩 text·
// title·aria-label·background)을 단언한다. 값 계산을 바꾸든 DOM 적용을 빠뜨리든 전부 잡힌다.

/** 이 모듈이 쓰는 DOM 조작만 담은 최소 인터페이스(실 document 와 구조적으로 호환). */
export interface MiniEl {
  className: string;
  title: string;
  textContent: string;
  style: Record<string, string>;
  setAttribute(name: string, value: string): void;
  appendChild(child: any): void;
}
export interface MiniDoc {
  createElement(tag: string): MiniEl;
}

/** 탭 루트와 제목 행을 만들고 본부 표시(좌측 바 색 + 칩)를 **실제로 적용**해 돌려준다. */
export function createHqTabShell(
  wsName: string,
  colors: HqColorMap,
  palette: readonly string[],
  fallbackColor: string,
  doc: MiniDoc,
): { tab: MiniEl; titleRow: MiniEl } {
  const tab = doc.createElement("div");
  const titleRow = doc.createElement("div");
  titleRow.className = "ws-title-row";
  paintHqTab(wsName, colors, palette, fallbackColor, {
    setBorderColor: (c) => {
      tab.style.borderLeftColor = c;
    },
    addHqChip: (c) => {
      const chip = doc.createElement("span");
      chip.className = "ws-hq-chip";
      chip.textContent = c.text;
      chip.style.background = c.background;
      chip.style.color = c.foreground;
      chip.title = c.title;
      chip.setAttribute("aria-label", c.ariaLabel);
      titleRow.appendChild(chip);
    },
  });
  return { tab, titleRow };
}
