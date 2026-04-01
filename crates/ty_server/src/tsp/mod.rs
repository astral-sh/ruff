//! Type Server Protocol (TSP) types and definitions.
//!
//! This module defines the Rust types that correspond to the TSP protocol
//! defined in `typeServerProtocol.ts`. These are used for JSON-RPC
//! communication between a TSP client (e.g., Pylance) and ty.
//!
//! The module is organized into:
//! - [`protocol`]: Method name constants and protocol version
//! - [`types`]: Type representations sent over the wire
//! - [`requests`]: Request/response parameter types
//! - [`handlers`]: Request handler implementations

pub(crate) mod handlers;
pub(crate) mod protocol;
// Types and requests still have members not yet consumed by handlers.
#[allow(dead_code)]
pub(crate) mod requests;
#[allow(dead_code)]
pub(crate) mod types;
