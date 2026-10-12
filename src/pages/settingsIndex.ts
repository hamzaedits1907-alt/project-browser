// The list of Settings sections and the settings search over it. Kept free of React so it can be tested.

import type { IconName } from "../components/ui/Icon";

export interface SectionMeta {
  id: string;
  label: string;
  icon: IconName;
  /** Extra words the search matches, so "password" finds Passwords & autofill. */
  keywords: string;
  group: string;
}

export const SECTION_META: SectionMeta[] = [
  { id: "general", label: "General", icon: "settings", keywords: "startup search engine downloads home page default", group: "Browser" },
  { id: "appearance", label: "Appearance", icon: "palette", keywords: "theme accent colour color new tab background motion animations density corners tabs vertical sidebar toolbar sounds wallpaper", group: "Browser" },
  { id: "performance", label: "Performance", icon: "gauge", keywords: "memory ram sleeping tabs cpu speed", group: "Browser" },
  { id: "gaming", label: "Gaming Mode", icon: "gamepad", keywords: "game fps background tabs downloads", group: "Browser" },
  { id: "privacy", label: "Privacy & security", icon: "shield", keywords: "clear data history cookies cache permissions camera location private", group: "Privacy" },
  { id: "shield", label: "Project Shield", icon: "shieldCheck", keywords: "ads ad blocker tracker blocking easylist easyprivacy filter lists allowlist exceptions", group: "Privacy" },
  { id: "tunnel", label: "Project Tunnel", icon: "globe", keywords: "vpn proxy socks http tunnel privacy ip address", group: "Privacy" },
  { id: "extensions", label: "Extensions", icon: "layers", keywords: "extensions add-ons addons plugins unpacked manifest dark reader ublock", group: "Privacy" },
  { id: "passwords", label: "Passwords & autofill", icon: "key", keywords: "passwords logins sign in fill address autofill saved export csv", group: "Privacy" },
  { id: "profiles", label: "Profiles", icon: "user", keywords: "profile account switch", group: "You" },
  { id: "import", label: "Import data", icon: "import", keywords: "chrome edge brave bookmarks history migrate move passwords tabs", group: "You" },
  { id: "ai", label: "AI assistant", icon: "sparkle", keywords: "nvidia api key model chat assistant llm", group: "You" },
  { id: "shortcuts", label: "Shortcuts", icon: "bolt", keywords: "keyboard keys hotkeys", group: "Help" },
  { id: "about", label: "About", icon: "info", keywords: "version update updates", group: "Help" },
];

/** The sections whose name or keywords contain every word typed in the settings search. */
export function matchSections(query: string): SectionMeta[] {
  const words = query.toLowerCase().split(/\s+/).filter(Boolean);
  if (words.length === 0) return SECTION_META;
  return SECTION_META.filter((s) => words.every((w) => `${s.label} ${s.keywords}`.toLowerCase().includes(w)));
}

/** The section to show: the remembered one if it still exists, else the first. */
export function resolveSection(saved: string | null | undefined): string {
  return SECTION_META.some((s) => s.id === saved) ? (saved as string) : "general";
}
