import { useState } from "react";
import type { CSSProperties } from "react";

import { isSafeImageSrc } from "../../lib/urls";

/** A stable hue for a site's lettered tile, so the same site always gets the same colour. */
export function hueFor(seed: string): number {
  let h = 0;
  for (let i = 0; i < seed.length; i++) h = (h * 31 + seed.charCodeAt(i)) >>> 0;
  return h % 360;
}

interface FaviconProps {
  src: string | null | undefined;
  /** Single letter shown when there is no icon (or it fails to load). */
  fallback: string;
  /** What decides the tile colour (the site's host); defaults to the letter. */
  seed?: string;
  size?: number;
}

/** Site icon with a lettered fallback; no third-party favicon service is ever contacted. */
export function Favicon({ src, fallback, seed, size = 16 }: FaviconProps) {
  const [failed, setFailed] = useState<string | null>(null);
  if (!isSafeImageSrc(src) || failed === src) {
    const style = { width: size, height: size, fontSize: Math.round(size * 0.6), "--fh": hueFor(seed ?? fallback) } as CSSProperties;
    return (
      <span className="favicon favicon-letter" style={style} aria-hidden="true">
        {fallback}
      </span>
    );
  }
  return <img className="favicon" src={src} alt="" width={size} height={size} draggable={false} onError={() => setFailed(src)} />;
}
