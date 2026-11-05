//! Privacy and convenience features that work inside pages, switched in Settings:
//!
//! * **Fingerprinting protection** (like Brave's "farbling"): canvas read-backs, audio samples and WebGL's renderer
//!   strings get small changes that are stable for one site in one session but differ between sites and sessions, so
//!   sites cannot use them to recognise the browser. Pages look and sound the same.
//! * **Cookie-banner blocker**: answers cookie consent banners with "reject all" / "only necessary" (known consent
//!   tools first, then buttons with those words inside an element that is clearly a consent banner).
//! * **Do Not Track**: `DNT: 1` on every request and `navigator.doNotTrack === "1"`.
//! * **HTTPS-Only Mode**: plain `http://` addresses are loaded over `https://`; if a site has no secure version, a warning
//!   page lets the user continue to the insecure site (which then stays allowed for this session).
//! * **Mouse gestures**: hold the right mouse button and draw: left = back, right = forward, up-down = reload,
//!   down-right = close tab, down-left = reopen closed tab, up = new tab, up-left / up-right = previous / next tab.
//!
//! Scripts are added when a tab starts (so a change applies to tabs opened afterwards); headers and HTTPS-Only apply at
//! once. UI thread only, except the setters.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Mutex;

use webview2_com::Microsoft::Web::WebView2::Win32::*;
use webview2_com::*;
use windows::core::HSTRING;

use crate::controller::ControllerHandle;
use crate::shortcuts::Command;
use crate::tabs::TabId;

static FINGERPRINT: AtomicBool = AtomicBool::new(true);
static COOKIE_BANNERS: AtomicBool = AtomicBool::new(true);
static DNT: AtomicBool = AtomicBool::new(false);
static HTTPS_ONLY: AtomicBool = AtomicBool::new(false);
static GESTURES: AtomicBool = AtomicBool::new(true);
static VIDEO_KEYS: AtomicBool = AtomicBool::new(true);
/// Per-session secret for fingerprinting protection (different every start).
static SEED: AtomicU32 = AtomicU32::new(0);
/// Hosts whose HTTPS version failed this session: the user may continue to them over HTTP.
static HTTP_ALLOWED: Mutex<Option<HashSet<String>>> = Mutex::new(None);

pub struct Switches {
    pub fingerprint: bool,
    pub cookie_banners: bool,
    pub dnt: bool,
    pub https_only: bool,
    pub gestures: bool,
    pub video_keys: bool,
}

pub fn set(s: Switches) {
    FINGERPRINT.store(s.fingerprint, Ordering::Relaxed);
    COOKIE_BANNERS.store(s.cookie_banners, Ordering::Relaxed);
    DNT.store(s.dnt, Ordering::Relaxed);
    HTTPS_ONLY.store(s.https_only, Ordering::Relaxed);
    GESTURES.store(s.gestures, Ordering::Relaxed);
    VIDEO_KEYS.store(s.video_keys, Ordering::Relaxed);
    if SEED.load(Ordering::Relaxed) == 0 {
        let mut b = [0u8; 4];
        let seed = if crate::password_gen::random_fill(&mut b) { u32::from_le_bytes(b) } else { std::process::id().wrapping_mul(2654435761) };
        SEED.store(seed | 1, Ordering::Relaxed);
    }
}

pub fn do_not_track() -> bool {
    DNT.load(Ordering::Relaxed)
}

// ---- scripts ------------------------------------------------------------------------------------------------------

