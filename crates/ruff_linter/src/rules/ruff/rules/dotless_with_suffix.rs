use crate::checkers::ast::Checker;
use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::{Expr, ExprAttribute, ExprCall, ExprStringLiteral, StringFlags};
use ruff_python_semantic::analyze::typing;
use ruff_python_semantic::SemanticModel;
use ruff_text_size::Ranged;

/// ## What it does
/// Checks for `Path.with_suffix()` calls where
/// the given suffix does not have a leading dot.
///
/// ## Why is this bad?
/// `Path.with_suffix()` will raise an error at runtime
/// if the given suffix is not prefixed with a dot.
///
/// ## Known problems
/// This rule is prone to false positives and negatives
/// due to type inference limitations.
///
/// ## Examples
///
/// ```python
/// path.with_suffix("py")
/// ```
///
/// Use instead:
///
/// ```python
/// path.with_suffix(".py")
/// ```
#[derive(ViolationMetadata)]
pub(crate) struct DotlessWithSuffix;

impl AlwaysFixableViolation for DotlessWithSuffix {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Dotless suffix passed to `.with_suffix()`".to_string()
    }

    fn fix_title(&self) -> String {
        "Add a leading dot".to_string()
    }
}

/// RUF049
pub(crate) fn dotless_with_suffix(checker: &mut Checker, call: &ExprCall) {
    let (func, arguments) = (&call.func, &call.arguments);

    if !is_path_with_suffix_call(checker.semantic(), func) {
        return;
    }

    if arguments.len() > 1 {
        return;
    }

    let Some(Expr::StringLiteral(string)) = arguments.find_argument("suffix", 0) else {
        return;
    };

    let string_value = string.value.to_str();

    if string_value.is_empty() || string_value.starts_with('.') {
        return;
    }

    let diagnostic = Diagnostic::new(DotlessWithSuffix, call.range);
    let Some(fix) = add_leading_dot_fix(string) else {
        unreachable!("Expected to always be able to fix this rule");
    };

    checker.diagnostics.push(diagnostic.with_fix(fix));
}

fn is_path_with_suffix_call(semantic: &SemanticModel, func: &Expr) -> bool {
    let Expr::Attribute(ExprAttribute { value, attr, .. }) = func else {
        return false;
    };

    if attr != "with_suffix" {
        return false;
    }

    let Expr::Name(name) = value.as_ref() else {
        return false;
    };
    let Some(binding) = semantic.only_binding(name).map(|id| semantic.binding(id)) else {
        return false;
    };

    typing::is_pathlib_path(binding, semantic)
}

fn add_leading_dot_fix(string: &ExprStringLiteral) -> Option<Fix> {
    let first_part = string.value.iter().next()?;

    let opener_length = first_part.flags.opener_len();
    let after_leading_quote = first_part.start().checked_add(opener_length)?;

    let edit = Edit::insertion(".".to_string(), after_leading_quote);

    Some(Fix::unsafe_edit(edit))
}
