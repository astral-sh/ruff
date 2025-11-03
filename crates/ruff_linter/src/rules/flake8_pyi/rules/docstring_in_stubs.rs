use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::{ExprStringLiteral, Stmt};
use ruff_text_size::Ranged;

use crate::checkers::ast::{Checker, DocstringState, ExpectedDocstringKind};
use crate::docstrings::extraction::docstring_from;
use crate::{AlwaysFixableViolation, Edit, Fix};

/// ## What it does
/// Checks for the presence of docstrings in stub files.
///
/// ## Why is this bad?
/// Stub files should omit docstrings, as they're intended to provide type
/// hints, rather than documentation.
///
/// ## Example
///
/// ```pyi
/// def func(param: int) -> str:
///     """This is a docstring."""
///     ...
/// ```
///
/// Use instead:
///
/// ```pyi
/// def func(param: int) -> str: ...
/// ```
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.0.253")]
pub(crate) struct DocstringInStub;

impl AlwaysFixableViolation for DocstringInStub {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Docstrings should not be included in stubs".to_string()
    }

    fn fix_title(&self) -> String {
        "Remove docstring".to_string()
    }
}

/// PYI021
pub(crate) fn docstring_in_stubs(checker: &Checker, body: &[Stmt]) {
    if !matches!(
        checker.docstring_state(),
        DocstringState::Expected(
            ExpectedDocstringKind::Module
                | ExpectedDocstringKind::Class
                | ExpectedDocstringKind::Function
        )
    ) {
        return;
    }

    let docstring = docstring_from(body);

    let Some(docstring_range) = docstring.map(ExprStringLiteral::range) else {
        return;
    };

    let edit = if body.len() == 1 {
        Edit::range_replacement("...".to_string(), docstring_range)
    } else {
        Edit::range_deletion(docstring_range)
    };

    let isolation_level = Checker::isolation(checker.semantic().current_statement_id());
    let fix = Fix::unsafe_edit(edit).isolate(isolation_level);

    checker
        .report_diagnostic(DocstringInStub, docstring_range)
        .set_fix(fix);
}
