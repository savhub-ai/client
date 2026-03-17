/// Package version from Cargo.toml (e.g. "0.1.0").
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Short git commit hash captured at build time (e.g. "a1b2c3d4e").
pub const GIT_HASH: &str = env!("SAVHUB_GIT_HASH");

/// Full version string usable as a `&'static str`: "0.1.0 (commit)".
///
/// Note: `concat!` cannot use `env!` with a custom var, so we provide
/// a runtime helper and a const for the pieces.
pub const VERSION_LONG: &str = concat!(
    env!("CARGO_PKG_VERSION"),
    " (",
    env!("SAVHUB_GIT_HASH"),
    ")"
);

/// Full version string: "0.1.0 (a1b2c3d4e)".
pub fn version_string() -> String {
    format!("{VERSION} ({GIT_HASH})")
}
