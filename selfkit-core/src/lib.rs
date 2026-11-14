//! selfkit-core: shared Rust library for all Selfkit apps.
//! Docs: docs/components/selfkit-core.md (code map), docs/data-and-sync.md, docs/security.md.

pub mod bundle;
pub mod conflict;
pub mod crypto;
pub mod error;
pub mod keys;
#[cfg(feature = "lan")]
pub mod lan;
pub mod paths;
pub mod settings;
pub mod store;
pub mod sync;
pub mod time;

#[cfg(feature = "keychain")]
pub mod keychain;

pub use error::{Error, Result};
pub use store::{Record, Store};
