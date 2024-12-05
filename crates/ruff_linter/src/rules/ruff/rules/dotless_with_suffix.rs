use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::{Arguments, Expr, ExprAttribute, ExprCall, ExprStringLiteral, StringFlags};
use ruff_python_semantic::analyze::typing;
use ruff_python_semantic::SemanticModel;

use crate::checkers::ast::Checker;

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

    let Some(string) = single_string_literal_argument(arguments) else {
        return;
    };

    if matches!(string.value.chars().next(), None | Some('.')) {
        return;
    }

    let diagnostic = Diagnostic::new(DotlessWithSuffix, call.range);
    let Some(fix) = add_leading_dot_fix(string) else {
        return;
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

fn single_string_literal_argument(arguments: &Arguments) -> Option<&ExprStringLiteral> {
    if arguments.len() > 1 {
        return None;
    }

    match arguments.find_argument("suffix", 0)? {
        Expr::StringLiteral(string) => Some(string),
        _ => None,
    }
}

fn add_leading_dot_fix(string: &ExprStringLiteral) -> Option<Fix> {
    let first_part = string.value.iter().next()?;

    // |r"foo"
    let before_prefix = first_part.range.start();

    // r|"foo"
    let prefix_length = first_part.flags.prefix().as_str().len();
    let after_prefix = before_prefix.checked_add(u32::try_from(prefix_length).ok()?.into())?;

    // r"|foo"
    let quote_length = first_part.flags.quote_str().len();
    let after_leading_quote = after_prefix.checked_add(u32::try_from(quote_length).ok()?.into())?;

    let edit = Edit::insertion(".".to_string(), after_leading_quote);

    Some(Fix::safe_edit(edit))
}
