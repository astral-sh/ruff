//! Database trait for the type system.
//!
//! This extends `ty_python_semantic::Db` to provide access to type inference
//! and type checking functionality.

use ty_python_semantic::Db as SemanticDb;

/// The database trait for the Python type system.
///
/// This trait extends the semantic database with type-system specific functionality.
/// Currently it inherits all methods from `ty_python_semantic::Db`.
#[salsa::db]
pub trait Db: SemanticDb {}
