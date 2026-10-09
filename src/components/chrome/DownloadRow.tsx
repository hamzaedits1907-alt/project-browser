import { useState } from "react";

import { downloadProgress, formatBytes, formatRelative, hostOf } from "../../lib/format";
import { api } from "../../lib/ipc";
import { attempt } from "../../lib/store";
import type { Download, DownloadCommand } from "../../lib/types";
import { Icon } from "../ui/Icon";
import type { IconName } from "../ui/Icon";
import { Badge, IconButton, ProgressBar } from "../ui/controls";

function statusText(d: Download): string {
  switch (d.state) {
    case "inprogress":
      return `${formatBytes(d.receivedBytes)}${d.totalBytes ? ` of ${formatBytes(d.totalBytes)}` : ""}`;
    case "paused":
      return d.pausedByGaming ? "On hold until Gaming Mode ends" : `Paused · ${formatBytes(d.receivedBytes)}${d.totalBytes ? ` of ${formatBytes(d.totalBytes)}` : ""}`;
    case "completed":
      return `${formatBytes(d.totalBytes ?? d.receivedBytes)} · ${hostOf(d.url)} · ${formatRelative(d.endedAt ?? d.startedAt)}`;
    case "cancelled":
      return "Cancelled";
    case "interrupted":
      return d.error ? `Failed: ${d.error}` : "Failed";
  }
}

function fileIcon(d: Download): IconName {
  if (d.danger) return "warning";
  return "file";
}

export function DownloadRow({ d, compact }: { d: Download; compact?: boolean }) {
  const [confirming, setConfirming] = useState(false);
  const run = (command: DownloadCommand) => void attempt(() => api.downloadCommand(d.id, command));
  const active = d.state === "inprogress" || d.state === "paused";

  return (
    <div className={`dl-row${compact ? " is-compact" : ""}${d.state === "interrupted" || d.state === "cancelled" ? " is-failed" : ""}`}>
      <span className={`dl-icon${d.danger ? " is-danger" : ""}`}>
        <Icon name={fileIcon(d)} size={18} />
      </span>
      <div className="dl-main">
        <div className="dl-name truncate" title={d.path}>
          {d.filename}
          {d.private && <Badge>Private</Badge>}
        </div>
        {active && <ProgressBar value={downloadProgress(d.receivedBytes, d.totalBytes)} tone={d.state === "paused" ? "warn" : "accent"} />}
        <div className="dl-status truncate">{statusText(d)}</div>
        {confirming && (
          <div className="dl-confirm">
            <Icon name="warning" size={14} />
            <span>{d.danger}</span>
            <button type="button" className="link-btn" onClick={() => { setConfirming(false); run("openConfirmed"); }}>
              Open anyway
            </button>
            <button type="button" className="link-btn dim" onClick={() => setConfirming(false)}>
              Cancel
            </button>
          </div>
        )}
        {!confirming && d.state === "completed" && d.danger && <div className="dl-warning">{d.danger}</div>}
      </div>
      <div className="dl-actions">
        {d.state === "inprogress" && <IconButton icon="pause" label="Pause" size={16} onClick={() => run("pause")} />}
        {d.state === "paused" && <IconButton icon="play" label="Resume" size={16} onClick={() => run("resume")} />}
        {active && <IconButton icon="close" label="Cancel download" size={16} onClick={() => run("cancel")} />}
        {d.state === "completed" && (
          <IconButton icon="external" label="Open file" size={16} onClick={() => (d.danger ? setConfirming(true) : run("open"))} />
        )}
        {(d.state === "completed" || d.state === "interrupted") && <IconButton icon="folderOpen" label="Show in folder" size={16} onClick={() => run("showInFolder")} />}
        {!active && <IconButton icon="trash" label="Remove from list" size={16} onClick={() => run("remove")} />}
      </div>
    </div>
  );
}
