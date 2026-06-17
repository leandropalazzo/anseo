//! Library surface for `anseo-mcp`.
//!
//! Exposes the core modules so that integration tests under `tests/` can
//! access `Dispatcher`, the transport layer, and the protocol types without
//! going through the binary entry point.  Story 16.6 adds this crate split to
//! support `tests/transport_parity.rs`.

pub mod benchmark_client;
pub mod dispatch;
pub mod error;
pub mod http_client;
pub mod protocol;
pub mod tools;
pub mod transport;
