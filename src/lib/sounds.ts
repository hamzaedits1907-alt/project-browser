// Optional interface sounds (Settings → Appearance → Interface sounds; off by default). Synthesised with Web Audio:
// no audio files, nothing loaded until the first sound plays, and a single short-lived node graph per sound.

import { getState } from "./store";

export type Cue = "tap" | "open" | "close" | "toggleOn" | "toggleOff" | "power" | "done" | "notice";

let ctx: AudioContext | null = null;

function audio(): AudioContext | null {
  if (typeof window === "undefined" || typeof window.AudioContext !== "function") return null;
  ctx ??= new window.AudioContext();
  return ctx;
}

const CUES: Record<Cue, { from: number; to: number; ms: number; gain: number; type: OscillatorType }> = {
  tap: { from: 1400, to: 900, ms: 45, gain: 0.035, type: "triangle" },
  open: { from: 520, to: 880, ms: 90, gain: 0.04, type: "sine" },
  close: { from: 760, to: 420, ms: 80, gain: 0.035, type: "sine" },
  toggleOn: { from: 660, to: 1180, ms: 70, gain: 0.04, type: "triangle" },
  toggleOff: { from: 900, to: 520, ms: 70, gain: 0.035, type: "triangle" },
  power: { from: 180, to: 960, ms: 260, gain: 0.05, type: "sawtooth" },
  // A download finished; an update is ready.
  done: { from: 880, to: 1320, ms: 140, gain: 0.04, type: "sine" },
  notice: { from: 660, to: 990, ms: 160, gain: 0.04, type: "triangle" },
};

/** Plays a cue if the user turned interface sounds on. Never throws. */
export function cue(name: Cue): void {
  const { uiSounds, uiSoundVolume } = getState().settings;
  if (!uiSounds) return;
  // The volume scales the cue's own (already quiet) level; 0 is silent. Website audio is never touched.
  const volume = Math.max(0, Math.min(100, uiSoundVolume ?? 60)) / 100;
  if (volume === 0) return;
  try {
    const ac = audio();
    if (!ac) return;
    const c = CUES[name];
    const t = ac.currentTime;
    const osc = ac.createOscillator();
    const gain = ac.createGain();
    osc.type = c.type;
    osc.frequency.setValueAtTime(c.from, t);
    osc.frequency.exponentialRampToValueAtTime(c.to, t + c.ms / 1000);
    gain.gain.setValueAtTime(0.0001, t);
    gain.gain.exponentialRampToValueAtTime(Math.max(0.0002, c.gain * volume * 1.6), t + 0.008);
    gain.gain.exponentialRampToValueAtTime(0.0001, t + c.ms / 1000);
    osc.connect(gain).connect(ac.destination);
    osc.start(t);
    osc.stop(t + c.ms / 1000 + 0.02);
  } catch {
    /* sound is decoration: a failure changes nothing */
  }
}