const FINGERPRINT_SCRIPT: &str = r#"(()=>{try{if(window.__pbFp)return;Object.defineProperty(window,'__pbFp',{value:1});
let h=__SEED__>>>0;for(const c of String(location.hostname)){h=Math.imul(h^c.charCodeAt(0),16777619)>>>0}
const site=h;const rng=(s)=>{let x=(s^site)>>>0||1;return()=>{x^=x<<13;x>>>=0;x^=x>>>17;x^=x<<5;x>>>=0;return x/4294967296}};
const farble=(d,w)=>{const r=rng(d.length*31+(w|0));const step=Math.max(4,((d.length/4)/64|0)*4);for(let i=(r()*step|0)&~3;i<d.length;i+=step){const k=i+(r()*3|0);d[k]=d[k]^1}};
const C2=CanvasRenderingContext2D.prototype,gid=C2.getImageData;
C2.getImageData=function(){const r=gid.apply(this,arguments);try{farble(r.data,r.width)}catch(e){}return r};
const H=HTMLCanvasElement.prototype,tdu=H.toDataURL,tbl=H.toBlob;
const copy=(c)=>{try{const ctx=c.getContext&&c.getContext('2d');if(!ctx||!c.width||!c.height)return null;const o=document.createElement('canvas');o.width=c.width;o.height=c.height;const oc=o.getContext('2d');oc.putImageData(C2.getImageData.call(ctx,0,0,c.width,c.height),0,0);return o}catch(e){return null}};
H.toDataURL=function(){const o=copy(this);return tdu.apply(o||this,arguments)};
H.toBlob=function(){const o=copy(this);return tbl.apply(o||this,arguments)};
if(window.AudioBuffer){const gcd=AudioBuffer.prototype.getChannelData;const done=new WeakSet();AudioBuffer.prototype.getChannelData=function(){const d=gcd.apply(this,arguments);try{if(!done.has(d)){done.add(d);const r=rng(d.length);for(let i=0;i<d.length;i+=97){d[i]=d[i]+(r()-0.5)*1e-7}}}catch(e){}return d}}
if(window.AnalyserNode){const gf=AnalyserNode.prototype.getFloatFrequencyData;AnalyserNode.prototype.getFloatFrequencyData=function(a){gf.call(this,a);try{const r=rng(a.length);for(let i=0;i<a.length;i+=7){a[i]=a[i]+(r()-0.5)*1e-4}}catch(e){}}}
for(const G of [window.WebGLRenderingContext,window.WebGL2RenderingContext]){if(!G)continue;const gp=G.prototype.getParameter;G.prototype.getParameter=function(p){if(p===0x9245)return'Google Inc.';if(p===0x9246)return'ANGLE (Generic GPU)';return gp.apply(this,arguments)}}
}catch(e){}})()"#;

const COOKIE_SCRIPT: &str = r#"(()=>{try{if(window.__pbCk)return;Object.defineProperty(window,'__pbCk',{value:1});
const sels=['#onetrust-reject-all-handler','#CybotCookiebotDialogBodyButtonDecline','#CybotCookiebotDialogBodyLevelButtonLevelOptinDeclineAll','#didomi-notice-disagree-button','.didomi-continue-without-agreeing','[data-testid="uc-deny-all-button"]','button.fc-cta-do-not-consent','.sp_choice_type_REJECT_ALL','#truste-consent-required','.osano-cm-denyAll','.cmpboxbtnno','#tarteaucitronAllDenied2','.iubenda-cs-reject-btn','#axeptio_btn_dismiss','.klaro .cm-btn-decline','#cookiescript_reject','.cky-btn-reject','.cmplz-deny','#wt-cli-reject-btn','.moove-gdpr-infobar-reject-btn','button[data-cookiebanner="reject_button"]','button[data-cookiefirst-action="reject"]','.cc-deny','.qc-cmp2-summary-buttons button[mode="secondary"]'];
const words=/^\s*(reject all|reject all cookies|reject|decline all|decline|deny|deny all|refuse all|only necessary|necessary only|only necessary cookies|use necessary cookies only|only essential|essential only|continue without accepting|alle ablehnen|ablehnen|nur notwendige|nur notwendige cookies|nur erforderliche|nur erforderliche cookies|tümünü reddet|reddet|tout refuser|refuser|continuer sans accepter|rechazar todo|rechazar|rifiuta tutto|rifiuta|alles weigeren|weigeren|odrzuć wszystkie|odrzuć)\s*$/i;
const scope=/cookie|consent|gdpr|privacy|cmp|didomi|onetrust|banner|notice/i;
const seen=(el)=>{const r=el.getBoundingClientRect();return r.width>0&&r.height>0};
function attempt(){for(const s of sels){const el=document.querySelector(s);if(el&&seen(el)){el.click();return true}}
for(const b of document.querySelectorAll('button,[role=button],a[role=button],input[type=button],input[type=submit]')){const t=(b.value||b.innerText||b.textContent||'').trim();if(!t||t.length>45||!words.test(t)||!seen(b))continue;
let ok=!!(b.form&&/consent/i.test(b.form.action||''));for(let p=b,i=0;!ok&&p&&i<10;i++,p=p.parentElement){const id=(p.id||'')+' '+(typeof p.className==='string'?p.className:'')+' '+((p.getAttribute&&(p.getAttribute('aria-label')||p.getAttribute('role')))||'');if(scope.test(id))ok=true}
if(ok){b.click();return true}}return false}
const start=()=>{if(attempt())return;let n=0;const o=new MutationObserver(()=>{if(++n>300||attempt())o.disconnect()});o.observe(document.documentElement,{childList:true,subtree:true,attributes:true,attributeFilter:['class','style']});setTimeout(()=>o.disconnect(),20000)};
if(document.readyState==='loading')document.addEventListener('DOMContentLoaded',start);else start();
}catch(e){}})()"#;

