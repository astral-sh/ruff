use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::any_over_expr;
use ruff_python_ast::{self as ast, ElifElseClause, Expr, Stmt};
use ruff_python_semantic::SemanticModel;
use ruff_text_size::{Ranged, TextRange};

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
pub(crate) fn use_ternary_operator(checker: &mut Checker, stmt: &Stmt) {
    let Stmt::If(ast::StmtIf {
        test,
        body,
        elif_else_clauses,
        range: _,
    }) = stmt
    else {
        return;
    };
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

    // Avoid suggesting ternary for `if sys.version_info >= ...`-style and
    // `if sys.platform.startswith("...")`-style checks.
    let ignored_call_paths: &[&[&str]] = &[&["sys", "version_info"], &["sys", "platform"]];
    if contains_call_path(test, ignored_call_paths, checker.semantic()) {
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

    let target_var = &body_target;
    let ternary = ternary(target_var, body_value, test, else_value);
    let contents = checker.generator().stmt(&ternary);

    // Don't flag if the resulting expression would exceed the maximum line length.
    if !fits(
        &contents,
        stmt.into(),
        checker.locator(),
        checker.settings.line_length,
        checker.settings.tab_size,
    ) {
        return;
    }

    let mut diagnostic = Diagnostic::new(
        IfElseBlockInsteadOfIfExp {
            contents: contents.clone(),
        },
        stmt.range(),
    );
    if !checker.indexer().has_comments(stmt, checker.locator()) {
        diagnostic.set_fix(Fix::unsafe_edit(Edit::range_replacement(
            contents,
            stmt.range(),
        )));
    }
    checker.diagnostics.push(diagnostic);
}

/// Return `true` if the `Expr` contains a reference to any of the given `${module}.${target}`.
fn contains_call_path(expr: &Expr, targets: &[&[&str]], semantic: &SemanticModel) -> bool {
    any_over_expr(expr, &|expr| {
        semantic
            .resolve_call_path(expr)
            .is_some_and(|call_path| targets.iter().any(|target| &call_path.as_slice() == target))
    })
}

fn ternary(target_var: &Expr, body_value: &Expr, test: &Expr, orelse_value: &Expr) -> Stmt {
    let node = ast::ExprIfExp {
        test: Box::new(test.clone()),
        body: Box::new(body_value.clone()),
        orelse: Box::new(orelse_value.clone()),
        range: TextRange::default(),
    };
    let node1 = ast::StmtAssign {
        targets: vec![target_var.clone()],
        value: Box::new(node.into()),
        range: TextRange::default(),
    };
    node1.into()
}
