export interface UsageAccount {
  provider: string;
  account_id: string;
  label: string;
  updated_at: number | null;
  rate: { label: string; used_pct: number; resets_at: number | null }[];
}

export interface SidebarUsageRow {
  key: string;
  accountKey: string;
  provider: string;
  label: string;
  group: string | null;
  window: string;
  remaining: number | null;
  resetsAt: number | null;
  reset: boolean;
  age: number | null;
  severity: "ok" | "warn" | "crit" | "none";
}

export function sidebarUsage(accounts: readonly UsageAccount[], hidden: readonly string[] = [], now = Date.now() / 1000,
  owners: Readonly<Record<string, string>> = {}): SidebarUsageRow[] {
  const rows: SidebarUsageRow[] = [];
  for (const account of accounts) {
    const key = `${account.provider}:${account.account_id}`;
    if (hidden.includes(key)) continue;
    const rates = account.rate ?? [];
    const windows = account.provider === "antigravity"
      ? ["5h", "7d", "Claude·GPT 5h", "Claude·GPT 7d", ...rates.map((r) => r.label)
        .filter((label) => !["5h", "7d", "Claude·GPT 5h", "Claude·GPT 7d"].includes(label))]
      : rates.map((r) => r.label);
    for (const window of windows) {
      const rate = rates.find((r) => r.label === window);
      if (!window) continue;
      const remaining = rate && Number.isFinite(rate.used_pct)
        ? Math.max(0, Math.min(100, Math.round(100 - rate.used_pct))) : null;
      const age = typeof account.updated_at === "number" && Number.isFinite(account.updated_at) && account.updated_at > 0
        ? Math.max(0, Math.floor((now - account.updated_at) / 60)) : null;
      const reset = rate?.resets_at != null && rate.resets_at <= now;
      rows.push({ key: `${key}:${window}`, accountKey: key, provider: account.provider,
        label: owners[key]?.trim() || account.label || account.provider,
        group: account.provider === "antigravity" ? window.startsWith("Claude·GPT ") ? "Claude·GPT" : "Gemini" : null,
        window: window.replace(/^Claude·GPT /, ""), remaining: reset ? null : remaining,
        resetsAt: rate?.resets_at ?? null, reset, age,
        severity: reset || remaining === null ? "none" : remaining > 50 ? "ok" : remaining >= 20 ? "warn" : "crit" });
    }
  }
  return rows;
}

export function resetTime(resetsAt: number | null): string {
  if (resetsAt == null || !Number.isFinite(resetsAt) || resetsAt <= 0) return "리셋 미상";
  const date = new Date(resetsAt * 1000);
  if (Number.isNaN(date.getTime())) return "리셋 미상";
  return `리셋 ${date.getMonth() + 1}/${date.getDate()} ${String(date.getHours()).padStart(2, "0")}:${String(date.getMinutes()).padStart(2, "0")}`;
}
