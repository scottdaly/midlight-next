//! Trait abstractions for dependency injection and testability.
//!
//! This module provides trait definitions that abstract over external dependencies
//! like file systems, HTTP clients, and time providers. This enables:
//! - Unit testing with mock implementations
//! - Flexibility to swap implementations
//! - Clear dependency boundaries

pub mod file_system;
pub mod http_client;
pub mod object_store;
pub mod time;

pub use file_system::{FileSystem, TokioFileSystem};
pub use http_client::{HttpClient, ReqwestHttpClient};
pub use object_store::ObjectStoreOps;
pub use time::{TimeProvider, RealTimeProvider};

#[cfg(test)]
pub use file_system::MockFileSystem;
#[cfg(test)]
pub use http_client::MockHttpClient;
#[cfg(test)]
pub use time::MockTimeProvider;
