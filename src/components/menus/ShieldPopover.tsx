import type { RefObject } from "react";

import { openSettingsSection } from "../../lib/ui";
import { Button } from "../ui/controls";
import { Popover } from "../ui/Popover";
import { ShieldSummary } from "./ShieldSummary";

/** What the Shield did on the current page, and the switch for this site. */
export function ShieldPopover({ open, onClose, anchor }: { open: boolean; onClose: () => void; anchor: RefObject<HTMLElement | null> }) {
  return (
    <Popover open={open} onClose={onClose} anchor={anchor} align="end" width={340} label="Project Shield" className="shield-pop">
      <div className="pop-head">
        <h3>Project Shield</h3>
        <Button
          size="sm"
          variant="ghost"
          onClick={() => {
            onClose();
            openSettingsSection("shield");
          }}
        >
          Settings
        </Button>
      </div>
      <ShieldSummary active={open} onReload={onClose} />
    </Popover>
  );
}
