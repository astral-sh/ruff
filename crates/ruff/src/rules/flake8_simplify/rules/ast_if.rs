use log::error;
use rustc_hash::FxHashSet;
use rustpython_parser::ast::{Cmpop, Constant, Expr, ExprContext, ExprKind, Stmt, StmtKind};
use unicode_width::UnicodeWidthStr;

use ruff_diagnostics::{AutofixKind, Diagnostic, Edit, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::comparable::{ComparableConstant, ComparableExpr, ComparableStmt};
use ruff_python_ast::helpers::{
    any_over_expr, contains_effect, create_expr, create_stmt, first_colon_range, has_comments,
    has_comments_in, unparse_expr, unparse_stmt,
};
use ruff_python_ast::newlines::StrExt;
use ruff_python_ast::types::Range;
use ruff_python_semantic::context::Context;

use crate::checkers::ast::Checker;
use crate::registry::AsRule;
use crate::rules::flake8_simplify::rules::fix_if;

fn compare_expr(expr1: &ComparableExpr, expr2: &ComparableExpr) -> bool {
    expr1.eq(expr2)
}

fn compare_stmt(stmt1: &ComparableStmt, stmt2: &ComparableStmt) -> bool {
    stmt1.eq(stmt2)
}

fn compare_body(body1: &[Stmt], body2: &[Stmt]) -> bool {
    if body1.len() != body2.len() {
        return false;
    }
    body1
        .iter()
        .zip(body2.iter())
        .all(|(stmt1, stmt2)| compare_stmt(&stmt1.into(), &stmt2.into()))
}

#[violation]
pub struct CollapsibleIf {
    pub fixable: bool,
}

impl Violation for CollapsibleIf {
    const AUTOFIX: AutofixKind = AutofixKind::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use a single `if` statement instead of nested `if` statements")
    }

    fn autofix_title_formatter(&self) -> Option<fn(&Self) -> String> {
        self.fixable
            .then_some(|_| format!("Combine `if` statements using `and`"))
    }
}

#[violation]
pub struct NeedlessBool {
    pub condition: String,
    pub fixable: bool,
}

impl Violation for NeedlessBool {
    const AUTOFIX: AutofixKind = AutofixKind::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let NeedlessBool { condition, .. } = self;
        format!("Return the condition `{condition}` directly")
    }

    fn autofix_title_formatter(&self) -> Option<fn(&Self) -> String> {
        self.fixable.then_some(|NeedlessBool { condition, .. }| {
            format!("Replace with `return {condition}`")
        })
    }
}

/// ## What it does
/// Checks for three or more consecutive if-statements with direct returns
///
/// ## Why is this bad?
/// These can be simplified by using a dictionary
///
/// ## Example
/// ```python
/// if x == 1:
///     return "Hello"
/// elif x == 2:
///     return "Goodbye"
/// else:
///     return "Goodnight"
/// ```
///
/// Use instead:
/// ```python
/// return {1: "Hello", 2: "Goodbye"}.get(x, "Goodnight")
/// ```
#[violation]
pub struct IfElseBlockInsteadOfDictLookup;

impl Violation for IfElseBlockInsteadOfDictLookup {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use a dictionary instead of consecutive `if` statements")
    }
}

#[violation]
pub struct IfElseBlockInsteadOfIfExp {
    pub contents: String,
    pub fixable: bool,
}

impl Violation for IfElseBlockInsteadOfIfExp {
    const AUTOFIX: AutofixKind = AutofixKind::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let IfElseBlockInsteadOfIfExp { contents, .. } = self;
        format!("Use ternary operator `{contents}` instead of `if`-`else`-block")
    }

    fn autofix_title_formatter(&self) -> Option<fn(&Self) -> String> {
        self.fixable
            .then_some(|IfElseBlockInsteadOfIfExp { contents, .. }| {
                format!("Replace `if`-`else`-block with `{contents}`")
            })
    }
}