/// Video speed keys on every page with a video (like the popular "Video Speed Controller" extension): S slower,
/// D faster, R normal speed, Z back 10 s, X forward 10 s. Ignored while typing and when no video is playing or paused
/// on the page. The new speed shows briefly over the video.
const VIDEO_SCRIPT: &str = r#"(()=>{try{if(window.__pbV)return;Object.defineProperty(window,'__pbV',{value:1});
let tip=null,timer=0;
const show=(v,text)=>{try{if(!tip){tip=document.createElement('div');tip.setAttribute('style','position:fixed;z-index:2147483647;pointer-events:none;font:600 14px Segoe UI,system-ui;color:#fff;background:rgba(0,0,0,.72);border-radius:8px;padding:5px 10px;transition:opacity .2s')}
const r=v.getBoundingClientRect();tip.style.left=Math.max(8,r.left+12)+'px';tip.style.top=Math.max(8,r.top+12)+'px';tip.textContent=text;tip.style.opacity='1';(document.fullscreenElement||document.body||document.documentElement).appendChild(tip);clearTimeout(timer);timer=setTimeout(()=>{tip.style.opacity='0'},900)}catch(e){}};
const pick=()=>{const all=[...document.querySelectorAll('video')].filter(v=>v.readyState>0);return all.find(v=>!v.paused)||all.sort((a,b)=>b.clientWidth*b.clientHeight-a.clientWidth*a.clientHeight)[0]};
addEventListener('keydown',e=>{if(e.ctrlKey||e.altKey||e.metaKey||e.defaultPrevented)return;const t=e.target;if(t&&(t.isContentEditable||/^(INPUT|TEXTAREA|SELECT)$/.test(t.tagName)))return;
const k=e.key.toLowerCase();if(!'sdrzx'.includes(k)||k.length!==1)return;const v=pick();if(!v)return;
if(k==='s')v.playbackRate=Math.max(0.1,Math.round((v.playbackRate-0.1)*10)/10);else if(k==='d')v.playbackRate=Math.min(16,Math.round((v.playbackRate+0.1)*10)/10);else if(k==='r')v.playbackRate=1;
else if(k==='z')v.currentTime=Math.max(0,v.currentTime-10);else if(k==='x')v.currentTime=Math.min(v.duration||1e9,v.currentTime+10);
show(v,k==='z'?'−10 s':k==='x'?'+10 s':v.playbackRate.toFixed(1)+'×');e.preventDefault();e.stopImmediatePropagation()},true);
}catch(e){}})()"#;

const DNT_SCRIPT: &str =
    "try{Object.defineProperty(Navigator.prototype,'doNotTrack',{get:function(){return '1'},configurable:true,enumerable:true})}catch(e){}";

