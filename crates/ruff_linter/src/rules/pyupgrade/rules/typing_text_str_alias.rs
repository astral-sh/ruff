use ruff_python_ast::Expr;

use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_semantic::Modules;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::{Edit, Fix, FixAvailability, Violation};

/// ## What it does
/// Checks for uses of `typing.Text`. In preview mode, also checks for `typing_extensions.Text`.
///
/// ## Why is this bad?
/// `typing.Text` is an alias for `str`, and only exists for Python 2
/// compatibility. As of Python 3.11, `typing.Text` is deprecated. Use `str`
/// instead.
///
/// ## Example
/// ```python
/// from typing import Text
/// from typing_extensions import Text  # Preview mode only
///
/// foo: Text = "bar"
/// ```
///
/// Use instead:
/// ```python
/// foo: str = "bar"
/// ```
///
/// ## References
/// - [Python documentation: `typing.Text`](https://docs.python.org/3/library/typing.html#typing.Text)
#[derive(ViolationMetadata)]
pub(crate) struct TypingTextStrAlias {
    module: String,
}

impl Violation for TypingTextStrAlias {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`{}.Text` is deprecated, use `str`", self.module)
    }

    fn fix_title(&self) -> Option<String> {
        Some("Replace with `str`".to_string())
    }
}

/// UP019
pub(crate) fn typing_text_str_alias(checker: &Checker, expr: &Expr) {
    if !checker
        .semantic()
        .seen_module(Modules::TYPING | Modules::TYPING_EXTENSIONS)
    {
        return;
    }

    if let Some(qualified_name) = checker.semantic().resolve_qualified_name(expr) {
        let segments = qualified_name.segments();
        if matches!(segments, ["typing", "Text"]) {
            // Always detect typing.Text (stable behavior)
            let mut diagnostic = checker.report_diagnostic(
                TypingTextStrAlias {
                    module: "typing".to_string(),
                },
                expr.range(),
            );
            diagnostic.add_primary_tag(ruff_db::diagnostic::DiagnosticTag::Deprecated);
            diagnostic.try_set_fix(|| {
                let (import_edit, binding) = checker.importer().get_or_import_builtin_symbol(
                    "str",
                    expr.start(),
                    checker.semantic(),
                )?;
                Ok(Fix::safe_edits(
                    Edit::range_replacement(binding, expr.range()),
                    import_edit,
                ))
            });
        } else if matches!(segments, ["typing_extensions", "Text"]) {
            // Only detect typing_extensions.Text in preview mode
            if checker.settings().preview.is_enabled() {
                let mut diagnostic = checker.report_diagnostic(
                    TypingTextStrAlias {
                        module: "typing_extensions".to_string(),
                    },
                    expr.range(),
                );
                diagnostic.add_primary_tag(ruff_db::diagnostic::DiagnosticTag::Deprecated);
                diagnostic.try_set_fix(|| {
                    let (import_edit, binding) = checker.importer().get_or_import_builtin_symbol(
                        "str",
                        expr.start(),
                        checker.semantic(),
                    )?;
                    Ok(Fix::safe_edits(
                        Edit::range_replacement(binding, expr.range()),
                        import_edit,
                    ))
                });
            }
        }
    }
}
