//! This helper's release version, for `version`: set when the release builds it (HERDR_CONTR_VERSION).

/// x.y.z from `v0.8.0` or `0.8.0`; anything else (a pre-release, a dev build) is None.
pub fn version(s: &str) -> Option<(u64, u64, u64)> {
    let mut it = s.trim().trim_start_matches('v').split('.');
    let v = (it.next()?.parse().ok()?, it.next()?.parse().ok()?, it.next()?.parse().ok()?);
    it.next().is_none().then_some(v)
}

fn show(v: (u64, u64, u64)) -> String {
    format!("{}.{}.{}", v.0, v.1, v.2)
}

/// The release version baked in at build time only (no test stand-in): `version` reports it, so the
/// release's package job can check that every helper it ships says the tag's version.
pub fn release() -> Option<String> {
    option_env!("HERDR_CONTR_VERSION").and_then(version).map(show)
}

#[cfg(test)]
mod tests {
    use super::version;

    #[test]
    fn versions() {
        assert_eq!(version("v0.8.0"), Some((0, 8, 0)));
        assert_eq!(version("0.10.2"), Some((0, 10, 2)));
        assert_eq!(version("v1.0.0-rc1"), None);
        assert_eq!(version("0.8"), None);
        assert_eq!(version("0.8.0.1"), None);
        assert!(version("0.10.0") > version("0.9.9"));
    }
}