const GESTURE_SCRIPT: &str = r#"(()=>{try{if(window.top!==window||window.__pbG)return;Object.defineProperty(window,'__pbG',{value:1});
const NAMES={L:'Back',R:'Forward',UD:'Reload',DR:'Close tab',DL:'Reopen closed tab',U:'New tab',UL:'Previous tab',UR:'Next tab'};
let down=false,path='',lx=0,ly=0,pts=[],svg=null,line=null,label=null,suppress=0;
const ns='http://www.w3.org/2000/svg';
const draw=()=>{if(!svg){svg=document.createElementNS(ns,'svg');svg.setAttribute('style','position:fixed;inset:0;width:100vw;height:100vh;z-index:2147483647;pointer-events:none');line=document.createElementNS(ns,'polyline');line.setAttribute('style','fill:none;stroke:#6ae3ff;stroke-width:4;stroke-linecap:round;stroke-linejoin:round;opacity:.85');svg.appendChild(line);label=document.createElement('div');label.setAttribute('style','position:fixed;z-index:2147483647;pointer-events:none;font:600 13px Segoe UI,system-ui;color:#06121a;background:#6ae3ff;border-radius:8px;padding:4px 10px;box-shadow:0 4px 18px rgba(0,0,0,.35)');(document.body||document.documentElement).append(svg,label)}
line.setAttribute('points',pts.map(p=>p.join(',')).join(' '));const n=NAMES[path];label.textContent=n||'…';const p=pts[pts.length-1];label.style.left=(p[0]+14)+'px';label.style.top=(p[1]+14)+'px'};
const clear=()=>{if(svg){svg.remove();label.remove();svg=null}};
addEventListener('mousedown',e=>{if(e.button!==2)return;down=true;path='';lx=e.clientX;ly=e.clientY;pts=[[lx,ly]]},true);
addEventListener('mousemove',e=>{if(!down)return;if(!(e.buttons&2)){down=false;clear();return}const dx=e.clientX-lx,dy=e.clientY-ly;pts.push([e.clientX,e.clientY]);if(pts.length>400)pts.shift();if(Math.abs(dx)<28&&Math.abs(dy)<28){if(svg)draw();return}
const d=Math.abs(dx)>Math.abs(dy)?(dx>0?'R':'L'):(dy>0?'D':'U');if(path[path.length-1]!==d&&path.length<3)path+=d;lx=e.clientX;ly=e.clientY;draw()},true);
addEventListener('mouseup',e=>{if(e.button!==2||!down)return;down=false;const g=path;clear();if(g){suppress=Date.now();if(NAMES[g]){try{chrome.webview.postMessage({pbGesture:g})}catch(_){}}}},true);
addEventListener('contextmenu',e=>{if(suppress&&Date.now()-suppress<500){suppress=0;e.preventDefault();e.stopImmediatePropagation()}},true);
addEventListener('blur',()=>{down=false;clear()});
}catch(e){}})()"#;

fn add_script(webview: &ICoreWebView2, script: &str) {
    unsafe {
        let _ = webview.AddScriptToExecuteOnDocumentCreated(
            &HSTRING::from(script),
            &AddScriptToExecuteOnDocumentCreatedCompletedHandler::create(Box::new(|_, _| Ok(()))),
        );
    }
}

/// The browser command a finished gesture stands for.
pub fn gesture_command(path: &str) -> Option<Command> {
    Some(match path {
        "L" => Command::Back,
        "R" => Command::Forward,
        "UD" => Command::Reload,
        "DR" => Command::CloseTab,
        "DL" => Command::ReopenTab,
        "U" => Command::NewTab,
        "UL" => Command::PrevTab,
        "UR" => Command::NextTab,
        _ => return None,
    })
}

thread_local! {
    static LAST_GESTURE: RefCell<HashMap<TabId, std::time::Instant>> = RefCell::new(HashMap::new());
}

/// Adds the page scripts the settings ask for to a new tab, and listens for finished mouse gestures.
pub fn install(webview: &ICoreWebView2, id: TabId, ctl: &ControllerHandle) {
    if FINGERPRINT.load(Ordering::Relaxed) {
        add_script(webview, &FINGERPRINT_SCRIPT.replace("__SEED__", &SEED.load(Ordering::Relaxed).to_string()));
    }
    if COOKIE_BANNERS.load(Ordering::Relaxed) {
        add_script(webview, COOKIE_SCRIPT);
    }
    if DNT.load(Ordering::Relaxed) {
        add_script(webview, DNT_SCRIPT);
    }
    if VIDEO_KEYS.load(Ordering::Relaxed) {
        add_script(webview, VIDEO_SCRIPT);
    }
    // Always added (it does nothing until the right button is dragged); the setting is checked when a gesture ends,
    // so switching gestures off applies to open tabs at once.
    add_script(webview, GESTURE_SCRIPT);
    let c = ctl.clone();
    let mut token = 0i64;
    unsafe {
        let _ = webview.add_WebMessageReceived(
            &WebMessageReceivedEventHandler::create(Box::new(move |_, args| {
                let Some(args) = args else { return Ok(()) };
                let json = crate::native::pwstr(|p| args.WebMessageAsJson(p));
                let Some(path) = serde_json::from_str::<serde_json::Value>(&json)
                    .ok()
                    .and_then(|v| v.get("pbGesture").and_then(|g| g.as_str().map(str::to_string)))
                else {
                    return Ok(());
                };
                if !GESTURES.load(Ordering::Relaxed) {
                    return Ok(());
                }
                // A page could post this message itself: only accept it right after the right button was released
                // (it is up now) and at most a few times a second.
                if right_button_down() {
                    return Ok(());
                }
                let now = std::time::Instant::now();
                let recent = LAST_GESTURE.with(|l| l.borrow().get(&id).is_some_and(|t| now.duration_since(*t).as_millis() < 350));
                if recent {
                    return Ok(());
                }
                LAST_GESTURE.with(|l| l.borrow_mut().insert(id, now));
                if let Some(cmd) = gesture_command(&path) {
                    log::info!("tab {id}: mouse gesture {path}");
                    c.post_tab(id, move |ctl| ctl.run_command(cmd));
                }
                Ok(())
            })),
            &mut token,
        );
    }
}

