import { useState } from "react";

import { Button, TextField } from "../../components/ui/controls";
import { Icon } from "../../components/ui/Icon";
import { ACCENTS, accentCss, SEARCH_ENGINES } from "../../lib/settings";
import { api, errorMessage } from "../../lib/ipc";
import { useOverlay } from "../../lib/overlay";
import { markOnboarded, toast, useStore } from "../../lib/store";
import { useSettings } from "../../lib/useSettings";
import { ImportFlow } from "../import/ImportFlow";
import type { SearchEngine } from "../../lib/types";

type Step = "welcome" | "personalize" | "import" | "ready";
const ORDER: Step[] = ["welcome", "personalize", "import", "ready"];

export function FirstRun() {
  const ready = useOverlay(true);
  const profile = useStore((s) => s.profile);
  const { settings, update } = useSettings();
  const [step, setStep] = useState<Step>("welcome");
  const [name, setName] = useState(profile?.name ?? "");
  const [engine, setEngine] = useState<SearchEngine>(settings.searchEngine);
  const [finishing, setFinishing] = useState(false);

  if (!ready) return null;

  const finish = async () => {
    setFinishing(true);
    try {
      await api.firstRunComplete(name.trim() || undefined, engine);
      markOnboarded();
    } catch (e) {
      toast(errorMessage(e));
      setFinishing(false);
    }
  };

  const index = ORDER.indexOf(step);
  return (
    <div className="firstrun" role="dialog" aria-modal="true" aria-label="Welcome to Project Browser">
      <div className="firstrun-glow" aria-hidden="true" />
      <div className="firstrun-card">
        <div className="steps" aria-label={`Step ${index + 1} of ${ORDER.length}`}>
          {ORDER.map((s, i) => <span key={s} className={i <= index ? "is-done" : ""} />)}
        </div>

        {step === "welcome" && (
          <div className="fr-body">
            <div className="fr-logo" aria-hidden="true">
              <svg viewBox="0 0 32 32" width="64" height="64">
                <defs>
                  <linearGradient id="fr-logo" x1="0" y1="0" x2="1" y2="1">
                    <stop offset="0" stopColor="var(--accent-hi)" />
                    <stop offset="1" stopColor="var(--accent-lo)" />
                  </linearGradient>
                </defs>
                <rect x="2" y="2" width="28" height="28" rx="9" fill="url(#fr-logo)" />
                <path d="M11 22V10h6.2a3.6 3.6 0 010 7.2H11" fill="none" stroke="var(--on-accent)" strokeWidth="2.6" strokeLinecap="round" strokeLinejoin="round" />
              </svg>
            </div>
            <h1>Welcome to Project Browser</h1>
            <p className="dim">A fast browser that gets out of your game's way. Sleeping tabs, a Gaming Mode that frees memory with one key, and a live look at what your browser is using.</p>
            <ul className="fr-points">
              <li><Icon name="bolt" size={18} /><span><strong>Gaming Mode</strong> Ctrl+Shift+G puts background tabs to sleep and holds downloads.</span></li>
              <li><Icon name="gauge" size={18} /><span><strong>Honest performance</strong> real memory, CPU and GPU readings per tab.</span></li>
              <li><Icon name="shield" size={18} /><span><strong>Private by default</strong> your data stays on this computer.</span></li>
            </ul>
            <div className="wizard-actions"><Button variant="primary" onClick={() => setStep("personalize")} autoFocus>Get started</Button></div>
          </div>
        )}

        {step === "personalize" && (
          <div className="fr-body">
            <h1>Make it yours</h1>
            <TextField label="Profile name" value={name} placeholder="e.g. Me" maxLength={40} data-autofocus onChange={(e) => setName(e.target.value)} />
            <div className="field">
              <label>Accent colour</label>
              <div className="swatches" role="radiogroup" aria-label="Accent colour">
                {ACCENTS.map((a) => (
                  <button key={a.id} type="button" role="radio" aria-checked={a.id === settings.accent} className={`swatch${a.id === settings.accent ? " is-on" : ""}`} style={{ "--sw": accentCss(a.hsl) } as React.CSSProperties} onClick={() => void update({ accent: a.id })}>
                    <span className="swatch-dot" />{a.label}
                  </button>
                ))}
              </div>
            </div>
            <div className="field">
              <label>Search engine</label>
              <div className="engine-grid" role="radiogroup" aria-label="Search engine">
                {SEARCH_ENGINES.map((e) => (
                  <button key={e.id} type="button" role="radio" aria-checked={e.id === engine} className={`chip${e.id === engine ? " is-on" : ""}`} onClick={() => setEngine(e.id)}>{e.label}</button>
                ))}
              </div>
            </div>
            <div className="wizard-actions">
              <Button variant="ghost" onClick={() => setStep("welcome")}>Back</Button>
              <Button variant="primary" onClick={() => setStep("import")}>Continue</Button>
            </div>
          </div>
        )}

        {step === "import" && (
          <div className="fr-body">
            <h1>Bring your stuff over</h1>
            <p className="dim">Bookmarks, history, saved passwords and open tabs from Chrome, Edge or Brave. You can skip this and do it later from the menu.</p>
            <ImportFlow onClose={() => setStep("ready")} cancelLabel="Skip for now" />
          </div>
        )}

        {step === "ready" && (
          <div className="fr-body">
            <div className="fr-check"><Icon name="check" size={30} /></div>
            <h1>You're all set</h1>
            <p className="dim">You can change anything later in Settings (Ctrl+,): import, passwords, and the NVIDIA AI assistant (just add your API key). Import is always available from the menu.</p>
            <div className="wizard-actions">
              <Button variant="primary" icon="rocket" disabled={finishing} onClick={() => void finish()}>{finishing ? "Starting…" : "Start browsing"}</Button>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
