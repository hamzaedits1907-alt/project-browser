// The screenshot the dialog shows. It is taken while the page is still on screen (a page hidden under a dialog cannot
// be captured), then the dialog opens with it.

import { hostOf } from "../../lib/format";
import { api, errorMessage } from "../../lib/ipc";
import { overlayIdle } from "../../lib/overlay";
import { activeTab, getState, toast } from "../../lib/store";
import { closeDialog, openDialog } from "../../lib/ui";
import { isInternalUrl } from "../../lib/urls";

export interface Shot {
  tabId: number;
  full: boolean;
  url: string;
  blob: Blob;
  host: string;
}
let pending: Shot | null = null;

/**
 * Takes the screenshot while the page is still on screen, then opens the dialog with the result (a page hidden under
 * a dialog cannot be captured).
 */
export async function startScreenshot(full: boolean): Promise<void> {
  const tab = activeTab(getState());
  if (!tab || isInternalUrl(tab.url)) {
    toast("Open a website to take a screenshot.");
    return;
  }
  closeDialog();
  // A dialog that was open releases the page on its next render; wait for that, then for the page to paint again.
  await new Promise((r) => setTimeout(r, 60));
  await overlayIdle();
  await new Promise((r) => setTimeout(r, 180));
  try {
    if (full) toast("Capturing the whole page…");
    const blob = await api.pageScreenshot(tab.id, full);
    if (pending) URL.revokeObjectURL(pending.url);
    pending = { tabId: tab.id, full, blob, url: URL.createObjectURL(blob), host: hostOf(tab.url) };
    openDialog("screenshot");
  } catch (e) {
    toast(errorMessage(e));
  }
}

export function getShot(): Shot | null {
  return pending;
}
