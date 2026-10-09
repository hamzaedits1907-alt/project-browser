import { useState } from "react";

import { isSafeImageSrc } from "../../lib/urls";

interface FaviconProps {
  src: string | null | undefined;
  /** Single letter shown when there is no icon (or it fails to load). */
  fallback: string;
  size?: number;
}

/** Site icon with a lettered fallback; no third-party favicon service is ever contacted. */
export function Favicon({ src, fallback, size = 16 }: FaviconProps) {
  const [failed, setFailed] = useState<string | null>(null);
  const style = { width: size, height: size, fontSize: Math.round(size * 0.62) };
  if (!isSafeImageSrc(src) || failed === src) {
    return (
      <span className="favicon favicon-letter" style={style} aria-hidden="true">
        {fallback}
      </span>
    );
  }
  return <img className="favicon" src={src} alt="" width={size} height={size} draggable={false} onError={() => setFailed(src)} />;
}
