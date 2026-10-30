// WCAG2 대비비(contrast ratio) 계산 — 순수 함수.
//
// theme.ts의 relativeLuminance는 사용자 커스텀 배경의 "밝은 글자/어두운 글자 전환" 휴리스틱용
// 선형 평균이라 WCAG AA 판정에는 못 쓴다(감마 보정이 없다 — 실제보다 밝게 나온다). 이 파일은
// sRGB→linear 감마 보정을 포함한 정식 WCAG2 공식이다(design-token-system 스킬·README 검증
// 스크립트와 동일 공식). style.css의 accent/error 배경 위 흰 글자 조합이 4.5:1 미달이던
// 결함(노아 R2 REVISE·2026-08-19)이 회귀 테스트 없이 통과했던 것이 이 파일이 메우는 갭이다.

import { hexToRgb } from "./theme";

function colorRgb(hex: string): [number, number, number] | null {
  const short = /^#([0-9a-fA-F]{3})$/.exec(hex.trim());
  return hexToRgb(short ? `#${[...short[1]].map((c) => c + c).join("")}` : hex);
}

function srgbToLinear(channel255: number): number {
  const c = channel255 / 255;
  return c <= 0.03928 ? c / 12.92 : Math.pow((c + 0.055) / 1.055, 2.4);
}

/** WCAG2 정식 상대휘도(0~1, 감마 보정 포함). 파싱 실패는 0(검정 취급). */
export function wcagLuminance(hex: string): number {
  const rgb = colorRgb(hex);
  if (!rgb) return 0;
  const [r, g, b] = rgb;
  return 0.2126 * srgbToLinear(r) + 0.7152 * srgbToLinear(g) + 0.0722 * srgbToLinear(b);
}

/** 두 색의 WCAG2 대비비(1~21). 순서 무관 — 밝은 쪽/어두운 쪽을 내부에서 정렬한다. */
export function wcagContrast(hexA: string, hexB: string): number {
  const la = wcagLuminance(hexA);
  const lb = wcagLuminance(hexB);
  const lighter = Math.max(la, lb);
  const darker = Math.min(la, lb);
  return (lighter + 0.05) / (darker + 0.05);
}

/**
 * CSS `color-mix(in srgb, hexA pctA%, hexB)` 와 동일한 채널별 선형보간(sRGB 비선형 공간에서
 * 직접 섞는다 — "in srgb"의 정의 그대로, linear 변환 없음). pctA는 0~100.
 */
export function mixSrgb(hexA: string, pctA: number, hexB: string): string {
  const a = colorRgb(hexA);
  const b = colorRgb(hexB);
  if (!a || !b) return hexA;
  const t = pctA / 100;
  const mixed = a.map((ca, i) => Math.round(ca * t + b[i] * (1 - t)));
  return "#" + mixed.map((c) => c.toString(16).padStart(2, "0")).join("");
}
