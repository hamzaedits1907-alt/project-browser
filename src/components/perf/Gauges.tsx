import type { ReactNode } from "react";

import { Sparkline } from "../ui/Sparkline";

export function MetricCard({ title, value, sub, values, max, icon }: { title: string; value: string; sub?: ReactNode; values?: (number | null)[]; max?: number; icon?: ReactNode }) {
  return (
    <div className="metric-card">
      <div className="metric-head">
        <span>{icon}{title}</span>
      </div>
      <div className="metric-value">{value}</div>
      {sub && <div className="metric-sub">{sub}</div>}
      {values && values.length > 1 && <Sparkline values={values} max={max} label={`${title} over time`} />}
    </div>
  );
}

/** Circular gauge, 0-100. Pure SVG; one cheap attribute changes per sample. */
export function RingGauge({ percent, label, value }: { percent: number | null; label: string; value: string }) {
  const r = 38;
  const c = 2 * Math.PI * r;
  const p = percent == null ? 0 : Math.max(0, Math.min(100, percent));
  return (
    <div className="ring" role="img" aria-label={`${label}: ${value}`}>
      <svg viewBox="0 0 100 100" width="112" height="112">
        <circle cx="50" cy="50" r={r} className="ring-track" />
        <circle cx="50" cy="50" r={r} className="ring-fill" strokeDasharray={`${(p / 100) * c} ${c}`} transform="rotate(-90 50 50)" />
      </svg>
      <div className="ring-text">
        <strong>{value}</strong>
        <span>{label}</span>
      </div>
    </div>
  );
}
