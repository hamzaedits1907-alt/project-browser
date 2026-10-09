import type { SVGProps } from "react";

// Stroke icons on a 24px grid. Paths are authored for this app; no icon font or dependency.
const PATHS = {
  back: "M15 5l-7 7 7 7",
  forward: "M9 5l7 7-7 7",
  reload: "M20 11a8 8 0 10-2.3 5.7M20 4v7h-7",
  stop: "M7.5 7.5h9v9h-9z",
  cancel: "M6 6l12 12M18 6L6 18",
  close: "M6 6l12 12M18 6L6 18",
  plus: "M12 5v14M5 12h14",
  minus: "M5 12h14",
  check: "M5 12.5l4.5 4.5L19 7.5",
  menu: "M4 7h16M4 12h16M4 17h16",
  dots: "M5 12h.01M12 12h.01M19 12h.01",
  chevronDown: "M6 9l6 6 6-6",
  chevronUp: "M6 15l6-6 6 6",
  chevronRight: "M9 6l6 6-6 6",
  chevronLeft: "M15 6l-6 6 6 6",
  search: "M11 18a7 7 0 100-14 7 7 0 000 14zM20 20l-4-4",
  star: "M12 3.5l2.6 5.4 5.9.8-4.3 4.1 1 5.9L12 16.9 6.8 19.7l1-5.9L3.5 9.7l5.9-.8z",
  home: "M4 11l8-7 8 7M6 10v9h12v-9",
  settings:
    "M12 15a3 3 0 100-6 3 3 0 000 6zM19.4 13a7.6 7.6 0 000-2l2-1.5-2-3.4-2.3.9a7.5 7.5 0 00-1.7-1L15 3.5h-4l-.4 2.5a7.5 7.5 0 00-1.7 1l-2.3-.9-2 3.4 2 1.5a7.6 7.6 0 000 2l-2 1.5 2 3.4 2.3-.9a7.5 7.5 0 001.7 1l.4 2.5h4l.4-2.5a7.5 7.5 0 001.7-1l2.3.9 2-3.4z",
  history: "M4 12a8 8 0 108-8 8.2 8.2 0 00-6 2.7M4 4v4h4M12 8v4l3 2",
  download: "M12 4v11M7 11l5 5 5-5M5 20h14",
  upload: "M12 16V5M7 9l5-5 5 5M5 20h14",
  bookmarks: "M7 4h10a1 1 0 011 1v15l-6-4-6 4V5a1 1 0 011-1z",
  gauge: "M4.5 17a8.5 8.5 0 1115 0M12 13l4-4M8 17h8",
  gamepad:
    "M7 8h10a4 4 0 014 4v2.5a2.5 2.5 0 01-4.2 1.8L15 14.5H9l-1.8 1.8A2.5 2.5 0 013 14.5V12a4 4 0 014-4zM8 10.5v3M6.5 12h3M15.5 11.5h.01M17.5 13h.01",
  shield: "M12 3l7 3v5.5c0 4.4-3 7.7-7 9.5-4-1.8-7-5.1-7-9.5V6z",
  shieldCheck: "M12 3l7 3v5.5c0 4.4-3 7.7-7 9.5-4-1.8-7-5.1-7-9.5V6zM8.8 12l2.4 2.4 4-4.4",
  lock: "M6 11h12v9H6zM8.5 11V8a3.5 3.5 0 017 0v3",
  unlock: "M6 11h12v9H6zM8.5 11V8a3.5 3.5 0 016.7-1.4",
  warning: "M12 4l9 16H3zM12 10v4.5M12 17.5h.01",
  info: "M12 21a9 9 0 100-18 9 9 0 000 18zM12 11v5M12 8h.01",
  globe: "M12 21a9 9 0 100-18 9 9 0 000 18zM3 12h18M12 3c2.6 2.6 3.8 5.6 3.8 9s-1.2 6.4-3.8 9c-2.6-2.6-3.8-5.6-3.8-9S9.4 5.6 12 3z",
  volume: "M4 10v4h3.5L12 18V6L7.5 10zM15.5 9.5a4 4 0 010 5M18 7a7.5 7.5 0 010 10",
  mute: "M4 10v4h3.5L12 18V6L7.5 10zM16 9.5l5 5M21 9.5l-5 5",
  moon: "M20 14.5A8 8 0 019.5 4a8 8 0 1010.5 10.5z",
  pin: "M9 4h6l-1 6 3 3H7l3-3zM12 13v7",
  copy: "M9 9h10v11H9zM5 15V4h10",
  external: "M14 4h6v6M20 4l-9 9M18 14v5H5V6h5",
  trash: "M5 7h14M10 7V4h4v3M7 7l1 13h8l1-13M10 11v6M14 11v6",
  folder: "M3.5 6.5h6l2 2h9v10.5h-17z",
  folderOpen: "M3.5 19V6.5h6l2 2h8V11M3.5 19l2.5-8h15l-2.5 8z",
  file: "M7 3h7l4 4v14H7zM14 3v4h4",
  sparkle: "M12 3l1.8 5.2L19 10l-5.2 1.8L12 17l-1.8-5.2L5 10l5.2-1.8zM18.5 16l.8 2.2 2.2.8-2.2.8-.8 2.2-.8-2.2-2.2-.8 2.2-.8z",
  send: "M4 12l16-8-6 16-3-7z",
  cpu: "M8 8h8v8H8zM10 3v3M14 3v3M10 18v3M14 18v3M3 10h3M3 14h3M18 10h3M18 14h3",
  memory: "M3 8h18v8H3zM7 8v8M11 8v8M15 8v8M19 8v8M6 19v-3M10 19v-3M14 19v-3M18 19v-3",
  gpu: "M3 7h15v10H3zM18 9h3v6h-3zM7 11a2 2 0 104 0 2 2 0 00-4 0zM3 20h6",
  bolt: "M13 3L5 13h6l-1 8 8-10h-6z",
  user: "M12 12a4 4 0 100-8 4 4 0 000 8zM4.5 20a7.5 7.5 0 0115 0",
  incognito: "M3 11l2.2-5.5a2 2 0 011.9-1.2h9.8a2 2 0 011.9 1.2L21 11M2 11h20M8 18a2.5 2.5 0 100-5 2.5 2.5 0 000 5zM16 18a2.5 2.5 0 100-5 2.5 2.5 0 000 5zM10.5 15.3h3",
  zoomIn: "M11 18a7 7 0 100-14 7 7 0 000 14zM20 20l-4-4M11 8v6M8 11h6",
  zoomOut: "M11 18a7 7 0 100-14 7 7 0 000 14zM20 20l-4-4M8 11h6",
  fullscreen: "M4 9V4h5M20 9V4h-5M4 15v5h5M20 15v5h-5",
  tabs: "M4 9h12v10H4zM8 9V5h12v10h-4",
  list: "M9 6h11M9 12h11M9 18h11M4.5 6h.01M4.5 12h.01M4.5 18h.01",
  pause: "M8 5v14M16 5v14",
  play: "M7 4.5v15l12-7.5z",
  key: "M14.5 9.5a4 4 0 11-2.8 6.8L4 17v-3l2-1v-2l2-1 3.7-.7a4 4 0 012.8-2.8zM17 9h.01",
  edit: "M4 20h4L19 9l-4-4L4 16zM13.5 6.5l4 4",
  refresh: "M20 11a8 8 0 10-2.3 5.7M20 4v7h-7",
  zap: "M13 3L5 13h6l-1 8 8-10h-6z",
  layers: "M12 4l9 5-9 5-9-5zM3 14l9 5 9-5",
  eye: "M2.5 12S6 5.5 12 5.5 21.5 12 21.5 12 18 18.5 12 18.5 2.5 12 2.5 12zM12 14.5a2.5 2.5 0 100-5 2.5 2.5 0 000 5z",
  maximize: "M5 5h14v14H5z",
  restore: "M8 8V5h11v11h-3M5 8h11v11H5z",
  minimizeWin: "M5 12h14",
  rocket: "M5 19c0-2 1-4 3-5M9 15l-1 4 4-1M14 10a8 8 0 016-6 8 8 0 01-6 6zM9 15c-1-4 1-7 5-10 2 3 3 6 1 10z",
  chat: "M4 5h16v11H9l-5 4z",
  link: "M10 14a4 4 0 005.7 0l3-3a4 4 0 00-5.7-5.7l-1 1M14 10a4 4 0 00-5.7 0l-3 3a4 4 0 005.7 5.7l1-1",
  broom: "M14 3l7 7M13 8l-9 9 3 3 9-9M7 14l3 3",
  import: "M12 3v12M7 11l5 5 5-5M4 20h16",
  mic: "M12 15a3 3 0 003-3V7a3 3 0 00-6 0v5a3 3 0 003 3zM6 11a6 6 0 0012 0M12 17v4",
  camera: "M4 8h3l2-3h6l2 3h3v11H4zM12 17a3.5 3.5 0 100-7 3.5 3.5 0 000 7z",
  pinFilled: "M9 4h6l-1 6 3 3H7l3-3zM12 13v7",
} as const;

export type IconName = keyof typeof PATHS;

interface IconProps extends Omit<SVGProps<SVGSVGElement>, "name"> {
  name: IconName;
  size?: number;
  filled?: boolean;
}

export function Icon({ name, size = 18, filled = false, strokeWidth = 1.75, ...rest }: IconProps) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill={filled ? "currentColor" : "none"}
      stroke="currentColor"
      strokeWidth={strokeWidth}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
      focusable="false"
      {...rest}
    >
      <path d={PATHS[name]} />
    </svg>
  );
}
