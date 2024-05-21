use ruff_diagnostics::{Diagnostic, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast, Expr, ExprAttribute};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for the uses of `typing.ByteString` and `collections.abc.ByteString`.
///
/// ## Why is this bad?
/// `ByteString` has been deprecated since Python 3.9 and will be removed in
/// Python 3.14. The Python documentation recommends using either
/// `collections.abc.Buffer` (for Python >=3.12) or a union like
/// `bytes | bytearray | memoryview` instead.
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
        return Some(format!("Do not use `{full_name}`"));
    }
}

/// PYI057
pub(crate) fn bytestring_attribute(checker: &mut Checker, attribute: &ExprAttribute) {
    let name = attribute.attr.as_str();
    if name != "ByteString" {
        return;
    }

    if let Expr::Name(ast::ExprName { id, .. }) = attribute.value.as_ref() {
        if id == "typing" {
            let full_name = format!("{}.{}", id, name);
            let diagnostic = Diagnostic::new(ByteStringUsage { full_name }, attribute.range());
            checker.diagnostics.push(diagnostic);
        }
    } else if let Expr::Attribute(ast::ExprAttribute { value, attr, .. }) = attribute.value.as_ref()
    {
        if attr.as_str() != "abc" {
            return;
        }
        if let Expr::Name(ast::ExprName { id, .. }) = value.as_ref() {
            if id == "collections" {
                let full_name = format!("{}.{}.{}", id, attr, name);
                let diagnostic = Diagnostic::new(ByteStringUsage { full_name }, attribute.range());
                checker.diagnostics.push(diagnostic);
            }
        }
    }
}

/// PYI057
pub(crate) fn bytestring_import(checker: &mut Checker, import_from: &ast::StmtImportFrom) {
    let ast::StmtImportFrom { names, module, .. } = import_from;

    for name in names {
        if let Some(module) = module {
            let full_name = format!("{}.{}", module.id, name.name);
            if full_name == "typing.ByteString" || full_name == "collections.abc.ByteString" {
                let diagnostic =
                    Diagnostic::new(ByteStringUsage { full_name }, import_from.range());
                checker.diagnostics.push(diagnostic);
            }
        }
    }
}
