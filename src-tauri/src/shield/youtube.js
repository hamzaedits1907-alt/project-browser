// Project Shield for YouTube. YouTube serves its ads from its own servers, in the same responses as the video, so
// blocking request URLs cannot stop them. Like uBlock Origin's YouTube filters, this script (added at document
// creation, before the page's own scripts) removes the ad schedule from the player's data, hides ad slots in the
// page, and, should an ad still start, skips it.
(() => {
  if (!/(^|\.)(youtube\.com|youtube-nocookie\.com)$/.test(location.hostname)) return;
  if (window.__pbShieldYt) return;
  Object.defineProperty(window, "__pbShieldYt", { value: true });

  const AD_KEYS = ["adPlacements", "adSlots", "playerAds", "adBreakHeartbeatParams"];
  const strip = (o) => {
    if (!o || typeof o !== "object") return o;
    for (const k of AD_KEYS) {
      if (k in o) {
        try {
          delete o[k];
        } catch (_) {}
      }
    }
    if (o.playerResponse && typeof o.playerResponse === "object") strip(o.playerResponse);
    return o;
  };
  const hasAds = (o) => !!o && typeof o === "object" && (AD_KEYS.some((k) => k in o) || (!!o.playerResponse && typeof o.playerResponse === "object"));

  // The first video's data arrives as `var ytInitialPlayerResponse = {...}` in the page.
  let initial;
  try {
    Object.defineProperty(window, "ytInitialPlayerResponse", { configurable: true, get: () => initial, set: (v) => (initial = strip(v)) });
  } catch (_) {}

  // Later videos' data arrives through fetch and is read with JSON.parse or Response.json.
  JSON.parse = new Proxy(JSON.parse, {
    apply(target, self, args) {
      const r = Reflect.apply(target, self, args);
      return hasAds(r) ? strip(r) : r;
    },
  });
  const json = Response.prototype.json;
  Response.prototype.json = new Proxy(json, {
    apply(target, self, args) {
      return Reflect.apply(target, self, args).then((r) => (hasAds(r) ? strip(r) : r));
    },
  });

  const css = [
    "ytd-ad-slot-renderer", "ytd-in-feed-ad-layout-renderer", "ytd-banner-promo-renderer", "ytd-promoted-sparkles-web-renderer",
    "ytd-promoted-video-renderer", "ytd-display-ad-renderer", "ytd-companion-slot-renderer", "ytd-action-companion-ad-renderer",
    "ytd-player-legacy-desktop-watch-ads-renderer", "ytd-engagement-panel-section-list-renderer[target-id='engagement-panel-ads']",
    "ytd-rich-item-renderer:has(ytd-ad-slot-renderer)", "ytd-rich-section-renderer:has(ytd-statement-banner-renderer)",
    "ytd-merch-shelf-renderer", "#player-ads", "#masthead-ad", ".ytp-ad-overlay-container", ".ytp-ad-overlay-slot",
    "ytm-promoted-sparkles-web-renderer", "ad-slot-renderer", "ytm-companion-ad-renderer",
  ].join(",") + "{display:none!important}";
  const addStyle = () => {
    const root = document.head || document.documentElement;
    if (!root) return false;
    const s = document.createElement("style");
    s.dataset.projectShield = "";
    s.textContent = css;
    root.appendChild(s);
    return true;
  };
  if (!addStyle()) document.addEventListener("DOMContentLoaded", addStyle, { once: true });

  // Fallback: if an ad plays anyway, jump to its end and press Skip.
  setInterval(() => {
    const player = document.querySelector(".html5-video-player.ad-showing");
    if (!player) return;
    const video = player.querySelector("video");
    if (video && Number.isFinite(video.duration) && video.duration > 0 && video.currentTime < video.duration - 0.1) video.currentTime = video.duration;
    const skip = player.querySelector(".ytp-skip-ad-button, .ytp-ad-skip-button, .ytp-ad-skip-button-modern");
    if (skip) skip.click();
  }, 300);
})();
