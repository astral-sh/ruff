//! Type Server Protocol (TSP) types for Pylance integration.
//!
//! This crate defines the Rust types that correspond to the TSP protocol
//! defined in `typeServerProtocol.ts`. These types are used for JSON-RPC
//! communication between Pylance and the ty type server.

pub mod protocol;
pub mod requests;
pub mod types;

pub use protocol::*;
pub use requests::*;
pub use types::*;
