import { api, errorMessage, on } from "../../lib/ipc";
import type { AiChatMessage, ConnectionState } from "../../lib/types";

/** Why a request to the model did not produce an answer. */
export class AskError extends Error {
  constructor(
    message: string,
    readonly state: ConnectionState | null = null,
    readonly cancelled = false,
  ) {
    super(message);
    this.name = "AskError";
  }
}

export interface AskHandle {
  requestId: string;
  /** Resolves with the complete reply; rejects with an `AskError` (also when the user cancelled). */
  promise: Promise<string>;
  /** Stops the request. Nothing more is read, and `promise` rejects with `cancelled` set. */
  cancel: () => void;
}

let sequence = 1;

/**
 * Sends one request built by the shell (a comparison, a claim check, a plan, a summary) through the core, which
 * holds the key and enforces the user's context limit. The reply is collected from the same `pb:ai` events the chat
 * uses, matched by request id, so it never mixes with another request. `onText` receives the text so far.
 */
export function ask(system: string, messages: AiChatMessage[], opts: { maxTokens?: number; onText?: (text: string) => void } = {}): AskHandle {
  const requestId = `ask-${Date.now()}-${sequence++}`;
  let cancelled = false;
  let stop: (() => void) | null = null;

  const promise = new Promise<string>((resolve, reject) => {
    let text = "";
    let unlisten: (() => void) | null = null;
    let settled = false;
    const settle = (finish: () => void) => {
      if (settled) return;
      settled = true;
      unlisten?.();
      finish();
    };
    stop = () => settle(() => reject(new AskError("Cancelled.", null, true)));

    void (async () => {
      try {
        const off = await on("pb:ai", (e) => {
          if (e.requestId !== requestId) return;
          if (e.kind === "delta") {
            text += e.text;
            opts.onText?.(text);
          } else if (e.kind === "done") {
            settle(() => (cancelled ? reject(new AskError("Cancelled.", null, true)) : resolve(text)));
          } else {
            settle(() => reject(new AskError(e.text, e.state ?? null)));
          }
        });
        if (settled) {
          off();
          return;
        }
        unlisten = off;
        await api.aiAsk(requestId, system, messages, opts.maxTokens);
      } catch (e) {
        settle(() => reject(new AskError(errorMessage(e))));
      }
    })();
  });

  return {
    requestId,
    promise,
    cancel: () => {
      if (cancelled) return;
      cancelled = true;
      void api.aiCancel(requestId).catch(() => {});
      stop?.();
    },
  };
}
