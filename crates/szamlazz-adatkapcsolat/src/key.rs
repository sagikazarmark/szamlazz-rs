//! The connection key: constant-time comparison of a presented
//! `X-Szamlazzhu-Key` with a configured one.

use subtle::ConstantTimeEq as _;

/// Whether `presented` is `expected`, in time that does not depend on where
/// the two first differ.
///
/// The header is the connection's only authentication, so a comparison that
/// returned at the first differing byte would leak the key's prefix through
/// timing. This is what the fixed-key router compares with, exported so a
/// `KeyResolver` (`axum` feature) that holds its keys as secrets (rather than
/// opaque ids it looks up) has the same comparison to call. Two keys of
/// different lengths are unequal at once: the length is not the secret.
///
/// ```
/// use szamlazz_adatkapcsolat::keys_match;
///
/// assert!(keys_match("k-1", "k-1"));
/// assert!(!keys_match("k-1", "k-2"));
/// ```
#[must_use]
pub fn keys_match(presented: &str, expected: &str) -> bool {
    presented.as_bytes().ct_eq(expected.as_bytes()).unwrap_u8() == 1
}

#[cfg(test)]
mod tests {
    use super::keys_match;

    #[test]
    fn equal_keys_match() {
        assert!(keys_match("secret-key", "secret-key"));
        assert!(keys_match("kulcs-árvíztűrő", "kulcs-árvíztűrő"));
    }

    #[test]
    fn one_differing_byte_does_not_match() {
        assert!(!keys_match("secret-key", "secret-kex"));
        assert!(!keys_match("Secret-key", "secret-key"));
    }

    #[test]
    fn differing_lengths_do_not_match() {
        assert!(!keys_match("secret-key", "secret-key-"));
        assert!(!keys_match("secret-key-", "secret-key"));
        assert!(!keys_match("", "secret-key"));
        assert!(!keys_match("secret-key", ""));
    }

    #[test]
    fn empty_matches_empty() {
        assert!(keys_match("", ""));
    }
}
