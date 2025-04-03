use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::comparable::ComparableExpr;
use ruff_python_ast::helpers::contains_effect;
use ruff_python_ast::{self as ast, CmpOp, Expr, Stmt};
use ruff_python_codegen::Generator;
use ruff_python_semantic::analyze::typing::is_set;
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;
use crate::fix::snippet::SourceCodeSnippet;

/// ## What it does
/// Checks for uses of `set.remove` that can be replaced with `set.discard`.
///
/// ## Why is this bad?
/// If an element should be removed from a set if it is present, it is more
/// succinct and idiomatic to use `discard`.
///
/// ## Known problems
/// This rule is prone to false negatives due to type inference limitations,
/// as it will only detect sets that are instantiated as literals or annotated
/// with a type annotation.
///
/// ## Example
/// ```python
/// nums = {123, 456}
///
/// if 123 in nums:
///     nums.remove(123)
/// ```
///
/// Use instead:
/// ```python
/// nums = {123, 456}
///
/// nums.discard(123)
/// ```
///
/// ## References
/// - [Python documentation: `set.discard()`](https://docs.python.org/3/library/stdtypes.html?highlight=list#frozenset.discard)
#[derive(ViolationMetadata)]
pub(crate) struct CheckAndRemoveFromSet {
    element: SourceCodeSnippet,
    set: String,
}

impl CheckAndRemoveFromSet {
    fn suggestion(&self) -> String {
        let set = &self.set;
        let element = self.element.truncated_display();
        format!("{set}.discard({element})")
    }
}

impl AlwaysFixableViolation for CheckAndRemoveFromSet {
    #[derive_message_formats]
    fn message(&self) -> String {
        let suggestion = self.suggestion();
        format!("Use `{suggestion}` instead of check and `remove`")
    }

    fn fix_title(&self) -> String {
        let suggestion = self.suggestion();
        format!("Replace with `{suggestion}`")
    }
}

/// FURB132
pub(crate) fn check_and_remove_from_set(checker: &Checker, if_stmt: &ast::StmtIf) {
    // In order to fit the profile, we need if without else clauses and with only one statement in its body.
    if if_stmt.body.len() != 1 || !if_stmt.elif_else_clauses.is_empty() {
        return;
    }

    // The `if` test should be `element in set`.
    let Some((check_element, check_set)) = match_check(if_stmt) else {
        return;
    };

    // The `if` body should be `set.remove(element)`.
    let Some((remove_element, remove_set)) = match_remove(if_stmt) else {
        return;
    };

    // `
    // `set` in the check should be the same as `set` in the body
    if check_set.id != remove_set.id
        // `element` in the check should be the same as `element` in the body
        || !compare(&check_element.into(), &remove_element.into())
        // `element` shouldn't have a side effect, otherwise we might change the semantics of the program.
        || contains_effect(check_element, |id| checker.semantic().has_builtin_binding(id))
    {
        return;
    }

    // Check if what we assume is set is indeed a set.
    if !checker
        .semantic()
        .only_binding(check_set)
        .map(|id| checker.semantic().binding(id))
        .is_some_and(|binding| is_set(binding, checker.semantic()))
    {
        return;
    }

    let mut diagnostic = Diagnostic::new(
        CheckAndRemoveFromSet {
            element: SourceCodeSnippet::from_str(checker.locator().slice(check_element)),
            set: check_set.id.to_string(),
        },
        if_stmt.range(),
    );
    diagnostic.set_fix(Fix::unsafe_edit(Edit::replacement(
        make_suggestion(check_set, check_element, checker.generator()),
        if_stmt.start(),
        if_stmt.end(),
    )));
    checker.report_diagnostic(diagnostic);
}

fn compare(lhs: &ComparableExpr, rhs: &ComparableExpr) -> bool {
    lhs == rhs
}

/// Match `if` condition to be `expr in name`, returns a tuple of (`expr`, `name`) on success.
fn match_check(if_stmt: &ast::StmtIf) -> Option<(&Expr, &ast::ExprName)> {
    let ast::ExprCompare {
        ops,
        left,
        comparators,
        ..
    } = if_stmt.test.as_compare_expr()?;

    if **ops != [CmpOp::In] {
        return None;
    }

    let [Expr::Name(right @ ast::ExprName { .. })] = &**comparators else {
        return None;
    };

    Some((left.as_ref(), right))
}

/// Match `if` body to be `name.remove(expr)`, returns a tuple of (`expr`, `name`) on success.
fn match_remove(if_stmt: &ast::StmtIf) -> Option<(&Expr, &ast::ExprName)> {
    let [Stmt::Expr(ast::StmtExpr { value: expr, .. })] = if_stmt.body.as_slice() else {
        return None;
    };

    let ast::ExprCall {
        func: attr,
        arguments: ast::Arguments { args, keywords, .. },
        ..
    } = expr.as_call_expr()?;

    let ast::ExprAttribute {
        value: receiver,
        attr: func_name,
        ..
    } = attr.as_attribute_expr()?;

    let Expr::Name(ref set @ ast::ExprName { .. }) = receiver.as_ref() else {
        return None;
    };

    let [arg] = &**args else {
        return None;
    };

    if func_name != "remove" || !keywords.is_empty() {
        return None;
    }

    Some((arg, set))
}

/// Construct the fix suggestion, ie `set.discard(element)`.
fn make_suggestion(set: &ast::ExprName, element: &Expr, generator: Generator) -> String {
    // Here we construct `set.discard(element)`
    //
    // Let's make `set.discard`.
    let attr = ast::ExprAttribute {
        value: Box::new(set.clone().into()),
        attr: ast::Identifier::new("discard".to_string(), TextRange::default()),
        ctx: ast::ExprContext::Load,
        range: TextRange::default(),
    };
    // Make the actual call `set.discard(element)`
    let call = ast::ExprCall {
        func: Box::new(attr.into()),
        arguments: ast::Arguments {
            args: Box::from([element.clone()]),
            keywords: Box::from([]),
            range: TextRange::default(),
        },
        range: TextRange::default(),
    };
    // And finally, turn it into a statement.
    let stmt = ast::StmtExpr {
        value: Box::new(call.into()),
        range: TextRange::default(),
    };
    generator.stmt(&stmt.into())
}
