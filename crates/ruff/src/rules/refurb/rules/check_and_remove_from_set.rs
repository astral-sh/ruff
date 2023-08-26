use ast::{comparable::ComparableExpr, helpers::contains_effect, CmpOp, Ranged};
use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast, Expr, Stmt};
use ruff_python_codegen::Generator;
use ruff_python_semantic::{Binding, SemanticModel};
use ruff_text_size::TextRange;

use crate::autofix::snippet::SourceCodeSnippet;
use crate::{checkers::ast::Checker, rules::refurb::helpers::is_set};

/// ## What it does
/// Checks for check and `remove` pattern that can be replaced via `discard`.
///
/// ## Why is this bad?
/// It is more succinct and idiomatic to use `discard`.
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
/// - [Python documentation: set.discard()](https://docs.python.org/3/library/stdtypes.html?highlight=list#frozenset.discard)
#[violation]
pub struct CheckAndRemoveFromSet {
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

impl AlwaysAutofixableViolation for CheckAndRemoveFromSet {
    #[derive_message_formats]
    fn message(&self) -> String {
        let suggestion = self.suggestion();
        format!("Use `{suggestion}` instead of check and `remove`")
    }

    fn autofix_title(&self) -> String {
        let suggestion = self.suggestion();
        format!("Replace with `{suggestion}`")
    }
}

// FURB132
pub(crate) fn check_and_remove_from_set(checker: &mut Checker, if_stmt: &ast::StmtIf) {
    // In order to fit the profile, we need if without else clauses and with only one statement in its body.
    if if_stmt.body.len() != 1 || !if_stmt.elif_else_clauses.is_empty() {
        return;
    }

    // if test should be `element in set`
    let Some((check_element, check_set)) = match_check(if_stmt) else {
        return;
    };

    // if body should be `set.remove(element)`
    let Some((remove_element, remove_set)) = match_remove(if_stmt) else {
        return;
    };

    // `element` in the check should be the same as `element` in the body
    if !compare(&check_element.into(), &remove_element.into())
    // `set` in the check should be the same as `set` in the body
        || check_set.id != remove_set.id
    // `element` shouldn't have a side effect, otherwise we might change the samntic of the program
        || contains_effect(check_element, |id| checker.semantic().is_builtin(id))
    {
        return;
    }

    // Check if what we assume is set is indeed a set.
    if !find_binding(checker.semantic(), &check_set.id).map_or(false, |binding| {
        is_set(checker.semantic(), binding, &check_set.id)
    }) {
        return;
    };

    let replacement = make_suggestion(check_set, check_element, checker.generator());
    let element_str = checker.generator().expr(check_element);

    let mut diagnostic = Diagnostic::new(
        CheckAndRemoveFromSet {
            element: SourceCodeSnippet::new(element_str),
            set: check_set.id.to_string(),
        },
        if_stmt.range(),
    );
    diagnostic.set_fix(Fix::suggested(Edit::replacement(
        replacement,
        if_stmt.start(),
        if_stmt.end(),
    )));

    checker.diagnostics.push(diagnostic);
}

fn compare(lhs: &ComparableExpr, rhs: &ComparableExpr) -> bool {
    lhs == rhs
}

/// Construct the fix suggesstion, ie `set.discard(element)`.
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
            args: vec![element.clone()],
            keywords: vec![],
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

/// Find the binding associated with the given name in the current scope.
fn find_binding<'a>(semantic: &'a SemanticModel, name: &str) -> Option<&'a Binding<'a>> {
    // Let's find definition for var
    let scope = semantic.current_scope();
    let bindings: Vec<&Binding> = scope
        .get_all(name)
        .map(|binding_id| semantic.binding(binding_id))
        .collect();

    let [binding @ Binding {
        source: Some(..), ..
    }] = bindings.as_slice()
    else {
        return None;
    };

    Some(binding)
}

/// Match `if` condition to be `expr in name`, returns a tuple of (`expr`, `name`) on success.
fn match_check(if_stmt: &ast::StmtIf) -> Option<(&Expr, &ast::ExprName)> {
    let Expr::Compare(ast::ExprCompare {
        ops,
        left,
        comparators,
        ..
    }) = if_stmt.test.as_ref()
    else {
        return None;
    };

    if ops.as_slice() != [CmpOp::In] {
        return None;
    }

    let [Expr::Name(right @ ast::ExprName { .. })] = comparators.as_slice() else {
        return None;
    };

    Some((left.as_ref(), right))
}

/// Match `if` body to be `name.remove(expr)`, returns a tuple of (`expr`, `name`) on success.
fn match_remove(if_stmt: &ast::StmtIf) -> Option<(&Expr, &ast::ExprName)> {
    let [Stmt::Expr(ast::StmtExpr { value: expr, .. })] = if_stmt.body.as_slice() else {
        return None;
    };

    let Expr::Call(ast::ExprCall {
        func: attr,
        arguments: ast::Arguments { args, keywords, .. },
        ..
    }) = expr.as_ref()
    else {
        return None;
    };

    let Expr::Attribute(ast::ExprAttribute {
        value: receiver,
        attr: func_name,
        ..
    }) = attr.as_ref()
    else {
        return None;
    };

    let Expr::Name(ref set @ ast::ExprName { .. }) = receiver.as_ref() else {
        return None;
    };

    let [arg] = args.as_slice() else {
        return None;
    };

    if func_name != "remove" || !keywords.is_empty() {
        return None;
    }

    Some((arg, set))
}
