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
  stale: boolean;
  age: number | null;
  severity: "ok" | "warn" | "crit" | "none";
}

const AGY_WINDOWS = ["Gemini 5h", "Gemini 7d", "Claude·GPT 5h", "Claude·GPT 7d", "Argon 7d"];
const AGY_LABELS: Record<string, string> = {
  "5h": "Gemini 5h", "7d": "Gemini 7d",
  "Gemini Models 5h": "Gemini 5h", "Gemini Models 7d": "Gemini 7d",
  "Claude and GPT models 5h": "Claude·GPT 5h", "Claude and GPT models 7d": "Claude·GPT 7d",
  "Claude·GPT 5h": "Claude·GPT 5h", "Claude·GPT 7d": "Claude·GPT 7d",
  "Gemini 4 Argon 7d": "Argon 7d", "Argon 7d": "Argon 7d",
};

export function sidebarUsage(accounts: readonly UsageAccount[], hidden: readonly string[] = [], now = Date.now() / 1000,
  owners: Readonly<Record<string, string>> = {}, accountOrder: readonly string[] = []): SidebarUsageRow[] {
  const rows: SidebarUsageRow[] = [];
  const measuredAt = new Map<UsageAccount["rate"][number], number | null>();
  const displayAccounts = accounts.filter((a) => a.provider !== "antigravity");
  const agy = accounts.filter((a) => a.provider === "antigravity")
    .sort((a, b) => (b.updated_at ?? 0) - (a.updated_at ?? 0));
  const measured = agy.find((a) => a.rate?.length);
  if (measured && !agy.some((a) => hidden.includes(`antigravity:${a.account_id}`))) {
    const windows = new Map<string, UsageAccount["rate"][number]>();
    for (const account of agy) for (const rate of account.rate ?? []) {
      const label = Object.hasOwn(AGY_LABELS, rate.label) ? AGY_LABELS[rate.label] : undefined;
      if (label && !windows.has(label)) {
        const normalized = { ...rate, label };
        windows.set(label, normalized);
        measuredAt.set(normalized, account.updated_at);
      }
    }
    displayAccounts.splice(Math.min(accounts.findIndex((a) => a.provider === "antigravity"), displayAccounts.length), 0,
      { ...measured, rate: AGY_WINDOWS.filter((label) => windows.has(label))
        .map((label) => windows.get(label)!) });
  }
  const position = (account: UsageAccount) => {
    const key = `${account.provider}:${account.account_id}`;
    const saved = accountOrder.indexOf(key);
    return saved >= 0 ? saved : accountOrder.length + accounts.findIndex((a) => `${a.provider}:${a.account_id}` === key);
  };
  displayAccounts.sort((a, b) => position(a) - position(b));
  for (const account of displayAccounts) {
    const key = `${account.provider}:${account.account_id}`;
    if (hidden.includes(key)) continue;
    const owner = owners[key]?.trim();
    const fableHome = account.provider === "fable" && owner
      ? displayAccounts.find((a) => a.provider === "claude" && owners[`claude:${a.account_id}`]?.trim() === owner)
      : undefined;
    const displayKey = fableHome ? `claude:${fableHome.account_id}` : key;
    const rates = account.rate ?? [];
    for (const rate of rates) {
      const window = rate.label;
      if (!window) continue;
      const remaining = rate && Number.isFinite(rate.used_pct)
        ? Math.max(0, Math.min(100, Math.round(100 - rate.used_pct))) : null;
      if (remaining === null) continue;
      const updatedAt = measuredAt.has(rate) ? measuredAt.get(rate) : account.updated_at;
      const age = typeof updatedAt === "number" && Number.isFinite(updatedAt) && updatedAt > 0
        ? Math.max(0, Math.floor((now - updatedAt) / 60)) : null;
      const reset = rate.resets_at != null && rate.resets_at <= now;
      const duration = window.match(/\b(\d+)(m|h|d)\b/);
      const windowSecs = duration ? Number(duration[1]) * ({ m: 60, h: 3600, d: 86400 }[duration[2]] ?? 0) : null;
      const stale = reset || age === null || age >= 10 || (windowSecs !== null && now - updatedAt! > windowSecs);
      rows.push({ key: `${key}:${window}`, accountKey: displayKey, provider: account.provider,
        label: owners[key]?.trim() || (account.provider === "antigravity" ? agy.map((a) => owners[`antigravity:${a.account_id}`]?.trim()).find(Boolean) : "") || account.label || account.provider,
        group: account.provider === "antigravity" ? window.replace(/ (5h|7d)$/, "") : fableHome ? "Fable" : null,
        window: account.provider === "antigravity" ? window.replace(/^.* /, "") : window,
        remaining: stale ? null : remaining,
        resetsAt: rate.resets_at ?? null, reset, stale, age,
        severity: stale ? "none" : remaining > 50 ? "ok" : remaining >= 20 ? "warn" : "crit" });
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
