// Ready-made requests for the assistant panel. Each one is an ordinary chat message sent with the current page
// attached, so it uses the same model, the same context limit and the same private-tab rule as everything else.

export const LANGUAGES = ["English", "German", "French", "Spanish", "Italian", "Portuguese", "Dutch", "Turkish", "Polish", "Russian", "Arabic", "Japanese", "Chinese"] as const;

const BY_CODE: Record<string, (typeof LANGUAGES)[number]> = {
  en: "English",
  de: "German",
  fr: "French",
  es: "Spanish",
  it: "Italian",
  pt: "Portuguese",
  nl: "Dutch",
  tr: "Turkish",
  pl: "Polish",
  ru: "Russian",
  ar: "Arabic",
  ja: "Japanese",
  zh: "Chinese",
};

/** The language to translate into by default: the browser's own language when it is one of the offered ones. */
export function defaultLanguage(locale: string): (typeof LANGUAGES)[number] {
  return BY_CODE[locale.toLowerCase().split(/[-_]/)[0] ?? ""] ?? "English";
}

const NEEDS_SELECTION = "If I haven't selected any text on the page, say so and stop.";

export const EXPLAIN_SELECTION = `Explain the text I selected on the page in simple terms, using the rest of the page only for context. ${NEEDS_SELECTION}`;

export function translateSelection(language: string): string {
  return `Translate the text I selected on the page into ${language}. Give only the translation, keeping the meaning and tone. ${NEEDS_SELECTION}`;
}

export const EXTRACT_DATA = "Extract the key structured data from this page (names, numbers, dates, prices, specifications) as a Markdown table or list. Include only what the page states; leave out anything it doesn't.";

export const CITED_SUMMARY =
  "Summarize this page in a few bullet points. After each point, quote the exact words from the page that support it, in quotation marks. Do not include a point you cannot quote, and do not add anything the page doesn't say.";

/** More one-click requests for the chat (each one sends the current page, like the others). */
export const PAGE_PRESETS: { label: string; title: string; prompt: string }[] = [
  { label: "TL;DR", title: "The page in three sentences", prompt: "Give me the gist of this page in at most three sentences." },
  { label: "Key takeaways", title: "What to remember from the page", prompt: "List the 5 most important takeaways from this page, each in one line." },
  { label: "Explain simply", title: "The page as if for a 12-year-old", prompt: "Explain what this page is about in simple words, as if to a 12-year-old. Keep it short." },
  { label: "Pros and cons", title: "Arguments for and against", prompt: "From this page, list the pros and cons (or arguments for and against) of what it discusses, as two short lists. Only use what the page says." },
  { label: "Action items", title: "Things to do from the page", prompt: "List the concrete actions, steps or to-dos this page asks for or describes, as a checklist. If there are none, say so." },
  { label: "Quiz me", title: "Five questions to test what you read", prompt: "Write 5 short quiz questions about this page, with the answers listed after all the questions." },
  { label: "Fact check", title: "Claims to double-check", prompt: "List the main factual claims this page makes. For each, say whether the page gives a source or evidence for it. Don't judge whether they are true; just show what is backed and what is not." },
  { label: "Reply draft", title: "Draft a reply to this message or post", prompt: "Draft a short, friendly reply to the message, email or post on this page. Give only the reply." },
];
