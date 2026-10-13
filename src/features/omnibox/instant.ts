// Instant answers in the address bar: arithmetic, unit conversion and "bang" shortcuts (!yt cats searches YouTube).
// Everything here is computed locally; nothing typed is sent anywhere until the user presses Enter.

export interface InstantAnswer {
  kind: "calc" | "convert";
  /** What the user typed, normalised (shown dimmed). */
  question: string;
  /** The answer as shown, e.g. "= 42" or "6.21 mi". */
  answer: string;
  /** The bare value copied by Enter or a click. */
  value: string;
}

// ---- arithmetic ------------------------------------------------------------------------------------

const FUNCS: Record<string, (x: number) => number> = {
  sqrt: Math.sqrt,
  abs: Math.abs,
  sin: Math.sin,
  cos: Math.cos,
  tan: Math.tan,
  log: Math.log10,
  ln: Math.log,
  round: Math.round,
  floor: Math.floor,
  ceil: Math.ceil,
};
const CONSTS: Record<string, number> = { pi: Math.PI, e: Math.E };

/**
 * Evaluates an arithmetic expression (+ - * / ^ %, parentheses, a few functions, pi and e) with a small recursive
 * descent parser. Never uses eval. Returns null for anything that is not a complete, finite expression.
 */
export function evaluate(input: string): number | null {
  const src = input.replace(/×/g, "*").replace(/÷/g, "/").replace(/,/g, ".").replace(/\s+/g, "").toLowerCase();
  if (!src || src.length > 200) return null;
  let i = 0;
  const peek = () => src[i] ?? "";
  const eat = (c: string) => {
    if (src[i] === c) {
      i++;
      return true;
    }
    return false;
  };
  const expr = (): number => {
    let v = term();
    for (;;) {
      if (eat("+")) v += term();
      else if (eat("-")) v -= term();
      else return v;
    }
  };
  const term = (): number => {
    let v = unary();
    for (;;) {
      if (eat("*")) v *= unary();
      else if (eat("/")) v /= unary();
      else if (peek() === "%" && !/[0-9.(a-z]/.test(src[i + 1] ?? "")) {
        i++;
        v /= 100;
      } else if (eat("%")) v %= unary();
      else return v;
    }
  };
  // Unary minus binds looser than ^, as in maths: -2^2 = -4.
  const unary = (): number => {
    if (eat("-")) return -unary();
    if (eat("+")) return unary();
    return power();
  };
  const power = (): number => {
    const base = atom();
    if (eat("^")) return Math.pow(base, unary());
    return base;
  };
  const atom = (): number => {
    if (eat("(")) {
      const v = expr();
      if (!eat(")")) throw new Error("paren");
      return v;
    }
    const num = /^(\d+\.?\d*|\.\d+)(e[+-]?\d+)?/.exec(src.slice(i));
    if (num) {
      i += num[0].length;
      return parseFloat(num[0]);
    }
    const word = /^[a-z]+/.exec(src.slice(i));
    if (word) {
      const w = word[0];
      i += w.length;
      if (w in CONSTS) return CONSTS[w] as number;
      const f = FUNCS[w];
      if (f && eat("(")) {
        const v = expr();
        if (!eat(")")) throw new Error("paren");
        return f(v);
      }
    }
    throw new Error("token");
  };
  try {
    const v = expr();
    if (i !== src.length || !Number.isFinite(v)) return null;
    return v;
  } catch {
    return null;
  }
}

export function formatNumber(n: number): string {
  if (Number.isInteger(n) && Math.abs(n) < 1e15) return n.toLocaleString("en-US").replace(/,/g, " ");
  const abs = Math.abs(n);
  if (abs !== 0 && (abs < 1e-6 || abs >= 1e15)) return n.toExponential(6).replace(/\.?0+e/, "e");
  return String(Number(n.toPrecision(12)));
}

/** True when the text looks like a calculation rather than a search (it has an operator and starts sensibly). */
function looksLikeMath(text: string): boolean {
  const t = text.trim();
  if (!/[0-9)]/.test(t) || !/^[\d\s.,+\-*/^%()×÷a-z]+$/i.test(t)) return false;
  if (/^[\d\s.,]+$/.test(t)) return false; // a bare number is a search
  return /[+\-*/^%×÷]/.test(t) || /^(sqrt|abs|sin|cos|tan|log|ln|round|floor|ceil)\(/i.test(t);
}

// ---- unit conversion -------------------------------------------------------------------------------

interface Unit {
  dim: string;
  /** Factor to the dimension's base unit. Temperature is handled separately. */
  f: number;
  label: string;
}

const UNITS: Record<string, Unit> = {};
function def(dim: string, f: number, label: string, ...names: string[]) {
  for (const n of names) UNITS[n] = { dim, f, label };
}
def("len", 1, "m", "m", "meter", "meters", "metre", "metres");
def("len", 1000, "km", "km", "kilometer", "kilometers", "kilometre", "kilometres");
def("len", 0.01, "cm", "cm", "centimeter", "centimeters");
def("len", 0.001, "mm", "mm", "millimeter", "millimeters");
def("len", 1609.344, "mi", "mi", "mile", "miles");
def("len", 0.9144, "yd", "yd", "yard", "yards");
def("len", 0.3048, "ft", "ft", "foot", "feet");
def("len", 0.0254, "in", "in", "inch", "inches");
def("mass", 1, "kg", "kg", "kilo", "kilos", "kilogram", "kilograms");
def("mass", 0.001, "g", "g", "gram", "grams");
def("mass", 0.45359237, "lb", "lb", "lbs", "pound", "pounds");
def("mass", 0.028349523125, "oz", "oz", "ounce", "ounces");
def("mass", 1000, "t", "tonne", "tonnes");
def("vol", 1, "l", "l", "liter", "liters", "litre", "litres");
def("vol", 0.001, "ml", "ml", "milliliter", "milliliters");
def("vol", 3.785411784, "gal", "gal", "gallon", "gallons");
def("vol", 0.2365882365, "cup", "cup", "cups");
def("data", 1, "B", "b", "byte", "bytes");
def("data", 1e3, "KB", "kb", "kilobyte", "kilobytes");
def("data", 1e6, "MB", "mb", "megabyte", "megabytes");
def("data", 1e9, "GB", "gb", "gigabyte", "gigabytes");
def("data", 1e12, "TB", "tb", "terabyte", "terabytes");
def("data", 1024, "KiB", "kib");
def("data", 1024 ** 2, "MiB", "mib");
def("data", 1024 ** 3, "GiB", "gib");
def("time", 1, "s", "s", "sec", "secs", "second", "seconds");
def("time", 60, "min", "min", "mins", "minute", "minutes");
def("time", 3600, "h", "h", "hr", "hrs", "hour", "hours");
def("time", 86400, "days", "day", "days");
def("time", 604800, "weeks", "week", "weeks");
def("speed", 1, "km/h", "kmh", "km/h", "kph");
def("speed", 1.609344, "mph", "mph");
def("speed", 3.6, "m/s", "m/s", "mps");
def("speed", 1.852, "kn", "kn", "knot", "knots");
const TEMPS: Record<string, string> = { c: "°C", celsius: "°C", "°c": "°C", f: "°F", fahrenheit: "°F", "°f": "°F", k: "K", kelvin: "K" };

function toCelsius(v: number, unit: string): number {
  return unit === "°F" ? ((v - 32) * 5) / 9 : unit === "K" ? v - 273.15 : v;
}
function fromCelsius(c: number, unit: string): number {
  return unit === "°F" ? (c * 9) / 5 + 32 : unit === "K" ? c + 273.15 : c;
}

/** "10 km in miles", "72 f to c", "3.5GB in MB". Returns null when the text is not a conversion. */
export function convert(text: string): InstantAnswer | null {
  const m = /^\s*(-?[\d.,]+)\s*([a-z°/]+)\s+(?:in|to|as|=)\s+([a-z°/]+)\s*$/i.exec(text);
  if (!m) return null;
  const value = parseFloat((m[1] as string).replace(",", "."));
  if (!Number.isFinite(value)) return null;
  const from = (m[2] as string).toLowerCase();
  const to = (m[3] as string).toLowerCase();
  let result: number;
  let label: string;
  let fromLabel: string;
  if (TEMPS[from] && TEMPS[to]) {
    fromLabel = TEMPS[from] as string;
    label = TEMPS[to] as string;
    result = fromCelsius(toCelsius(value, fromLabel), label);
  } else {
    const a = UNITS[from];
    const b = UNITS[to];
    if (!a || !b || a.dim !== b.dim) return null;
    result = (value * a.f) / b.f;
    label = b.label;
    fromLabel = a.label;
  }
  const shown = formatNumber(Number(result.toPrecision(6)));
  return { kind: "convert", question: `${formatNumber(value)} ${fromLabel}`, answer: `${shown} ${label}`, value: shown };
}

export function instantAnswer(text: string): InstantAnswer | null {
  const t = text.trim();
  if (!t || t.length > 200) return null;
  const c = convert(t);
  if (c) return c;
  const expr = t.startsWith("=") ? t.slice(1) : t;
  if (!t.startsWith("=") && !looksLikeMath(expr)) return null;
  const v = evaluate(expr);
  if (v === null) return null;
  const shown = formatNumber(v);
  return { kind: "calc", question: expr.trim(), answer: `= ${shown}`, value: shown.replace(/ /g, "") };
}

// ---- bangs -----------------------------------------------------------------------------------------

export interface Bang {
  key: string;
  name: string;
  /** `%s` is replaced by the encoded query. */
  url: string;
}

export const BANGS: Bang[] = [
  { key: "yt", name: "YouTube", url: "https://www.youtube.com/results?search_query=%s" },
  { key: "w", name: "Wikipedia", url: "https://en.wikipedia.org/w/index.php?search=%s" },
  { key: "gh", name: "GitHub", url: "https://github.com/search?q=%s" },
  { key: "r", name: "Reddit", url: "https://www.reddit.com/search/?q=%s" },
  { key: "a", name: "Amazon", url: "https://www.amazon.com/s?k=%s" },
  { key: "m", name: "Google Maps", url: "https://www.google.com/maps/search/%s" },
  { key: "i", name: "Google Images", url: "https://www.google.com/search?tbm=isch&q=%s" },
  { key: "g", name: "Google", url: "https://www.google.com/search?q=%s" },
  { key: "ddg", name: "DuckDuckGo", url: "https://duckduckgo.com/?q=%s" },
  { key: "b", name: "Bing", url: "https://www.bing.com/search?q=%s" },
  { key: "so", name: "Stack Overflow", url: "https://stackoverflow.com/search?q=%s" },
  { key: "mdn", name: "MDN", url: "https://developer.mozilla.org/en-US/search?q=%s" },
  { key: "npm", name: "npm", url: "https://www.npmjs.com/search?q=%s" },
  { key: "crates", name: "crates.io", url: "https://crates.io/search?q=%s" },
  { key: "x", name: "X", url: "https://x.com/search?q=%s" },
  { key: "twitch", name: "Twitch", url: "https://www.twitch.tv/search?term=%s" },
  { key: "steam", name: "Steam", url: "https://store.steampowered.com/search/?term=%s" },
  { key: "imdb", name: "IMDb", url: "https://www.imdb.com/find/?q=%s" },
  { key: "tr", name: "Google Translate", url: "https://translate.google.com/?sl=auto&tl=en&text=%s" },
  { key: "def", name: "Dictionary", url: "https://www.merriam-webster.com/dictionary/%s" },
  { key: "spotify", name: "Spotify", url: "https://open.spotify.com/search/%s" },
  { key: "maps", name: "OpenStreetMap", url: "https://www.openstreetmap.org/search?query=%s" },
  { key: "wiki", name: "Wikipedia", url: "https://en.wikipedia.org/w/index.php?search=%s" },
];

/** `!yt lofi` or `lofi !yt`: the bang and the address it opens. Null when there is no known bang or no query. */
export function resolveBang(text: string, custom: Bang[] = []): { bang: Bang; query: string; url: string } | null {
  const t = text.trim();
  const lead = /^!(\w+)\s+(.+)$/.exec(t);
  const trail = /^(.+?)\s+!(\w+)$/.exec(t);
  const key = (lead?.[1] ?? trail?.[2])?.toLowerCase();
  const query = (lead?.[2] ?? trail?.[1])?.trim();
  if (!key || !query) return null;
  // The user's own shortcuts come first, so they can also redefine a built-in one.
  const bang = custom.find((b) => b.key === key) ?? BANGS.find((b) => b.key === key);
  if (!bang) return null;
  return { bang, query, url: bang.url.replace("%s", encodeURIComponent(query)) };
}
