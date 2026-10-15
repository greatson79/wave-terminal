export interface UsageSample {
  agent: string;
  updated_at: number;
  rate_updated_at?: number | null;
  rate: { label: string; used_pct: number; resets_at: number | null }[];
}

export function sidebarUsage(samples: UsageSample[], now = Date.now() / 1000) {
  const groups = new Map<string, Map<string, { remaining: number; updated: number; reset: boolean }>>();
  for (const sample of samples) {
    const agent = sample.agent.toLowerCase();
    if (agent !== "claude" && agent !== "codex") continue;
    if (!groups.has(agent)) groups.set(agent, new Map());
    for (const rate of sample.rate ?? []) {
      if (rate.label !== "5h" && rate.label !== "7d" || !Number.isFinite(rate.used_pct)) continue;
      const rows = groups.get(agent)!;
      const remaining = Math.max(0, Math.min(100, Math.round(100 - rate.used_pct)));
      const old = rows.get(rate.label);
      const reset = rate.resets_at != null && rate.resets_at <= now;
      if (!old || old.reset && !reset || old.reset === reset && remaining < old.remaining) rows.set(rate.label, {
        remaining, updated: sample.rate_updated_at ?? sample.updated_at,
        reset,
      });
    }
  }
  return [...groups].map(([agent, rows]) => ({ agent, rows: ["5h", "7d"].map((label) => {
    const row = rows.get(label);
    return { label, remaining: row?.remaining ?? null, reset: row?.reset ?? false,
      age: row ? Math.max(0, Math.floor((now - row.updated) / 60)) : null,
      severity: row ? row.remaining > 50 ? "ok" : row.remaining >= 20 ? "warn" : "crit" : "none" };
  }) }));
}