fn right_button_down() -> bool {
    use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_RBUTTON};
    unsafe { (GetAsyncKeyState(VK_RBUTTON.0 as i32) as u16 & 0x8000) != 0 }
}

/// Adds the request headers the settings ask for (Do Not Track).
pub fn add_request_headers(headers: &ICoreWebView2HttpRequestHeaders) {
    if DNT.load(Ordering::Relaxed) {
        unsafe {
            let _ = headers.SetHeader(windows::core::w!("DNT"), windows::core::w!("1"));
        }
    }
}

// ---- automatic picture-in-picture --------------------------------------------------------------------------------

/// Picks the largest playing video with sound and shows it picture-in-picture. Run with a user gesture (DevTools'
/// `Runtime.evaluate`), as leaving the tab is one.
const PIP_ENTER: &str = "(()=>{const v=[...document.querySelectorAll('video')].filter(v=>!v.paused&&!v.muted&&v.readyState>1&&!v.disablePictureInPicture).sort((a,b)=>b.clientWidth*b.clientHeight-a.clientWidth*a.clientHeight)[0];if(!v||document.pictureInPictureElement)return false;return v.requestPictureInPicture().then(()=>true,()=>false)})()";
const PIP_LEAVE: &str = "(()=>{if(document.pictureInPictureElement)return document.exitPictureInPicture().then(()=>true,()=>false);return false})()";

thread_local! {
    /// Tabs whose video went to picture-in-picture automatically (it comes back when the tab is shown again).
    static AUTO_PIP: RefCell<HashSet<TabId>> = RefCell::new(HashSet::new());
}

fn evaluate_with_gesture(id: TabId, script: &str, then: impl FnOnce(bool) + 'static) {
    let Some(wv) = crate::native::tab_webview(id) else { return };
    let params = serde_json::json!({ "expression": script, "userGesture": true, "awaitPromise": true, "returnByValue": true }).to_string();
    unsafe {
        let _ = wv.CallDevToolsProtocolMethod(
            windows::core::w!("Runtime.evaluate"),
            &HSTRING::from(params),
            &CallDevToolsProtocolMethodCompletedHandler::create(Box::new(move |res, json| {
                let ok = res.is_ok()
                    && serde_json::from_str::<serde_json::Value>(&json)
                        .ok()
                        .and_then(|v| v.pointer("/result/value").and_then(|b| b.as_bool()))
                        .unwrap_or(false);
                then(ok);
                Ok(())
            })),
        );
    }
}

/// Runs `script` in tab `id`'s page, waits for the promise it returns, and replies with its JSON value. UI thread.
pub fn evaluate_await(id: TabId, script: &str, reply: tokio::sync::oneshot::Sender<Option<serde_json::Value>>) {
    let Some(wv) = crate::native::tab_webview(id) else {
        let _ = reply.send(None);
        return;
    };
    let params = serde_json::json!({ "expression": script, "awaitPromise": true, "returnByValue": true }).to_string();
    unsafe {
        let _ = wv.CallDevToolsProtocolMethod(
            windows::core::w!("Runtime.evaluate"),
            &HSTRING::from(params),
            &CallDevToolsProtocolMethodCompletedHandler::create(Box::new(move |res, json| {
                let value =
                    res.ok().and_then(|_| serde_json::from_str::<serde_json::Value>(&json).ok()).and_then(|v| v.pointer("/result/value").cloned());
                let _ = reply.send(value);
                Ok(())
            })),
        );
    }
}

