//! Strong passwords, like Chrome's "Suggest strong password": 20 characters from upper- and lower-case letters, digits
//! and symbols, at least one of each, without look-alike characters (0/O, 1/l/I). Randomness comes from Windows'
//! system generator (`BCryptGenRandom`), never from a seeded generator.

const LOWER: &[u8] = b"abcdefghijkmnopqrstuvwxyz";
const UPPER: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ";
const DIGITS: &[u8] = b"23456789";
const SYMBOLS: &[u8] = b"-_.!?#%+=@";

pub const LENGTH: usize = 20;

#[cfg(windows)]
fn random_bytes(buf: &mut [u8]) -> bool {
    use windows::Win32::Security::Cryptography::{BCryptGenRandom, BCRYPT_USE_SYSTEM_PREFERRED_RNG};
    unsafe { BCryptGenRandom(None, buf, BCRYPT_USE_SYSTEM_PREFERRED_RNG).is_ok() }
}

#[cfg(not(windows))]
fn random_bytes(_buf: &mut [u8]) -> bool {
    false
}

/// A uniformly random index below `n` (rejection sampling, so no bias).
fn pick(n: usize, next: &mut impl FnMut() -> Option<u8>) -> Option<usize> {
    let limit = 256 - (256 % n);
    loop {
        let b = next()? as usize;
        if b < limit {
            return Some(b % n);
        }
    }
}

/// Builds a password from the random bytes `next` hands out (`None` when they run out).
fn build(mut next: impl FnMut() -> Option<u8>) -> Option<String> {
    let all: Vec<u8> = [LOWER, UPPER, DIGITS, SYMBOLS].concat();
    let mut out: Vec<u8> = Vec::with_capacity(LENGTH);
    for set in [LOWER, UPPER, DIGITS, SYMBOLS] {
        out.push(set[pick(set.len(), &mut next)?]);
    }
    while out.len() < LENGTH {
        out.push(all[pick(all.len(), &mut next)?]);
    }
    // Shuffle so the guaranteed classes are not always first (Fisher-Yates).
    for i in (1..out.len()).rev() {
        let j = pick(i + 1, &mut next)?;
        out.swap(i, j);
    }
    String::from_utf8(out).ok()
}

/// A new strong password, or `None` if Windows' random generator failed.
pub fn generate() -> Option<String> {
    let mut pool = [0u8; 512];
    if !random_bytes(&mut pool) {
        return None;
    }
    let mut i = 0;
    build(move || {
        let b = pool.get(i).copied();
        i += 1;
        b
    })
}

/// Script that puts `password` into the focused text field of the page (as typing would) and reports "ok" or "nofield".
pub fn insert_script(password: &str) -> String {
    let pw = serde_json::to_string(password).unwrap_or_else(|_| "\"\"".into());
    format!(
        r#"(() => {{
  const el = document.activeElement;
  if (!el || !(el instanceof HTMLInputElement || el instanceof HTMLTextAreaElement) || el.readOnly || el.disabled) return "nofield";
  if (el instanceof HTMLInputElement && !["password", "text", "email", ""].includes(el.type)) return "nofield";
  const setter = Object.getOwnPropertyDescriptor(Object.getPrototypeOf(el), "value").set;
  setter.call(el, {pw});
  el.dispatchEvent(new Event("input", {{ bubbles: true }}));
  el.dispatchEvent(new Event("change", {{ bubbles: true }}));
  return "ok";
}})()"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn passwords_have_every_class_the_right_length_and_no_lookalikes() {
        for _ in 0..50 {
            let p = generate().expect("Windows random generator");
            assert_eq!(p.len(), LENGTH);
            assert!(p.bytes().any(|b| LOWER.contains(&b)));
            assert!(p.bytes().any(|b| UPPER.contains(&b)));
            assert!(p.bytes().any(|b| DIGITS.contains(&b)));
            assert!(p.bytes().any(|b| SYMBOLS.contains(&b)));
            assert!(!p.contains(['0', 'O', '1', 'l', 'I']));
        }
        assert_ne!(generate(), generate());
    }

    #[test]
    fn running_out_of_randomness_gives_nothing_instead_of_a_weak_password() {
        let mut n = 0u8;
        assert_eq!(
            build(|| {
                n += 1;
                (n < 5).then_some(n)
            }),
            None
        );
    }

    #[test]
    fn the_insert_script_quotes_the_password() {
        let s = insert_script("a\"b</script>");
        assert!(s.contains(r#"setter.call(el, "a\"b</script>");"#));
    }
}
