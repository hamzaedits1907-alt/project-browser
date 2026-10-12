import { describe, expect, it } from "vitest";

import { convert, evaluate, formatNumber, instantAnswer, resolveBang } from "./instant";

describe("calculator", () => {
  it("follows precedence, parentheses and powers", () => {
    expect(evaluate("2+3*4")).toBe(14);
    expect(evaluate("(2+3)*4")).toBe(20);
    expect(evaluate("2^3^2")).toBe(512);
    expect(evaluate("-2^2")).toBe(-4);
    expect(evaluate("10/4")).toBe(2.5);
    expect(evaluate("7 % 3")).toBe(1);
    expect(evaluate("50%")).toBe(0.5);
    expect(evaluate("sqrt(16)+pi-pi")).toBe(4);
    expect(evaluate("1,5*2")).toBe(3);
    expect(evaluate("3×4÷2")).toBe(6);
  });

  it("refuses anything that is not a complete expression", () => {
    expect(evaluate("2+")).toBeNull();
    expect(evaluate("(1")).toBeNull();
    expect(evaluate("alert(1)")).toBeNull();
    expect(evaluate("1/0")).toBeNull();
    expect(evaluate("")).toBeNull();
  });

  it("answers in the address bar only for things that look like maths", () => {
    expect(instantAnswer("12*7")?.answer).toBe("= 84");
    expect(instantAnswer("=sqrt(2)")?.value).toBe("1.41421356237");
    expect(instantAnswer("2024")).toBeNull();
    expect(instantAnswer("weather berlin")).toBeNull();
    expect(instantAnswer("covid-19")).toBeNull();
    expect(instantAnswer("1000000*3")?.answer).toBe("= 3 000 000");
  });

  it("formats numbers readably", () => {
    expect(formatNumber(0.1 + 0.2)).toBe("0.3");
    expect(formatNumber(1e-9)).toBe("1e-9");
  });
});

describe("unit conversion", () => {
  it("converts lengths, weights, data and temperatures", () => {
    expect(convert("10 km in miles")?.answer).toBe("6.21371 mi");
    expect(convert("5 lb to kg")?.answer).toBe("2.26796 kg");
    expect(convert("100 f to c")?.answer).toBe("37.7778 °C");
    expect(convert("0 c in k")?.answer).toBe("273.15 K");
    expect(convert("2 gb in mb")?.answer).toBe("2 000 MB");
    expect(convert("90 min to h")?.answer).toBe("1.5 h");
  });

  it("refuses mismatched or unknown units", () => {
    expect(convert("10 km in kg")).toBeNull();
    expect(convert("10 parsecs in m")).toBeNull();
    expect(convert("how far is 10 km")).toBeNull();
  });
});

describe("bangs", () => {
  it("resolves leading and trailing bangs", () => {
    expect(resolveBang("!yt lofi beats")?.url).toBe("https://www.youtube.com/results?search_query=lofi%20beats");
    expect(resolveBang("rust ownership !so")?.bang.name).toBe("Stack Overflow");
    expect(resolveBang("!w C++")?.url).toBe("https://en.wikipedia.org/w/index.php?search=C%2B%2B");
  });

  it("ignores unknown bangs and empty queries", () => {
    expect(resolveBang("!nope test")).toBeNull();
    expect(resolveBang("!yt")).toBeNull();
    expect(resolveBang("hello world")).toBeNull();
  });
});
