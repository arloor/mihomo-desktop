use std::sync::LazyLock;

pub const VERSION: &str = env!("APP_VERSION");
pub const BASE_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const GIT_SHA: &str = env!("APP_GIT_SHA");
pub const BUILD_TIME: &str = env!("APP_BUILD_TIME");
pub const BUILD_FEATURES: &str = env!("APP_BUILD_FEATURES");

pub const SHORT_CLI_VERSION: &str = concat!("mihomo-desktop ", env!("APP_VERSION"));

static LONG_VERSION: LazyLock<String> = LazyLock::new(|| {
    format!("{VERSION} (commit {GIT_SHA}, built {BUILD_TIME}, features: {BUILD_FEATURES})")
});

static LONG_CLI_VERSION: LazyLock<String> =
    LazyLock::new(|| format!("mihomo-desktop {}", long_version()));

pub fn long_version() -> &'static str {
    LONG_VERSION.as_str()
}

pub fn long_cli_version() -> &'static str {
    LONG_CLI_VERSION.as_str()
}

#[cfg(test)]
mod tests {
    use super::*;
    use semver::Version;

    #[test]
    fn compiled_versions_are_valid_semver() {
        let base = Version::parse(BASE_VERSION).expect("BASE_VERSION should be valid SemVer");
        let display = Version::parse(VERSION).expect("VERSION should be valid SemVer");
        assert_eq!(
            base.cmp_precedence(&display),
            std::cmp::Ordering::Equal,
            "build metadata must not change SemVer precedence"
        );
        assert_eq!(SHORT_CLI_VERSION, format!("mihomo-desktop {VERSION}"));
        assert!(long_cli_version().starts_with(SHORT_CLI_VERSION));
        assert!(long_version().contains("commit "));
        assert!(long_version().contains("built "));
        assert!(long_version().contains("features: "));
    }
}
