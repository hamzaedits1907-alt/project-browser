import type { AiProvider } from "../../lib/types";

export interface AiPreset {
  id: string;
  label: string;
  provider: AiProvider;
  baseUrl: string;
  /** Suggested model; the user can change it. Empty when it depends on what is installed. */
  model: string;
  needsKey: boolean;
  note: string;
}

export const AI_PRESETS: AiPreset[] = [
  { id: "ollama", label: "Ollama (local)", provider: "openAiCompatible", baseUrl: "http://localhost:11434/v1", model: "", needsKey: false, note: "Runs on your computer. Enter the name of a model you have pulled, e.g. llama3.2." },
  { id: "lmstudio", label: "LM Studio (local)", provider: "openAiCompatible", baseUrl: "http://localhost:1234/v1", model: "", needsKey: false, note: "Start the local server in LM Studio, then enter the loaded model's name." },
  { id: "openai", label: "OpenAI", provider: "openAiCompatible", baseUrl: "https://api.openai.com/v1", model: "", needsKey: true, note: "Needs an OpenAI API key. Messages are sent to OpenAI." },
  { id: "openrouter", label: "OpenRouter", provider: "openAiCompatible", baseUrl: "https://openrouter.ai/api/v1", model: "", needsKey: true, note: "Needs an OpenRouter API key. Messages are sent to OpenRouter." },
  { id: "anthropic", label: "Anthropic", provider: "anthropic", baseUrl: "https://api.anthropic.com", model: "claude-sonnet-5-5", needsKey: true, note: "Needs an Anthropic API key. Messages are sent to Anthropic." },
];

export function presetFor(provider: AiProvider, baseUrl: string): AiPreset | null {
  const url = baseUrl.replace(/\/+$/, "");
  return AI_PRESETS.find((p) => p.provider === provider && p.baseUrl === url) ?? null;
}

/** Whether the endpoint is on this machine (nothing leaves the computer). */
export function isLocalEndpoint(baseUrl: string): boolean {
  try {
    const host = new URL(baseUrl).hostname;
    return host === "localhost" || host === "127.0.0.1" || host === "[::1]" || host === "::1";
  } catch {
    return false;
  }
}
