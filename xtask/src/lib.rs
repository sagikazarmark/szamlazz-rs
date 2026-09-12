//! Repository-only checks, shared by the command and its regression tests.
pub mod migration;
pub mod schemas;

use std::path::PathBuf;

/// The source checkout containing this build.
#[must_use]
pub fn workspace() -> PathBuf {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.pop();
    path
}