/// Tab `id` (playing sound) was left: its video goes picture-in-picture. UI thread.
pub fn auto_pip_enter(id: TabId) {
    evaluate_with_gesture(id, PIP_ENTER, move |ok| {
        if ok {
            log::info!("tab {id}: video moved to picture-in-picture (tab left)");
            AUTO_PIP.with(|s| s.borrow_mut().insert(id));
        }
    });
}

/// Tab `id` is shown again: a video it sent to picture-in-picture comes back into the page. UI thread.
pub fn auto_pip_leave(id: TabId) {
    if AUTO_PIP.with(|s| s.borrow_mut().remove(&id)) {
        evaluate_with_gesture(id, PIP_LEAVE, |_| {});
    }
}

// ---- site controls ------------------------------------------------------------------------------------------------

/// Per-site choices from the site information panel (site -> control), set by the controller.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SiteControl {
    /// JavaScript is switched off on this site.
    pub block_scripts: bool,
    /// Tabs on this site start muted.
    pub muted: bool,
}

static SITE_CONTROLS: Mutex<Option<HashMap<String, SiteControl>>> = Mutex::new(None);

pub fn set_site_controls(map: HashMap<String, SiteControl>) {
    if let Ok(mut m) = SITE_CONTROLS.lock() {
        *m = Some(map);
    }
}

/// The site key used for site controls: the host without "www.".
pub fn site_of(url: &str) -> Option<String> {
    let u = url::Url::parse(url).ok()?;
    if u.scheme() != "http" && u.scheme() != "https" {
        return None;
    }
    Some(u.host_str()?.trim_start_matches("www.").to_ascii_lowercase())
}

/// The control for `url`'s site (also matching parent domains: a choice for example.com covers www.example.com).
pub fn control_for(url: &str) -> SiteControl {
    let Some(host) = site_of(url) else { return SiteControl::default() };
    let Ok(m) = SITE_CONTROLS.lock() else { return SiteControl::default() };
    let Some(map) = m.as_ref() else { return SiteControl::default() };
    let mut h = host.as_str();
    loop {
        if let Some(c) = map.get(h) {
            return c.clone();
        }
        match h.find('.') {
            Some(i) if h[i + 1..].contains('.') => h = &h[i + 1..],
            _ => return SiteControl::default(),
        }
    }
}

/// Top-level navigation of tab `id` to `uri` is starting: JavaScript is switched on or off for the site. Returns true
/// when the setting had to change; the navigation is then started again (WebView2 applies the setting to the next
/// navigation), so the page loads with the right setting.
pub fn sync_scripts(webview: &ICoreWebView2, uri: &str) -> bool {
    let want = !control_for(uri).block_scripts;
    unsafe {
        let Ok(settings) = webview.Settings() else { return false };
        let mut on = windows::core::BOOL::default();
        if settings.IsScriptEnabled(&mut on).is_err() || on.as_bool() == want {
            return false;
        }
        settings.SetIsScriptEnabled(want).is_ok()
    }
}

// ---- HTTPS-Only Mode ----------------------------------------------------------------------------------------------

/// Hosts that are never upgraded: this computer and private networks (they rarely have certificates).
fn is_local(host: &str) -> bool {
    let h = host.trim_start_matches('[').trim_end_matches(']');
    if h == "localhost"
        || h.ends_with(".localhost")
        || h.ends_with(".local")
        || h.ends_with(".test")
        || h.ends_with(".internal")
        || h.ends_with(".lan")
        || h.ends_with(".home.arpa")
    {
        return true;
    }
    if let Ok(ip) = h.parse::<std::net::IpAddr>() {
        return match ip {
            std::net::IpAddr::V4(v) => v.is_loopback() || v.is_private() || v.is_link_local(),
            std::net::IpAddr::V6(v) => v.is_loopback() || (v.segments()[0] & 0xfe00) == 0xfc00 || (v.segments()[0] & 0xffc0) == 0xfe80,
        };
    }
    // A single-label name (an intranet host).
    !h.contains('.')
}

fn http_allowed(host: &str) -> bool {
    HTTP_ALLOWED.lock().ok().is_some_and(|s| s.as_ref().is_some_and(|s| s.contains(host)))
}

/// The user chose to continue to `host` over HTTP (from the warning page): allowed for the rest of the session.
pub fn allow_http(host: &str) {
    if let Ok(mut s) = HTTP_ALLOWED.lock() {
        s.get_or_insert_with(HashSet::new).insert(host.to_ascii_lowercase());
    }
}

