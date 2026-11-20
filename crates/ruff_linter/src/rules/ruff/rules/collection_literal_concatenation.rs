use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::{Builder, Expr, ExprBinOpFields, ExprFields, ExprKind, NodeList, Operator};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::codes::Category;
use crate::fix::snippet::SourceCodeSnippet;
use crate::fix::syntax;
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
    Elements(NodeList<'a, ruff_python_ast::kind::Expr>),
    Splat(Expr<'a>),
}

/// Collect operands in evaluation order. A splat takes its collection type from
/// the other side of an addition. Two splats alone have no known collection type.
fn collect_parts<'a>(expr: Expr<'a>, parts: &mut Vec<Part<'a>>) -> Option<OperandType> {
    match expr.classify() {
        ExprKind::List(list) => {
            parts.push(Part::Elements(list.elts()));
            Some(OperandType::Collection(Type::List))
        }
        ExprKind::Tuple(tuple) => {
            parts.push(Part::Elements(tuple.elts()));
            Some(OperandType::Collection(Type::Tuple))
        }
        ExprKind::BinOp(sum) if sum.op() == Operator::Add => {
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
        ExprKind::Call(_) | ExprKind::Attribute(_) | ExprKind::Name(_) => {
            parts.push(Part::Splat(expr));
            Some(OperandType::Splat)
        }
        ExprKind::Subscript(subscript) if subscript.slice().is_slice_expr() => {
            parts.push(Part::Splat(expr));
            Some(OperandType::Splat)
        }
        _ => None,
    }
}

fn concatenate_expressions(expr: Expr<'_>, checker: &Checker) -> Option<String> {
    if !expr.is_bin_op_expr() {
        return None;
    }
    let mut parts = Vec::new();
    let OperandType::Collection(type_) = collect_parts(expr, &mut parts)? else {
        return None;
    };
    let mut builder = Builder::default();
    let mut new_elts = Vec::new();
    for part in parts {
        match part {
            Part::Elements(elements) => {
                new_elts.extend(elements.iter().map(|element| builder.copy_node(element)));
            }
            Part::Splat(element) => {
                let value = builder.copy_node(element);
                new_elts.push(syntax::starred(&mut builder, value));
            }
        }
    }
    let elts = builder.list(new_elts);
    Some(match type_ {
        Type::List => {
            let list = syntax::list(&mut builder, elts);
            checker.generator().expr(builder.get(list))
        }
        Type::Tuple => {
            let tuple = syntax::tuple(&mut builder, elts);
            format!("({})", checker.generator().expr(builder.get(tuple)))
        }
    })
}
/// RUF005
pub(crate) fn collection_literal_concatenation(checker: &Checker, &expr: &Expr<'_>) {
    // If the expression is already a child of an addition, we'll have analyzed it already.
    if matches!(
        checker
            .semantic()
            .current_expression_parent()
            .map(Expr::fields),
        Some(ExprFields::BinOp(ExprBinOpFields {
            op: Operator::Add,
            ..
        }))
    ) {
        return;
    }

    let Some(contents) = concatenate_expressions(expr, checker) else {
        return;
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
