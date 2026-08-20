import type { UsageSnapshot } from '../lib/types';

export function UsagePanel({ usage }: { usage: UsageSnapshot }) {
  const weekly = usage.weeklyUsagePercent;
  const weeklyLabel = weekly === null ? '—' : `${Math.round(weekly)}%`;
  const resetLabel = usage.weeklyResetAtMs === null ? '—' : new Intl.DateTimeFormat('zh-CN', { month: 'numeric', day: 'numeric' }).format(usage.weeklyResetAtMs);
  const syncedLabel = usage.syncedAtMs === null ? '—' : new Intl.DateTimeFormat('zh-CN', { hour: '2-digit', minute: '2-digit', second: '2-digit' }).format(usage.syncedAtMs);
  return (
    <section className="usage-panel" aria-label="Codex 使用量">
      <div className="usage-row"><span>本周使用</span><strong>{weeklyLabel}</strong></div>
      <div className="usage-track" aria-label={`本周 Codex 使用比例 ${weeklyLabel}`}>
        <span style={{ width: `${weekly ?? 0}%` }} />
      </div>
      <div className="usage-row usage-row--tokens"><span>最近同步</span><strong>{syncedLabel}</strong></div>
      <div className="usage-row usage-row--reset"><span>下次重置</span><strong>{resetLabel}</strong></div>
    </section>
  );
}
