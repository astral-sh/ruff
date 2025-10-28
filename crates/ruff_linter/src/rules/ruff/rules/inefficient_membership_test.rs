use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::{self as ast, CmpOp, Expr, InterpolatedStringElement};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::{Edit, Fix, FixAvailability, Violation};

/// ## What it does
/// Checks for membership tests on sequence (list/tuple) or set literals with
/// non-trivial elements that prevent Python's `LOAD_CONST` bytecode optimization.
///
/// ## Why is this bad?
/// Python's bytecode compiler can optimize membership tests against simple
/// literal containers (like `x in (1, 2, 3)`) by converting them to a single
/// `LOAD_CONST` operation. However, when containers contain non-trivial values
/// (like nested lists, dictionaries, function calls, or operations),
/// Python must reconstruct the container elements on every membership test,
/// leading to significant performance degradation.
///
/// This is particularly problematic in hot code paths, as the container is
/// rebuilt every time the membership test is evaluated.
///
/// ## Example
///
/// ```python
/// # List of lists forces BUILD_LIST operations
/// if item in [[1, 2], [3, 4]]:
///     ...
///
/// # List with function calls
/// if value in [func(), other()]:
///     ...
/// ```
///
/// Use instead:
///
/// ```python
/// # Direct equality checks
/// if item == [1, 2] or item == [3, 4]:
///     ...
///
/// # Or pre-compute at module level
/// VALID_ITEMS = [[1, 2], [3, 4]]
/// if item in VALID_ITEMS:
///     ...
/// ```
///
/// ## Fix safety
/// The fix is marked as unsafe because:
/// - Converting `x in container` to `x == a or x == b` changes short-circuit
///   evaluation behavior (original evaluates all elements before membership test).
/// - Custom `__eq__` implementations may have side effects.
/// - Comments within the expression might be lost during the transformation.
#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "0.15.0")]
pub(crate) struct InefficientMembershipTest {
    container_type: ContainerType,
}

impl Violation for InefficientMembershipTest {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Always;

    #[derive_message_formats]
    fn message(&self) -> String {
        let InefficientMembershipTest { container_type } = self;
        let container = match container_type {
            ContainerType::Sequence => "sequence",
            ContainerType::Set => "set",
        };
        format!(
            "Membership test on {container} with complex elements forces element reconstruction on each test"
        )
    }

    fn fix_title(&self) -> Option<String> {
        Some("Convert to equality comparison".to_string())
    }
}

/// RUF066
pub(crate) fn inefficient_membership_test(checker: &Checker, compare: &ast::ExprCompare) {
    let [op] = compare.ops.as_ref() else {
        return;
    };

    if !matches!(op, CmpOp::In | CmpOp::NotIn) {
        return;
    }

    let [right] = &*compare.comparators else {
        return;
    };

    let Some(container_type) = has_non_trivial_elements(right) else {
        return;
    };

    let fix = generate_fix(compare, right, *op, checker);
    let mut diagnostic = checker.report_diagnostic(
        InefficientMembershipTest { container_type },
        compare.range(),
    );
    diagnostic.set_fix(Fix::unsafe_edit(fix));
}

#[derive(Debug, Copy, Clone)]
enum ContainerType {
    Sequence,
    Set,
}

/// Check if the expression contains non-trivial elements that prevent `LOAD_CONST` optimization.
fn has_non_trivial_elements(expr: &Expr) -> Option<ContainerType> {
    let (elts, container_type) = match expr {
        Expr::List(ast::ExprList { elts, .. }) | Expr::Tuple(ast::ExprTuple { elts, .. }) => {
            (elts, ContainerType::Sequence)
        }
        Expr::Set(ast::ExprSet { elts, .. }) => (elts, ContainerType::Set),
        _ => return None,
    };

    (!elts.is_empty() && elts.iter().any(is_complex_element)).then_some(container_type)
}

/// Check if an element requires runtime construction (not const-folded by Python).
///
/// Returns `true` if the element prevents Python's `LOAD_CONST` optimization.
fn is_complex_element(expr: &Expr) -> bool {
    match expr {
        // Literals are const-folded
        _ if expr.is_literal_expr() => false,

        // Tuples of literals are const-folded
        Expr::Tuple(ast::ExprTuple { elts, .. }) => elts.iter().any(is_complex_element),

        // Operations on literals are const-folded (e.g., `1+2` → `3`)
        Expr::BinOp(ast::ExprBinOp { left, right, .. }) => {
            is_complex_element(left) || is_complex_element(right)
        }
        Expr::UnaryOp(ast::ExprUnaryOp { operand, .. }) => is_complex_element(operand),

        // Boolean operations on literals are const-folded (e.g., `2.0 or True` → `2.0`)
        Expr::BoolOp(ast::ExprBoolOp { values, .. }) => values.iter().any(is_complex_element),

        // F-strings without interpolation are const-folded (e.g., `f'hello'` → `'hello'`)
        Expr::FString(f_string) => f_string
            .value
            .elements()
            .any(|element| matches!(element, InterpolatedStringElement::Interpolation(_))),

        // Everything else requires runtime construction (names, calls, containers, etc.)
        _ => true,
    }
}

/// Check if an expression needs parentheses when used as the right operand of `==` or `!=`.
fn needs_parentheses(expr: &Expr) -> bool {
    matches!(
        expr,
        Expr::Lambda(_) | Expr::BoolOp(_) | Expr::Named(_) | Expr::Compare(_) | Expr::If(_)
    )
}

/// Generate a fix by converting to equality comparisons.
fn generate_fix(
    compare: &ast::ExprCompare,
    container: &Expr,
    op: CmpOp,
    checker: &Checker,
) -> Edit {
    let (Expr::List(ast::ExprList { elts: elements, .. })
    | Expr::Tuple(ast::ExprTuple { elts: elements, .. })
    | Expr::Set(ast::ExprSet { elts: elements, .. })) = container
    else {
        unreachable!("has_non_trivial_elements already validated container type")
    };

    let left_source = &checker.source()[compare.left.range()];
    let is_not_in = matches!(op, CmpOp::NotIn);
    let cmp_op = if is_not_in { " != " } else { " == " };
    let logical_op = if is_not_in { " and " } else { " or " };

    let replacement = elements
        .iter()
        .map(|element| {
            let right_source = &checker.source()[element.range()];
            if needs_parentheses(element) {
                format!("{left_source}{cmp_op}({right_source})")
            } else {
                format!("{left_source}{cmp_op}{right_source}")
            }
        })
        .collect::<Vec<_>>()
        .join(logical_op);

    Edit::range_replacement(replacement, compare.range())
}
