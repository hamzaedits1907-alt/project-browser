// Project Browser SponsorSkip: seeks past sponsor segments of the current YouTube video.
// The core looks the segments up (see sponsor.rs) and calls window.__pbSponsorSkip({videoId, segments}).
// No network access and no storage here; DOM is built with textContent only (YouTube enforces Trusted Types).
(() => {
  if (window.__pbSponsorSkip) return;
  const LABEL = { sponsor: "sponsor", selfpromo: "self-promotion", interaction: "subscribe reminder" };
  let data = { videoId: "", segments: [] };
  const allowed = new Set(); // segments the user chose to watch (Undo)
  let hooked = null;
  let toast = null;
  let toastTimer = 0;
  // timeupdate covers playback; seeked and playing cover a jump straight into a segment.
  const EVENTS = ["timeupdate", "seeked", "playing"];

  const currentId = () => {
    try {
      const u = new URL(location.href);
      if (u.pathname === "/watch") return u.searchParams.get("v") || "";
      const m = u.pathname.match(/^\/(shorts|embed|live)\/([\w-]{11})/);
      return m ? m[2] : "";
    } catch {
      return "";
    }
  };

  const showToast = (seg, video) => {
    if (!document.body) return;
    if (!toast) {
      toast = document.createElement("div");
      toast.setAttribute("role", "status");
      toast.style.cssText =
        "position:fixed;left:50%;bottom:84px;transform:translateX(-50%);z-index:2147483647;display:flex;gap:12px;" +
        "align-items:center;padding:9px 14px;border-radius:10px;background:rgba(15,17,23,.92);color:#fff;" +
        "font:500 13px/1.3 'Segoe UI',system-ui,sans-serif;box-shadow:0 6px 24px rgba(0,0,0,.35);pointer-events:auto";
      document.body.appendChild(toast);
    }
    toast.textContent = "";
    const text = document.createElement("span");
    text.textContent = "Skipped " + (LABEL[seg.category] || "segment");
    const undo = document.createElement("button");
    undo.textContent = "Undo";
    undo.style.cssText = "all:unset;cursor:pointer;color:#5ee7ff;font-weight:600";
    undo.onclick = () => {
      allowed.add(seg.start + ":" + seg.end);
      video.currentTime = seg.start;
      toast.style.display = "none";
    };
    toast.append(text, undo);
    toast.style.display = "flex";
    clearTimeout(toastTimer);
    toastTimer = setTimeout(() => toast && (toast.style.display = "none"), 4500);
  };

  const check = () => {
    const video = hooked;
    if (!video || !data.segments.length || currentId() !== data.videoId || video.seeking) return;
    const t = video.currentTime;
    for (const seg of data.segments) {
      if (t >= seg.start && t < seg.end - 0.35 && !allowed.has(seg.start + ":" + seg.end)) {
        video.currentTime = Math.min(seg.end, video.duration || seg.end);
        showToast(seg, video);
        return;
      }
    }
  };

  const hook = () => {
    const video = document.querySelector("video.html5-main-video") || document.querySelector("video");
    if (video && video !== hooked) {
      for (const ev of EVENTS) {
        if (hooked) hooked.removeEventListener(ev, check);
        video.addEventListener(ev, check);
      }
      hooked = video;
    }
  };

  window.__pbSponsorSkip = (next) => {
    if (!next || typeof next.videoId !== "string" || !Array.isArray(next.segments)) return;
    if (next.videoId !== data.videoId) allowed.clear();
    data = next;
    hook();
    check();
  };
  setInterval(hook, 1500);
})();
