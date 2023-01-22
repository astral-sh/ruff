use std::path::Path;

use crate::ast::types::Range;
use crate::registry::Diagnostic;
use crate::{fs, violations};

/// INP001
pub fn implicit_namespace_package(path: &Path, package: Option<&Path>) -> Option<Diagnostic> {
    if package.is_none() {
        Some(Diagnostic::new(
            violations::ImplicitNamespacePackage(fs::relativize_path(path).to_string()),
            Range::default(),
        ))
    } else {
        None
    }
}
