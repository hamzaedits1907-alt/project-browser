//! Project Shield on YouTube. YouTube's ads come from youtube.com and googlevideo.com in the same responses as the
//! videos, so URL rules cannot block them without breaking playback. `youtube.js` is added to tabs at document
//! creation while the Shield is on for YouTube: it removes the ad schedule from the player's data, hides ad slots and
//! skips an ad that still starts. It does nothing on any other site.

/// The script added to a tab while the Shield is on for YouTube.
pub const SCRIPT: &str = include_str!("youtube.js");

/// Is `host` one of YouTube's sites (www, m, music, the no-cookie embed domain)?
pub fn is_youtube(host: &str) -> bool {
    ["youtube.com", "youtube-nocookie.com"].iter().any(|d| host == *d || host.ends_with(&format!(".{d}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_youtube_hosts_match() {
        assert!(is_youtube("www.youtube.com"));
        assert!(is_youtube("m.youtube.com"));
        assert!(is_youtube("music.youtube.com"));
        assert!(is_youtube("youtube.com"));
        assert!(is_youtube("www.youtube-nocookie.com"));
        assert!(!is_youtube("notyoutube.com"));
        assert!(!is_youtube("youtube.com.evil.example"));
        assert!(!is_youtube(""));
    }

    #[test]
    fn the_script_checks_the_host_itself() {
        assert!(SCRIPT.contains("location.hostname"));
        assert!(SCRIPT.contains("adPlacements"));
    }
}
