use ruff_diagnostics::{Diagnostic, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast, Expr};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for uses of `typing.ByteString` or `collections.abc.ByteString`.
///
/// ## Why is this bad?
/// `ByteString` has been deprecated since Python 3.9 and will be removed in
/// Python 3.14. The Python documentation recommends using either
/// `collections.abc.Buffer` (or the `typing_extensions` backport
/// on Python <3.12) or a union like `bytes | bytearray | memoryview` instead.
///
/// ## Example
/// ```python
/// from typing import ByteString
/// ```
///
/// Use instead:
/// ```python
/// from collections.abc import Buffer
/// ```
///
/// ## References
/// - [Python documentation: The `ByteString` type](https://docs.python.org/3/library/typing.html#typing.ByteString)
#[violation]
pub struct ByteStringUsage {
    full_name: String,
}

impl Violation for ByteStringUsage {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::None;

    #[derive_message_formats]
    fn message(&self) -> String {
        let ByteStringUsage { full_name } = self;
        format!("Do not use `{full_name}`, which has unclear semantics and is deprecated")
    }

    fn fix_title(&self) -> Option<String> {
        let ByteStringUsage { full_name } = self;
        Some(format!("Do not use `{full_name}`"))
    }
}

/// PYI057
pub(crate) fn bytestring_attribute(checker: &mut Checker, attribute: &Expr) {
    if let Some(full_name) = checker
        .semantic()
        .resolve_qualified_name(attribute)
        .and_then(|qualified_name| match qualified_name.segments() {
            ["typing", "ByteString"] => Some("typing.ByteString"),
            ["collections", "abc", "ByteString"] => Some("collections.abc.ByteString"),
            _ => None,
        })
    {
        let diagnostic = Diagnostic::new(
            ByteStringUsage {
                full_name: full_name.to_string(),
            },
            attribute.range(),
        );
        checker.diagnostics.push(diagnostic);
    }
}

/// PYI057
pub(crate) fn bytestring_import(checker: &mut Checker, import_from: &ast::StmtImportFrom) {
    let ast::StmtImportFrom { names, module, .. } = import_from;

    for name in names {
        if let Some(module) = module {
            let full_name = format!("{}.{}", module.id, name.name);
            if full_name == "typing.ByteString" || full_name == "collections.abc.ByteString" {
                let diagnostic = Diagnostic::new(ByteStringUsage { full_name }, name.range());
                checker.diagnostics.push(diagnostic);
            }
        }
    }
}
