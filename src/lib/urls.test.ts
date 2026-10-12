import { describe, expect, it } from "vitest";

import type { Tab } from "./types";
import { idleAddress, internalPageOf, isInternalUrl, isSafeImageSrc, monogram, originOf, splitUrl, tabTitle } from "./urls";

const tab = (over: Partial<Tab>): Tab => ({
  id: 1, url: "https://example.com/", title: "", favicon: null, pinned: false, private: false, loading: false, canGoBack: false, canGoForward: false,
  zoom: 1, audible: false, muted: false, lifecycle: "live", security: "secure", error: null, heavy: false, bookmarked: false, lastActive: 0, blockedPopup: null, ...over,
});

describe("internal pages", () => {
  it("recognises only known browser:// pages", () => {
    expect(isInternalUrl("browser://settings")).toBe(true);
    expect(internalPageOf("browser://settings")).toBe("settings");
    expect(internalPageOf("browser://SETTINGS/x?y")).toBe("settings");
    expect(internalPageOf("browser://nope")).toBeNull();
    expect(internalPageOf("https://settings")).toBeNull();
  });
});

describe("address bar helpers", () => {
  it("splits http(s) URLs for styling", () => {
    expect(splitUrl("https://en.wikipedia.org/wiki/Rust?x=1#top")).toEqual({ scheme: "https://", host: "en.wikipedia.org", rest: "/wiki/Rust?x=1#top" });
    expect(splitUrl("https://example.com/")).toEqual({ scheme: "https://", host: "example.com", rest: "" });
    expect(splitUrl("file:///C:/x.txt").scheme).toBe("");
  });
  it("shows nothing on the new tab page and the failed URL on error pages", () => {
    expect(idleAddress(tab({ url: "browser://newtab" }))).toBe("");
    expect(idleAddress(tab({ url: "about:blank", error: { url: "https://bad.example/", code: "X", description: "" } }))).toBe("https://bad.example/");
    expect(idleAddress(null)).toBe("");
  });
  it("derives titles and monograms", () => {
    expect(tabTitle(tab({ title: "Hello" }))).toBe("Hello");
    expect(tabTitle(tab({ url: "https://news.example.org/a" }))).toBe("news.example.org");
    expect(tabTitle(tab({ url: "browser://newtab" }))).toBe("New Tab");
    expect(monogram({ title: "rust", url: "https://x.y" })).toBe("R");
    expect(monogram({ title: "", url: "https://www.github.com" })).toBe("G");
  });
  it("extracts origins for permissions and rejects non-web pages", () => {
    expect(originOf("https://a.example.com:8443/x")).toBe("https://a.example.com:8443");
    expect(originOf("about:blank")).toBeNull();
    expect(originOf("browser://settings")).toBeNull();
  });
  it("only accepts safe image sources", () => {
    expect(isSafeImageSrc("https://x/y.png")).toBe(true);
    expect(isSafeImageSrc("data:image/png;base64,AA")).toBe(true);
    expect(isSafeImageSrc("javascript:alert(1)")).toBe(false);
    expect(isSafeImageSrc("file:///C:/a.png")).toBe(false);
    expect(isSafeImageSrc(null)).toBe(false);
  });
});
