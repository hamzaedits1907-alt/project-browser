import { useId } from "react";

interface SparklineProps {
  values: (number | null)[];
  /** Fixed ceiling (e.g. 100 for percentages). Defaults to the largest value. */
  max?: number;
  height?: number;
  label: string;
}

const W = 200;

/** Tiny area chart. Plain SVG, no animation: cheap enough to redraw every sample. */
export function Sparkline({ values, max, height = 48, label }: SparklineProps) {
  const gid = useId();
  const nums = values.map((v) => (v == null ? 0 : v));
  // Percentages use a fixed 0..max scale; other series are zoomed to their own range so changes stay visible.
  const hi = max ?? Math.max(...nums, 1e-9) * 1.04;
  const lo = max !== undefined ? 0 : Math.min(...nums) * 0.92;
  const span = Math.max(hi - lo, 1e-9);
  const step = nums.length > 1 ? W / (nums.length - 1) : W;
  const pts = nums.map((v, i) => [i * step, height - ((v - lo) / span) * (height - 4) - 2] as const);
  const line = pts.map(([x, y], i) => `${i === 0 ? "M" : "L"}${x.toFixed(1)},${y.toFixed(1)}`).join(" ");
  const area = pts.length > 1 ? `${line} L${W},${height} L0,${height} Z` : "";
  return (
    <svg className="sparkline" viewBox={`0 0 ${W} ${height}`} preserveAspectRatio="none" role="img" aria-label={label} style={{ height }}>
      <defs>
        <linearGradient id={gid} x1="0" y1="0" x2="0" y2="1">
          <stop offset="0%" stopColor="var(--accent)" stopOpacity="0.35" />
          <stop offset="100%" stopColor="var(--accent)" stopOpacity="0" />
        </linearGradient>
      </defs>
      {area && <path d={area} fill={`url(#${gid})`} />}
      {line && <path d={line} fill="none" stroke="var(--accent)" strokeWidth="1.6" vectorEffect="non-scaling-stroke" strokeLinejoin="round" />}
    </svg>
  );
}
