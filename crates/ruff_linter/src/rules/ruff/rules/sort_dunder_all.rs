use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast as ast;
use ruff_text_size::TextRange;

use crate::checkers::ast::Checker;
use crate::rules::ruff::rules::sequence_sorting::{
    sort_single_line_elements_sequence, MultilineStringSequenceValue, SequenceKind,
    SortClassification, SortingStyle,
};

/// ## What it does
/// Checks for `__all__` definitions that are not ordered
/// according to an "isort-style" sort.
///
/// An isort-style sort orders items first according to their casing:
/// SCREAMING_SNAKE_CASE names (conventionally used for global constants)
/// come first, followed by CamelCase names (conventionally used for
/// classes), followed by anything else. Within each category,
/// a [natural sort](https://en.wikipedia.org/wiki/Natural_sort_order)
/// is used to order the elements.
///
/// ## Why is this bad?
/// Consistency is good. Use a common convention for `__all__` to make your
/// code more readable and idiomatic.
///
/// ## Example
/// ```python
/// import sys
///
/// __all__ = [
///     "b",
///     "c",
///     "a",
/// ]
///
/// if sys.platform == "win32":
///     __all__ += ["z", "y"]
/// ```
///
/// Use instead:
/// ```python
/// import sys
///
/// __all__ = [
///     "a",
///     "b",
///     "c",
/// ]
///
/// if sys.platform == "win32":
///     __all__ += ["y", "z"]
/// ```
///
/// ## Fix safety
/// This rule's fix is marked as always being safe, in that
/// it should never alter the semantics of any Python code.
/// However, note that for multiline `__all__` definitions
/// that include comments on their own line, it can be hard
/// to tell where the comments should be moved to when sorting
/// the contents of `__all__`. While this rule's fix will
/// never delete a comment, it might *sometimes* move a
/// comment to an unexpected location.
#[violation]
pub struct UnsortedDunderAll;

impl Violation for UnsortedDunderAll {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`__all__` is not sorted")
    }

    fn fix_title(&self) -> Option<String> {
        Some("Apply an isort-style sorting to `__all__`".to_string())
    }
}

const SORTING_STYLE: SortingStyle = SortingStyle::Isort;

/// Sort an `__all__` definition represented by a `StmtAssign` AST node.
/// For example: `__all__ = ["b", "c", "a"]`.
pub(crate) fn sort_dunder_all_assign(
    checker: &mut Checker,
    ast::StmtAssign { value, targets, .. }: &ast::StmtAssign,
) {
    if let [expr] = targets.as_slice() {
        sort_dunder_all(checker, expr, value);
    }
}

/// Sort an `__all__` mutation represented by a `StmtAugAssign` AST node.
/// For example: `__all__ += ["b", "c", "a"]`.
pub(crate) fn sort_dunder_all_aug_assign(checker: &mut Checker, node: &ast::StmtAugAssign) {
    if node.op.is_add() {
        sort_dunder_all(checker, &node.target, &node.value);
    }
}

/// Sort a tuple or list passed to `__all__.extend()`.
pub(crate) fn sort_dunder_all_extend_call(
    checker: &mut Checker,
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
        sort_dunder_all(checker, value, value_passed);
    }
}

/// Sort an `__all__` definition represented by a `StmtAnnAssign` AST node.
/// For example: `__all__: list[str] = ["b", "c", "a"]`.
pub(crate) fn sort_dunder_all_ann_assign(checker: &mut Checker, node: &ast::StmtAnnAssign) {
    if let Some(value) = &node.value {
        sort_dunder_all(checker, &node.target, value);
    }
}

/// Sort a tuple or list that defines or mutates the global variable `__all__`.
///
/// This routine checks whether the tuple or list is sorted, and emits a
/// violation if it is not sorted. If the tuple/list was not sorted,
/// it attempts to set a `Fix` on the violation.
fn sort_dunder_all(checker: &mut Checker, target: &ast::Expr, node: &ast::Expr) {
    let ast::Expr::Name(ast::ExprName { id, .. }) = target else {
        return;
    };

    if id != "__all__" {
        return;
    }

    // We're only interested in `__all__` in the global scope
    if !checker.semantic().current_scope().kind.is_module() {
        return;
    }

    let (elts, range, kind) = match node {
        ast::Expr::List(ast::ExprList { elts, range, .. }) => (elts, *range, SequenceKind::List),
        ast::Expr::Tuple(tuple_node @ ast::ExprTuple { elts, range, .. }) => {
            (elts, *range, SequenceKind::Tuple(tuple_node))
        }
        _ => return,
    };

    let elts_analysis = SortClassification::of_elements(elts, SORTING_STYLE);
    if elts_analysis.is_not_a_list_of_string_literals() || elts_analysis.is_sorted() {
        return;
    }

    let mut diagnostic = Diagnostic::new(UnsortedDunderAll, range);

    if let SortClassification::UnsortedAndMaybeFixable { items } = elts_analysis {
        if let Some(fix) = create_fix(range, elts, &items, &kind, checker) {
            diagnostic.set_fix(fix);
        }
    }

    checker.diagnostics.push(diagnostic);
}

/// Attempt to return `Some(fix)`, where `fix` is a `Fix`
/// that can be set on the diagnostic to sort the user's
/// `__all__` definition
///
/// Return `None` if it's a multiline `__all__` definition
/// and the token-based analysis in
/// `MultilineDunderAllValue::from_source_range()` encounters
/// something it doesn't expect, meaning the violation
/// is unfixable in this instance.
fn create_fix(
    range: TextRange,
    elts: &[ast::Expr],
    string_items: &[&str],
    kind: &SequenceKind,
    checker: &Checker,
) -> Option<Fix> {
    let locator = checker.locator();
    let is_multiline = locator.contains_line_break(range);

    let sorted_source_code = {
        // The machinery in the `MultilineDunderAllValue` is actually
        // sophisticated enough that it would work just as well for
        // single-line `__all__` definitions, and we could reduce
        // the number of lines of code in this file by doing that.
        // Unfortunately, however, `MultilineDunderAllValue::from_source_range()`
        // must process every token in an `__all__` definition as
        // part of its analysis, and this is quite slow. For
        // single-line `__all__` definitions, it's also unnecessary,
        // as it's impossible to have comments in between the
        // `__all__` elements if the `__all__` definition is all on
        // a single line. Therefore, as an optimisation, we do the
        // bare minimum of token-processing for single-line `__all__`
        // definitions:
        if is_multiline {
            let value = MultilineStringSequenceValue::from_source_range(range, kind, locator)?;
            assert_eq!(value.len(), elts.len());
            value.into_sorted_source_code(SORTING_STYLE, locator, checker.stylist())
        } else {
            sort_single_line_elements_sequence(kind, elts, string_items, locator, SORTING_STYLE)
        }
    };

    Some(Fix::safe_edit(Edit::range_replacement(
        sorted_source_code,
        range,
    )))
}
