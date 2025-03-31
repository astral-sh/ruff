use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::{self as ast, Expr, ExprContext, Operator};
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;
use crate::fix::snippet::SourceCodeSnippet;

/// ## What it does
/// Checks for uses of the `+` operator to concatenate collections.
///
/// ## Why is this bad?
/// In Python, the `+` operator can be used to concatenate collections (e.g.,
/// `x + y` to concatenate the lists `x` and `y`).
///
/// However, collections can be concatenated more efficiently using the
/// unpacking operator (e.g., `[*x, *y]` to concatenate `x` and `y`).
///
/// Prefer the unpacking operator to concatenate collections, as it is more
/// readable and flexible. The `*` operator can unpack any iterable, whereas
///  `+` operates only on particular sequences which, in many cases, must be of
/// the same type.
///
/// ## Example
/// ```python
/// foo = [2, 3, 4]
/// bar = [1] + foo + [5, 6]
/// ```
///
/// Use instead:
/// ```python
/// foo = [2, 3, 4]
/// bar = [1, *foo, 5, 6]
/// ```
///
/// ## References
/// - [PEP 448 – Additional Unpacking Generalizations](https://peps.python.org/pep-0448/)
/// - [Python documentation: Sequence Types — `list`, `tuple`, `range`](https://docs.python.org/3/library/stdtypes.html#sequence-types-list-tuple-range)
#[derive(ViolationMetadata)]
pub(crate) struct CollectionLiteralConcatenation {
    expression: SourceCodeSnippet,
}

impl Violation for CollectionLiteralConcatenation {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        if let Some(expression) = self.expression.full_display() {
            format!("Consider `{expression}` instead of concatenation")
        } else {
            "Consider iterable unpacking instead of concatenation".to_string()
        }
    }

    fn fix_title(&self) -> Option<String> {
        let title = match self.expression.full_display() {
            Some(expression) => format!("Replace with `{expression}`"),
            None => "Replace with iterable unpacking".to_string(),
        };
        Some(title)
    }
}

fn make_splat_elts(
    splat_element: &Expr,
    other_elements: &[Expr],
    splat_at_left: bool,
) -> Vec<Expr> {
    let mut new_elts = other_elements.to_owned();
    let node = ast::ExprStarred {
        value: Box::from(splat_element.clone()),
        ctx: ExprContext::Load,
        range: TextRange::default(),
    };
    let splat = node.into();
    if splat_at_left {
        new_elts.insert(0, splat);
    } else {
        new_elts.push(splat);
    }
    new_elts
}

#[derive(Debug, Copy, Clone)]
enum Type {
    List,
    Tuple,
}

/// Recursively merge all the tuples and lists in the expression.
fn concatenate_expressions(expr: &Expr, should_support_slices: bool) -> Option<(Expr, Type)> {
    let Expr::BinOp(ast::ExprBinOp {
        left,
        op: Operator::Add,
        right,
        range: _,
    }) = expr
    else {
        return None;
    };

    let new_left = match left.as_ref() {
        Expr::BinOp(ast::ExprBinOp { .. }) => {
            match concatenate_expressions(left, should_support_slices) {
                Some((new_left, _)) => new_left,
                None => *left.clone(),
            }
        }
        _ => *left.clone(),
    };

    let new_right = match right.as_ref() {
        Expr::BinOp(ast::ExprBinOp { .. }) => {
            match concatenate_expressions(right, should_support_slices) {
                Some((new_right, _)) => new_right,
                None => *right.clone(),
            }
        }
        _ => *right.clone(),
    };

    // Figure out which way the splat is, and the type of the collection.
    let (type_, splat_element, other_elements, splat_at_left) = match (&new_left, &new_right) {
        (Expr::List(ast::ExprList { elts: l_elts, .. }), _) => {
            (Type::List, &new_right, l_elts, false)
        }
        (Expr::Tuple(ast::ExprTuple { elts: l_elts, .. }), _) => {
            (Type::Tuple, &new_right, l_elts, false)
        }
        (_, Expr::List(ast::ExprList { elts: r_elts, .. })) => {
            (Type::List, &new_left, r_elts, true)
        }
        (_, Expr::Tuple(ast::ExprTuple { elts: r_elts, .. })) => {
            (Type::Tuple, &new_left, r_elts, true)
        }
        _ => return None,
    };

    let new_elts = match splat_element {
        // We'll be a bit conservative here; only calls, names and attribute accesses
        // will be considered as splat elements.
        Expr::Call(_) | Expr::Attribute(_) | Expr::Name(_) => {
            make_splat_elts(splat_element, other_elements, splat_at_left)
        }
        // Subscripts are also considered safe-ish to splat if the indexer is a slice.
        Expr::Subscript(ast::ExprSubscript { slice, .. })
            if should_support_slices && matches!(&**slice, Expr::Slice(_)) =>
        {
            make_splat_elts(splat_element, other_elements, splat_at_left)
        }
        // If the splat element is itself a list/tuple, insert them in the other list/tuple.
        Expr::List(ast::ExprList { elts, .. }) if matches!(type_, Type::List) => {
            other_elements.iter().chain(elts).cloned().collect()
        }
        Expr::Tuple(ast::ExprTuple { elts, .. }) if matches!(type_, Type::Tuple) => {
            other_elements.iter().chain(elts).cloned().collect()
        }
        _ => return None,
    };

    let new_expr = match type_ {
        Type::List => ast::ExprList {
            elts: new_elts,
            ctx: ExprContext::Load,
            range: TextRange::default(),
        }
        .into(),
        Type::Tuple => ast::ExprTuple {
            elts: new_elts,
            ctx: ExprContext::Load,
            range: TextRange::default(),
            parenthesized: true,
        }
        .into(),
    };

    Some((new_expr, type_))
}

/// RUF005
pub(crate) fn collection_literal_concatenation(checker: &Checker, expr: &Expr) {
    // If the expression is already a child of an addition, we'll have analyzed it already.
    if matches!(
        checker.semantic().current_expression_parent(),
        Some(Expr::BinOp(ast::ExprBinOp {
            op: Operator::Add,
            ..
        }))
    ) {
        return;
    }

    let should_support_slices = checker.settings.preview.is_enabled();

    let Some((new_expr, type_)) = concatenate_expressions(expr, should_support_slices) else {
        return;
    };

    let contents = match type_ {
        // Wrap the new expression in parentheses if it was a tuple.
        Type::Tuple => format!("({})", checker.generator().expr(&new_expr)),
        Type::List => checker.generator().expr(&new_expr),
    };
    let mut diagnostic = Diagnostic::new(
        CollectionLiteralConcatenation {
            expression: SourceCodeSnippet::new(contents.clone()),
        },
        expr.range(),
    );
    if !checker
        .comment_ranges()
        .has_comments(expr, checker.source())
    {
        // This suggestion could be unsafe if the non-literal expression in the
        // expression has overridden the `__add__` (or `__radd__`) magic methods.
        diagnostic.set_fix(Fix::unsafe_edit(Edit::range_replacement(
            contents,
            expr.range(),
        )));
    }
    checker.report_diagnostic(diagnostic);
}
