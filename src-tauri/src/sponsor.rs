//! SponsorSkip: jumps over sponsor reads and self-promotion in YouTube videos, using the community segment database
//! of the SponsorBlock project (sponsor.ajay.app, segments submitted and voted on by viewers).
//!
//! Privacy: the video id is never sent. The browser sends the first four hex digits of the id's SHA-256 hash and gets
//! back the segments of every video in that bucket (thousands of videos share one prefix), then picks its own video
//! locally. No cookies, no account, nothing stored. Off in private tabs.
//!
//! The lookup runs in the core (`Controller::sponsor_lookup`); the page gets the segments through a small script
//! (`sponsor.js`) that seeks past them while the video plays and shows a short "Skipped" notice with an Undo link.

use serde::{Deserialize, Serialize};

pub const API: &str = "https://sponsor.ajay.app/api/skipSegments/";
/// Categories skipped: paid promotion, unpaid self-promotion, and "like and subscribe" reminders.
pub const CATEGORIES: &[&str] = &["sponsor", "selfpromo", "interaction"];
pub const SCRIPT: &str = include_str!("sponsor.js");

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Segment {
    pub start: f64,
    pub end: f64,
    pub category: String,
}

#[derive(Deserialize)]
struct ApiVideo {
    #[serde(rename = "videoID")]
    video_id: String,
    segments: Vec<ApiSegment>,
}

#[derive(Deserialize)]
struct ApiSegment {
    category: String,
    segment: (f64, f64),
    #[serde(default, rename = "actionType")]
    action_type: Option<String>,
}

/// The YouTube video id in a watch, shorts, embed or youtu.be address.
pub fn video_id(address: &str) -> Option<String> {
    let url = url::Url::parse(address).ok()?;
    let host = url.host_str()?.trim_start_matches("www.").trim_start_matches("m.");
    let id = match host {
        "youtube.com" | "music.youtube.com" => {
            if url.path() == "/watch" {
                url.query_pairs().find(|(k, _)| k == "v").map(|(_, v)| v.into_owned())
            } else {
                let mut parts = url.path_segments()?;
                match parts.next() {
                    Some("shorts") | Some("embed") | Some("live") => parts.next().map(str::to_string),
                    _ => None,
                }
            }
        }
        "youtu.be" => url.path_segments()?.next().map(str::to_string),
        _ => None,
    }?;
    let valid = id.len() == 11 && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
    valid.then_some(id)
}

/// The lookup address for a video: only a 4-digit prefix of the id's hash leaves the browser.
pub fn lookup_url(video_id: &str) -> String {
    let prefix: String = hex(&sha256(video_id.as_bytes())).chars().take(4).collect();
    let cats = CATEGORIES.iter().map(|c| format!("\"{c}\"")).collect::<Vec<_>>().join(",");
    let cats = url::form_urlencoded::byte_serialize(format!("[{cats}]").as_bytes()).collect::<String>();
    format!("{API}{prefix}?categories={cats}")
}

/// Picks this video's skippable segments out of the bucket the server returned.
pub fn parse(body: &str, video_id: &str) -> Vec<Segment> {
    let Ok(videos) = serde_json::from_str::<Vec<ApiVideo>>(body) else { return Vec::new() };
    let mut out: Vec<Segment> = videos
        .into_iter()
        .filter(|v| v.video_id == video_id)
        .flat_map(|v| v.segments)
        .filter(|s| s.action_type.as_deref().unwrap_or("skip") == "skip" && CATEGORIES.contains(&s.category.as_str()))
        .filter(|s| s.segment.0.is_finite() && s.segment.1.is_finite() && s.segment.1 - s.segment.0 >= 1.0 && s.segment.0 >= 0.0)
        .map(|s| Segment { start: s.segment.0, end: s.segment.1, category: s.category })
        .collect();
    out.sort_by(|a, b| a.start.total_cmp(&b.start));
    out
}

/// The script that hands the segments to the page's skipper (installing it first if needed).
pub fn inject_script(video_id: &str, segments: &[Segment]) -> String {
    let data = serde_json::json!({ "videoId": video_id, "segments": segments });
    format!("{SCRIPT}\n;window.__pbSponsorSkip && window.__pbSponsorSkip({data});")
}

// ---- SHA-256 (FIPS 180-4), so no extra crate is needed for one hash per video ----------------------------------

const K: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3,
    0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13,
    0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3, 0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208,
    0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

