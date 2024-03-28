use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast, Arguments, ElifElseClause, Expr, ExprContext, Stmt};
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;
use crate::fix::snippet::SourceCodeSnippet;

/// ## What it does
/// Checks for `if` statements that can be replaced with a single assignment.
///
/// ## Why is this bad?
/// `if` statements that assign `True` for a truthy condition and `False` for
/// a falsey condition can be replaced with a single assignment.
///
/// ## Example
/// ```python
/// if x > 0:
///     y = True
/// else:
///     y = False
/// ```
///
/// Use instead:
/// ```python
/// y = x > 0
/// ```
///
#[violation]
pub struct SimplifiableIfStatement {
    condition: SourceCodeSnippet,
    replacement: Option<SourceCodeSnippet>,
}

impl Violation for SimplifiableIfStatement {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let SimplifiableIfStatement { condition, .. } = self;
        if let Some(condition) = condition.full_display() {
            format!("Assign the condition `{condition}` directly")
        } else {
            format!("Assign the condition directly")
        }
    }

    fn fix_title(&self) -> Option<String> {
        let SimplifiableIfStatement { replacement, .. } = self;
        if let Some(replacement) = replacement
            .as_ref()
            .and_then(SourceCodeSnippet::full_display)
        {
            Some(format!("Replace with `{replacement}`"))
        } else {
            Some(format!("Assign condition inline"))
        }
    }
}

/// PLR1703
pub(crate) fn simplifiable_if_statement(checker: &mut Checker, stmt: &Stmt) {
    let Stmt::If(stmt_if) = stmt else { return };
    let ast::StmtIf {
        test: if_test,
        body: if_body,
        elif_else_clauses,
        ..
    } = stmt_if;

    // Only consider single else
    // ```python
    // if x > 0:
    //     return True
    // else:
    //     return False
    // ```
    let [ElifElseClause {
        body: else_body,
        test: None,
        ..
    }] = elif_else_clauses.as_slice()
    else {
        return;
    };

    // Both branches must be one-liners that assign a boolean.
    let (Some((if_assign_value, if_assign_target)), Some((else_assign_value, else_assign_target))) = (
        is_one_line_assign_bool(if_body),
        is_one_line_assign_bool(else_body.as_slice()),
    ) else {
        return;
    };

    // Both branches must assign to the same variable
    if !match_targets(&if_assign_target, &else_assign_target) {
        return;
    }

    // Determine whether the assigned values are inverted, as in:
    // ```python
    // if x > 0:
    //     y = False
    // else:
    //     y = True
    // ```
    let inverted = match (if_assign_value, else_assign_value) {
        (Bool::True, Bool::False) => false,
        (Bool::False, Bool::True) => true,
        // If the branches have the same condition, abort (although the code could be
        // simplified).
        _ => return,
    };

    let if_test = if_test.as_ref();
    let condition = checker.locator().slice(if_test);
    let replacement = if checker
        .indexer()
        .has_comments(&stmt_if.range(), checker.locator())
    {
        None
    } else {
        // If the assigned values are inverted, wrap the condition in a `not`.
        if inverted {
            let node = ast::StmtAssign {
                value: Box::new(Expr::UnaryOp(ast::ExprUnaryOp {
                    op: ast::UnaryOp::Not,
                    operand: Box::new(if_test.clone()),
                    range: TextRange::default(),
                })),
                targets: if_assign_target,
                range: TextRange::default(),
            };
            Some(checker.generator().stmt(&node.into()))
        } else if if_test.is_compare_expr() {
            // If the condition is a comparison, we can replace it with the condition, since we
            // know it's a boolean.
            let node = ast::StmtAssign {
                value: Box::new(if_test.clone()),
                targets: if_assign_target,
                range: TextRange::default(),
            };
            Some(checker.generator().stmt(&node.into()))
        } else if checker.semantic().is_builtin("bool") {
            // Otherwise, we need to wrap the condition in a call to `bool`.
            let func_node = ast::ExprName {
                id: "bool".into(),
                ctx: ExprContext::Load,
                range: TextRange::default(),
            };
            let value_node = ast::ExprCall {
                func: Box::new(func_node.into()),
                arguments: Arguments {
                    args: Box::from([if_test.clone()]),
                    keywords: Box::from([]),
                    range: TextRange::default(),
                },
                range: TextRange::default(),
            };
            let assign_node = ast::StmtAssign {
                value: Box::new(value_node.into()),
                targets: if_assign_target,
                range: TextRange::default(),
            };
            Some(checker.generator().stmt(&assign_node.into()))
        } else {
            None
        }
    };

    let mut diagnostic = Diagnostic::new(
        SimplifiableIfStatement {
            condition: SourceCodeSnippet::from_str(condition),
            replacement: replacement.clone().map(SourceCodeSnippet::new),
        },
        stmt_if.range(),
    );
    if let Some(replacement) = replacement {
        diagnostic.set_fix(Fix::unsafe_edit(Edit::range_replacement(
            replacement,
            stmt_if.range(),
        )));
    }
    checker.diagnostics.push(diagnostic);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Bool {
    True,
    False,
}

impl From<bool> for Bool {
    fn from(value: bool) -> Self {
        if value {
            Bool::True
        } else {
            Bool::False
        }
    }
}

fn is_one_line_assign_bool(stmts: &[Stmt]) -> Option<(Bool, Vec<Expr>)> {
    let [stmt] = stmts else {
        return None;
    };
    let Stmt::Assign(ast::StmtAssign {
        value,
        targets,
        range: _,
    }) = stmt
    else {
        return None;
    };
    if targets.len() != 1 {
        return None;
    }
    let Expr::BooleanLiteral(ast::ExprBooleanLiteral { value, .. }) = **value else {
        return None;
    };
    Some((value.into(), targets.clone()))
}

fn match_targets(a: &Vec<Expr>, b: &Vec<Expr>) -> bool {
    let [ast::Expr::Name(target_a)] = a.as_slice() else {
        return false;
    };
    let [ast::Expr::Name(target_b)] = b.as_slice() else {
        return false;
    };
    target_a.id == target_b.id
}
