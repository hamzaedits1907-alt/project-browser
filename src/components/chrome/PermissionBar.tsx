import { useState } from "react";

import { hostOf, permissionLabel } from "../../lib/format";
import { api } from "../../lib/ipc";
import { attempt, useStore } from "../../lib/store";
import type { PermissionPrompt } from "../../lib/types";
import { Icon } from "../ui/Icon";
import type { IconName } from "../ui/Icon";
import { Button } from "../ui/controls";

const KIND_ICON: Record<string, IconName> = {
  camera: "camera",
  microphone: "mic",
  geolocation: "globe",
  notifications: "info",
  clipboard: "copy",
  downloads: "download",
  files: "folder",
};

/** Docked permission request for the active tab (never hidden behind the page). */
export function PermissionBar() {
  const activeId = useStore((s) => s.browser.activeId);
  const tabs = useStore((s) => s.browser.tabs);
  const prompts = useStore((s) => s.prompts);
  const mine = prompts.filter((p) => p.tabId === activeId);
  const first = mine[0];
  if (!first) return null;
  const isPrivate = tabs.find((t) => t.id === first.tabId)?.private ?? false;
  return <PromptRow key={first.id} prompt={first} extra={mine.length - 1} isPrivate={isPrivate} />;
}

function PromptRow({ prompt, extra, isPrivate }: { prompt: PermissionPrompt; extra: number; isPrivate: boolean }) {
  const [remember, setRemember] = useState(!isPrivate);
  const respond = (allow: boolean) => void attempt(() => api.permissionRespond(prompt.id, allow, remember && !isPrivate));
  return (
    <div className="permbar" role="alertdialog" aria-label="Permission request">
      <span className="perm-icon"><Icon name={KIND_ICON[prompt.kind] ?? "shield"} size={18} /></span>
      <div className="perm-text">
        <strong>{hostOf(prompt.origin)}</strong> wants to {permissionLabel(prompt.kind).toLowerCase()}
        {extra > 0 && <span className="dim"> · {extra} more request{extra > 1 ? "s" : ""} waiting</span>}
      </div>
      {!isPrivate && (
        <label className="perm-remember">
          <input type="checkbox" checked={remember} onChange={(e) => setRemember(e.target.checked)} />
          Remember
        </label>
      )}
      <Button size="sm" variant="primary" onClick={() => respond(true)}>Allow</Button>
      <Button size="sm" onClick={() => respond(false)}>Block</Button>
    </div>
  );
}
