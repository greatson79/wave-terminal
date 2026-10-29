export interface UsageAccount {
  provider: string;
  account_id: string;
  label: string;
  updated_at: number | null;
  rate: { label: string; used_pct: number; resets_at: number | null }[];
}

export interface SidebarUsageRow {
  key: string;
  label: string;
  window: string;
  remaining: number | null;
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
    for (const rate of account.rate ?? []) {
      if (!rate.label || !Number.isFinite(rate.used_pct)) continue;
      const remaining = Math.max(0, Math.min(100, Math.round(100 - rate.used_pct)));
      const age = typeof account.updated_at === "number" ? Math.max(0, Math.floor((now - account.updated_at) / 60)) : null;
      const reset = rate.resets_at != null && rate.resets_at <= now;
      rows.push({ key: `${key}:${rate.label}`, label: owners[key]?.trim() || account.label || account.provider,
        window: rate.label, remaining: reset ? null : remaining, reset, age,
        severity: reset ? "none" : remaining > 50 ? "ok" : remaining >= 20 ? "warn" : "crit" });
    }
  }
  return rows;
}
