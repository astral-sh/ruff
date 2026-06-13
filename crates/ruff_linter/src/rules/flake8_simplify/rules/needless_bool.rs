use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::comparable::ComparableExpr;
use ruff_python_ast::name::Name;
use ruff_python_ast::traversal;
use ruff_python_ast::{self as ast, Arguments, BoolOp, ElifElseClause, Expr, ExprContext, Stmt};
use ruff_python_semantic::analyze::typing::{is_sys_version_block, is_type_checking_block};
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;
use crate::fix::snippet::SourceCodeSnippet;
use crate::{Edit, Fix, FixAvailability, Violation};

/// ## What it does
/// Checks for `if` statements that can be replaced with `bool`.
///
/// ## Why is this bad?
/// `if` statements that return `True` for a truthy condition and `False` for
/// a falsy condition can be replaced with boolean casts.
///
/// ## Example
/// Given:
/// ```python
/// def foo(x: int) -> bool:
///     if x > 0:
///         return True
///     else:
///         return False
/// ```
///
/// Use instead:
/// ```python
/// def foo(x: int) -> bool:
///     return x > 0
/// ```
///
/// Or, given:
/// ```python
/// def foo(x: int) -> bool:
///     if x > 0:
///         return True
///     return False
/// ```
///
/// Use instead:
/// ```python
/// def foo(x: int) -> bool:
///     return x > 0
/// ```
///
/// ## Fix safety
///
/// This fix is marked as unsafe because it may change the program’s behavior if the condition does not
/// return a proper Boolean. While the fix will try to wrap non-boolean values in a call to bool,
/// custom implementations of comparison functions like `__eq__` can avoid the bool call and still
/// lead to altered behavior.
///
/// ## References
/// - [Python documentation: Truth Value Testing](https://docs.python.org/3/library/stdtypes.html#truth-value-testing)
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.0.214")]
pub(crate) struct NeedlessBool {
    condition: Option<SourceCodeSnippet>,
    negate: bool,
}

impl Violation for NeedlessBool {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let NeedlessBool { condition, negate } = self;
        if let Some(condition) = condition.as_ref().and_then(SourceCodeSnippet::full_display) {
            format!("Return the condition `{condition}` directly")
        } else if *negate {
            "Return the negated condition directly".to_string()
        } else {
            "Return the condition directly".to_string()
        }
    }

    fn fix_title(&self) -> Option<String> {
        let NeedlessBool { condition, .. } = self;
        Some(
            if let Some(condition) = condition.as_ref().and_then(SourceCodeSnippet::full_display) {
                format!("Replace with `return {condition}`")
            } else {
                "Inline condition".to_string()
            },
        )
    }
}

