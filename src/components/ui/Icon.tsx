import type { SVGProps } from "react";

// Stroke icons on a 24px grid, drawn for this app; no icon font or dependency.
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
  dotsV: "M12 5h.01M12 12h.01M12 19h.01",
  chevronDown: "M6 9l6 6 6-6",
  chevronUp: "M6 15l6-6 6 6",
  chevronRight: "M9 6l6 6-6 6",
  chevronLeft: "M15 6l-6 6 6 6",
  arrowUp: "M12 19V5M6 11l6-6 6 6",
  arrowDown: "M12 5v14M18 13l-6 6-6-6",
  search: "M11 18a7 7 0 100-14 7 7 0 000 14zM20 20l-4-4",
  star: "M12 3.5l2.6 5.4 5.9.8-4.3 4.1 1 5.9L12 16.9 6.8 19.7l1-5.9L3.5 9.7l5.9-.8z",
  home: "M4 11l8-7 8 7M6 10v9h12v-9",
  settings:
    "M12 15a3 3 0 100-6 3 3 0 000 6zM19.4 13a7.6 7.6 0 000-2l2-1.5-2-3.4-2.3.9a7.5 7.5 0 00-1.7-1L15 3.5h-4l-.4 2.5a7.5 7.5 0 00-1.7 1l-2.3-.9-2 3.4 2 1.5a7.6 7.6 0 000 2l-2 1.5 2 3.4 2.3-.9a7.5 7.5 0 001.7 1l.4 2.5h4l.4-2.5a7.5 7.5 0 001.7-1l2.3.9 2-3.4z",
  sliders: "M4 7h9M17 7h3M4 17h3M11 17h9M15 5v4M9 15v4",
  history: "M4 12a8 8 0 108-8 8.2 8.2 0 00-6 2.7M4 4v4h4M12 8v4l3 2",
  download: "M12 4v11M7 11l5 5 5-5M5 20h14",
  upload: "M12 16V5M7 9l5-5 5 5M5 20h14",
  bookmarks: "M7 4h10a1 1 0 011 1v15l-6-4-6 4V5a1 1 0 011-1z",
  gauge: "M4.5 17a8.5 8.5 0 1115 0M12 13l4-4M8 17h8",
  gamepad:
    "M7 8h10a4 4 0 014 4v2.5a2.5 2.5 0 01-4.2 1.8L15 14.5H9l-1.8 1.8A2.5 2.5 0 013 14.5V12a4 4 0 014-4zM8 10.5v3M6.5 12h3M15.5 11.5h.01M17.5 13h.01",
  shield: "M12 3l7 3v5.5c0 4.4-3 7.7-7 9.5-4-1.8-7-5.1-7-9.5V6z",
  shieldCheck: "M12 3l7 3v5.5c0 4.4-3 7.7-7 9.5-4-1.8-7-5.1-7-9.5V6zM8.8 12l2.4 2.4 4-4.4",
  shieldOff: "M12 3l7 3v5.5c0 1.6-.4 3.1-1.1 4.4M6 6.5v5c0 4.4 3 7.7 6 9.5 1.8-.8 3.3-1.9 4.5-3.3M3 3l18 18",
  lock: "M6 11h12v9H6zM8.5 11V8a3.5 3.5 0 017 0v3",
  unlock: "M6 11h12v9H6zM8.5 11V8a3.5 3.5 0 016.7-1.4",
  warning: "M12 4l9 16H3zM12 10v4.5M12 17.5h.01",
  info: "M12 21a9 9 0 100-18 9 9 0 000 18zM12 11v5M12 8h.01",
  globe: "M12 21a9 9 0 100-18 9 9 0 000 18zM3 12h18M12 3c2.6 2.6 3.8 5.6 3.8 9s-1.2 6.4-3.8 9c-2.6-2.6-3.8-5.6-3.8-9S9.4 5.6 12 3z",
  tunnel: "M3 20v-8a9 9 0 0118 0v8M7.5 20v-7.5a4.5 4.5 0 019 0V20M2 20h20",
  volume: "M4 10v4h3.5L12 18V6L7.5 10zM15.5 9.5a4 4 0 010 5M18 7a7.5 7.5 0 010 10",
  mute: "M4 10v4h3.5L12 18V6L7.5 10zM16 9.5l5 5M21 9.5l-5 5",
  sound: "M4 10v4h3.5L12 18V6L7.5 10zM16 9.5a4 4 0 010 5",
  moon: "M20 14.5A8 8 0 019.5 4a8 8 0 1010.5 10.5z",
  pin: "M9 4h6l-1 6 3 3H7l3-3zM12 13v7",
  pinFilled: "M9 4h6l-1 6 3 3H7l3-3zM12 13v7",
  copy: "M9 9h10v11H9zM5 15V4h10",
  cut: "M6.5 19.5a2.5 2.5 0 100-5 2.5 2.5 0 000 5zM17.5 19.5a2.5 2.5 0 100-5 2.5 2.5 0 000 5zM8.6 15.2L18 4M15.4 15.2L6 4",
  paste: "M8 5H6v15h12V5h-2M9 3h6v4H9z",
  undo: "M9 14L4 9l5-5M4 9h10.5a5.5 5.5 0 010 11H11",
  redo: "M15 14l5-5-5-5M20 9H9.5a5.5 5.5 0 000 11H13",
  selectAll: "M4 8V4h4M16 4h4v4M20 16v4h-4M8 20H4v-4M8.5 9h7v6h-7z",
  external: "M14 4h6v6M20 4l-9 9M18 14v5H5V6h5",
  trash: "M5 7h14M10 7V4h4v3M7 7l1 13h8l1-13M10 11v6M14 11v6",
  folder: "M3.5 6.5h6l2 2h9v10.5h-17z",
  folderOpen: "M3.5 19V6.5h6l2 2h8V11M3.5 19l2.5-8h15l-2.5 8z",
  file: "M7 3h7l4 4v14H7zM14 3v4h4",
  image: "M4 5h16v14H4zM4 16l5-5 4 4 3-3 4 4M15.5 9h.01",
  lens: "M3 8V5.5A2.5 2.5 0 015.5 3H8M16 3h2.5A2.5 2.5 0 0121 5.5V8M21 16v2.5a2.5 2.5 0 01-2.5 2.5H16M8 21H5.5A2.5 2.5 0 013 18.5V16M12 15.5a3.5 3.5 0 100-7 3.5 3.5 0 000 7z",
  print: "M7 9V4h10v5M7 17H4v-7h16v7h-3M7 14h10v6H7z",
  save: "M5 4h11l3 3v13H5zM8 4v5h7V4M8 20v-6h8v6",
  pip: "M3 5h18v14H3zM12.5 11.5h6.5v5h-6.5z",
  loop: "M17 3l3 3-3 3M4 11V9.5A3.5 3.5 0 017.5 6H20M7 21l-3-3 3-3M20 13v1.5a3.5 3.5 0 01-3.5 3.5H4",
  translate: "M4 5h8M8 3v2M5.5 5c.8 3.6 3 6.4 6.5 8M10.5 5c-1 4.2-3.6 7.3-6.5 9M13 21l4-10 4 10M14.5 17.5h5",
  code: "M8 7l-5 5 5 5M16 7l5 5-5 5M13.5 4l-3 16",
  sparkle: "M12 3l1.8 5.2L19 10l-5.2 1.8L12 17l-1.8-5.2L5 10l5.2-1.8zM18.5 16l.8 2.2 2.2.8-2.2.8-.8 2.2-.8-2.2-2.2-.8 2.2-.8z",
  brain: "M9.5 4A3 3 0 006.5 7a3 3 0 00-2.2 5 3 3 0 002.2 5 3 3 0 006 1.2V5.6A2.5 2.5 0 009.5 4zM14.5 4a3 3 0 013 3 3 3 0 012.2 5 3 3 0 01-2.2 5 3 3 0 01-6 1.2",
  flask: "M9 3h6M10 3v6l-5.5 9.5A1.7 1.7 0 006 21h12a1.7 1.7 0 001.5-2.5L14 9V3M7 15h10",
  send: "M4 12l16-8-6 16-3-7z",
  cpu: "M8 8h8v8H8zM10 3v3M14 3v3M10 18v3M14 18v3M3 10h3M3 14h3M18 10h3M18 14h3",
  memory: "M3 8h18v8H3zM7 8v8M11 8v8M15 8v8M19 8v8M6 19v-3M10 19v-3M14 19v-3M18 19v-3",
  gpu: "M3 7h15v10H3zM18 9h3v6h-3zM7 11a2 2 0 104 0 2 2 0 00-4 0zM3 20h6",
  bolt: "M13 3L5 13h6l-1 8 8-10h-6z",
  power: "M12 3v8M6.3 6.6a8 8 0 1011.4 0",
  user: "M12 12a4 4 0 100-8 4 4 0 000 8zM4.5 20a7.5 7.5 0 0115 0",
  incognito: "M3 11l2.2-5.5a2 2 0 011.9-1.2h9.8a2 2 0 011.9 1.2L21 11M2 11h20M8 18a2.5 2.5 0 100-5 2.5 2.5 0 000 5zM16 18a2.5 2.5 0 100-5 2.5 2.5 0 000 5zM10.5 15.3h3",
  zoomIn: "M11 18a7 7 0 100-14 7 7 0 000 14zM20 20l-4-4M11 8v6M8 11h6",
  zoomOut: "M11 18a7 7 0 100-14 7 7 0 000 14zM20 20l-4-4M8 11h6",
  fullscreen: "M4 9V4h5M20 9V4h-5M4 15v5h5M20 15v5h-5",
  tabs: "M4 9h12v10H4zM8 9V5h12v10h-4",
  tabsVertical: "M4 4h6v16H4zM13.5 6.5h6.5M13.5 10.5h6.5M13.5 14.5h4",
  group: "M3 7.5h6.5l1.5-2h10v13.5H3zM3 11h18",
  list: "M9 6h11M9 12h11M9 18h11M4.5 6h.01M4.5 12h.01M4.5 18h.01",
  grip: "M9 6h.01M15 6h.01M9 12h.01M15 12h.01M9 18h.01M15 18h.01",
  widgets: "M4 4h7v7H4zM13 4h7v7h-7zM4 13h7v7H4zM13 13h7v7h-7z",
  sidebar: "M4 5h16v14H4zM9 5v14",
  panelRight: "M4 5h16v14H4zM15 5v14",
  window: "M3 5h18v14H3zM3 9h18",
  windowNew: "M15 19H3V5h18v6M3 9h18M18 14v6M15 17h6",
  pause: "M8 5v14M16 5v14",
  play: "M7 4.5v15l12-7.5z",
  key: "M14.5 9.5a4 4 0 11-2.8 6.8L4 17v-3l2-1v-2l2-1 3.7-.7a4 4 0 012.8-2.8zM17 9h.01",
  edit: "M4 20h4L19 9l-4-4L4 16zM13.5 6.5l4 4",
  refresh: "M20 11a8 8 0 10-2.3 5.7M20 4v7h-7",
  zap: "M13 3L5 13h6l-1 8 8-10h-6z",
  layers: "M12 4l9 5-9 5-9-5zM3 14l9 5 9-5",
  eye: "M2.5 12S6 5.5 12 5.5 21.5 12 21.5 12 18 18.5 12 18.5 2.5 12 2.5 12zM12 14.5a2.5 2.5 0 100-5 2.5 2.5 0 000 5z",
  eyeOff: "M3 3l18 18M10.6 6.1c.5-.1.9-.1 1.4-.1 6 0 9.5 6 9.5 6a17 17 0 01-2.8 3.5M6.6 6.6C3.9 8.4 2.5 12 2.5 12S6 18.5 12 18.5c1.6 0 3.1-.4 4.4-1.1M9.9 9.9a3 3 0 004.2 4.2",
  palette: "M12 3a9 9 0 100 18c1.2 0 1.6-.9 1.2-1.8-.5-1-.2-2.2 1.1-2.2H17a4 4 0 004-4c0-5.5-4-10-9-10zM7.5 11.5h.01M10 7.5h.01M14.5 7.5h.01",
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
  monitor: "M3 4h18v12H3zM8 20h8M12 16v4",
  tag: "M3 12V4h8l9 9-8 8zM7.5 7.5h.01",
  clock: "M12 21a9 9 0 100-18 9 9 0 000 18zM12 7v5l3 2",
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
