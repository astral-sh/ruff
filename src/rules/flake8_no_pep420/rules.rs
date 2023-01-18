use std::path::Path;

use crate::ast::types::Range;
use crate::registry::Diagnostic;
use crate::violations;

/// INP001
pub fn implicit_namespace_package(path: &Path) -> Option<Diagnostic> {
    if let Some(parent) = path.parent() {
        if !parent.join("__init__.py").as_path().exists() {
            return Some(Diagnostic::new(
                violations::ImplicitNamespacePackage(path.to_string_lossy().to_string()),
                Range::default(),
            ));
        }
    }
    None
}
