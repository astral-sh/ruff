use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast as ast;
use ruff_python_codegen::Stylist;
use ruff_python_trivia::leading_indentation;
use ruff_source_file::LineRanges;
use ruff_text_size::TextRange;

use crate::checkers::ast::Checker;
use crate::{Edit, Fix, FixAvailability, Locator, Violation};

/// ## What it does
/// Checks for `__all__` definitions with multiple items that are not
/// formatted across multiple lines.
///
/// ## Why is this bad?
/// When `__all__` contains multiple items, placing each item on its own
/// line improves readability, reduces merge conflicts when adding or
/// removing exports, and produces cleaner diffs.
///
/// ## Example
/// ```python
/// __all__ = ["a", "b", "c"]
/// ```
///
/// Use instead:
/// ```python
/// __all__ = [
///     "a",
///     "b",
///     "c",
/// ]
/// ```
///
/// ## Formatter compatibility
/// This rule is incompatible with the [formatter] when using
/// [`format.skip-magic-trailing-comma`]. The formatter uses the trailing
/// comma as a signal to keep sequences multiline. With
/// `skip-magic-trailing-comma` enabled, the formatter will collapse the
/// expanded `__all__` back onto a single line (if it fits within the line
/// width), causing the rule to trigger again on the next lint pass.
///
/// [formatter]: https://docs.astral.sh/ruff/formatter/
/// [`format.skip-magic-trailing-comma`]: https://docs.astral.sh/ruff/settings/#format-skip-magic-trailing-comma
///
/// ## Fix safety
/// This rule's fix is always safe, as it only changes whitespace formatting
/// without altering the runtime value of `__all__`.
#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "0.14.14")]
pub(crate) struct MultilineDunderAll;

impl Violation for MultilineDunderAll {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Always;

    #[derive_message_formats]
    fn message(&self) -> String {
        "Single-line `__all__` with multiple items should be multiline".to_string()
    }

    fn fix_title(&self) -> Option<String> {
        Some("Format `__all__` across multiple lines".to_string())
    }
}

/// Check an `__all__ = [...]` assignment.
pub(crate) fn multiline_dunder_all_assign(
    checker: &Checker,
    ast::StmtAssign { value, targets, .. }: &ast::StmtAssign,
) {
    if let [expr] = targets.as_slice() {
        check_multiline_dunder_all(checker, expr, value);
    }
}

/// Check an `__all__ += [...]` augmented assignment.
pub(crate) fn multiline_dunder_all_aug_assign(checker: &Checker, node: &ast::StmtAugAssign) {
    if node.op.is_add() {
        check_multiline_dunder_all(checker, &node.target, &node.value);
    }
}

/// Check an `__all__.extend([...])` call.
pub(crate) fn multiline_dunder_all_extend_call(
    checker: &Checker,
    ast::ExprCall {
        func,
        arguments: ast::Arguments { args, keywords, .. },
        ..
    }: &ast::ExprCall,
) {
    let ([value_passed], []) = (&**args, &**keywords) else {
        return;
    };
    let ast::Expr::Attribute(ast::ExprAttribute {
        ref value,
        ref attr,
        ..
    }) = **func
    else {
        return;
    };
    if attr == "extend" {
        check_multiline_dunder_all(checker, value, value_passed);
    }
}

/// Check an `__all__: list[str] = [...]` annotated assignment.
pub(crate) fn multiline_dunder_all_ann_assign(checker: &Checker, node: &ast::StmtAnnAssign) {
    if let Some(value) = &node.value {
        check_multiline_dunder_all(checker, &node.target, value);
    }
}

/// Core logic: detect single-line `__all__` with multiple items and emit a diagnostic with fix.
fn check_multiline_dunder_all(checker: &Checker, target: &ast::Expr, node: &ast::Expr) {
    let ast::Expr::Name(ast::ExprName { id, .. }) = target else {
        return;
    };

    if id != "__all__" {
        return;
    }

    if !checker.semantic().current_scope().kind.is_module() {
        return;
    }

    let (elts, range, opening, closing) = match node {
        ast::Expr::List(ast::ExprList { elts, range, .. }) => (elts, *range, "[", "]"),
        ast::Expr::Tuple(ast::ExprTuple {
            elts,
            range,
            parenthesized,
            ..
        }) => {
            if !parenthesized {
                return;
            }
            (elts, *range, "(", ")")
        }
        _ => return,
    };

    if elts.len() < 2 {
        return;
    }

    let locator = checker.locator();

    if locator.contains_line_break(range) {
        return;
    }

    let mut diagnostic = checker.report_diagnostic(MultilineDunderAll, range);

    diagnostic.set_fix(create_fix(range, elts, opening, closing, locator, checker.stylist()));
}

fn create_fix(
    range: TextRange,
    elts: &[ast::Expr],
    opening: &str,
    closing: &str,
    locator: &Locator,
    stylist: &Stylist,
) -> Fix {
    let newline = stylist.line_ending().as_str();
    let indent = stylist.indentation().as_str();
    let leading = leading_indentation(locator.full_line_str(range.start()));
    let item_indent = format!("{leading}{indent}");

    let mut result = String::new();
    result.push_str(opening);
    result.push_str(newline);

    for elt in elts {
        result.push_str(&item_indent);
        result.push_str(locator.slice(elt));
        result.push(',');
        result.push_str(newline);
    }

    result.push_str(leading);
    result.push_str(closing);

    let edit = Edit::range_replacement(result, range);
    Fix::safe_edit(edit)
}
