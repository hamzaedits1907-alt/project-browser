//! Link Cleaner: removes click-tracking parameters from addresses before a page loads.
//!
//! Ad networks and newsletters append identifiers (`fbclid`, `gclid`, `utm_source`, …) to links so the destination
//! can tie the visit to a click. They are never needed to show the page. Top-level navigations whose address carries
//! one are cancelled and re-started without them (see `native::install_tab_inner`), and "Copy clean link" uses the same
//! rules. Only the query string is touched; the path, the fragment and every other parameter stay exactly as they were.

use url::Url;

/// Parameters removed on every site. All of them are click or campaign identifiers.
const GLOBAL: &[&str] = &[
    "fbclid",
    "gclid",
    "gclsrc",
    "dclid",
    "gbraid",
    "wbraid",
    "msclkid",
    "twclid",
    "ttclid",
    "li_fat_id",
    "igshid",
    "mc_cid",
    "mc_eid",
    "yclid",
    "_openstat",
    "vero_conv",
    "vero_id",
    "wickedid",
    "oly_anon_id",
    "oly_enc_id",
    "rb_clickid",
    "s_cid",
    "ml_subscriber",
    "ml_subscriber_hash",
    "_hsenc",
    "_hsmi",
    "__hssc",
    "__hstc",
    "__hsfp",
    "hsctatracking",
    "mkt_tok",
    "srsltid",
    "epik",
    "irclickid",
    "utm_source",
    "utm_medium",
    "utm_campaign",
    "utm_term",
    "utm_content",
    "utm_id",
    "utm_name",
    "utm_reader",
    "utm_social",
    "utm_social-type",
    "utm_brand",
];

/// Parameters that are tracking only on particular sites (elsewhere the same name can mean something real).
const SITE: &[(&str, &[&str])] = &[
    ("youtube.com", &["si", "pp", "feature"]),
    ("youtu.be", &["si", "feature"]),
    ("open.spotify.com", &["si", "context"]),
    ("instagram.com", &["igsh", "img_index"]),
    ("twitter.com", &["s", "t", "ref_src", "ref_url"]),
    ("x.com", &["s", "t", "ref_src", "ref_url"]),
    ("amazon.com", &["ref_", "pd_rd_r", "pd_rd_w", "pd_rd_wg", "pf_rd_p", "pf_rd_r", "pd_rd_i", "content-id"]),
    ("reddit.com", &["share_id", "rdt"]),
    ("linkedin.com", &["trk", "trackingid", "lipi", "licu", "refid"]),
    ("tiktok.com", &["is_from_webapp", "sender_device", "is_copy_url", "_r", "_t"]),
];

fn host_matches(host: &str, site: &str) -> bool {
    host == site || host.ends_with(&format!(".{site}"))
}

fn is_tracking(host: &str, name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    GLOBAL.contains(&lower.as_str()) || SITE.iter().any(|(site, names)| host_matches(host, site) && names.contains(&lower.as_str()))
}

/// The address without tracking parameters, or `None` when there is nothing to remove (or it is not a web address).
pub fn clean(address: &str) -> Option<String> {
    let mut url = Url::parse(address).ok()?;
    if !matches!(url.scheme(), "http" | "https") || url.query().is_none() {
        return None;
    }
    let host = url.host_str().unwrap_or("").trim_start_matches("www.").to_ascii_lowercase();
    // Work on the raw pairs so that kept parameters keep their exact encoding.
    let query = url.query().unwrap_or("").to_string();
    let pairs: Vec<&str> = query.split('&').collect();
    let kept: Vec<&str> = pairs
        .iter()
        .copied()
        .filter(|p| {
            let name = p.split('=').next().unwrap_or("");
            let decoded = url::form_urlencoded::parse(name.as_bytes()).next().map(|(k, _)| k.into_owned()).unwrap_or_default();
            !is_tracking(&host, &decoded)
        })
        .collect();
    if kept.len() == pairs.len() {
        return None;
    }
    let kept: Vec<&str> = kept.into_iter().filter(|p| !p.is_empty()).collect();
    url.set_query(if kept.is_empty() { None } else { Some(&kept.join("&")) });
    Some(url.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removes_click_ids_and_campaign_tags() {
        assert_eq!(clean("https://shop.example/item?id=7&fbclid=AbC").as_deref(), Some("https://shop.example/item?id=7"));
        assert_eq!(clean("https://news.example/a?utm_source=x&utm_medium=y").as_deref(), Some("https://news.example/a"));
        assert_eq!(clean("https://a.example/?GCLID=1&q=hello%20world").as_deref(), Some("https://a.example/?q=hello%20world"));
    }

    #[test]
    fn keeps_everything_else_exactly() {
        assert_eq!(clean("https://a.example/p?q=1&x=%E2%9C%93#frag-fbclid"), None, "nothing to remove");
        assert_eq!(clean("https://a.example/p?q=a+b&fbclid=1&page=2#top").as_deref(), Some("https://a.example/p?q=a+b&page=2#top"));
        assert_eq!(clean("https://a.example/"), None);
        assert_eq!(clean("ftp://a.example/?fbclid=1"), None);
        assert_eq!(clean("not a url"), None);
    }

    #[test]
    fn site_rules_apply_only_to_their_site() {
        assert_eq!(clean("https://www.youtube.com/watch?v=dQw4w9WgXcQ&si=abc").as_deref(), Some("https://www.youtube.com/watch?v=dQw4w9WgXcQ"));
        assert_eq!(clean("https://youtu.be/dQw4w9WgXcQ?si=abc").as_deref(), Some("https://youtu.be/dQw4w9WgXcQ"));
        // `si` and `t` mean something else on other sites.
        assert_eq!(clean("https://maps.example/?si=1&t=2"), None);
        assert_eq!(clean("https://x.com/user/status/1?s=20&t=abc").as_deref(), Some("https://x.com/user/status/1"));
    }
}