/// ## What it does
/// Checks for `if` branches with identical arm bodies.
///
/// ## Why is this bad?
/// If multiple arms of an `if` statement have the same body, using `or`
/// better signals the intent of the statement.
///
/// ## Example
/// ```python
/// if x == 1:
///     print("Hello")
/// elif x == 2:
///     print("Hello")
/// ```
///
/// Use instead:
/// ```python
/// if x == 1 or x == 2:
///     print("Hello")
/// ```
#[violation]
pub struct IfWithSameArms;

impl Violation for IfWithSameArms {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Combine `if` branches using logical `or` operator")
    }
}

#[violation]
pub struct IfElseBlockInsteadOfDictGet {
    pub contents: String,
    pub fixable: bool,
}

impl Violation for IfElseBlockInsteadOfDictGet {
    const AUTOFIX: AutofixKind = AutofixKind::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let IfElseBlockInsteadOfDictGet { contents, .. } = self;
        format!("Use `{contents}` instead of an `if` block")
    }

    fn autofix_title_formatter(&self) -> Option<fn(&Self) -> String> {
        self.fixable
            .then_some(|IfElseBlockInsteadOfDictGet { contents, .. }| {
                format!("Replace with `{contents}`")
            })
    }
}

fn is_main_check(expr: &Expr) -> bool {
    if let ExprKind::Compare {
        left, comparators, ..
    } = &expr.node
    {
        if let ExprKind::Name { id, .. } = &left.node {
            if id == "__name__" {
                if comparators.len() == 1 {
                    if let ExprKind::Constant {
                        value: Constant::Str(value),
                        ..
                    } = &comparators[0].node
                    {
                        if value == "__main__" {
                            return true;
                        }
                    }
                }
            }
        }
    }
    false
}

/// Find the last nested if statement and return the test expression and the
/// first statement.
///
/// ```python
/// if xxx:
///     if yyy:
///      # ^^^ returns this expression
///         z = 1
///       # ^^^^^ and this statement
///         ...
/// ```
fn find_last_nested_if(body: &[Stmt]) -> Option<(&Expr, &Stmt)> {
    let [Stmt { node: StmtKind::If { test, body: inner_body, orelse }, ..}] = body else { return None };
    if !orelse.is_empty() {
        return None;
    }
    find_last_nested_if(inner_body).or_else(|| {
        Some((
            test,
            inner_body.last().expect("Expected body to be non-empty"),
        ))
    })
}

