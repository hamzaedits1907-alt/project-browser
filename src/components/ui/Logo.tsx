import { useId } from "react";

/**
 * The Project Browser mark: a bevelled octagon (the shape language of the whole interface) carrying a "P" whose bowl
 * opens forward like a speed line. Coloured by the theme's accent.
 */
export function Logo({ size = 22, glow = false, className }: { size?: number; glow?: boolean; className?: string }) {
  const id = useId();
  return (
    <svg viewBox="0 0 32 32" width={size} height={size} className={className} aria-hidden="true" focusable="false">
      <defs>
        <linearGradient id={`${id}-fill`} x1="0" y1="0" x2="1" y2="1">
          <stop offset="0" stopColor="var(--accent-hi)" />
          <stop offset="0.55" stopColor="var(--accent)" />
          <stop offset="1" stopColor="var(--accent-2)" />
        </linearGradient>
        {glow && (
          <filter id={`${id}-glow`} x="-40%" y="-40%" width="180%" height="180%">
            <feGaussianBlur stdDeviation="2.4" />
          </filter>
        )}
      </defs>
      {glow && <path d="M10 2.5h12l7.5 7.5v12L22 29.5H10L2.5 22V10z" fill={`url(#${id}-fill)`} opacity="0.55" filter={`url(#${id}-glow)`} />}
      <path d="M10 2.5h12l7.5 7.5v12L22 29.5H10L2.5 22V10z" fill={`url(#${id}-fill)`} />
      <path d="M10 2.5h12l7.5 7.5v12L22 29.5H10L2.5 22V10z" fill="none" stroke="rgb(255 255 255 / 0.35)" strokeWidth="0.8" />
      <path d="M11.5 23.5v-15h7.2a4.6 4.6 0 010 9.2h-7.2" fill="none" stroke="var(--on-accent)" strokeWidth="2.7" strokeLinecap="square" strokeLinejoin="miter" />
      <path d="M19.5 21.5h3.5" stroke="var(--on-accent)" strokeWidth="2.2" strokeLinecap="square" opacity="0.75" />
    </svg>
  );
}
