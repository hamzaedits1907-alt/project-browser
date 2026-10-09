import { createStore } from "../../lib/createStore";
import { api, errorMessage, on } from "../../lib/ipc";
import type { AiChatMessage, AiConfigView } from "../../lib/types";

export interface AiMessage extends AiChatMessage {
  id: number;
  error?: boolean;
}

interface AiState {
  open: boolean;
  config: AiConfigView | null;
  messages: AiMessage[];
  streaming: boolean;
  includePage: boolean;
  requestId: string | null;
}

export const ai = createStore<AiState>({
  open: false,
  config: null,
  messages: [],
  streaming: false,
  includePage: false,
  requestId: null,
});

let seq = 1;
let listening = false;

function ensureListening(): void {
  if (listening) return;
  listening = true;
  void on("pb:ai", (e) => {
    const s = ai.get();
    if (e.requestId !== s.requestId) return;
    if (e.kind === "delta") {
      ai.set((cur) => {
        const msgs = cur.messages.slice();
        const last = msgs[msgs.length - 1];
        if (last && last.role === "assistant") msgs[msgs.length - 1] = { ...last, content: last.content + e.text };
        return { messages: msgs };
      });
    } else if (e.kind === "done") {
      ai.set({ streaming: false, requestId: null });
    } else {
      finishWithError(e.text);
    }
  });
}

function finishWithError(message: string): void {
  ai.set((cur) => {
    const msgs = cur.messages.slice();
    const last = msgs[msgs.length - 1];
    if (last && last.role === "assistant" && last.content === "") msgs.pop();
    msgs.push({ id: seq++, role: "assistant", content: message, error: true });
    return { messages: msgs, streaming: false, requestId: null };
  });
}

export async function loadAiConfig(): Promise<AiConfigView | null> {
  ensureListening();
  try {
    const config = await api.aiConfigGet();
    ai.set({ config });
    return config;
  } catch {
    return null;
  }
}

export function setAiOpen(open: boolean): void {
  ai.set({ open });
  if (open) void loadAiConfig();
}

export function toggleAi(): void {
  setAiOpen(!ai.get().open);
}

export function setIncludePage(includePage: boolean): void {
  ai.set({ includePage });
}

export function clearConversation(): void {
  const { requestId } = ai.get();
  if (requestId) void api.aiCancel(requestId).catch(() => {});
  ai.set({ messages: [], streaming: false, requestId: null });
}

export async function sendMessage(text: string, includePage?: boolean): Promise<void> {
  const prompt = text.trim();
  const s = ai.get();
  if (!prompt || s.streaming) return;
  ensureListening();
  const requestId = `ai-${Date.now()}-${seq++}`;
  const history = s.messages.filter((m) => !m.error && m.content !== "").map(({ role, content }) => ({ role, content }));
  const messages: AiMessage[] = [
    ...s.messages,
    { id: seq++, role: "user", content: prompt },
    { id: seq++, role: "assistant", content: "" },
  ];
  ai.set({ messages, streaming: true, requestId });
  try {
    await api.aiChat(requestId, [...history, { role: "user", content: prompt }], includePage ?? s.includePage);
  } catch (e) {
    finishWithError(errorMessage(e));
  }
}

export function stopStreaming(): void {
  const { requestId } = ai.get();
  if (requestId) void api.aiCancel(requestId).catch(() => {});
  ai.set({ streaming: false, requestId: null });
}
