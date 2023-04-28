use rustpython_parser::ast::Constant;

use ruff_python_ast::source_code::Locator;

use crate::cst::helpers::is_radix_literal;
use crate::cst::visitor;
use crate::cst::visitor::Visitor;
use crate::cst::{Expr, ExprKind, Stmt, StmtKind};
use crate::trivia::Parenthesize;

/// Modify an [`Expr`] to infer parentheses, rather than respecting any user-provided trivia.
fn use_inferred_parens(expr: &mut Expr) {
    // Remove parentheses, unless it's a generator expression, in which case, keep them.
    if !matches!(expr.node, ExprKind::GeneratorExp { .. }) {
        expr.trivia.retain(|trivia| !trivia.kind.is_parentheses());
    }

    // If it's a tuple, add parentheses if it's a singleton; otherwise, we only need parentheses
    // if the tuple expands.
    if let ExprKind::Tuple { elts, .. } = &expr.node {
        expr.parentheses = if elts.len() > 1 {
            Parenthesize::IfExpanded
        } else {
            Parenthesize::Always
        };
    }
}

struct ParenthesesNormalizer<'a> {
    locator: &'a Locator<'a>,
}

impl<'a> Visitor<'a> for ParenthesesNormalizer<'_> {
    fn visit_stmt(&mut self, stmt: &'a mut Stmt) {
        // Always remove parentheses around statements, unless it's an expression statement,
        // in which case, remove parentheses around the expression.
        let before = stmt.trivia.len();
        stmt.trivia.retain(|trivia| !trivia.kind.is_parentheses());
        let after = stmt.trivia.len();
        if let StmtKind::Expr { value } = &mut stmt.node {
            if before != after {
                stmt.parentheses = Parenthesize::Always;
                value.parentheses = Parenthesize::Never;
            }
        }

        // In a variety of contexts, remove parentheses around sub-expressions. Right now, the
        // pattern is consistent (and repeated), but it may not end up that way.
        // https://black.readthedocs.io/en/stable/the_black_code_style/current_style.html#parentheses
        match &mut stmt.node {
            StmtKind::FunctionDef { .. } => {}
            StmtKind::AsyncFunctionDef { .. } => {}
            StmtKind::ClassDef { .. } => {}
            StmtKind::Return { value } => {
                if let Some(value) = value {
                    use_inferred_parens(value);
                }
            }
            StmtKind::Delete { .. } => {}
            StmtKind::Assign { targets, value, .. } => {
                for target in targets {
                    use_inferred_parens(target);
                }
                use_inferred_parens(value);
            }
            StmtKind::AugAssign { value, .. } => {
                use_inferred_parens(value);
            }
            StmtKind::AnnAssign { value, .. } => {
                if let Some(value) = value {
                    use_inferred_parens(value);
                }
            }
            StmtKind::For { target, iter, .. } | StmtKind::AsyncFor { target, iter, .. } => {
                use_inferred_parens(target);
                if !matches!(iter.node, ExprKind::Tuple { .. }) {
                    use_inferred_parens(iter);
                }
            }
            StmtKind::While { test, .. } => {
                use_inferred_parens(test);
            }
            StmtKind::If { test, .. } => {
                use_inferred_parens(test);
            }
            StmtKind::With { .. } => {}
            StmtKind::AsyncWith { .. } => {}
            StmtKind::Match { .. } => {}
            StmtKind::Raise { .. } => {}
            StmtKind::Try { .. } => {}
            StmtKind::TryStar { .. } => {}
            StmtKind::Assert { test, msg } => {
                use_inferred_parens(test);
                if let Some(msg) = msg {
                    use_inferred_parens(msg);
                }
            }
            StmtKind::Import { .. } => {}
            StmtKind::ImportFrom { .. } => {}
            StmtKind::Global { .. } => {}
            StmtKind::Nonlocal { .. } => {}
            StmtKind::Expr { .. } => {}
            StmtKind::Pass => {}
            StmtKind::Break => {}
            StmtKind::Continue => {}
        }

        visitor::walk_stmt(self, stmt);
    }

    fn visit_expr(&mut self, expr: &'a mut Expr) {
        // Always retain parentheses around expressions.
        let before = expr.trivia.len();
        expr.trivia.retain(|trivia| !trivia.kind.is_parentheses());
        let after = expr.trivia.len();
        if before != after {
            expr.parentheses = Parenthesize::Always;
        }

        match &mut expr.node {
            ExprKind::BoolOp { .. } => {}
            ExprKind::NamedExpr { .. } => {}
            ExprKind::BinOp { .. } => {}
            ExprKind::UnaryOp { .. } => {}
            ExprKind::Lambda { .. } => {}
            ExprKind::IfExp { .. } => {}
            ExprKind::Dict { .. } => {}
            ExprKind::Set { .. } => {}
            ExprKind::ListComp { .. } => {}
            ExprKind::SetComp { .. } => {}
            ExprKind::DictComp { .. } => {}
            ExprKind::GeneratorExp { .. } => {}
            ExprKind::Await { .. } => {}
            ExprKind::Yield { .. } => {}
            ExprKind::YieldFrom { .. } => {}
            ExprKind::Compare { .. } => {}
            ExprKind::Call { .. } => {}
            ExprKind::FormattedValue { .. } => {}
            ExprKind::JoinedStr { .. } => {}
            ExprKind::Constant { .. } => {}
            ExprKind::Attribute { value, .. } => {
                if matches!(
                    value.node,
                    ExprKind::Constant {
                        value: Constant::Float(..),
                        ..
                    },
                ) {
                    value.parentheses = Parenthesize::Always;
                } else if matches!(
                    value.node,
                    ExprKind::Constant {
                        value: Constant::Int(..),
                        ..
                    },
                ) {
                    // TODO(charlie): Encode this in the AST via separate node types.
                    if !is_radix_literal(self.locator.slice(value.range())) {
                        value.parentheses = Parenthesize::Always;
                    }
                }
            }
            ExprKind::Subscript { value, slice, .. } => {
                // If the slice isn't manually parenthesized, ensure that we _never_ parenthesize
                // the value.
                if !slice
                    .trivia
                    .iter()
                    .any(|trivia| trivia.kind.is_parentheses())
                {
                    value.parentheses = Parenthesize::Never;
                }
            }
            ExprKind::Starred { .. } => {}
            ExprKind::Name { .. } => {}
            ExprKind::List { .. } => {}
            ExprKind::Tuple { .. } => {}
            ExprKind::Slice { .. } => {}
        }

        visitor::walk_expr(self, expr);
    }
}

/// Normalize parentheses in a Python CST.
///
/// It's not always possible to determine the correct parentheses to use during formatting
/// from the node (and trivia) alone; sometimes, we need to know the parent node. This
/// visitor normalizes parentheses via a top-down traversal, which simplifies the formatting
/// code later on.
///
/// TODO(charlie): It's weird that we have both `TriviaKind::Parentheses` (which aren't used
/// during formatting) and `Parenthesize` (which are used during formatting).
pub fn normalize_parentheses(python_cst: &mut [Stmt], locator: &Locator) {
    let mut normalizer = ParenthesesNormalizer { locator };
    for stmt in python_cst {
        normalizer.visit_stmt(stmt);
    }
}
