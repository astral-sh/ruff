use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast, ElifElseClause, Expr, Stmt};
use ruff_python_semantic::analyze::typing::{is_sys_version_block, is_type_checking_block};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::fix::edits::fits;

/// ## What it does
/// Check for `if`-`else`-blocks that can be replaced with a ternary operator.
///
/// ## Why is this bad?
/// `if`-`else`-blocks that assign a value to a variable in both branches can
/// be expressed more concisely by using a ternary operator.
///
/// ## Example
/// ```python
/// if foo:
///     bar = x
/// else:
///     bar = y
/// ```
///
/// Use instead:
/// ```python
/// bar = x if foo else y
/// ```
///
/// ## References
/// - [Python documentation: Conditional expressions](https://docs.python.org/3/reference/expressions.html#conditional-expressions)
#[violation]
pub struct IfElseBlockInsteadOfIfExp {
    contents: String,
}

impl Violation for IfElseBlockInsteadOfIfExp {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let IfElseBlockInsteadOfIfExp { contents } = self;
        format!("Use ternary operator `{contents}` instead of `if`-`else`-block")
    }

    fn fix_title(&self) -> Option<String> {
        let IfElseBlockInsteadOfIfExp { contents } = self;
        Some(format!("Replace `if`-`else`-block with `{contents}`"))
    }
}

/// SIM108
pub(crate) fn if_else_block_instead_of_if_exp(checker: &mut Checker, stmt_if: &ast::StmtIf) {
    let ast::StmtIf {
        test,
        body,
        elif_else_clauses,
        range: _,
    } = stmt_if;

    // `test: None` to only match an `else` clause
    let [ElifElseClause {
        body: else_body,
        test: None,
        ..
    }] = elif_else_clauses.as_slice()
    else {
        return;
    };
    let [Stmt::Assign(ast::StmtAssign {
        targets: body_targets,
        value: body_value,
        ..
    })] = body.as_slice()
    else {
        return;
    };
    let [Stmt::Assign(ast::StmtAssign {
        targets: else_targets,
        value: else_value,
        ..
    })] = else_body.as_slice()
    else {
        return;
    };
    let ([body_target], [else_target]) = (body_targets.as_slice(), else_targets.as_slice()) else {
        return;
    };
    let Expr::Name(ast::ExprName { id: body_id, .. }) = body_target else {
        return;
    };
    let Expr::Name(ast::ExprName { id: else_id, .. }) = else_target else {
        return;
    };
    if body_id != else_id {
        return;
    }

    // Avoid suggesting ternary for `if (yield ...)`-style checks.
    // TODO(charlie): Fix precedence handling for yields in generator.
    if matches!(
        body_value.as_ref(),
        Expr::Yield(_) | Expr::YieldFrom(_) | Expr::Await(_)
    ) {
        return;
    }
    if matches!(
        else_value.as_ref(),
        Expr::Yield(_) | Expr::YieldFrom(_) | Expr::Await(_)
    ) {
        return;
    }

    let locator = checker.locator();

    // If the original expression has a line break
    // in either the "if-value" or the "else-value",
    // don't suggest to turn it into a ternary expression
    if locator.contains_line_break(body_value.range())
        || locator.contains_line_break(else_value.range())
    {
        return;
    }

    // Avoid suggesting ternary for `if sys.version_info >= ...`-style checks.
    if is_sys_version_block(stmt_if, checker.semantic()) {
        return;
    }

    // Avoid suggesting ternary for `if TYPE_CHECKING:`-style checks.
    if is_type_checking_block(stmt_if, checker.semantic()) {
        return;
    }

    let possible_replacement = format!(
        "{} = {} if {} else {}",
        locator.slice(body_target),
        locator.slice(&**body_value),
        locator.slice(&**test),
        locator.slice(&**else_value)
    );

    // Don't flag if the resulting expression would exceed the maximum line length.
    if !fits(
        &possible_replacement,
        stmt_if.into(),
        locator,
        checker.settings.pycodestyle.max_line_length,
        checker.settings.tab_size,
    ) {
        return;
    }

    let mut diagnostic = Diagnostic::new(
        IfElseBlockInsteadOfIfExp {
            contents: possible_replacement.clone(),
        },
        stmt_if.range(),
    );
    if !checker.indexer().has_comments(stmt_if, locator) {
        diagnostic.set_fix(Fix::unsafe_edit(Edit::range_replacement(
            possible_replacement,
            stmt_if.range(),
        )));
    }
    checker.diagnostics.push(diagnostic);
}
