// Short diagnostic lines from the shell into the browser's own log (shell_trace). Used for what a CI probe or a bug
// report needs to see (popups opening and closing, and why). Never page content, addresses or anything typed.

import { api } from "./ipc";
import { inTauri } from "./window";

export function trace(message: string): void {
  if (!inTauri) return;
  void api.shellTrace(message.slice(0, 200)).catch(() => {});
}
