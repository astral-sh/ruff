use log::error;
use ruff_text_size::TextRange;
use rustc_hash::FxHashSet;
use rustpython_parser::ast::{self, CmpOp, Constant, Expr, ExprContext, Identifier, Ranged, Stmt};

use ruff_diagnostics::{AutofixKind, Diagnostic, Edit, Fix, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::comparable::{ComparableConstant, ComparableExpr, ComparableStmt};
use ruff_python_ast::helpers::{
    any_over_expr, contains_effect, first_colon_range, has_comments, has_comments_in,
};
use ruff_python_semantic::SemanticModel;
use ruff_python_whitespace::UniversalNewlines;

use crate::checkers::ast::Checker;
use crate::line_width::LineWidth;
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

/// ## What it does
/// Checks for nested `if` statements that can be collapsed into a single `if`
/// statement.
///
/// ## Why is this bad?
/// Nesting `if` statements leads to deeper indentation and makes code harder to
/// read. Instead, combine the conditions into a single `if` statement with an
/// `and` operator.
///
/// ## Example
/// ```python
/// if foo:
///     if bar:
///         ...
/// ```
///
/// Use instead:
/// ```python
/// if foo and bar:
///     ...
/// ```
///
/// ## References
/// - [Python documentation: The `if` statement](https://docs.python.org/3/reference/compound_stmts.html#the-if-statement)
/// - [Python documentation: Boolean operations](https://docs.python.org/3/reference/expressions.html#boolean-operations)
#[violation]
pub struct CollapsibleIf;

impl Violation for CollapsibleIf {
    const AUTOFIX: AutofixKind = AutofixKind::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use a single `if` statement instead of nested `if` statements")
    }

    fn autofix_title(&self) -> Option<String> {
        Some("Combine `if` statements using `and`".to_string())
    }
}

/// ## What it does
/// Checks for `if` statements that can be replaced with `bool`.
///
/// ## Why is this bad?
/// `if` statements that return `True` for a truthy condition and `False` for
/// a falsey condition can be replaced with boolean casts.
///
/// ## Example
/// ```python
/// if foo:
///     return True
/// else:
///     return False
/// ```
///
/// Use instead:
/// ```python
/// return bool(foo)
/// ```
///
/// ## References
/// - [Python documentation: Truth Value Testing](https://docs.python.org/3/library/stdtypes.html#truth-value-testing)
#[violation]
pub struct NeedlessBool {
    condition: String,
}

impl Violation for NeedlessBool {
    const AUTOFIX: AutofixKind = AutofixKind::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let NeedlessBool { condition } = self;
        format!("Return the condition `{condition}` directly")
    }

    fn autofix_title(&self) -> Option<String> {
        let NeedlessBool { condition } = self;
        Some(format!("Replace with `return {condition}`"))
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
    const AUTOFIX: AutofixKind = AutofixKind::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let IfElseBlockInsteadOfIfExp { contents } = self;
        format!("Use ternary operator `{contents}` instead of `if`-`else`-block")
    }

    fn autofix_title(&self) -> Option<String> {
        let IfElseBlockInsteadOfIfExp { contents } = self;
        Some(format!("Replace `if`-`else`-block with `{contents}`"))
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

/// ## What it does
/// Checks for `if` statements that can be replaced with `dict.get` calls.
///
/// ## Why is this bad?
/// `dict.get()` calls can be used to replace `if` statements that assign a
/// value to a variable in both branches, falling back to a default value if
/// the key is not found. When possible, using `dict.get` is more concise and
/// more idiomatic.
///
/// ## Example
/// ```python
/// if "bar" in foo:
///     value = foo["bar"]
/// else:
///     value = 0
/// ```
///
/// Use instead:
/// ```python
/// value = foo.get("bar", 0)
/// ```
///
/// ## References
/// - [Python documentation: Mapping Types](https://docs.python.org/3/library/stdtypes.html#mapping-types-dict)
#[violation]
pub struct IfElseBlockInsteadOfDictGet {
    contents: String,
}

impl Violation for IfElseBlockInsteadOfDictGet {
    const AUTOFIX: AutofixKind = AutofixKind::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let IfElseBlockInsteadOfDictGet { contents } = self;
        format!("Use `{contents}` instead of an `if` block")
    }

    fn autofix_title(&self) -> Option<String> {
        let IfElseBlockInsteadOfDictGet { contents } = self;
        Some(format!("Replace with `{contents}`"))
    }
}

