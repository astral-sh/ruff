#![warn(
    clippy::disallowed_methods,
    reason = "Prefer System trait methods over std methods in ty crates"
)]
pub use env_vars::*;

mod env_vars;
