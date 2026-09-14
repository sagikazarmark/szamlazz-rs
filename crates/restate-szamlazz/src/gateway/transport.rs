//! Execution-local transport. Native reqwest; Workers Fetch with explicit policy.

#[cfg(not(target_arch = "wasm32"))]
pub(super) use szamlazz_agent::{Client, ClientError};
#[cfg(target_arch = "wasm32")]
mod wasm;
#[cfg(target_arch = "wasm32")]
pub(super) use wasm::{Client, ClientError};