fn is_main_check(expr: &Expr) -> bool {
    if let Expr::Compare(ast::ExprCompare {
        left, comparators, ..
    }) = expr
    {
        if let Expr::Name(ast::ExprName { id, .. }) = left.as_ref() {
            if id == "__name__" {
                if comparators.len() == 1 {
                    if let Expr::Constant(ast::ExprConstant {
                        value: Constant::Str(value),
                        ..
                    }) = &comparators[0]
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
    let [Stmt::If(ast::StmtIf {
        test,
        body: inner_body,
        orelse,
        ..
    })] = body
    else {
        return None;
    };
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
pub(crate) fn nested_if_statements(
    checker: &mut Checker,
    stmt: &Stmt,
    test: &Expr,
    body: &[Stmt],
    orelse: &[Stmt],
    parent: Option<&Stmt>,
) {
    // If the parent could contain a nested if-statement, abort.
    if let Some(Stmt::If(ast::StmtIf { body, orelse, .. })) = parent {
        if orelse.is_empty() && body.len() == 1 {
            return;
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
        test,
        Expr::Constant(ast::ExprConstant {
            value: Constant::Bool(..),
            ..
        })
    ) {
        return;
    }

    // Find the deepest nested if-statement, to inform the range.
    let Some((test, first_stmt)) = find_last_nested_if(body) else {
        return;
    };

    let colon = first_colon_range(
        TextRange::new(test.end(), first_stmt.start()),
        checker.locator,
    );

    let mut diagnostic = Diagnostic::new(
        CollapsibleIf,
        colon.map_or_else(
            || stmt.range(),
            |colon| TextRange::new(stmt.start(), colon.end()),
        ),
    );
    if checker.patch(diagnostic.kind.rule()) {
        // The fixer preserves comments in the nested body, but removes comments between
        // the outer and inner if statements.
        let nested_if = &body[0];
        if !has_comments_in(
            TextRange::new(stmt.start(), nested_if.start()),
            checker.locator,
        ) {
            match fix_if::fix_nested_if_statements(checker.locator, checker.stylist, stmt) {
                Ok(edit) => {
                    if edit
                        .content()
                        .unwrap_or_default()
                        .universal_newlines()
                        .all(|line| {
                            LineWidth::new(checker.settings.tab_size).add_str(&line)
                                <= checker.settings.line_length
                        })
                    {
                        diagnostic.set_fix(Fix::suggested(edit));
                    }
                }
                Err(err) => error!("Failed to fix nested if: {err}"),
            }
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
    let Stmt::Return(ast::StmtReturn { value, range: _ }) = &stmts[0] else {
        return None;
    };
    let Some(Expr::Constant(ast::ExprConstant { value, .. })) = value.as_deref() else {
        return None;
    };
    let Constant::Bool(value) = value else {
        return None;
    };
    Some((*value).into())
}

/// SIM103
pub(crate) fn needless_bool(checker: &mut Checker, stmt: &Stmt) {
    let Stmt::If(ast::StmtIf {
        test,
        body,
        orelse,
        range: _,
    }) = stmt
    else {
        return;
    };
    let (Some(if_return), Some(else_return)) = (
        is_one_line_return_bool(body),
        is_one_line_return_bool(orelse),
    ) else {
        return;
    };

    // If the branches have the same condition, abort (although the code could be
    // simplified).
    if if_return == else_return {
        return;
    }

    let condition = checker.generator().expr(test);
    let mut diagnostic = Diagnostic::new(NeedlessBool { condition }, stmt.range());
    if checker.patch(diagnostic.kind.rule()) {
        if matches!(if_return, Bool::True)
            && matches!(else_return, Bool::False)
            && !has_comments(stmt, checker.locator)
            && (test.is_compare_expr() || checker.semantic().is_builtin("bool"))
        {
            if test.is_compare_expr() {
                // If the condition is a comparison, we can replace it with the condition.
                let node = ast::StmtReturn {
                    value: Some(test.clone()),
                    range: TextRange::default(),
                };
                diagnostic.set_fix(Fix::suggested(Edit::range_replacement(
                    checker.generator().stmt(&node.into()),
                    stmt.range(),
                )));
            } else {
                // Otherwise, we need to wrap the condition in a call to `bool`. (We've already
                // verified, above, that `bool` is a builtin.)
                let node = ast::ExprName {
                    id: "bool".into(),
                    ctx: ExprContext::Load,
                    range: TextRange::default(),
                };
                let node1 = ast::ExprCall {
                    func: Box::new(node.into()),
                    args: vec![(**test).clone()],
                    keywords: vec![],
                    range: TextRange::default(),
                };
                let node2 = ast::StmtReturn {
                    value: Some(Box::new(node1.into())),
                    range: TextRange::default(),
                };
                diagnostic.set_fix(Fix::suggested(Edit::range_replacement(
                    checker.generator().stmt(&node2.into()),
                    stmt.range(),
                )));
            };
        }
    }
    checker.diagnostics.push(diagnostic);
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
        type_comment: None,
        range: TextRange::default(),
    };
    node1.into()
}

/// Return `true` if the `Expr` contains a reference to `${module}.${target}`.
fn contains_call_path(expr: &Expr, target: &[&str], semantic: &SemanticModel) -> bool {
    any_over_expr(expr, &|expr| {
        semantic
            .resolve_call_path(expr)
            .map_or(false, |call_path| call_path.as_slice() == target)
    })
}

/// SIM108
pub(crate) fn use_ternary_operator(checker: &mut Checker, stmt: &Stmt, parent: Option<&Stmt>) {
    let Stmt::If(ast::StmtIf {
        test,
        body,
        orelse,
        range: _,
    }) = stmt
    else {
        return;
    };
    if body.len() != 1 || orelse.len() != 1 {
        return;
    }
    let Stmt::Assign(ast::StmtAssign {
        targets: body_targets,
        value: body_value,
        ..
    }) = &body[0]
    else {
        return;
    };
    let Stmt::Assign(ast::StmtAssign {
        targets: orelse_targets,
        value: orelse_value,
        ..
    }) = &orelse[0]
    else {
        return;
    };
    if body_targets.len() != 1 || orelse_targets.len() != 1 {
        return;
    }
    let Expr::Name(ast::ExprName { id: body_id, .. }) = &body_targets[0] else {
        return;
    };
    let Expr::Name(ast::ExprName { id: orelse_id, .. }) = &orelse_targets[0] else {
        return;
    };
    if body_id != orelse_id {
        return;
    }

    // Avoid suggesting ternary for `if sys.version_info >= ...`-style checks.
    if contains_call_path(test, &["sys", "version_info"], checker.semantic()) {
        return;
    }

    // Avoid suggesting ternary for `if sys.platform.startswith("...")`-style
    // checks.
    if contains_call_path(test, &["sys", "platform"], checker.semantic()) {
        return;
    }

    // It's part of a bigger if-elif block:
    // https://github.com/MartinThoma/flake8-simplify/issues/115
    if let Some(Stmt::If(ast::StmtIf {
        orelse: parent_orelse,
        ..
    })) = parent
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
        body_value.as_ref(),
        Expr::Yield(_) | Expr::YieldFrom(_) | Expr::Await(_)
    ) {
        return;
    }
    if matches!(
        orelse_value.as_ref(),
        Expr::Yield(_) | Expr::YieldFrom(_) | Expr::Await(_)
    ) {
        return;
    }

    let target_var = &body_targets[0];
    let ternary = ternary(target_var, body_value, test, orelse_value);
    let contents = checker.generator().stmt(&ternary);

    // Don't flag if the resulting expression would exceed the maximum line length.
    let line_start = checker.locator.line_start(stmt.start());
    if LineWidth::new(checker.settings.tab_size)
        .add_str(&checker.locator.contents()[TextRange::new(line_start, stmt.start())])
        .add_str(&contents)
        > checker.settings.line_length
    {
        return;
    }

    let mut diagnostic = Diagnostic::new(
        IfElseBlockInsteadOfIfExp {
            contents: contents.clone(),
        },
        stmt.range(),
    );
    if checker.patch(diagnostic.kind.rule()) {
        if !has_comments(stmt, checker.locator) {
            diagnostic.set_fix(Fix::suggested(Edit::range_replacement(
                contents,
                stmt.range(),
            )));
        }
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
        let Stmt::If(ast::StmtIf {
            test,
            body,
            orelse: orelse_orelse,
            range: _,
        }) = &orelse[0]
        else {
            break;
        };
        pairs.push((test, body));
        orelse = orelse_orelse;
    }
    pairs
}

/// SIM114
pub(crate) fn if_with_same_arms(checker: &mut Checker, stmt: &Stmt, parent: Option<&Stmt>) {
    let Stmt::If(ast::StmtIf {
        test,
        body,
        orelse,
        range: _,
    }) = stmt
    else {
        return;
    };

    // It's part of a bigger if-elif block:
    // https://github.com/MartinThoma/flake8-simplify/issues/115
    if let Some(Stmt::If(ast::StmtIf {
        orelse: parent_orelse,
        ..
    })) = parent
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
                TextRange::new(
                    if i == 0 { stmt.start() } else { test.start() },
                    next_body.last().unwrap().end(),
                ),
            ));
        }
    }
}

/// SIM116
pub(crate) fn manual_dict_lookup(
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
    let Expr::Compare(ast::ExprCompare {
        left,
        ops,
        comparators,
        range: _,
    }) = &test
    else {
        return;
    };
    let Expr::Name(ast::ExprName { id: target, .. }) = left.as_ref() else {
        return;
    };
    if body.len() != 1 {
        return;
    }
    if orelse.len() != 1 {
        return;
    }
    if !(ops.len() == 1 && ops[0] == CmpOp::Eq) {
        return;
    }
    if comparators.len() != 1 {
        return;
    }
    let Expr::Constant(ast::ExprConstant {
        value: constant, ..
    }) = &comparators[0]
    else {
        return;
    };
    let Stmt::Return(ast::StmtReturn { value, range: _ }) = &body[0] else {
        return;
    };
    if value.as_ref().map_or(false, |value| {
        contains_effect(value, |id| checker.semantic().is_builtin(id))
    }) {
        return;
    }

    // It's part of a bigger if-elif block:
    // https://github.com/MartinThoma/flake8-simplify/issues/115
    if let Some(Stmt::If(ast::StmtIf {
        orelse: parent_orelse,
        ..
    })) = parent
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
        let Stmt::If(ast::StmtIf {
            test,
            body,
            orelse,
            range: _,
        }) = &current
        else {
            return;
        };
        if body.len() != 1 {
            return;
        }
        if orelse.len() > 1 {
            return;
        }
        let Expr::Compare(ast::ExprCompare {
            left,
            ops,
            comparators,
            range: _,
        }) = test.as_ref()
        else {
            return;
        };
        let Expr::Name(ast::ExprName { id, .. }) = left.as_ref() else {
            return;
        };
        if !(id == target && ops.len() == 1 && ops[0] == CmpOp::Eq) {
            return;
        }
        if comparators.len() != 1 {
            return;
        }
        let Expr::Constant(ast::ExprConstant {
            value: constant, ..
        }) = &comparators[0]
        else {
            return;
        };
        let Stmt::Return(ast::StmtReturn { value, range: _ }) = &body[0] else {
            return;
        };
        if value.as_ref().map_or(false, |value| {
            contains_effect(value, |id| checker.semantic().is_builtin(id))
        }) {
            return;
        };

        constants.insert(constant.into());
        if let Some(orelse) = orelse.first() {
            match orelse {
                Stmt::If(_) => {
                    child = Some(orelse);
                }
                Stmt::Return(_) => {
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
        stmt.range(),
    ));
}

/// SIM401
pub(crate) fn use_dict_get_with_default(
    checker: &mut Checker,
    stmt: &Stmt,
    test: &Expr,
    body: &[Stmt],
    orelse: &[Stmt],
    parent: Option<&Stmt>,
) {
    if body.len() != 1 || orelse.len() != 1 {
        return;
    }
    let Stmt::Assign(ast::StmtAssign {
        targets: body_var,
        value: body_value,
        ..
    }) = &body[0]
    else {
        return;
    };
    if body_var.len() != 1 {
        return;
    };
    let Stmt::Assign(ast::StmtAssign {
        targets: orelse_var,
        value: orelse_value,
        ..
    }) = &orelse[0]
    else {
        return;
    };
    if orelse_var.len() != 1 {
        return;
    };
    let Expr::Compare(ast::ExprCompare {
        left: test_key,
        ops,
        comparators: test_dict,
        range: _,
    }) = &test
    else {
        return;
    };
    if test_dict.len() != 1 {
        return;
    }
    let (expected_var, expected_value, default_var, default_value) = match ops[..] {
        [CmpOp::In] => (&body_var[0], body_value, &orelse_var[0], orelse_value),
        [CmpOp::NotIn] => (&orelse_var[0], orelse_value, &body_var[0], body_value),
        _ => {
            return;
        }
    };
    let test_dict = &test_dict[0];
    let Expr::Subscript(ast::ExprSubscript {
        value: expected_subscript,
        slice: expected_slice,
        ..
    }) = expected_value.as_ref()
    else {
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
    if contains_effect(default_value, |id| checker.semantic().is_builtin(id)) {
        return;
    }

    // It's part of a bigger if-elif block:
    // https://github.com/MartinThoma/flake8-simplify/issues/115
    if let Some(Stmt::If(ast::StmtIf {
        orelse: parent_orelse,
        ..
    })) = parent
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

    let node = *default_value.clone();
    let node1 = *test_key.clone();
    let node2 = ast::ExprAttribute {
        value: expected_subscript.clone(),
        attr: Identifier::new("get".to_string(), TextRange::default()),
        ctx: ExprContext::Load,
        range: TextRange::default(),
    };
    let node3 = ast::ExprCall {
        func: Box::new(node2.into()),
        args: vec![node1, node],
        keywords: vec![],
        range: TextRange::default(),
    };
    let node4 = expected_var.clone();
    let node5 = ast::StmtAssign {
        targets: vec![node4],
        value: Box::new(node3.into()),
        type_comment: None,
        range: TextRange::default(),
    };
    let contents = checker.generator().stmt(&node5.into());

    // Don't flag if the resulting expression would exceed the maximum line length.
    let line_start = checker.locator.line_start(stmt.start());
    if LineWidth::new(checker.settings.tab_size)
        .add_str(&checker.locator.contents()[TextRange::new(line_start, stmt.start())])
        .add_str(&contents)
        > checker.settings.line_length
    {
        return;
    }

    let mut diagnostic = Diagnostic::new(
        IfElseBlockInsteadOfDictGet {
            contents: contents.clone(),
        },
        stmt.range(),
    );
    if checker.patch(diagnostic.kind.rule()) {
        if !has_comments(stmt, checker.locator) {
            diagnostic.set_fix(Fix::suggested(Edit::range_replacement(
                contents,
                stmt.range(),
            )));
        }
    }
    checker.diagnostics.push(diagnostic);
}
