use std::path::Path;

use ruff_macros::derive_message_formats;

use crate::ast::types::Range;
use crate::registry::Diagnostic;
use crate::violation::Violation;
use crate::{define_violation, fs};

define_violation!(
    pub struct ImplicitNamespacePackage(pub String);
);
impl Violation for ImplicitNamespacePackage {
    #[derive_message_formats]
    fn message(&self) -> String {
        let ImplicitNamespacePackage(filename) = self;
        format!("File `{filename}` is part of an implicit namespace package. Add an `__init__.py`.")
    }
}

/// INP001
pub fn implicit_namespace_package(path: &Path, package: Option<&Path>) -> Option<Diagnostic> {
    if package.is_none() {
        Some(Diagnostic::new(
            ImplicitNamespacePackage(fs::relativize_path(path)),
            Range::default(),
        ))
    } else {
        None
    }
}
