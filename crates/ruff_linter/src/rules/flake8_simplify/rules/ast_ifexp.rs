use ruff_python_ast::{self as ast, Arguments, Expr, ExprContext, UnaryOp, flat};
use ruff_text_size::{Ranged, TextRange};

use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::flat::helpers::{is_const_false, is_const_true};
use ruff_python_ast::name::Name;
use ruff_python_ast::token::parenthesized_range;

use crate::checkers::ast::Checker;
use crate::codes::Category;
use crate::{AlwaysFixableViolation, Edit, Fix, FixAvailability, Violation};

/// ## What it does
/// Checks for `if` expressions that can be replaced with `bool()` calls.
///
/// ## Why is this bad?
/// `if` expressions that evaluate to `True` for a truthy condition an `False`
/// for a falsey condition can be replaced with `bool()` calls, which are more
/// concise and readable.
///
/// ## Example
/// ```python
/// True if a else False
/// ```
///
/// Use instead:
/// ```python
/// bool(a)
/// ```
///
/// ## Fix safety
///
/// This fix is marked as unsafe because it may change the program’s behavior if the condition does not
/// return a proper Boolean. While the fix will try to wrap non-boolean values in a call to bool,
/// custom implementations of comparison functions like `__eq__` can avoid the bool call and still
/// lead to altered behavior. Moreover, the fix may remove comments.
///
/// ## References
/// - [Python documentation: Truth Value Testing](https://docs.python.org/3/library/stdtypes.html#truth-value-testing)
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.0.214", category = Category::Complexity)]
pub(crate) struct IfExprWithTrueFalse {
    is_compare: bool,
}

impl Violation for IfExprWithTrueFalse {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        if self.is_compare {
            "Remove unnecessary `True if ... else False`".to_string()
        } else {
            "Use `bool(...)` instead of `True if ... else False`".to_string()
        }
    }

    fn fix_title(&self) -> Option<String> {
        let title = if self.is_compare {
            "Remove unnecessary `True if ... else False`"
        } else {
            "Replace with `bool(...)"
        };
        Some(title.to_string())
    }
}

/// ## What it does
/// Checks for `if` expressions that can be replaced by negating a given
/// condition.
///
/// ## Why is this bad?
/// `if` expressions that evaluate to `False` for a truthy condition and `True`
/// for a falsey condition can be replaced with `not` operators, which are more
/// concise and readable.
///
/// ## Example
/// ```python
/// False if a else True
/// ```
///
/// Use instead:
/// ```python
/// not a
/// ```
///
/// ## References
/// - [Python documentation: Truth Value Testing](https://docs.python.org/3/library/stdtypes.html#truth-value-testing)
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.0.214", category = Category::Complexity)]
pub(crate) struct IfExprWithFalseTrue;

impl AlwaysFixableViolation for IfExprWithFalseTrue {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Use `not ...` instead of `False if ... else True`".to_string()
    }

    fn fix_title(&self) -> String {
        "Replace with `not ...`".to_string()
    }
}

/// ## What it does
/// Checks for `if` expressions that check against a negated condition.
///
/// ## Why is this bad?
/// `if` expressions that check against a negated condition are more difficult
/// to read than `if` expressions that check against the condition directly.
///
/// ## Example
/// ```python
/// b if not a else a
/// ```
///
/// Use instead:
/// ```python
/// a if a else b
/// ```
///
/// ## References
/// - [Python documentation: Truth Value Testing](https://docs.python.org/3/library/stdtypes.html#truth-value-testing)
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.0.214", category = Category::Pedantic)]
pub(crate) struct IfExprWithTwistedArms {
    expr_body: String,
    expr_else: String,
}

impl AlwaysFixableViolation for IfExprWithTwistedArms {
    #[derive_message_formats]
    fn message(&self) -> String {
        let IfExprWithTwistedArms {
            expr_body,
            expr_else,
        } = self;
        format!(
            "Use `{expr_else} if {expr_else} else {expr_body}` instead of `{expr_body} if not \
             {expr_else} else {expr_else}`"
        )
    }

    fn fix_title(&self) -> String {
        let IfExprWithTwistedArms {
            expr_body,
            expr_else,
        } = self;
        format!("Replace with `{expr_else} if {expr_else} else {expr_body}`")
    }
}

