use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::flat::{self, fields};
use ruff_python_ast::{self as ast, Expr, ExprContext, Operator};
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;
use crate::codes::Category;
use crate::fix::snippet::SourceCodeSnippet;
use crate::{Edit, Fix, FixAvailability, Violation};

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
/// ## Fix safety
///
/// The fix is always marked as unsafe because the `+` operator uses the `__add__` magic method and
/// `*`-unpacking uses the `__iter__` magic method. Both of these could have custom
/// implementations, causing the fix to change program behaviour.
///
/// ## References
/// - [PEP 448 – Additional Unpacking Generalizations](https://peps.python.org/pep-0448/)
/// - [Python documentation: Sequence Types — `list`, `tuple`, `range`](https://docs.python.org/3/library/stdtypes.html#sequence-types-list-tuple-range)
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.0.227", category = Category::Pedantic)]
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

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum Type {
    List,
    Tuple,
}

enum OperandType {
    Collection(Type),
    Splat,
}

enum Part<'a> {
    Elements(flat::NodeList<'a, Expr>),
    Splat(flat::View<'a, Expr>),
}

/// Collect operands in evaluation order. A splat takes its collection type from
/// the other side of an addition. Two splats alone have no known collection type.
fn collect_parts<'a>(expr: flat::View<'a, Expr>, parts: &mut Vec<Part<'a>>) -> Option<OperandType> {
    match expr.classify() {
        flat::ExprKind::List(list) => {
            parts.push(Part::Elements(list.elts()));
            Some(OperandType::Collection(Type::List))
        }
        flat::ExprKind::Tuple(tuple) => {
            parts.push(Part::Elements(tuple.elts()));
            Some(OperandType::Collection(Type::Tuple))
        }
        flat::ExprKind::BinOp(sum) if sum.op() == Operator::Add => {
            let left = collect_parts(sum.left(), parts)?;
            let right = collect_parts(sum.right(), parts)?;
            match (left, right) {
                (OperandType::Collection(a), OperandType::Collection(b)) if a == b => {
                    Some(OperandType::Collection(a))
                }
                (OperandType::Collection(kind), OperandType::Splat)
                | (OperandType::Splat, OperandType::Collection(kind)) => {
                    Some(OperandType::Collection(kind))
                }
                _ => None,
            }
        }
        flat::ExprKind::Call(_) | flat::ExprKind::Attribute(_) | flat::ExprKind::Name(_) => {
            parts.push(Part::Splat(expr));
            Some(OperandType::Splat)
        }
        flat::ExprKind::Subscript(subscript) if subscript.slice().is_slice_expr() => {
            parts.push(Part::Splat(expr));
            Some(OperandType::Splat)
        }
        _ => None,
    }
}

fn concatenate_expressions(expr: flat::View<'_, Expr>) -> Option<(Expr, Type)> {
    if !expr.is_bin_op_expr() {
        return None;
    }
    let mut parts = Vec::new();
    let OperandType::Collection(type_) = collect_parts(expr, &mut parts)? else {
        return None;
    };
    let mut new_elts = Vec::new();
    for part in parts {
        match part {
            Part::Elements(elements) => {
                new_elts.extend(elements.iter().map(flat::View::to_owned_node));
            }
            Part::Splat(element) => new_elts.push(Expr::Starred(ast::ExprStarred {
                value: Box::new(element.to_owned_node()),
                ctx: ExprContext::Load,
                range: TextRange::default(),
                node_index: ruff_python_ast::NodeIndex::NONE,
            })),
        }
    }
    let new_expr = match type_ {
        Type::List => ast::ExprList {
            elts: new_elts,
            ctx: ExprContext::Load,
            range: TextRange::default(),
            node_index: ruff_python_ast::NodeIndex::NONE,
        }
        .into(),
        Type::Tuple => ast::ExprTuple {
            elts: new_elts,
            ctx: ExprContext::Load,
            range: TextRange::default(),
            node_index: ruff_python_ast::NodeIndex::NONE,
            parenthesized: true,
        }
        .into(),
    };

    Some((new_expr, type_))
}

/// RUF005
pub(crate) fn collection_literal_concatenation(checker: &Checker, &expr: &flat::View<'_, Expr>) {
    // If the expression is already a child of an addition, we'll have analyzed it already.
    if matches!(
        checker
            .semantic()
            .current_expression_parent()
            .map(flat::View::<Expr>::fields),
        Some(fields::Expr::BinOp(fields::ExprBinOp {
            op: Operator::Add,
            ..
        }))
    ) {
        return;
    }

    let Some((new_expr, type_)) = concatenate_expressions(expr) else {
        return;
    };

    let contents = match type_ {
        // Wrap the new expression in parentheses if it was a tuple.
        Type::Tuple => format!("({})", checker.generator().expr(&new_expr)),
        Type::List => checker.generator().expr(&new_expr),
    };
    let mut diagnostic = checker.report_diagnostic(
        CollectionLiteralConcatenation {
            expression: SourceCodeSnippet::new(contents.clone()),
        },
        expr.range(),
    );
    if !checker
        .comment_ranges()
        .has_comments(&expr, checker.source())
    {
        // This suggestion could be unsafe if the non-literal expression in the
        // expression has overridden the `__add__` (or `__radd__`) magic methods.
        diagnostic.set_fix(Fix::unsafe_edit(Edit::range_replacement(
            contents,
            expr.range(),
        )));
    }
}
