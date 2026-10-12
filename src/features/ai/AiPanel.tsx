import { useEffect, useRef, useState } from "react";
import type { KeyboardEvent } from "react";

import { Button, IconButton, Spinner, Toggle } from "../../components/ui/controls";
import { Icon } from "../../components/ui/Icon";
import { api } from "../../lib/ipc";
import { writeLocal } from "../../lib/storage";
import { activeTab, attempt, useStore } from "../../lib/store";
import { isInternalUrl } from "../../lib/urls";
import { assistantReady, resolveSelection } from "./aiModels";
import { ai, clearConversation, loadAiConfig, loadModels, selectModel, sendMessage, setAiOpen, setIncludePage, stopStreaming } from "./aiStore";
import type { AiMessage } from "./aiStore";
import { parseBlocks } from "./markdown";
import type { Span } from "./markdown";
import { ModelPicker } from "./ModelPicker";

const SUGGESTIONS: { label: string; prompt: string; page: boolean }[] = [
  { label: "Summarize this page", prompt: "Summarize this page in a few bullet points.", page: true },
  { label: "Explain it simply", prompt: "Explain the main idea of this page in simple terms.", page: true },
  { label: "Key facts", prompt: "List the key facts, numbers and names from this page.", page: true },
];

function Spans({ spans }: { spans: Span[] }) {
  return (
    <>
      {spans.map((s, i) =>
        s.kind === "code" ? <code key={i}>{s.text}</code> : s.kind === "bold" ? <strong key={i}>{s.text}</strong> : <span key={i}>{s.text}</span>,
      )}
    </>
  );
}

function Message({ m, streaming }: { m: AiMessage; streaming: boolean }) {
  if (m.role === "user") return <div className="ai-msg ai-user" data-selectable>{m.content}</div>;
  if (m.error) {
    return (
      <div className="ai-msg ai-error" role="alert">
        <Icon name="warning" size={15} />
        <span data-selectable>{m.content}</span>
      </div>
    );
  }
  const blocks = parseBlocks(m.content);
  return (
    <div className="ai-msg ai-assistant" data-selectable>
      {blocks.length === 0 && streaming && <Spinner size={14} />}
      {blocks.map((b, i) => {
        switch (b.type) {
          case "code":
            return <pre key={i}><code>{b.text}</code></pre>;
          case "heading":
            return <h4 key={i}><Spans spans={b.spans} /></h4>;
          case "list": {
            const Tag = b.ordered ? "ol" : "ul";
            return (
              <Tag key={i}>
                {b.items.map((it, j) => <li key={j}><Spans spans={it} /></li>)}
              </Tag>
            );
          }
          default:
            return <p key={i}><Spans spans={b.spans} /></p>;
        }
      })}
    </div>
  );
}

export function AiPanel() {
  const { messages, streaming, includePage, config, models, unavailable } = ai.use((s) => s);
  const tab = useStore(activeTab);
  const [draft, setDraft] = useState("");
  const listRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLTextAreaElement>(null);

  const pageAvailable = !!tab && !isInternalUrl(tab.url) && !tab.private && !tab.error;
  const selection = resolveSelection(models.models, config?.model ?? "", unavailable);
  const configured = !!config && assistantReady(config.hasKey, selection) && config.model.trim().length > 0;
  const needsModel = !!config && config.hasKey && !configured;

  useEffect(() => {
    void loadAiConfig().then((c) => {
      if (c?.hasKey) void loadModels();
    });
    inputRef.current?.focus();
  }, []);

  useEffect(() => {
    const el = listRef.current;
    if (el) el.scrollTop = el.scrollHeight;
  }, [messages]);

  const send = (text: string, page = includePage && pageAvailable) => {
    if (!text.trim()) return;
    setDraft("");
    void sendMessage(text, page);
  };

  const onKey = (e: KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      send(draft);
    }
  };

  const openAiSettings = () => {
    writeLocal("settings.section", "ai");
    void attempt(() => api.runCommand({ type: "openSettings" }));
  };

  return (
    <aside className="ai-panel" aria-label="AI assistant">
      <header className="ai-head">
        <div className="ai-title"><Icon name="sparkle" size={17} /> Assistant</div>
        <div className="ai-head-actions">
          <IconButton icon="trash" label="New conversation" size={16} disabled={messages.length === 0} onClick={clearConversation} />
          <IconButton icon="settings" label="AI settings" size={16} onClick={openAiSettings} />
          <IconButton icon="close" label="Close assistant" size={16} onClick={() => setAiOpen(false)} />
        </div>
      </header>

      {config?.hasKey && models.models.length > 0 && (
        <div className="ai-model-bar">
          <ModelPicker
            compact
            models={models.models}
            value={config.model}
            unavailable={unavailable}
            busy={models.phase === "loading"}
            onChange={(id) => void selectModel(id)}
          />
        </div>
      )}

      <div className="ai-messages" ref={listRef}>
        {config && !config.hasKey && (
          <div className="ai-setup">
            <Icon name="key" size={20} />
            <h3>Connect NVIDIA AI</h3>
            <p className="dim">Paste your NVIDIA API key in AI settings and choose a model. Nothing is sent anywhere until you ask a question.</p>
            <Button variant="primary" onClick={openAiSettings}>Add API key</Button>
          </div>
        )}
        {needsModel && (
          <div className="ai-setup">
            <Icon name="sparkle" size={20} />
            <h3>{selection.status === "none" ? "Choose a model" : "Choose another model"}</h3>
            <p className="dim">
              {selection.status === "missing"
                ? `“${selection.id}” is no longer offered by NVIDIA.`
                : selection.status === "unsupported"
                  ? selection.reason
                  : "Pick one of NVIDIA's models to start chatting."}
            </p>
            {models.models.length === 0 && models.phase !== "loading" && <Button onClick={() => void loadModels(true)}>Load models</Button>}
          </div>
        )}
        {configured && messages.length === 0 && (
          <div className="ai-empty">
            <p className="dim">Ask about the page you're on, or anything else.</p>
            <div className="ai-suggestions">
              {SUGGESTIONS.map((s) => (
                <button key={s.label} type="button" className="chip" disabled={s.page && !pageAvailable} title={s.page && !pageAvailable ? "Open a web page first (not a private tab)" : undefined} onClick={() => { setIncludePage(s.page); send(s.prompt, s.page); }}>
                  {s.label}
                </button>
              ))}
            </div>
          </div>
        )}
        {messages.map((m, i) => <Message key={m.id} m={m} streaming={streaming && i === messages.length - 1} />)}
      </div>

      <footer className="ai-compose">
        <label className="ai-page-toggle">
          <Toggle label="Include this page" checked={includePage && pageAvailable} disabled={!pageAvailable} onChange={setIncludePage} />
          <span>
            Include this page
            <small>{pageAvailable ? "Shares the page's text with the model" : tab?.private ? "Not available in private tabs" : "Open a web page to share it"}</small>
          </span>
        </label>
        <div className="ai-input">
          <textarea ref={inputRef} rows={2} value={draft} placeholder={configured ? "Ask anything…" : config?.hasKey ? "Choose a model first" : "Add your NVIDIA key first"} aria-label="Message the assistant" disabled={!configured} onChange={(e) => setDraft(e.target.value)} onKeyDown={onKey} />
          {streaming ? (
            <IconButton icon="stop" label="Stop" className="ai-send" onClick={stopStreaming} />
          ) : (
            <IconButton icon="send" label="Send (Enter)" className="ai-send" disabled={!configured || !draft.trim()} onClick={() => send(draft)} />
          )}
        </div>
      </footer>
    </aside>
  );
}
