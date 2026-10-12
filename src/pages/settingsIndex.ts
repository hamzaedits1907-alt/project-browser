// The list of Settings sections and the settings search over it. Kept free of React so it can be tested.

import type { IconName } from "../components/ui/Icon";

export interface SectionMeta {
  id: string;
  label: string;
  icon: IconName;
  /** Extra words the search matches, so "password" finds Passwords & autofill. */
  keywords: string;
}

export const SECTION_META: SectionMeta[] = [
  { id: "general", label: "General", icon: "settings", keywords: "startup search engine downloads home page default" },
  { id: "appearance", label: "Appearance", icon: "eye", keywords: "theme accent colour color new tab background motion animations" },
  { id: "performance", label: "Performance", icon: "gauge", keywords: "memory ram sleeping tabs cpu speed" },
  { id: "gaming", label: "Gaming Mode", icon: "gamepad", keywords: "game fps background tabs downloads" },
  { id: "privacy", label: "Privacy & security", icon: "shield", keywords: "clear data history cookies cache permissions camera location private" },
  { id: "shield", label: "Project Shield", icon: "shieldCheck", keywords: "ads ad blocker tracker blocking easylist easyprivacy filter lists allowlist exceptions" },
  { id: "tunnel", label: "Project Tunnel", icon: "globe", keywords: "vpn proxy socks http tunnel privacy ip address" },
  { id: "passwords", label: "Passwords & autofill", icon: "key", keywords: "passwords logins sign in fill address autofill saved export csv" },
  { id: "profiles", label: "Profiles", icon: "user", keywords: "profile account switch" },
  { id: "import", label: "Import data", icon: "import", keywords: "chrome edge brave bookmarks history migrate move passwords tabs" },
  { id: "ai", label: "AI assistant", icon: "sparkle", keywords: "nvidia api key model chat assistant llm" },
  { id: "shortcuts", label: "Shortcuts", icon: "bolt", keywords: "keyboard keys hotkeys" },
  { id: "about", label: "About", icon: "info", keywords: "version update updates" },
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
