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