/// SIM210
pub(crate) fn if_expr_with_true_false(
    checker: &Checker,
    &expr: &flat::View<'_, Expr>,
    test: flat::View<'_, Expr>,
    body: flat::View<'_, Expr>,
    orelse: flat::View<'_, Expr>,
) {
    if !is_const_true(body) || !is_const_false(orelse) {
        return;
    }

    let mut diagnostic = checker.report_diagnostic(
        IfExprWithTrueFalse {
            is_compare: test.is_compare_expr(),
        },
        expr.range(),
    );
    if test.is_compare_expr() {
        diagnostic.set_fix(Fix::unsafe_edit(Edit::range_replacement(
            checker
                .locator()
                .slice(
                    parenthesized_range(test, expr.into(), checker.tokens())
                        .unwrap_or(test.range()),
                )
                .to_string(),
            expr.range(),
        )));
    } else if checker.semantic().has_builtin_binding("bool") {
        diagnostic.set_fix(Fix::unsafe_edit(Edit::range_replacement(
            checker.generator().expr(
                &ast::ExprCall {
                    func: Box::new(
                        ast::ExprName {
                            id: Name::new_static("bool"),
                            ctx: ExprContext::Load,
                            range: TextRange::default(),
                            node_index: ruff_python_ast::NodeIndex::NONE,
                        }
                        .into(),
                    ),
                    arguments: Arguments {
                        args: [test].map(flat::View::to_owned_node).into(),
                        keywords: std::iter::empty().collect(),
                        range: TextRange::default(),
                        node_index: ruff_python_ast::NodeIndex::NONE,
                    },
                    range_start: ruff_text_size::TextSize::default(),
                    node_index: ruff_python_ast::NodeIndex::NONE,
                }
                .into(),
            ),
            expr.range(),
        )));
    }
}

/// SIM211
pub(crate) fn if_expr_with_false_true(
    checker: &Checker,
    &expr: &flat::View<'_, Expr>,
    test: flat::View<'_, Expr>,
    body: flat::View<'_, Expr>,
    orelse: flat::View<'_, Expr>,
) {
    if !is_const_false(body) || !is_const_true(orelse) {
        return;
    }

    let mut diagnostic = checker.report_diagnostic(IfExprWithFalseTrue, expr.range());
    diagnostic.set_fix(Fix::unsafe_edit(Edit::range_replacement(
        checker.generator().expr(
            &ast::ExprUnaryOp {
                op: UnaryOp::Not,
                operand: Box::new(test.to_owned_node()),
                range: TextRange::default(),
                node_index: ruff_python_ast::NodeIndex::NONE,
            }
            .into(),
        ),
        expr.range(),
    )));
}

/// SIM212
pub(crate) fn twisted_arms_in_ifexpr(
    checker: &Checker,
    &expr: &flat::View<'_, Expr>,
    test: flat::View<'_, Expr>,
    body: flat::View<'_, Expr>,
    orelse: flat::View<'_, Expr>,
) {
    let flat::ExprKind::UnaryOp(unary_op) = test.classify() else {
        return;
    };
    let op = unary_op.op();
    let operand = unary_op.operand();
    if !op.is_not() {
        return;
    }

    // Check if the test operand and else branch use the same variable.
    let flat::ExprKind::Name(name_node) = operand.classify() else {
        return;
    };
    let test_id = name_node.id();
    let flat::ExprKind::Name(name_node) = orelse.classify() else {
        return;
    };
    let orelse_id = name_node.id();
    if !test_id.eq(orelse_id) {
        return;
    }

    let mut diagnostic = checker.report_diagnostic(
        IfExprWithTwistedArms {
            expr_body: checker.generator().expr_view(body),
            expr_else: checker.generator().expr_view(orelse),
        },
        expr.range(),
    );
    let node = body;
    let node1 = orelse;
    let node2 = orelse;
    let node3 = ast::ExprIf {
        test: Box::new(node2.to_owned_node()),
        body: Box::new(node1.to_owned_node()),
        orelse: Box::new(node.to_owned_node()),
        range: TextRange::default(),
        node_index: ruff_python_ast::NodeIndex::NONE,
    };
    diagnostic.set_fix(Fix::unsafe_edit(Edit::range_replacement(
        checker.generator().expr(&node3.into()),
        expr.range(),
    )));
}
