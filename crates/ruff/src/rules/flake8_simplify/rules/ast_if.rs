use log::error;
use ruff_python_ast::{
    self as ast, Arguments, CmpOp, Constant, ElifElseClause, Expr, ExprContext, Identifier, Stmt,
};
use ruff_text_size::{Ranged, TextRange};
use rustc_hash::FxHashSet;

use ruff_diagnostics::{AutofixKind, Diagnostic, Edit, Fix, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::comparable::{ComparableConstant, ComparableExpr, ComparableStmt};
use ruff_python_ast::helpers::{any_over_expr, contains_effect};
use ruff_python_ast::stmt_if::{if_elif_branches, IfElifBranch};
use ruff_python_semantic::SemanticModel;
use ruff_python_trivia::{SimpleTokenKind, SimpleTokenizer};
use ruff_source_file::{Locator, UniversalNewlines};

use crate::checkers::ast::Checker;
use crate::line_width::LineWidthBuilder;
use crate::registry::AsRule;
use crate::rules::flake8_simplify::rules::fix_if;

fn compare_expr(expr1: &ComparableExpr, expr2: &ComparableExpr) -> bool {
    expr1.eq(expr2)
}

fn compare_stmt(stmt1: &ComparableStmt, stmt2: &ComparableStmt) -> bool {
    stmt1.eq(stmt2)
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
                if let [Expr::Constant(ast::ExprConstant {
                    value: Constant::Str(ast::StringConstant { value, .. }),
                    ..
                })] = comparators.as_slice()
                {
                    if value == "__main__" {
                        return true;
                    }
                }
            }
        }
    }
    false
}

/// Find the last nested if statement and return the test expression and the
/// last statement.
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
        elif_else_clauses,
        ..
    })] = body
    else {
        return None;
    };
    if !elif_else_clauses.is_empty() {
        return None;
    }
    find_last_nested_if(inner_body).or_else(|| {
        Some((
            test,
            inner_body.last().expect("Expected body to be non-empty"),
        ))
    })
}

/// Returns the body, the range of the `if` or `elif` and whether the range is for an `if` or `elif`
fn nested_if_body(stmt_if: &ast::StmtIf) -> Option<(&[Stmt], TextRange, bool)> {
    let ast::StmtIf {
        test,
        body,
        elif_else_clauses,
        ..
    } = stmt_if;

    // It must be the last condition, otherwise there could be another `elif` or `else` that only
    // depends on the outer of the two conditions
    let (test, body, range, is_elif) = if let Some(clause) = elif_else_clauses.last() {
        if let Some(test) = &clause.test {
            (test, &clause.body, clause.range(), true)
        } else {
            // The last condition is an `else` (different rule)
            return None;
        }
    } else {
        (test.as_ref(), body, stmt_if.range(), false)
    };

    // The nested if must be the only child, otherwise there is at least one more statement that
    // only depends on the outer condition
    if body.len() > 1 {
        return None;
    }

    // Allow `if __name__ == "__main__":` statements.
    if is_main_check(test) {
        return None;
    }

    // Allow `if True:` and `if False:` statements.
    if matches!(
        test,
        Expr::Constant(ast::ExprConstant {
            value: Constant::Bool(..),
            ..
        })
    ) {
        return None;
    }

    Some((body, range, is_elif))
}