/// SIM103
pub(crate) fn needless_bool(checker: &Checker, stmt: &Stmt) {
    let Stmt::If(stmt_if) = stmt else { return };
    let ast::StmtIf {
        test: if_test,
        body: if_body,
        elif_else_clauses,
        ..
    } = stmt_if;

    // Extract an `if`/`elif` chain followed by an `else`, in which every
    // non-`else` branch returns the same boolean and the `else` returns the
    // opposite boolean.
    if let Some((last_clause, elif_clauses)) = elif_else_clauses.split_last() {
        if let ElifElseClause {
            body: else_body,
            test: None,
            ..
        } = last_clause
        {
            if !elif_clauses.is_empty() {
                let mut tests = vec![if_test.as_ref()];
                let mut returns = vec![is_one_line_return_bool(if_body)];

                for clause in elif_clauses {
                    let ElifElseClause {
                        body,
                        test: Some(test),
                        ..
                    } = clause
                    else {
                        tests.clear();
                        break;
                    };
                    tests.push(test);
                    returns.push(is_one_line_return_bool(body));
                }

                let else_return = is_one_line_return_bool(else_body);
                if let (true, Some(else_return)) = (
                    returns.iter().all(Option::is_some) && tests.len() > 1,
                    else_return,
                ) {
                    let branch_return = returns[0].unwrap();
                    if returns
                        .iter()
                        .all(|return_value| *return_value == Some(branch_return))
                        && branch_return != else_return
                        && !is_sys_version_block(stmt_if, checker.semantic())
                        && !is_type_checking_block(stmt_if, checker.semantic())
                    {
                        let range = stmt_if.range();
                        let condition = if checker
                            .comment_ranges()
                            .has_comments(&range, checker.source())
                        {
                            None
                        } else {
                            let combined_condition = common_conjunction_over_complementary_tests(
                                &tests,
                            )
                            .unwrap_or_else(|| {
                                Expr::BoolOp(ast::ExprBoolOp {
                                    op: BoolOp::Or,
                                    values: tests.iter().map(|test| (*test).clone()).collect(),
                                    range: TextRange::default(),
                                    node_index: ruff_python_ast::AtomicNodeIndex::NONE,
                                })
                            });
                            let combined_condition_is_boolean =
                                is_boolean_expression(&combined_condition);

                            if branch_return == Bool::False {
                                Some(Expr::UnaryOp(ast::ExprUnaryOp {
                                    op: ast::UnaryOp::Not,
                                    operand: Box::new(combined_condition),
                                    range: TextRange::default(),
                                    node_index: ruff_python_ast::AtomicNodeIndex::NONE,
                                }))
                            } else if combined_condition_is_boolean {
                                Some(combined_condition)
                            } else if checker.semantic().has_builtin_binding("bool") {
                                Some(bool_call(combined_condition))
                            } else {
                                None
                            }
                        };

                        let replacement = condition.as_ref().map(|expr| {
                            Stmt::Return(ast::StmtReturn {
                                value: Some(Box::new(expr.clone())),
                                range: TextRange::default(),
                                node_index: ruff_python_ast::AtomicNodeIndex::NONE,
                            })
                        });

                        let replacement = replacement
                            .as_ref()
                            .map(|stmt| checker.generator().stmt(stmt));
                        let condition = condition
                            .as_ref()
                            .map(|expr| checker.generator().expr(expr));

                        let mut diagnostic = checker.report_diagnostic(
                            NeedlessBool {
                                condition: condition.map(SourceCodeSnippet::new),
                                negate: branch_return == Bool::False,
                            },
                            range,
                        );
                        if let Some(replacement) = replacement {
                            diagnostic.set_fix(Fix::unsafe_edit(Edit::range_replacement(
                                replacement,
                                range,
                            )));
                        }
                        return;
                    }
                }
            }
        }
    }

    // Extract an `if` or `elif` (that returns) followed by an else (that returns the same value)
    let (if_test, if_body, else_body, range) = match elif_else_clauses.as_slice() {
        // if-else case:
        // ```python
        // if x > 0:
        //     return True
        // else:
        //     return False
        // ```
        [
            ElifElseClause {
                body: else_body,
                test: None,
                ..
            },
        ] => (
            if_test.as_ref(),
            if_body,
            else_body.as_slice(),
            stmt_if.range(),
        ),
        // elif-else case
        // ```python
        // if x > 0:
        //     return True
        // elif x < 0:
        //     return False
        // ```
        [
            ..,
            ElifElseClause {
                body: elif_body,
                test: Some(elif_test),
                range: elif_range,
                node_index: _,
            },
            ElifElseClause {
                body: else_body,
                test: None,
                range: else_range,
                node_index: _,
            },
        ] => (
            elif_test,
            elif_body,
            else_body.as_slice(),
            TextRange::new(elif_range.start(), else_range.end()),
        ),
        // if-implicit-else case:
        // ```python
        // if x > 0:
        //     return True
        // return False
        // ```
        [] => {
            // Fetching the next sibling is expensive, so do some validation early.
            if is_one_line_return_bool(if_body).is_none() {
                return;
            }

            // Fetch the next sibling statement.
            let Some(next_stmt) = checker
                .semantic()
                .current_statement_parent()
                .and_then(|parent| traversal::suite(stmt, parent))
                .and_then(|suite| suite.next_sibling())
            else {
                return;
            };

            // If the next sibling is not a return statement, abort.
            if !next_stmt.is_return_stmt() {
                return;
            }

            (
                if_test.as_ref(),
                if_body,
                std::slice::from_ref(next_stmt),
                TextRange::new(stmt_if.start(), next_stmt.end()),
            )
        }
        _ => return,
    };

    // Both branches must be one-liners that return a boolean.
    let (Some(if_return), Some(else_return)) = (
        is_one_line_return_bool(if_body),
        is_one_line_return_bool(else_body),
    ) else {
        return;
    };

    // Determine whether the return values are inverted, as in:
    // ```python
    // if x > 0:
    //     return False
    // else:
    //     return True
    // ```
    let inverted = match (if_return, else_return) {
        (Bool::True, Bool::False) => false,
        (Bool::False, Bool::True) => true,
        // If the branches have the same condition, abort (although the code could be
        // simplified).
        _ => return,
    };

    // Avoid suggesting ternary for `if sys.version_info >= ...`-style checks.
    if is_sys_version_block(stmt_if, checker.semantic()) {
        return;
    }

    // Avoid suggesting ternary for `if TYPE_CHECKING:`-style checks.
    if is_type_checking_block(stmt_if, checker.semantic()) {
        return;
    }

    // Generate the replacement condition.
    let condition = if checker
        .comment_ranges()
        .has_comments(&range, checker.source())
    {
        None
    } else {
        // If the return values are inverted, wrap the condition in a `not`.
        if inverted {
            match if_test {
                Expr::UnaryOp(ast::ExprUnaryOp {
                    op: ast::UnaryOp::Not,
                    operand,
                    ..
                }) => Some((**operand).clone()),

                Expr::Compare(ast::ExprCompare {
                    ops,
                    left,
                    comparators,
                    ..
                }) if matches!(
                    ops.as_ref(),
                    [ast::CmpOp::Eq
                        | ast::CmpOp::NotEq
                        | ast::CmpOp::In
                        | ast::CmpOp::NotIn
                        | ast::CmpOp::Is
                        | ast::CmpOp::IsNot]
                ) =>
                {
                    let ([op], [right]) = (ops.as_ref(), comparators.as_ref()) else {
                        unreachable!("Single comparison with multiple comparators");
                    };

                    Some(Expr::Compare(ast::ExprCompare {
                        ops: Box::new([op.negate()]),
                        left: left.clone(),
                        comparators: Box::new([right.clone()]),
                        range: TextRange::default(),
                        node_index: ruff_python_ast::AtomicNodeIndex::NONE,
                    }))
                }

                _ => Some(Expr::UnaryOp(ast::ExprUnaryOp {
                    op: ast::UnaryOp::Not,
                    operand: Box::new(if_test.clone()),
                    range: TextRange::default(),
                    node_index: ruff_python_ast::AtomicNodeIndex::NONE,
                })),
            }
        } else if if_test.is_compare_expr() {
            // If the condition is a comparison, we can replace it with the condition, since we
            // know it's a boolean.
            Some(if_test.clone())
        } else if checker.semantic().has_builtin_binding("bool") {
            // Otherwise, we need to wrap the condition in a call to `bool`.
            Some(bool_call(if_test.clone()))
        } else {
            None
        }
    };

    // Generate the replacement `return` statement.
    let replacement = condition.as_ref().map(|expr| {
        Stmt::Return(ast::StmtReturn {
            value: Some(Box::new(expr.clone())),
            range: TextRange::default(),
            node_index: ruff_python_ast::AtomicNodeIndex::NONE,
        })
    });

    // Generate source code.
    let replacement = replacement
        .as_ref()
        .map(|stmt| checker.generator().stmt(stmt));
    let condition = condition
        .as_ref()
        .map(|expr| checker.generator().expr(expr));

    let mut diagnostic = checker.report_diagnostic(
        NeedlessBool {
            condition: condition.map(SourceCodeSnippet::new),
            negate: inverted,
        },
        range,
    );
    if let Some(replacement) = replacement {
        diagnostic.set_fix(Fix::unsafe_edit(Edit::range_replacement(
            replacement,
            range,
        )));
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Bool {
    True,
    False,
}

impl From<bool> for Bool {
    fn from(value: bool) -> Self {
        if value { Bool::True } else { Bool::False }
    }
}

fn is_one_line_return_bool(stmts: &[Stmt]) -> Option<Bool> {
    let [stmt] = stmts else {
        return None;
    };
    let Stmt::Return(ast::StmtReturn {
        value,
        range: _,
        node_index: _,
    }) = stmt
    else {
        return None;
    };
    let Some(Expr::BooleanLiteral(ast::ExprBooleanLiteral { value, .. })) = value.as_deref() else {
        return None;
    };
    Some((*value).into())
}

fn common_conjunction_over_complementary_tests(tests: &[&Expr]) -> Option<Expr> {
    let [left, right] = tests else {
        return None;
    };

    let left_conjuncts = split_conjuncts(left);
    let right_conjuncts = split_conjuncts(right);

    let mut right_common = vec![false; right_conjuncts.len()];
    let mut common = Vec::new();
    let mut left_remainder = Vec::new();

    for left_conjunct in left_conjuncts {
        if let Some((index, _)) =
            right_conjuncts
                .iter()
                .enumerate()
                .find(|(index, right_conjunct)| {
                    !right_common[*index]
                        && ComparableExpr::from(left_conjunct)
                            == ComparableExpr::from(**right_conjunct)
                })
        {
            right_common[index] = true;
            common.push(left_conjunct.clone());
        } else {
            left_remainder.push(left_conjunct);
        }
    }

    let right_remainder: Vec<&Expr> = right_conjuncts
        .into_iter()
        .enumerate()
        .filter_map(|(index, conjunct)| (!right_common[index]).then_some(conjunct))
        .collect();

    let [left_remainder] = left_remainder.as_slice() else {
        return None;
    };
    let [right_remainder] = right_remainder.as_slice() else {
        return None;
    };

    if common.is_empty() || !are_complementary_tests(left_remainder, right_remainder) {
        return None;
    }

    Some(if common.len() == 1 {
        common.pop().unwrap()
    } else {
        Expr::BoolOp(ast::ExprBoolOp {
            op: BoolOp::And,
            values: common,
            range: TextRange::default(),
            node_index: ruff_python_ast::AtomicNodeIndex::NONE,
        })
    })
}

fn split_conjuncts(expr: &Expr) -> Vec<&Expr> {
    if let Expr::BoolOp(ast::ExprBoolOp {
        op: BoolOp::And,
        values,
        ..
    }) = expr
    {
        values.iter().collect()
    } else {
        vec![expr]
    }
}

fn are_complementary_tests(left: &Expr, right: &Expr) -> bool {
    if let Expr::UnaryOp(ast::ExprUnaryOp {
        op: ast::UnaryOp::Not,
        operand,
        ..
    }) = left
    {
        return ComparableExpr::from(operand.as_ref()) == ComparableExpr::from(right);
    }

    if let Expr::UnaryOp(ast::ExprUnaryOp {
        op: ast::UnaryOp::Not,
        operand,
        ..
    }) = right
    {
        return ComparableExpr::from(left) == ComparableExpr::from(operand.as_ref());
    }

    false
}

fn bool_call(expr: Expr) -> Expr {
    let func_node = ast::ExprName {
        id: Name::new_static("bool"),
        ctx: ExprContext::Load,
        range: TextRange::default(),
        node_index: ruff_python_ast::AtomicNodeIndex::NONE,
    };
    let call_node = ast::ExprCall {
        func: Box::new(func_node.into()),
        arguments: Arguments {
            args: Box::from([expr]),
            keywords: Box::from([]),
            range: TextRange::default(),
            node_index: ruff_python_ast::AtomicNodeIndex::NONE,
        },
        range: TextRange::default(),
        node_index: ruff_python_ast::AtomicNodeIndex::NONE,
    };
    Expr::Call(call_node)
}

fn is_boolean_expression(expr: &Expr) -> bool {
    match expr {
        Expr::Compare(_) => true,
        Expr::UnaryOp(ast::ExprUnaryOp {
            op: ast::UnaryOp::Not,
            ..
        }) => true,
        Expr::BoolOp(ast::ExprBoolOp { values, .. }) => values.iter().all(is_boolean_expression),
        _ => false,
    }
}
