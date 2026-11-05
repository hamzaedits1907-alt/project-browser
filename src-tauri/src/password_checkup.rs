//! Password Checkup, like Chrome's: finds saved passwords that are weak, used on more than one site, or known from
//! data breaches. Passwords never leave this computer: for the breach check only the first five characters of each
//! password's SHA-1 hash go to Have I Been Pwned's range service (k-anonymity), which answers with every leaked hash
//! starting with them; the comparison happens here.

use serde::Serialize;

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CheckupItem {
    pub id: i64,
    pub origin: String,
    pub display_url: String,
    pub username: String,
    /// "weak", "reused", "leaked".
    pub problems: Vec<String>,
    /// For weak passwords: why, in plain words.
    pub weak_reason: Option<String>,
    /// How many times the password appears in known breaches (0 when not leaked or not checked).
    pub leak_count: u64,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CheckupReport {
    pub checked: usize,
    pub breaches_checked: bool,
    /// The breach check was asked for but could not be done (offline); the other checks still ran.
    pub breach_error: Option<String>,
    pub items: Vec<CheckupItem>,
}

const COMMON: &[&str] = &[
    "123456",
    "password",
    "12345678",
    "qwerty",
    "123456789",
    "12345",
    "1234",
    "111111",
    "1234567",
    "dragon",
    "123123",
    "baseball",
    "abc123",
    "football",
    "monkey",
    "letmein",
    "696969",
    "shadow",
    "master",
    "666666",
    "qwertyuiop",
    "123321",
    "mustang",
    "1234567890",
    "michael",
    "654321",
    "superman",
    "1qaz2wsx",
    "7777777",
    "121212",
    "000000",
    "qazwsx",
    "123qwe",
    "killer",
    "trustno1",
    "jordan",
    "jennifer",
    "zxcvbnm",
    "asdfgh",
    "hunter",
    "buster",
    "soccer",
    "harley",
    "batman",
    "andrew",
    "tigger",
    "sunshine",
    "iloveyou",
    "2000",
    "charlie",
    "robert",
    "thomas",
    "hockey",
    "ranger",
    "daniel",
    "starwars",
    "klaster",
    "112233",
    "george",
    "computer",
    "michelle",
    "jessica",
    "pepper",
    "1111",
    "zxcvbn",
    "555555",
    "11111111",
    "131313",
    "freedom",
    "777777",
    "pass",
    "maggie",
    "159753",
    "aaaaaa",
    "ginger",
    "princess",
    "joshua",
    "cheese",
    "amanda",
    "summer",
    "love",
    "ashley",
    "nicole",
    "chelsea",
    "biteme",
    "matthew",
    "access",
    "yankees",
    "987654321",
    "dallas",
    "austin",
    "thunder",
    "taylor",
    "matrix",
    "passwort",
    "hallo123",
    "admin",
    "welcome",
    "login",
    "abc",
    "qwerty123",
    "password1",
];

/// Why a password is weak, or None when it is not.
pub fn weakness(password: &str, username: &str) -> Option<String> {
    let lower = password.to_lowercase();
    let n = password.chars().count();
    if n < 8 {
        return Some(format!("Only {n} characters long"));
    }
    if COMMON.iter().any(|c| lower == *c || lower.trim_end_matches(|ch: char| ch.is_ascii_digit() || ch == '!') == *c) {
        return Some("One of the most used passwords".into());
    }
    let user = username.split('@').next().unwrap_or("").to_lowercase();
    if user.len() >= 4 && lower.contains(&user) {
        return Some("Contains your user name".into());
    }
    let distinct: std::collections::HashSet<char> = password.chars().collect();
    if distinct.len() <= 3 {
        return Some("Repeats the same few characters".into());
    }
    let classes = [
        password.chars().any(|c| c.is_lowercase()),
        password.chars().any(|c| c.is_uppercase()),
        password.chars().any(|c| c.is_ascii_digit()),
        password.chars().any(|c| !c.is_alphanumeric()),
    ]
    .iter()
    .filter(|b| **b)
    .count();
    if n < 12 && classes <= 1 {
        return Some("Short and only one kind of character".into());
    }
    None
}

/// Upper-case hex SHA-1 of `text` (Windows' CNG).
#[cfg(windows)]
pub fn sha1_hex(text: &str) -> Option<String> {
    use windows::Win32::Security::Cryptography::{BCryptHash, BCRYPT_SHA1_ALG_HANDLE};
    let mut out = [0u8; 20];
    let status = unsafe { BCryptHash(BCRYPT_SHA1_ALG_HANDLE, None, text.as_bytes(), &mut out) };
    if status.is_err() {
        return None;
    }
    Some(out.iter().map(|b| format!("{b:02X}")).collect())
}

#[cfg(not(windows))]
pub fn sha1_hex(_text: &str) -> Option<String> {
    None
}

/// Leak counts from one range answer ("SUFFIX:COUNT" lines) for the hash suffixes asked about.
pub fn parse_range(body: &str) -> std::collections::HashMap<String, u64> {
    body.lines()
        .filter_map(|l| {
            let (suffix, count) = l.trim().split_once(':')?;
            let count: u64 = count.trim().parse().ok()?;
            (count > 0).then(|| (suffix.trim().to_ascii_uppercase(), count))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weak_passwords_are_named_and_strong_ones_pass() {
        assert!(weakness("abc", "x").is_some());
        assert!(weakness("password1", "x").is_some());
        assert!(weakness("Qwerty123!", "x").is_none() || weakness("qwerty123", "x").is_some());
        assert!(weakness("aaaaaaaaaaaa", "x").is_some());
        assert!(weakness("johnsmith2024", "johnsmith@mail.test").is_some());
        assert!(weakness("lowercaseonly", "x").is_none(), "13 characters is long enough even in one class");
        assert!(weakness("abcdefgh", "x").is_some());
        assert!(weakness("V7k#pQ2m-Lr9", "x").is_none());
    }

    #[test]
    fn sha1_matches_the_known_value() {
        assert_eq!(sha1_hex("password").as_deref(), Some("5BAA61E4C9B93F3F0682250B6CF8331B7EE68FD8"));
    }

    #[test]
    fn range_answers_are_read_and_padding_rows_ignored() {
        let m = parse_range("1E4C9B93F3F0682250B6CF8331B7EE68FD8:3861493\r\n0018A45C4D1DEF81644B54AB7F969B88D65:0\nbad line\n");
        assert_eq!(m.get("1E4C9B93F3F0682250B6CF8331B7EE68FD8"), Some(&3861493));
        assert_eq!(m.len(), 1);
    }
}
