import type { Tab } from "../../lib/types";
import { isInternalUrl } from "../../lib/urls";

/** Why this tab can't be read right now (`null` = it can). The core checks again before it reads anything. */
export function unreadableReason(tab: Pick<Tab, "private" | "url" | "error" | "lifecycle">): string | null {
  if (tab.private) return "Private tab: never shared";
  if (isInternalUrl(tab.url)) return "Browser page";
  if (tab.error) return "Failed to load";
  if (tab.lifecycle === "sleeping") return "Asleep: open it once first";
  if (tab.lifecycle === "unloaded") return "Not loaded: open it once first";
  return null;
}