pub fn sha256(data: &[u8]) -> [u8; 32] {
    let mut h: [u32; 8] = [0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19];
    let mut msg = data.to_vec();
    let bit_len = (data.len() as u64).wrapping_mul(8);
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bit_len.to_be_bytes());
    for chunk in msg.chunks(64) {
        let mut w = [0u32; 64];
        for (i, word) in chunk.chunks(4).enumerate() {
            w[i] = u32::from_be_bytes([word[0], word[1], word[2], word[3]]);
        }
        for i in 16..64 {
            let (a, b) = (w[i - 15], w[i - 2]);
            let s0 = a.rotate_right(7) ^ a.rotate_right(18) ^ (a >> 3);
            let s1 = b.rotate_right(17) ^ b.rotate_right(19) ^ (b >> 10);
            let next = w[i - 16].wrapping_add(s0).wrapping_add(w[i - 7]).wrapping_add(s1);
            w[i] = next;
        }
        let mut v = h;
        for (k, wi) in K.iter().zip(w.iter()) {
            let s1 = v[4].rotate_right(6) ^ v[4].rotate_right(11) ^ v[4].rotate_right(25);
            let ch = (v[4] & v[5]) ^ (!v[4] & v[6]);
            let t1 = v[7].wrapping_add(s1).wrapping_add(ch).wrapping_add(*k).wrapping_add(*wi);
            let s0 = v[0].rotate_right(2) ^ v[0].rotate_right(13) ^ v[0].rotate_right(22);
            let maj = (v[0] & v[1]) ^ (v[0] & v[2]) ^ (v[1] & v[2]);
            let t2 = s0.wrapping_add(maj);
            v = [t1.wrapping_add(t2), v[0], v[1], v[2], v[3].wrapping_add(t1), v[4], v[5], v[6]];
        }
        for (a, b) in h.iter_mut().zip(v) {
            *a = a.wrapping_add(b);
        }
    }
    let mut out = [0u8; 32];
    for (i, word) in h.iter().enumerate() {
        out[i * 4..i * 4 + 4].copy_from_slice(&word.to_be_bytes());
    }
    out
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_matches_the_standard_vectors() {
        assert_eq!(hex(&sha256(b"")), "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
        assert_eq!(hex(&sha256(b"abc")), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
        assert_eq!(
            hex(&sha256(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq")),
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
        );
    }

    #[test]
    fn finds_the_video_id_in_every_kind_of_address() {
        let id = Some("dQw4w9WgXcQ".to_string());
        assert_eq!(video_id("https://www.youtube.com/watch?v=dQw4w9WgXcQ&t=42"), id);
        assert_eq!(video_id("https://m.youtube.com/watch?feature=x&v=dQw4w9WgXcQ"), id);
        assert_eq!(video_id("https://youtu.be/dQw4w9WgXcQ?si=1"), id);
        assert_eq!(video_id("https://www.youtube.com/shorts/dQw4w9WgXcQ"), id);
        assert_eq!(video_id("https://www.youtube.com/"), None);
        assert_eq!(video_id("https://www.youtube.com/watch?v=short"), None);
        assert_eq!(video_id("https://notyoutube.com/watch?v=dQw4w9WgXcQ"), None);
    }

    #[test]
    fn only_a_hash_prefix_is_sent() {
        let url = lookup_url("dQw4w9WgXcQ");
        assert!(!url.contains("dQw4w9WgXcQ"), "the id itself never leaves the browser");
        let prefix = &url[API.len()..API.len() + 4];
        assert_eq!(prefix, &hex(&sha256(b"dQw4w9WgXcQ"))[..4]);
        assert!(url.contains("categories=%5B%22sponsor%22%2C%22selfpromo%22%2C%22interaction%22%5D"));
    }

    #[test]
    fn keeps_only_this_videos_skip_segments() {
        let body = r#"[
          {"videoID":"other000000","segments":[{"category":"sponsor","segment":[1,50],"actionType":"skip"}]},
          {"videoID":"dQw4w9WgXcQ","segments":[
            {"category":"selfpromo","segment":[300.5,320],"actionType":"skip"},
            {"category":"sponsor","segment":[10,70.2],"actionType":"skip"},
            {"category":"sponsor","segment":[100,100.4],"actionType":"skip"},
            {"category":"sponsor","segment":[200,260],"actionType":"mute"},
            {"category":"music_offtopic","segment":[0,30],"actionType":"skip"}
          ]}
        ]"#;
        let segs = parse(body, "dQw4w9WgXcQ");
        assert_eq!(segs.len(), 2);
        assert_eq!(segs[0], Segment { start: 10.0, end: 70.2, category: "sponsor".into() });
        assert_eq!(segs[1].category, "selfpromo");
        assert!(parse("not json", "dQw4w9WgXcQ").is_empty());
        assert!(parse("[]", "dQw4w9WgXcQ").is_empty());
    }

    #[test]
    fn the_injected_script_carries_the_segments() {
        let s = inject_script("dQw4w9WgXcQ", &[Segment { start: 1.0, end: 5.0, category: "sponsor".into() }]);
        assert!(s.contains(r#""videoId":"dQw4w9WgXcQ""#));
        assert!(s.contains("__pbSponsorSkip"));
    }
}
