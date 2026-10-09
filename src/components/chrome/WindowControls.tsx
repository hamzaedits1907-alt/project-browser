import { useEffect, useState } from "react";

import { closeWindow, isMaximized, minimizeWindow, toggleMaximize } from "../../lib/window";
import { Icon } from "../ui/Icon";

export function WindowControls() {
  const [maximized, setMaximized] = useState(false);

  useEffect(() => {
    let frame = 0;
    const refresh = () => {
      cancelAnimationFrame(frame);
      frame = requestAnimationFrame(() => void isMaximized().then(setMaximized).catch(() => {}));
    };
    refresh();
    window.addEventListener("resize", refresh);
    return () => {
      cancelAnimationFrame(frame);
      window.removeEventListener("resize", refresh);
    };
  }, []);

  return (
    <div className="window-controls">
      <button type="button" className="wc" aria-label="Minimize" title="Minimize" onClick={() => void minimizeWindow()}>
        <Icon name="minimizeWin" size={14} strokeWidth={1.5} />
      </button>
      <button type="button" className="wc" aria-label={maximized ? "Restore" : "Maximize"} title={maximized ? "Restore" : "Maximize"} onClick={() => void toggleMaximize()}>
        <Icon name={maximized ? "restore" : "maximize"} size={13} strokeWidth={1.5} />
      </button>
      <button type="button" className="wc wc-close" aria-label="Close" title="Close" onClick={() => void closeWindow()}>
        <Icon name="close" size={15} strokeWidth={1.5} />
      </button>
    </div>
  );
}
