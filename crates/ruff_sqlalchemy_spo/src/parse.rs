//! Source → raw classes. Parses a Python source string and walks its
//! top-level class definitions (Flask-SQLAlchemy models are always
//! module-level classes, mirroring the Odoo frontend's assumption).

use ruff_python_ast::Stmt;
use ruff_python_parser::parse_module;

use crate::RawClass;
use crate::walk::walk_class;

/// Parse one Python source string into the SQLAlchemy model classes it
/// declares.
///
/// Returns an empty vec if the source fails to parse — a file that doesn't
/// AST-parse contributes nothing (mirrors `ruff_python_spo`'s silent-skip
/// invariant).
#[must_use]
pub(crate) fn parse_source(source: &str) -> Vec<RawClass> {
    let Ok(parsed) = parse_module(source) else {
        return Vec::new();
    };

    parsed
        .syntax()
        .body
        .iter()
        .filter_map(|stmt| match stmt {
            Stmt::ClassDef(class) => walk_class(class),
            _ => None,
        })
        .collect()
}
