//! Reserved migration keys outlive the ordinary cache TTL only while their seat is live.

/// Whether a well-formed migration key should retain its live seat's idempotency entry.
pub fn retain_live_rename_key(key: &str, surface_alive: bool) -> bool {
    if !surface_alive { return false; }
    let Some(rest) = key.strip_prefix("wave-rename:") else { return false; };
    let Some((nonce, sid)) = rest.split_once(':') else { return false; };
    nonce.len() == 32
        && nonce.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        && !sid.starts_with('0')
        && sid.bytes().all(|b| b.is_ascii_digit())
        && sid.parse::<u64>().is_ok_and(|sid| sid > 0)
}

#[cfg(test)]
mod tests {
    use super::retain_live_rename_key;

    #[test]
    fn valid_reserved_keys_retain_live_seats() {
        for sid in ["1", "42", "18446744073709551615"] {
            assert!(retain_live_rename_key(
                &format!("wave-rename:0123456789abcdef0123456789abcdef:{sid}"), true
            ));
        }
    }

    #[test]
    fn dead_seats_and_ordinary_keys_do_not_gain_retention() {
        assert!(!retain_live_rename_key("wave-rename:0123456789abcdef0123456789abcdef:1", false));
        for key in ["ordinary", "rename-attempt-1", "wave-rename", "wave-rename:"] {
            assert!(!retain_live_rename_key(key, true));
        }
    }

    #[test]
    fn malformed_reserved_keys_do_not_gain_retention() {
        for key in [
            "wave-rename:0123456789abcdef0123456789abcdef:0",
            "wave-rename:0123456789abcdef0123456789abcdef:01",
            "wave-rename:0123456789abcdef0123456789abcdef:-1",
            "wave-rename:0123456789abcdef0123456789abcdef:+1",
            "wave-rename:0123456789abcdef0123456789abcdef:18446744073709551616",
            "wave-rename:0123456789abcdef0123456789abcdef:",
            "wave-rename:0123456789abcdef0123456789abcdef:1:extra",
            "wave-rename:0123456789abcdef0123456789abcde:1",
            "wave-rename:0123456789abcdef0123456789abcdef0:1",
            "wave-rename:0123456789abcdef0123456789abcdeF:1",
            "wave-rename:0123456789abcdef0123456789abcdeg:1",
            "other:0123456789abcdef0123456789abcdef:1",
        ] {
            assert!(!retain_live_rename_key(key, true), "malformed key retained: {key}");
        }
    }
}