/// SIM102
pub fn nested_if_statements(
    checker: &mut Checker,
    stmt: &Stmt,
    test: &Expr,
    body: &[Stmt],
    orelse: &[Stmt],
    parent: Option<&Stmt>,
) {
    // If the parent could contain a nested if-statement, abort.
    if let Some(parent) = parent {
        if let StmtKind::If { body, orelse, .. } = &parent.node {
            if orelse.is_empty() && body.len() == 1 {
                return;
            }
        }
    }

    // If this if-statement has an else clause, or more than one child, abort.
    if !(orelse.is_empty() && body.len() == 1) {
        return;
    }

    // Allow `if __name__ == "__main__":` statements.
    if is_main_check(test) {
        return;
    }

    // Allow `if True:` and `if False:` statements.
    if matches!(
        test.node,
        ExprKind::Constant {
            value: Constant::Bool(..),
            ..
        }
    ) {
        return;
    }

    // Find the deepest nested if-statement, to inform the range.
    let Some((test, first_stmt)) = find_last_nested_if(body) else {
        return;
    };

    let colon = first_colon_range(
        Range::new(test.end_location.unwrap(), first_stmt.location),
        checker.locator,
    );

    // The fixer preserves comments in the nested body, but removes comments between
    // the outer and inner if statements.
    let nested_if = &body[0];
    let fixable = !has_comments_in(
        Range::new(stmt.location, nested_if.location),
        checker.locator,
    );

    let mut diagnostic = Diagnostic::new(
        CollapsibleIf { fixable },
        colon.map_or_else(
            || Range::from(stmt),
            |colon| Range::new(stmt.location, colon.end_location),
        ),
    );
    if fixable && checker.patch(diagnostic.kind.rule()) {
        match fix_if::fix_nested_if_statements(checker.locator, checker.stylist, stmt) {
            Ok(fix) => {
                if fix
                    .content()
                    .unwrap_or_default()
                    .universal_newlines()
                    .all(|line| line.width() <= checker.settings.line_length)
                {
                    diagnostic.set_fix(fix);
                }
            }
            Err(err) => error!("Failed to fix nested if: {err}"),
        }
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

fn is_one_line_return_bool(stmts: &[Stmt]) -> Option<Bool> {
    if stmts.len() != 1 {
        return None;
    }
    let StmtKind::Return { value } = &stmts[0].node else {
        return None;
    };
    let Some(ExprKind::Constant { value, .. }) = value.as_ref().map(|value| &value.node) else {
        return None;
    };
    let Constant::Bool(value) = value else {
        return None;
    };
    Some((*value).into())
}

/// SIM103
pub fn needless_bool(checker: &mut Checker, stmt: &Stmt) {
    let StmtKind::If { test, body, orelse } = &stmt.node else {
        return;
    };
    let (Some(if_return), Some(else_return)) = (is_one_line_return_bool(body), is_one_line_return_bool(orelse)) else {
        return;
    };

    // If the branches have the same condition, abort (although the code could be
    // simplified).
    if if_return == else_return {
        return;
    }

    let condition = unparse_expr(test, checker.stylist);
    let fixable = matches!(if_return, Bool::True)
        && matches!(else_return, Bool::False)
        && !has_comments(stmt, checker.locator)
        && (matches!(test.node, ExprKind::Compare { .. }) || checker.ctx.is_builtin("bool"));

    let mut diagnostic = Diagnostic::new(NeedlessBool { condition, fixable }, Range::from(stmt));
    if fixable && checker.patch(diagnostic.kind.rule()) {
        if matches!(test.node, ExprKind::Compare { .. }) {
            // If the condition is a comparison, we can replace it with the condition.
            diagnostic.set_fix(Edit::replacement(
                unparse_stmt(
                    &create_stmt(StmtKind::Return {
                        value: Some(test.clone()),
                    }),
                    checker.stylist,
                ),
                stmt.location,
                stmt.end_location.unwrap(),
            ));
        } else {
            // Otherwise, we need to wrap the condition in a call to `bool`. (We've already
            // verified, above, that `bool` is a builtin.)
            diagnostic.set_fix(Edit::replacement(
                unparse_stmt(
                    &create_stmt(StmtKind::Return {
                        value: Some(Box::new(create_expr(ExprKind::Call {
                            func: Box::new(create_expr(ExprKind::Name {
                                id: "bool".to_string(),
                                ctx: ExprContext::Load,
                            })),
                            args: vec![(**test).clone()],
                            keywords: vec![],
                        }))),
                    }),
                    checker.stylist,
                ),
                stmt.location,
                stmt.end_location.unwrap(),
            ));
        };
    }
    checker.diagnostics.push(diagnostic);
}

fn ternary(target_var: &Expr, body_value: &Expr, test: &Expr, orelse_value: &Expr) -> Stmt {
    create_stmt(StmtKind::Assign {
        targets: vec![target_var.clone()],
        value: Box::new(create_expr(ExprKind::IfExp {
            test: Box::new(test.clone()),
            body: Box::new(body_value.clone()),
            orelse: Box::new(orelse_value.clone()),
        })),
        type_comment: None,
    })
}

/// Return `true` if the `Expr` contains a reference to `${module}.${target}`.
fn contains_call_path(ctx: &Context, expr: &Expr, target: &[&str]) -> bool {
    any_over_expr(expr, &|expr| {
        ctx.resolve_call_path(expr)
            .map_or(false, |call_path| call_path.as_slice() == target)
    })
}

/// SIM108
pub fn use_ternary_operator(checker: &mut Checker, stmt: &Stmt, parent: Option<&Stmt>) {
    let StmtKind::If { test, body, orelse } = &stmt.node else {
        return;
    };
    if body.len() != 1 || orelse.len() != 1 {
        return;
    }
    let StmtKind::Assign { targets: body_targets, value: body_value, .. } = &body[0].node else {
        return;
    };
    let StmtKind::Assign { targets: orelse_targets, value: orelse_value, .. } = &orelse[0].node else {
        return;
    };
    if body_targets.len() != 1 || orelse_targets.len() != 1 {
        return;
    }
    let ExprKind::Name { id: body_id, .. } = &body_targets[0].node else {
        return;
    };
    let ExprKind::Name { id: orelse_id, .. } = &orelse_targets[0].node else {
        return;
    };
    if body_id != orelse_id {
        return;
    }

    // Avoid suggesting ternary for `if sys.version_info >= ...`-style checks.
    if contains_call_path(&checker.ctx, test, &["sys", "version_info"]) {
        return;
    }

    // Avoid suggesting ternary for `if sys.platform.startswith("...")`-style
    // checks.
    if contains_call_path(&checker.ctx, test, &["sys", "platform"]) {
        return;
    }

    // It's part of a bigger if-elif block:
    // https://github.com/MartinThoma/flake8-simplify/issues/115
    if let Some(StmtKind::If {
        orelse: parent_orelse,
        ..
    }) = parent.map(|parent| &parent.node)
    {
        if parent_orelse.len() == 1 && stmt == &parent_orelse[0] {
            // TODO(charlie): These two cases have the same AST:
            //
            // if True:
            //     pass
            // elif a:
            //     b = 1
            // else:
            //     b = 2
            //
            // if True:
            //     pass
            // else:
            //     if a:
            //         b = 1
            //     else:
            //         b = 2
            //
            // We want to flag the latter, but not the former. Right now, we flag neither.
            return;
        }
    }

    // Avoid suggesting ternary for `if (yield ...)`-style checks.
    // TODO(charlie): Fix precedence handling for yields in generator.
    if matches!(
        body_value.node,
        ExprKind::Yield { .. } | ExprKind::YieldFrom { .. } | ExprKind::Await { .. }
    ) {
        return;
    }
    if matches!(
        orelse_value.node,
        ExprKind::Yield { .. } | ExprKind::YieldFrom { .. } | ExprKind::Await { .. }
    ) {
        return;
    }

    let target_var = &body_targets[0];
    let ternary = ternary(target_var, body_value, test, orelse_value);
    let contents = unparse_stmt(&ternary, checker.stylist);

    // Don't flag if the resulting expression would exceed the maximum line length.
    if stmt.location.column() + contents.width() > checker.settings.line_length {
        return;
    }

    let fixable = !has_comments(stmt, checker.locator);
    let mut diagnostic = Diagnostic::new(
        IfElseBlockInsteadOfIfExp {
            contents: contents.clone(),
            fixable,
        },
        Range::from(stmt),
    );
    if fixable && checker.patch(diagnostic.kind.rule()) {
        diagnostic.set_fix(Edit::replacement(
            contents,
            stmt.location,
            stmt.end_location.unwrap(),
        ));
    }
    checker.diagnostics.push(diagnostic);
}

fn get_if_body_pairs<'a>(
    test: &'a Expr,
    body: &'a [Stmt],
    orelse: &'a [Stmt],
) -> Vec<(&'a Expr, &'a [Stmt])> {
    let mut pairs = vec![(test, body)];
    let mut orelse = orelse;
    loop {
        if orelse.len() != 1 {
            break;
        }
        let StmtKind::If { test, body, orelse: orelse_orelse, .. } = &orelse[0].node else {
            break;
        };
        pairs.push((test, body));
        orelse = orelse_orelse;
    }
    pairs
}

/// SIM114
pub fn if_with_same_arms(checker: &mut Checker, stmt: &Stmt, parent: Option<&Stmt>) {
    let StmtKind::If { test, body, orelse } = &stmt.node else {
        return;
    };

    // It's part of a bigger if-elif block:
    // https://github.com/MartinThoma/flake8-simplify/issues/115
    if let Some(StmtKind::If {
        orelse: parent_orelse,
        ..
    }) = parent.map(|parent| &parent.node)
    {
        if parent_orelse.len() == 1 && stmt == &parent_orelse[0] {
            // TODO(charlie): These two cases have the same AST:
            //
            // if True:
            //     pass
            // elif a:
            //     b = 1
            // else:
            //     b = 2
            //
            // if True:
            //     pass
            // else:
            //     if a:
            //         b = 1
            //     else:
            //         b = 2
            //
            // We want to flag the latter, but not the former. Right now, we flag neither.
            return;
        }
    }

    let if_body_pairs = get_if_body_pairs(test, body, orelse);
    for i in 0..(if_body_pairs.len() - 1) {
        let (test, body) = &if_body_pairs[i];
        let (.., next_body) = &if_body_pairs[i + 1];
        if compare_body(body, next_body) {
            checker.diagnostics.push(Diagnostic::new(
                IfWithSameArms,
                Range::new(
                    if i == 0 { stmt.location } else { test.location },
                    next_body.last().unwrap().end_location.unwrap(),
                ),
            ));
        }
    }
}

/// SIM116
pub fn manual_dict_lookup(
    checker: &mut Checker,
    stmt: &Stmt,
    test: &Expr,
    body: &[Stmt],
    orelse: &[Stmt],
    parent: Option<&Stmt>,
) {
    // Throughout this rule:
    // * Each if-statement's test must consist of a constant equality check with the same variable.
    // * Each if-statement's body must consist of a single `return`.
    // * Each if-statement's orelse must be either another if-statement or empty.
    // * The final if-statement's orelse must be empty, or a single `return`.
    let ExprKind::Compare {
        left,
        ops,
        comparators,
    } = &test.node else {
        return;
    };
    let ExprKind::Name { id: target, .. } = &left.node else {
        return;
    };
    if body.len() != 1 {
        return;
    }
    if orelse.len() != 1 {
        return;
    }
    if !(ops.len() == 1 && ops[0] == Cmpop::Eq) {
        return;
    }
    if comparators.len() != 1 {
        return;
    }
    let ExprKind::Constant { value: constant, .. } = &comparators[0].node else {
        return;
    };
    let StmtKind::Return { value, .. } = &body[0].node else {
        return;
    };
    if value.as_ref().map_or(false, |value| {
        contains_effect(value, |id| checker.ctx.is_builtin(id))
    }) {
        return;
    }

    // It's part of a bigger if-elif block:
    // https://github.com/MartinThoma/flake8-simplify/issues/115
    if let Some(StmtKind::If {
        orelse: parent_orelse,
        ..
    }) = parent.map(|parent| &parent.node)
    {
        if parent_orelse.len() == 1 && stmt == &parent_orelse[0] {
            // TODO(charlie): These two cases have the same AST:
            //
            // if True:
            //     pass
            // elif a:
            //     b = 1
            // else:
            //     b = 2
            //
            // if True:
            //     pass
            // else:
            //     if a:
            //         b = 1
            //     else:
            //         b = 2
            //
            // We want to flag the latter, but not the former. Right now, we flag neither.
            return;
        }
    }

    let mut constants: FxHashSet<ComparableConstant> = FxHashSet::default();
    constants.insert(constant.into());

    let mut child: Option<&Stmt> = orelse.get(0);
    while let Some(current) = child.take() {
        let StmtKind::If { test, body, orelse } = &current.node else {
            return;
        };
        if body.len() != 1 {
            return;
        }
        if orelse.len() > 1 {
            return;
        }
        let ExprKind::Compare {
            left,
            ops,
            comparators,
        } = &test.node else {
            return;
        };
        let ExprKind::Name { id, .. } = &left.node else {
            return;
        };
        if !(id == target && ops.len() == 1 && ops[0] == Cmpop::Eq) {
            return;
        }
        if comparators.len() != 1 {
            return;
        }
        let ExprKind::Constant { value: constant, .. } = &comparators[0].node else {
            return;
        };
        let StmtKind::Return { value, .. } = &body[0].node else {
            return;
        };
        if value.as_ref().map_or(false, |value| {
            contains_effect(value, |id| checker.ctx.is_builtin(id))
        }) {
            return;
        };

        constants.insert(constant.into());
        if let Some(orelse) = orelse.first() {
            match &orelse.node {
                StmtKind::If { .. } => {
                    child = Some(orelse);
                }
                StmtKind::Return { .. } => {
                    child = None;
                }
                _ => return,
            }
        } else {
            child = None;
        }
    }

    if constants.len() < 3 {
        return;
    }

    checker.diagnostics.push(Diagnostic::new(
        IfElseBlockInsteadOfDictLookup,
        Range::from(stmt),
    ));
}

/// SIM401
pub fn use_dict_get_with_default(
    checker: &mut Checker,
    stmt: &Stmt,
    test: &Expr,
    body: &Vec<Stmt>,
    orelse: &Vec<Stmt>,
    parent: Option<&Stmt>,
) {
    if body.len() != 1 || orelse.len() != 1 {
        return;
    }
    let StmtKind::Assign { targets: body_var, value: body_value, ..} = &body[0].node else {
        return;
    };
    if body_var.len() != 1 {
        return;
    };
    let StmtKind::Assign { targets: orelse_var, value: orelse_value, .. } = &orelse[0].node else {
        return;
    };
    if orelse_var.len() != 1 {
        return;
    };
    let ExprKind::Compare { left: test_key, ops , comparators: test_dict } = &test.node else {
        return;
    };
    if test_dict.len() != 1 {
        return;
    }
    let (expected_var, expected_value, default_var, default_value) = match ops[..] {
        [Cmpop::In] => (&body_var[0], body_value, &orelse_var[0], orelse_value),
        [Cmpop::NotIn] => (&orelse_var[0], orelse_value, &body_var[0], body_value),
        _ => {
            return;
        }
    };
    let test_dict = &test_dict[0];
    let ExprKind::Subscript { value: expected_subscript, slice: expected_slice, .. }  =  &expected_value.node else {
        return;
    };

    // Check that the dictionary key, target variables, and dictionary name are all
    // equivalent.
    if !compare_expr(&expected_slice.into(), &test_key.into())
        || !compare_expr(&expected_var.into(), &default_var.into())
        || !compare_expr(&test_dict.into(), &expected_subscript.into())
    {
        return;
    }

    // Check that the default value is not "complex".
    if contains_effect(default_value, |id| checker.ctx.is_builtin(id)) {
        return;
    }

    // It's part of a bigger if-elif block:
    // https://github.com/MartinThoma/flake8-simplify/issues/115
    if let Some(StmtKind::If {
        orelse: parent_orelse,
        ..
    }) = parent.map(|parent| &parent.node)
    {
        if parent_orelse.len() == 1 && stmt == &parent_orelse[0] {
            // TODO(charlie): These two cases have the same AST:
            //
            // if True:
            //     pass
            // elif a:
            //     b = 1
            // else:
            //     b = 2
            //
            // if True:
            //     pass
            // else:
            //     if a:
            //         b = 1
            //     else:
            //         b = 2
            //
            // We want to flag the latter, but not the former. Right now, we flag neither.
            return;
        }
    }

    let contents = unparse_stmt(
        &create_stmt(StmtKind::Assign {
            targets: vec![create_expr(expected_var.node.clone())],
            value: Box::new(create_expr(ExprKind::Call {
                func: Box::new(create_expr(ExprKind::Attribute {
                    value: expected_subscript.clone(),
                    attr: "get".to_string(),
                    ctx: ExprContext::Load,
                })),
                args: vec![
                    create_expr(test_key.node.clone()),
                    create_expr(default_value.node.clone()),
                ],
                keywords: vec![],
            })),
            type_comment: None,
        }),
        checker.stylist,
    );

    // Don't flag if the resulting expression would exceed the maximum line length.
    if stmt.location.column() + contents.width() > checker.settings.line_length {
        return;
    }

    let fixable = !has_comments(stmt, checker.locator);
    let mut diagnostic = Diagnostic::new(
        IfElseBlockInsteadOfDictGet {
            contents: contents.clone(),
            fixable,
        },
        Range::from(stmt),
    );
    if fixable && checker.patch(diagnostic.kind.rule()) {
        diagnostic.set_fix(Edit::replacement(
            contents,
            stmt.location,
            stmt.end_location.unwrap(),
        ));
    }
    checker.diagnostics.push(diagnostic);
}
