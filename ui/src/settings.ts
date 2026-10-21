import { FONT_CHOICES } from "./appearance";

export const SETTINGS_KEY = "wave-settings-v1";
export const BAR_IDS = ["new", "split-h", "split-v", "equalize", "close", "files", "cc", "update", "restart-daemon"];

export interface Settings {
  ver: 1;
  look: {
    bg: string | null; font: string | null; titleSize: number; titleWeight: number;
    titleIcon: boolean; bodyWeight: number; menuWeight: number; menuScale: number;
  };
  bar: { order: string[]; hidden: string[] };
}

export const DEFAULT_SETTINGS: Settings = {
  ver: 1,
  look: { bg: null, font: null, titleSize: 11, titleWeight: 400,
    titleIcon: true, bodyWeight: 400, menuWeight: 400, menuScale: 100 },
  bar: { order: [...BAR_IDS], hidden: [] },
};

const validColor = (v: unknown): v is string => typeof v === "string" && /^#[0-9a-fA-F]{6}$/.test(v);
const validFont = (v: unknown): v is string => typeof v === "string" && FONT_CHOICES.some((c) => c.face === v);
const numberIn = (v: unknown, lo: number, hi: number) => typeof v === "number" && Number.isInteger(v) && v >= lo && v <= hi;

export function readSettings(raw: string | null, legacyBg: string | null, legacyFont: string | null,
  ids: string[] = BAR_IDS): Settings {
  let parsed: any = null;
  try { parsed = raw ? JSON.parse(raw) : null; } catch { /* damaged settings use defaults */ }
  const look = parsed?.ver === 1 ? parsed.look : null;
  const bar = parsed?.ver === 1 ? parsed.bar : null;
  const bg = validColor(legacyBg) ? legacyBg : legacyBg === null ? null : validColor(look?.bg) ? look.bg : null;
  const font = validFont(legacyFont) ? legacyFont : legacyFont === null ? null : validFont(look?.font) ? look.font : null;
  const savedOrder = Array.isArray(bar?.order) ? bar.order : [];
  const savedHidden = Array.isArray(bar?.hidden) ? bar.hidden : [];
  const order = [...new Set([...savedOrder.filter((id: unknown) => ids.includes(id as string)), ...ids])];
  return {
    ver: 1,
    look: {
      bg, font,
      titleSize: numberIn(look?.titleSize, 9, 24) ? look.titleSize : DEFAULT_SETTINGS.look.titleSize,
      titleWeight: [400, 500, 600, 700].includes(look?.titleWeight) ? look.titleWeight : DEFAULT_SETTINGS.look.titleWeight,
      titleIcon: typeof look?.titleIcon === "boolean" ? look.titleIcon : DEFAULT_SETTINGS.look.titleIcon,
      bodyWeight: [400, 500, 600, 700].includes(look?.bodyWeight) ? look.bodyWeight : DEFAULT_SETTINGS.look.bodyWeight,
      menuWeight: [400, 500, 600, 700].includes(look?.menuWeight) ? look.menuWeight : DEFAULT_SETTINGS.look.menuWeight,
      menuScale: numberIn(look?.menuScale, 70, 150) ? look.menuScale : DEFAULT_SETTINGS.look.menuScale,
    },
    bar: { order, hidden: [...new Set(savedHidden.filter((id: unknown) => ids.includes(id as string))) ] },
  };
}