/// SIM102
pub(crate) fn nested_if_statements(
    checker: &mut Checker,
    stmt_if: &ast::StmtIf,
    parent: Option<&Stmt>,
) {
    let Some((body, range, is_elif)) = nested_if_body(stmt_if) else {
        return;
    };

    // Find the deepest nested if-statement, to inform the range.
    let Some((test, _first_stmt)) = find_last_nested_if(body) else {
        return;
    };

    // Check if the parent is already emitting a larger diagnostic including this if statement
    if let Some(Stmt::If(stmt_if)) = parent {
        if let Some((body, _range, _is_elif)) = nested_if_body(stmt_if) {
            // In addition to repeating the `nested_if_body` and `find_last_nested_if` check, we
            // also need to be the first child in the parent
            if matches!(&body[0], Stmt::If(inner) if inner == stmt_if)
                && find_last_nested_if(body).is_some()
            {
                return;
            }
        }
    }

    let Some(colon) = SimpleTokenizer::starts_at(test.end(), checker.locator().contents())
        .skip_trivia()
        .find(|token| token.kind == SimpleTokenKind::Colon)
    else {
        return;
    };

    let mut diagnostic = Diagnostic::new(CollapsibleIf, TextRange::new(range.start(), colon.end()));
    if checker.patch(diagnostic.kind.rule()) {
        // The fixer preserves comments in the nested body, but removes comments between
        // the outer and inner if statements.
        let nested_if = &body[0];
        if !checker
            .indexer()
            .comment_ranges()
            .intersects(TextRange::new(range.start(), nested_if.start()))
        {
            match fix_if::fix_nested_if_statements(
                checker.locator(),
                checker.stylist(),
                range,
                is_elif,
            ) {
                Ok(edit) => {
                    if edit
                        .content()
                        .unwrap_or_default()
                        .universal_newlines()
                        .all(|line| {
                            LineWidthBuilder::new(checker.settings.tab_size).add_str(&line)
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
    let [stmt] = stmts else {
        return None;
    };
    let Stmt::Return(ast::StmtReturn { value, range: _ }) = stmt else {
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
        test: if_test,
        body: if_body,
        elif_else_clauses,
        range: _,
    }) = stmt
    else {
        return;
    };
    // Extract an `if` or `elif` (that returns) followed by an else (that returns the same value)
    let (if_test, if_body, else_body, range) = match elif_else_clauses.as_slice() {
        // if-else case
        [ElifElseClause {
            body: else_body,
            test: None,
            ..
        }] => (if_test.as_ref(), if_body, else_body, stmt.range()),
        // elif-else case
        [.., ElifElseClause {
            body: elif_body,
            test: Some(elif_test),
            range: elif_range,
        }, ElifElseClause {
            body: else_body,
            test: None,
            range: else_range,
        }] => (
            elif_test,
            elif_body,
            else_body,
            TextRange::new(elif_range.start(), else_range.end()),
        ),
        _ => return,
    };

    let (Some(if_return), Some(else_return)) = (
        is_one_line_return_bool(if_body),
        is_one_line_return_bool(else_body),
    ) else {
        return;
    };

    // If the branches have the same condition, abort (although the code could be
    // simplified).
    if if_return == else_return {
        return;
    }

    let condition = checker.generator().expr(if_test);
    let mut diagnostic = Diagnostic::new(NeedlessBool { condition }, range);
    if checker.patch(diagnostic.kind.rule()) {
        if matches!(if_return, Bool::True)
            && matches!(else_return, Bool::False)
            && !checker.indexer().has_comments(&range, checker.locator())
            && (if_test.is_compare_expr() || checker.semantic().is_builtin("bool"))
        {
            if if_test.is_compare_expr() {
                // If the condition is a comparison, we can replace it with the condition.
                let node = ast::StmtReturn {
                    value: Some(Box::new(if_test.clone())),
                    range: TextRange::default(),
                };
                diagnostic.set_fix(Fix::suggested(Edit::range_replacement(
                    checker.generator().stmt(&node.into()),
                    range,
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
                    arguments: Arguments {
                        args: vec![if_test.clone()],
                        keywords: vec![],
                        range: TextRange::default(),
                    },
                    range: TextRange::default(),
                };
                let node2 = ast::StmtReturn {
                    value: Some(Box::new(node1.into())),
                    range: TextRange::default(),
                };
                diagnostic.set_fix(Fix::suggested(Edit::range_replacement(
                    checker.generator().stmt(&node2.into()),
                    range,
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
        range: TextRange::default(),
    };
    node1.into()
}

/// Return `true` if the `Expr` contains a reference to any of the given `${module}.${target}`.
fn contains_call_path(expr: &Expr, targets: &[&[&str]], semantic: &SemanticModel) -> bool {
    any_over_expr(expr, &|expr| {
        semantic
            .resolve_call_path(expr)
            .is_some_and(|call_path| targets.iter().any(|target| &call_path.as_slice() == target))
    })
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
    let line_start = checker.locator().line_start(stmt.start());
    if LineWidthBuilder::new(checker.settings.tab_size)
        .add_str(&checker.locator().contents()[TextRange::new(line_start, stmt.start())])
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
        if !checker.indexer().has_comments(stmt, checker.locator()) {
            diagnostic.set_fix(Fix::suggested(Edit::range_replacement(
                contents,
                stmt.range(),
            )));
        }
    }
    checker.diagnostics.push(diagnostic);
}

/// Return the [`TextRange`] of an [`IfElifBranch`]'s body (from the end of the test to the end of
/// the body).
fn body_range(branch: &IfElifBranch, locator: &Locator) -> TextRange {
    TextRange::new(
        locator.line_end(branch.test.end()),
        locator.line_end(branch.end()),
    )
}

/// SIM114
pub(crate) fn if_with_same_arms(checker: &mut Checker, locator: &Locator, stmt_if: &ast::StmtIf) {
    let mut branches_iter = if_elif_branches(stmt_if).peekable();
    while let Some(current_branch) = branches_iter.next() {
        let Some(following_branch) = branches_iter.peek() else {
            continue;
        };

        // The bodies must have the same code ...
        if current_branch.body.len() != following_branch.body.len() {
            continue;
        }
        if !current_branch
            .body
            .iter()
            .zip(following_branch.body.iter())
            .all(|(stmt1, stmt2)| compare_stmt(&stmt1.into(), &stmt2.into()))
        {
            continue;
        }

        // ...and the same comments
        let first_comments = checker
            .indexer()
            .comment_ranges()
            .comments_in_range(body_range(&current_branch, locator))
            .iter()
            .map(|range| locator.slice(*range));
        let second_comments = checker
            .indexer()
            .comment_ranges()
            .comments_in_range(body_range(following_branch, locator))
            .iter()
            .map(|range| locator.slice(*range));
        if !first_comments.eq(second_comments) {
            continue;
        }

        checker.diagnostics.push(Diagnostic::new(
            IfWithSameArms,
            TextRange::new(current_branch.start(), following_branch.end()),
        ));
    }
}

/// SIM116
pub(crate) fn manual_dict_lookup(checker: &mut Checker, stmt_if: &ast::StmtIf) {
    // Throughout this rule:
    // * Each if or elif statement's test must consist of a constant equality check with the same variable.
    // * Each if or elif statement's body must consist of a single `return`.
    // * The else clause must be empty, or a single `return`.
    let ast::StmtIf {
        body,
        test,
        elif_else_clauses,
        ..
    } = stmt_if;

    let Expr::Compare(ast::ExprCompare {
        left,
        ops,
        comparators,
        range: _,
    }) = test.as_ref()
    else {
        return;
    };
    let Expr::Name(ast::ExprName { id: target, .. }) = left.as_ref() else {
        return;
    };
    if ops != &[CmpOp::Eq] {
        return;
    }
    let [Expr::Constant(ast::ExprConstant {
        value: constant, ..
    })] = comparators.as_slice()
    else {
        return;
    };
    let [Stmt::Return(ast::StmtReturn { value, range: _ })] = body.as_slice() else {
        return;
    };
    if value
        .as_ref()
        .is_some_and(|value| contains_effect(value, |id| checker.semantic().is_builtin(id)))
    {
        return;
    }

    let mut constants: FxHashSet<ComparableConstant> = FxHashSet::default();
    constants.insert(constant.into());

    for clause in elif_else_clauses {
        let ElifElseClause { test, body, .. } = clause;
        let [Stmt::Return(ast::StmtReturn { value, range: _ })] = body.as_slice() else {
            return;
        };

        match test.as_ref() {
            // `else`
            None => {
                // The else must also be a single effect-free return statement
                let [Stmt::Return(ast::StmtReturn { value, range: _ })] = body.as_slice() else {
                    return;
                };
                if value.as_ref().is_some_and(|value| {
                    contains_effect(value, |id| checker.semantic().is_builtin(id))
                }) {
                    return;
                };
            }
            // `elif`
            Some(Expr::Compare(ast::ExprCompare {
                left,
                ops,
                comparators,
                range: _,
            })) => {
                let Expr::Name(ast::ExprName { id, .. }) = left.as_ref() else {
                    return;
                };
                if id != target || ops != &[CmpOp::Eq] {
                    return;
                }
                let [Expr::Constant(ast::ExprConstant {
                    value: constant, ..
                })] = comparators.as_slice()
                else {
                    return;
                };

                if value.as_ref().is_some_and(|value| {
                    contains_effect(value, |id| checker.semantic().is_builtin(id))
                }) {
                    return;
                };

                constants.insert(constant.into());
            }
            // Different `elif`
            _ => {
                return;
            }
        }
    }

    if constants.len() < 3 {
        return;
    }

    checker.diagnostics.push(Diagnostic::new(
        IfElseBlockInsteadOfDictLookup,
        stmt_if.range(),
    ));
}

/// SIM401
pub(crate) fn use_dict_get_with_default(checker: &mut Checker, stmt_if: &ast::StmtIf) {
    let ast::StmtIf {
        test,
        body,
        elif_else_clauses,
        ..
    } = stmt_if;

    let [body_stmt] = body.as_slice() else {
        return;
    };
    let [ElifElseClause {
        body: else_body,
        test: None,
        ..
    }] = elif_else_clauses.as_slice()
    else {
        return;
    };
    let [else_body_stmt] = else_body.as_slice() else {
        return;
    };
    let Stmt::Assign(ast::StmtAssign {
        targets: body_var,
        value: body_value,
        ..
    }) = &body_stmt
    else {
        return;
    };
    let [body_var] = body_var.as_slice() else {
        return;
    };
    let Stmt::Assign(ast::StmtAssign {
        targets: orelse_var,
        value: orelse_value,
        ..
    }) = &else_body_stmt
    else {
        return;
    };
    let [orelse_var] = orelse_var.as_slice() else {
        return;
    };
    let Expr::Compare(ast::ExprCompare {
        left: test_key,
        ops,
        comparators: test_dict,
        range: _,
    }) = test.as_ref()
    else {
        return;
    };
    let [test_dict] = test_dict.as_slice() else {
        return;
    };
    let (expected_var, expected_value, default_var, default_value) = match ops[..] {
        [CmpOp::In] => (body_var, body_value, orelse_var, orelse_value.as_ref()),
        [CmpOp::NotIn] => (orelse_var, orelse_value, body_var, body_value.as_ref()),
        _ => {
            return;
        }
    };
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

    let node = default_value.clone();
    let node1 = *test_key.clone();
    let node2 = ast::ExprAttribute {
        value: expected_subscript.clone(),
        attr: Identifier::new("get".to_string(), TextRange::default()),
        ctx: ExprContext::Load,
        range: TextRange::default(),
    };
    let node3 = ast::ExprCall {
        func: Box::new(node2.into()),
        arguments: Arguments {
            args: vec![node1, node],
            keywords: vec![],
            range: TextRange::default(),
        },
        range: TextRange::default(),
    };
    let node4 = expected_var.clone();
    let node5 = ast::StmtAssign {
        targets: vec![node4],
        value: Box::new(node3.into()),
        range: TextRange::default(),
    };
    let contents = checker.generator().stmt(&node5.into());

    // Don't flag if the resulting expression would exceed the maximum line length.
    let line_start = checker.locator().line_start(stmt_if.start());
    if LineWidthBuilder::new(checker.settings.tab_size)
        .add_str(&checker.locator().contents()[TextRange::new(line_start, stmt_if.start())])
        .add_str(&contents)
        > checker.settings.line_length
    {
        return;
    }

    let mut diagnostic = Diagnostic::new(
        IfElseBlockInsteadOfDictGet {
            contents: contents.clone(),
        },
        stmt_if.range(),
    );
    if checker.patch(diagnostic.kind.rule()) {
        if !checker.indexer().has_comments(stmt_if, checker.locator()) {
            diagnostic.set_fix(Fix::suggested(Edit::range_replacement(
                contents,
                stmt_if.range(),
            )));
        }
    }
    checker.diagnostics.push(diagnostic);
}
