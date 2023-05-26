use ruff_text_size::TextRange;
use rustpython_parser::ast::{self, Cmpop, Expr, ExprContext, Ranged, Stmt, Unaryop};

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_semantic::scope::ScopeKind;

use crate::checkers::ast::Checker;
use crate::registry::AsRule;

#[violation]
pub struct NegateEqualOp {
    left: String,
    right: String,
}

impl AlwaysAutofixableViolation for NegateEqualOp {
    #[derive_message_formats]
    fn message(&self) -> String {
        let NegateEqualOp { left, right } = self;
        format!("Use `{left} != {right}` instead of `not {left} == {right}`")
    }

    fn autofix_title(&self) -> String {
        "Replace with `!=` operator".to_string()
    }
}

#[violation]
pub struct NegateNotEqualOp {
    left: String,
    right: String,
}

impl AlwaysAutofixableViolation for NegateNotEqualOp {
    #[derive_message_formats]
    fn message(&self) -> String {
        let NegateNotEqualOp { left, right } = self;
        format!("Use `{left} == {right}` instead of `not {left} != {right}`")
    }

    fn autofix_title(&self) -> String {
        "Replace with `==` operator".to_string()
    }
}

#[violation]
pub struct DoubleNegation {
    expr: String,
}

impl AlwaysAutofixableViolation for DoubleNegation {
    #[derive_message_formats]
    fn message(&self) -> String {
        let DoubleNegation { expr } = self;
        format!("Use `{expr}` instead of `not (not {expr})`")
    }

    fn autofix_title(&self) -> String {
        let DoubleNegation { expr } = self;
        format!("Replace with `{expr}`")
    }
}

const DUNDER_METHODS: &[&str] = &["__eq__", "__ne__", "__lt__", "__le__", "__gt__", "__ge__"];

fn is_exception_check(stmt: &Stmt) -> bool {
    let Stmt::If(ast::StmtIf {test: _, body, orelse: _, range: _ })= stmt else {
        return false;
    };
    if body.len() != 1 {
        return false;
    }
    if matches!(body[0], Stmt::Raise(_)) {
        return true;
    }
    false
}

/// SIM201
pub(crate) fn negation_with_equal_op(
    checker: &mut Checker,
    expr: &Expr,
    op: Unaryop,
    operand: &Expr,
) {
    if !matches!(op, Unaryop::Not) {
        return;
    }
    let Expr::Compare(ast::ExprCompare { left, ops, comparators, range: _}) = operand else {
        return;
    };
    if !matches!(&ops[..], [Cmpop::Eq]) {
        return;
    }
    if is_exception_check(checker.semantic_model().stmt()) {
        return;
    }

    // Avoid flagging issues in dunder implementations.
    if let ScopeKind::Function(ast::StmtFunctionDef { name, .. })
    | ScopeKind::AsyncFunction(ast::StmtAsyncFunctionDef { name, .. }) =
        &checker.semantic_model().scope().kind
    {
        if DUNDER_METHODS.contains(&name.as_str()) {
            return;
        }
    }

    let mut diagnostic = Diagnostic::new(
        NegateEqualOp {
            left: checker.generator().expr(left),
            right: checker.generator().expr(&comparators[0]),
        },
        expr.range(),
    );
    if checker.patch(diagnostic.kind.rule()) {
        let node = ast::ExprCompare {
            left: left.clone(),
            ops: vec![Cmpop::NotEq],
            comparators: comparators.clone(),
            range: TextRange::default(),
        };
        #[allow(deprecated)]
        diagnostic.set_fix(Fix::unspecified(Edit::range_replacement(
            checker.generator().expr(&node.into()),
            expr.range(),
        )));
    }
    checker.diagnostics.push(diagnostic);
}

/// SIM202
pub(crate) fn negation_with_not_equal_op(
    checker: &mut Checker,
    expr: &Expr,
    op: Unaryop,
    operand: &Expr,
) {
    if !matches!(op, Unaryop::Not) {
        return;
    }
    let Expr::Compare(ast::ExprCompare { left, ops, comparators, range: _}) = operand else {
        return;
    };
    if !matches!(&ops[..], [Cmpop::NotEq]) {
        return;
    }
    if is_exception_check(checker.semantic_model().stmt()) {
        return;
    }

    // Avoid flagging issues in dunder implementations.
    if let ScopeKind::Function(ast::StmtFunctionDef { name, .. })
    | ScopeKind::AsyncFunction(ast::StmtAsyncFunctionDef { name, .. }) =
        &checker.semantic_model().scope().kind
    {
        if DUNDER_METHODS.contains(&name.as_str()) {
            return;
        }
    }

    let mut diagnostic = Diagnostic::new(
        NegateNotEqualOp {
            left: checker.generator().expr(left),
            right: checker.generator().expr(&comparators[0]),
        },
        expr.range(),
    );
    if checker.patch(diagnostic.kind.rule()) {
        let node = ast::ExprCompare {
            left: left.clone(),
            ops: vec![Cmpop::Eq],
            comparators: comparators.clone(),
            range: TextRange::default(),
        };
        #[allow(deprecated)]
        diagnostic.set_fix(Fix::unspecified(Edit::range_replacement(
            checker.generator().expr(&node.into()),
            expr.range(),
        )));
    }
    checker.diagnostics.push(diagnostic);
}

/// SIM208
pub(crate) fn double_negation(checker: &mut Checker, expr: &Expr, op: Unaryop, operand: &Expr) {
    if !matches!(op, Unaryop::Not) {
        return;
    }
    let Expr::UnaryOp(ast::ExprUnaryOp { op: operand_op, operand, range: _ }) = operand else {
        return;
    };
    if !matches!(operand_op, Unaryop::Not) {
        return;
    }

    let mut diagnostic = Diagnostic::new(
        DoubleNegation {
            expr: checker.generator().expr(operand),
        },
        expr.range(),
    );
    if checker.patch(diagnostic.kind.rule()) {
        if checker.semantic_model().in_boolean_test() {
            #[allow(deprecated)]
            diagnostic.set_fix(Fix::unspecified(Edit::range_replacement(
                checker.generator().expr(operand),
                expr.range(),
            )));
        } else if checker.semantic_model().is_builtin("bool") {
            let node = ast::ExprName {
                id: "bool".into(),
                ctx: ExprContext::Load,
                range: TextRange::default(),
            };
            let node1 = ast::ExprCall {
                func: Box::new(node.into()),
                args: vec![*operand.clone()],
                keywords: vec![],
                range: TextRange::default(),
            };
            #[allow(deprecated)]
            diagnostic.set_fix(Fix::unspecified(Edit::range_replacement(
                checker.generator().expr(&node1.into()),
                expr.range(),
            )));
        };
    }
    checker.diagnostics.push(diagnostic);
}