/// The `https://` address to load instead of `uri`, when HTTPS-Only Mode is on and `uri` should be upgraded.
pub fn https_upgrade(uri: &str) -> Option<String> {
    if !HTTPS_ONLY.load(Ordering::Relaxed) {
        return None;
    }
    upgrade_target(uri, http_allowed)
}

/// Pure part of `https_upgrade` (tested).
pub fn upgrade_target(uri: &str, allowed: impl Fn(&str) -> bool) -> Option<String> {
    let mut u = url::Url::parse(uri).ok()?;
    if u.scheme() != "http" {
        return None;
    }
    let host = u.host_str()?.to_ascii_lowercase();
    if is_local(&host) || allowed(&host) {
        return None;
    }
    // An explicit non-standard port is kept as it is (it usually only speaks HTTP).
    if u.port().is_some() {
        return None;
    }
    u.set_scheme("https").ok()?;
    Some(u.to_string())
}

/// Upgraded navigations waiting for their outcome: tab -> (the https address, the http address the user asked for).
static UPGRADES: Mutex<Option<HashMap<TabId, (String, String)>>> = Mutex::new(None);

pub fn note_upgrade(id: TabId, secure: &str, original: &str) {
    if let Ok(mut m) = UPGRADES.lock() {
        m.get_or_insert_with(HashMap::new).insert(id, (secure.to_string(), original.to_string()));
    }
}

/// Tab `id` failed to load `url`. If that was an HTTPS-Only upgrade, the insecure address the user asked for is
/// returned and its host is allowed over HTTP from now on (so "Continue" on the warning page works).
pub fn failed_upgrade(id: TabId, url: &str) -> Option<String> {
    let mut m = UPGRADES.lock().ok()?;
    let (secure, original) = m.as_mut()?.remove(&id)?;
    let same = secure.trim_end_matches('/') == url.trim_end_matches('/');
    if !same {
        return None;
    }
    if let Some(host) = url::Url::parse(&original).ok().and_then(|u| u.host_str().map(str::to_string)) {
        allow_http(&host);
    }
    Some(original)
}

/// Tab `id` loaded something: a pending upgrade (if any) worked.
pub fn upgrade_done(id: TabId) {
    if let Ok(mut m) = UPGRADES.lock() {
        if let Some(m) = m.as_mut() {
            m.remove(&id);
        }
    }
}

/// Warning page shown when the secure version of an upgraded address did not load.
pub fn insecure_page(http_url: &str) -> String {
    crate::error_page::render_https_only(http_url)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn http_addresses_are_upgraded_except_local_ones_and_allowed_hosts() {
        let none = |_: &str| false;
        assert_eq!(upgrade_target("http://example.com/a?b=1", none).as_deref(), Some("https://example.com/a?b=1"));
        assert_eq!(upgrade_target("https://example.com/", none), None);
        assert_eq!(upgrade_target("http://localhost:3000/", none), None);
        assert_eq!(upgrade_target("http://192.168.1.1/", none), None);
        assert_eq!(upgrade_target("http://10.0.0.2/", none), None);
        assert_eq!(upgrade_target("http://router/", none), None);
        assert_eq!(upgrade_target("http://printer.local/", none), None);
        assert_eq!(upgrade_target("http://example.com:8080/", none), None);
        assert_eq!(upgrade_target("http://insecure.example/", |h| h == "insecure.example"), None);
    }

    #[test]
    fn gestures_map_to_browser_commands() {
        assert_eq!(gesture_command("L"), Some(Command::Back));
        assert_eq!(gesture_command("R"), Some(Command::Forward));
        assert_eq!(gesture_command("UD"), Some(Command::Reload));
        assert_eq!(gesture_command("DR"), Some(Command::CloseTab));
        assert_eq!(gesture_command("LRL"), None);
    }

    #[test]
    fn scripts_are_single_expressions_with_the_seed_slot() {
        assert!(FINGERPRINT_SCRIPT.contains("__SEED__"));
        for s in [FINGERPRINT_SCRIPT, COOKIE_SCRIPT, GESTURE_SCRIPT, VIDEO_SCRIPT] {
            assert!(s.starts_with("(()=>{") && s.ends_with("})()"));
        }
    }
}
