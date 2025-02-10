use ruff_diagnostics::{Diagnostic, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::{self as ast, Expr};
use ruff_python_semantic::Modules;
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
#[derive(ViolationMetadata)]
pub(crate) struct ByteStringUsage {
    origin: ByteStringOrigin,
}

impl Violation for ByteStringUsage {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::None;

    #[derive_message_formats]
    fn message(&self) -> String {
        let ByteStringUsage { origin } = self;
        format!("Do not use `{origin}.ByteString`, which has unclear semantics and is deprecated")
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum ByteStringOrigin {
    Typing,
    CollectionsAbc,
}

impl std::fmt::Display for ByteStringOrigin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Typing => "typing",
            Self::CollectionsAbc => "collections.abc",
        })
    }
}

/// PYI057
pub(crate) fn bytestring_attribute(checker: &Checker, attribute: &Expr) {
    let semantic = checker.semantic();
    if !semantic
        .seen
        .intersects(Modules::TYPING | Modules::COLLECTIONS)
    {
        return;
    }
    let Some(qualified_name) = semantic.resolve_qualified_name(attribute) else {
        return;
    };
    let origin = match qualified_name.segments() {
        ["typing", "ByteString"] => ByteStringOrigin::Typing,
        ["collections", "abc", "ByteString"] => ByteStringOrigin::CollectionsAbc,
        _ => return,
    };
    checker.report_diagnostic(Diagnostic::new(
        ByteStringUsage { origin },
        attribute.range(),
    ));
}

/// PYI057
pub(crate) fn bytestring_import(checker: &Checker, import_from: &ast::StmtImportFrom) {
    let ast::StmtImportFrom { names, module, .. } = import_from;

    let module_id = match module {
        Some(module) => module.id.as_str(),
        None => return,
    };

    let origin = match module_id {
        "typing" => ByteStringOrigin::Typing,
        "collections.abc" => ByteStringOrigin::CollectionsAbc,
        _ => return,
    };

    for name in names {
        if name.name.as_str() == "ByteString" {
            checker.report_diagnostic(Diagnostic::new(ByteStringUsage { origin }, name.range()));
        }
    }
}
