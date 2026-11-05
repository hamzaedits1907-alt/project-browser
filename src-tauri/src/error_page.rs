//! Branded network error page, loaded into the tab with NavigateToString.
//! It is a self-contained document with no bridge back into the browser; the
//! retry button simply navigates to the failed URL.

fn escape_html(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

/// A JS string literal that is safe to embed inside a <script> element.
fn js_string(s: &str) -> String {
    serde_json::to_string(s).unwrap_or_else(|_| "\"\"".into()).replace("</", "<\\/").replace('\u{2028}', "\\u2028").replace('\u{2029}', "\\u2029")
}

pub fn render(url: &str, code: &str, description: &str) -> String {
    let host = url::Url::parse(url).ok().and_then(|u| u.host_str().map(str::to_string)).unwrap_or_else(|| url.to_string());
    let title = match code {
        "INTERNET_DISCONNECTED" => "You're offline",
        "NAME_NOT_RESOLVED" => "Site can't be found",
        "TIMED_OUT" => "Connection timed out",
        "CERT_ERROR" => "Connection isn't secure",
        _ => "Page didn't load",
    };
    format!(
        r#"<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="color-scheme" content="dark">
<title>{title_e}</title>
<style>
:root{{color-scheme:dark}}
body{{margin:0;min-height:100vh;display:grid;place-items:center;background:#0b0d12;color:#e6e8ee;font:15px/1.5 "Segoe UI Variable Text","Segoe UI",system-ui,sans-serif}}
main{{max-width:520px;padding:32px}}
.code{{display:inline-block;font:600 11px/1 ui-monospace,Consolas,monospace;letter-spacing:.08em;color:#6ae3ff;border:1px solid rgba(106,227,255,.35);border-radius:6px;padding:6px 8px;margin-bottom:20px}}
h1{{font-size:26px;font-weight:650;letter-spacing:-.01em;margin:0 0 8px}}
p{{color:#9aa3b2;margin:0 0 6px}}
.host{{color:#e6e8ee;font-weight:600;word-break:break-all}}
button{{margin-top:22px;background:#6ae3ff;color:#06121a;border:0;border-radius:8px;padding:10px 18px;font:600 14px "Segoe UI",system-ui;cursor:pointer}}
button:hover{{background:#8fecff}} button:focus-visible{{outline:2px solid #fff;outline-offset:2px}}
</style></head><body><main>
<div class="code">{code_e}</div>
<h1>{title_e}</h1>
<p><span class="host">{host_e}</span></p>
<p>{desc_e}</p>
<button id="retry" autofocus>Try again</button>
</main><script>document.getElementById("retry").onclick=function(){{location.replace({url_js})}};</script></body></html>"#,
        title_e = escape_html(title),
        code_e = escape_html(code),
        host_e = escape_html(&host),
        desc_e = escape_html(description),
        url_js = js_string(url),
    )
}

/// HTTPS-Only Mode: the secure version of `http_url` did not load. The button continues to the insecure address
/// (the browser allows that host over HTTP for the rest of the session before this page is shown).
pub fn render_https_only(http_url: &str) -> String {
    let host = url::Url::parse(http_url).ok().and_then(|u| u.host_str().map(str::to_string)).unwrap_or_else(|| http_url.to_string());
    format!(
        r#"<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="color-scheme" content="dark">
<title>Connection is not secure</title>
<style>
:root{{color-scheme:dark}}
body{{margin:0;min-height:100vh;display:grid;place-items:center;background:#0b0d12;color:#e6e8ee;font:15px/1.5 "Segoe UI Variable Text","Segoe UI",system-ui,sans-serif}}
main{{max-width:540px;padding:32px}}
.code{{display:inline-block;font:600 11px/1 ui-monospace,Consolas,monospace;letter-spacing:.08em;color:#ffcf5a;border:1px solid rgba(255,207,90,.4);border-radius:6px;padding:6px 8px;margin-bottom:20px}}
h1{{font-size:26px;font-weight:650;margin:0 0 8px}} p{{color:#9aa3b2;margin:0 0 6px}} .host{{color:#e6e8ee;font-weight:600;word-break:break-all}}
.row{{display:flex;gap:10px;margin-top:22px;flex-wrap:wrap}}
button{{border:0;border-radius:8px;padding:10px 18px;font:600 14px "Segoe UI",system-ui;cursor:pointer}}
#back{{background:#6ae3ff;color:#06121a}} #go{{background:transparent;color:#e6e8ee;border:1px solid rgba(255,255,255,.18)}}
button:focus-visible{{outline:2px solid #fff;outline-offset:2px}}
</style></head><body><main>
<div class="code">HTTPS-ONLY MODE</div>
<h1>This site doesn't offer a secure connection</h1>
<p><span class="host">{host_e}</span></p>
<p>HTTPS-Only Mode tried the secure version and it didn't load. On the insecure version, people on your network can see and change what you send and receive.</p>
<div class="row"><button id="back" autofocus>Go back</button><button id="go">Continue to the site (not secure)</button></div>
</main><script>document.getElementById("back").onclick=function(){{history.length>1?history.back():location.replace("about:blank")}};document.getElementById("go").onclick=function(){{location.replace({url_js})}};</script></body></html>"#,
        host_e = escape_html(&host),
        url_js = js_string(http_url),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_hostile_urls() {
        let evil = "https://x.test/\"><script>alert(1)</script>";
        let html = render(evil, "FAILED", "<b>bad</b>");
        assert!(!html.contains("<script>alert(1)</script>"));
        assert!(!html.contains("<b>bad</b>"));
        assert!(html.contains("&lt;b&gt;bad&lt;/b&gt;"));
        // The retry script cannot be broken out of.
        assert!(html.contains(r#"<\/script>"#));
    }

    #[test]
    fn the_https_only_page_escapes_the_address_and_offers_the_insecure_one() {
        let html = render_https_only("http://x.test/\"><script>alert(1)</script>");
        assert!(!html.contains("<script>alert(1)</script>"));
        assert!(html.contains(r#"<\/script>"#));
        assert!(html.contains("Continue to the site"));
    }

    #[test]
    fn friendly_titles() {
        assert!(render("https://a.b/", "INTERNET_DISCONNECTED", "x").contains("You&#39;re offline"), "apostrophes are HTML-escaped");
        assert!(render("https://a.b/", "NAME_NOT_RESOLVED", "x").contains("a.b"));
    }
}
