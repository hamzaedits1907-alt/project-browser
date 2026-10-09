import type { RefObject } from "react";

import { api } from "../../lib/ipc";
import { attempt, useStore } from "../../lib/store";
import { DownloadRow } from "../chrome/DownloadRow";
import { Button } from "../ui/controls";
import { Popover } from "../ui/Popover";
import { EmptyState } from "../ui/Section";

export function DownloadsPopover({ open, onClose, anchor }: { open: boolean; onClose: () => void; anchor: RefObject<HTMLElement | null> }) {
  const downloads = useStore((s) => s.downloads);
  const recent = downloads.slice(0, 6);
  return (
    <Popover open={open} onClose={onClose} anchor={anchor} align="end" width={380} label="Downloads" className="downloads-pop">
      <div className="pop-head">
        <h3>Downloads</h3>
        <Button size="sm" variant="ghost" onClick={() => { onClose(); void attempt(() => api.runCommand({ type: "openDownloads" })); }}>
          See all
        </Button>
      </div>
      {recent.length === 0 ? (
        <EmptyState icon="download" title="No downloads yet">Files you download will show up here.</EmptyState>
      ) : (
        <div className="pop-list">
          {recent.map((d) => (
            <DownloadRow key={d.id} d={d} compact />
          ))}
        </div>
      )}
    </Popover>
  );
}
