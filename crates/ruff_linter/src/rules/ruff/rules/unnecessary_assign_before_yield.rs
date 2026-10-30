use anyhow::Context;
use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::flat::visitor;
use ruff_python_ast::flat::visitor::Visitor;
use ruff_python_ast::flat::{self, fields as ast};
use ruff_python_ast::token::TokenKind;
use ruff_python_ast::{Expr, Identifier, Stmt, StmtAssign};
use ruff_python_semantic::SemanticModel;
use ruff_text_size::{Ranged, TextRange};
use rustc_hash::FxHashSet;

use crate::checkers::ast::Checker;
use crate::codes::Category;
use crate::fix::edits;
use crate::rules::flake8_return::has_conditional_body;
use crate::{AlwaysFixableViolation, Edit, Fix};

/// ## What it does
/// Checks for variable assignments that immediately precede a `yield` (or
/// `yield from`) of the assigned variable, where the variable is not
/// referenced anywhere else.
///
/// ## Why is this bad?
/// The variable assignment is not necessary, as the value can be yielded
/// directly.
///
/// ## Example
/// ```python
/// def gen():
///     x = 1
///     yield x
/// ```
///
/// Use instead:
/// ```python
/// def gen():
///     yield 1
/// ```
///
/// ## Fix safety
/// This fix is always marked as unsafe because removing the intermediate
/// variable assignment changes the local variable bindings visible to
/// `locals()` and debuggers when the generator is suspended at the `yield`.
#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "0.15.3", category = Category::Pedantic)]
pub(crate) struct UnnecessaryAssignBeforeYield {
    name: String,
    is_yield_from: bool,
}

impl AlwaysFixableViolation for UnnecessaryAssignBeforeYield {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UnnecessaryAssignBeforeYield {
            name,
            is_yield_from,
        } = self;
        if *is_yield_from {
            format!("Unnecessary assignment to `{name}` before `yield from` statement")
        } else {
            format!("Unnecessary assignment to `{name}` before `yield` statement")
        }
    }

    fn fix_title(&self) -> String {
        "Remove unnecessary assignment".to_string()
    }
}

/// RUF070
pub(crate) fn unnecessary_assign_before_yield(
    checker: &Checker,
    function_stmt: flat::View<'_, Stmt>,
) {
    let flat::StmtKind::FunctionDef(function_def) = function_stmt.classify() else {
        return;
    };

    let Some(function_scope) = checker.semantic().function_scope(function_def) else {
        return;
    };

    let visitor = {
        let mut visitor = YieldVisitor::new(checker.semantic());
        visitor.visit_body(function_def.body());
        visitor
    };

    for (assign, yield_expr, stmt) in visitor.assignment_yield {
        let (value, is_yield_from) = match yield_expr.classify() {
            flat::ExprKind::Yield(expr_yield) if let Some(value) = expr_yield.value() => {
                (value, false)
            }
            flat::ExprKind::YieldFrom(expr_yield_from) => {
                let value = expr_yield_from.value();

                (value, true)
            }
            _ => continue,
        };

        let flat::ExprKind::Name(name_node) = value.classify() else {
            continue;
        };
        let yielded_id = name_node.id();

        if let Some([ast::Expr::Name(ast::ExprName {
            id: assigned_id, ..
        })]) = assign.targets().as_array().map(|items| items.map(flat::View::<Expr>::fields))
            && *yielded_id == *assigned_id
            && !visitor.annotations.contains(assigned_id.as_str())
            && !visitor.non_locals.contains(assigned_id.as_str())
            && let Some(assigned_binding) = function_scope
                .get(assigned_id)
                .map(|binding_id| checker.semantic().binding(binding_id))
            // Unlike `return`, `yield` doesn't exit the function, so the variable could be
            // referenced elsewhere. Only flag if the binding has exactly one reference (the
            // yield itself).
            && assigned_binding.references().count() == 1
            && assigned_binding
                .references()
                .map(|reference_id| checker.semantic().reference(reference_id))
                .all(|reference| reference.scope_id() == assigned_binding.scope)
        {
            checker
                .report_diagnostic(
                    UnnecessaryAssignBeforeYield {
                        name: assigned_id.to_string(),
                        is_yield_from,
                    },
                    value.range(),
                )
                .try_set_fix(|| {
                    let delete_yield =
                        edits::delete_stmt(stmt, None, checker.locator(), checker.indexer());

                    let eq_token = checker
                        .tokens()
                        .before(assign.value().start())
                        .iter()
                        .rfind(|token| token.kind() == TokenKind::Equal)
                        .context("Expected an equals token")?;

                    let keyword = if is_yield_from { "yield from" } else { "yield" };
                    let needs_parens = matches!(
                        assign.value().classify(),
                        flat::ExprKind::Yield(_) | flat::ExprKind::YieldFrom(_)
                    );

                    let replace_assign = Edit::range_replacement(
                        if eq_token.end() < assign.value().start() {
                            keyword.to_string()
                        } else {
                            format!("{keyword} ")
                        },
                        TextRange::new(assign.start(), eq_token.range().end()),
                    );

                    let mut edits = vec![replace_assign, delete_yield];
                    if needs_parens {
                        edits.push(Edit::insertion("(".to_string(), assign.value().start()));
                        edits.push(Edit::insertion(")".to_string(), assign.value().end()));
                    }

                    Ok(Fix::unsafe_edits(edits.remove(0), edits))
                });
        }
    }
}

struct YieldVisitor<'semantic, 'a> {
    /// The semantic model of the current file.
    semantic: &'semantic SemanticModel<'a>,
    /// The non-local variables in the current function.
    non_locals: FxHashSet<&'a str>,
    /// The annotated variables in the current function.
    annotations: FxHashSet<&'a str>,
    /// The `assignment`-to-`yield` statement pairs in the current function.
    assignment_yield: Vec<(
        flat::View<'a, StmtAssign>,
        flat::View<'a, Expr>,
        flat::View<'a, Stmt>,
    )>,
    /// The preceding sibling of the current node.
    sibling: Option<flat::View<'a, Stmt>>,
}

impl<'semantic, 'a> YieldVisitor<'semantic, 'a> {
    fn new(semantic: &'semantic SemanticModel<'a>) -> Self {
        Self {
            semantic,
            non_locals: FxHashSet::default(),
            annotations: FxHashSet::default(),
            assignment_yield: Vec::new(),
            sibling: None,
        }
    }
}

impl<'a> Visitor<'a> for YieldVisitor<'_, 'a> {
    fn visit_stmt(&mut self, stmt: flat::View<'a, Stmt>) {
        match stmt.fields() {
            ast::Stmt::ClassDef(_) | ast::Stmt::FunctionDef(_) => {
                // Do not recurse into nested class/function bodies.
                self.sibling = Some(stmt);
                return;
            }
            ast::Stmt::Global(ast::StmtGlobal { names, .. })
            | ast::Stmt::Nonlocal(ast::StmtNonlocal { names, .. }) => {
                self.non_locals
                    .extend(names.iter().map(flat::View::<Identifier>::as_str));
            }
            ast::Stmt::AnnAssign(ast::StmtAnnAssign { target, value, .. }) => {
                // Ex) `x: int`
                if value.is_none()
                    && let flat::ExprKind::Name(name) = target.classify()
                {
                    self.annotations.insert(name.id().as_str());
                }
            }
            ast::Stmt::Expr(ast::StmtExpr { value, .. }) => {
                if matches!(
                    value.classify(),
                    flat::ExprKind::Yield(_) | flat::ExprKind::YieldFrom(_)
                ) {
                    match self.sibling.map(flat::View::<Stmt>::classify) {
                        Some(flat::StmtKind::Assign(stmt_assign)) => {
                            self.assignment_yield.push((stmt_assign, value, stmt));
                        }
                        Some(flat::StmtKind::With(with)) => {
                            if let Some(stmt_assign) =
                                with.body().last().and_then(flat::View::as_assign_stmt)
                                && !has_conditional_body(with, self.semantic)
                            {
                                self.assignment_yield.push((stmt_assign, value, stmt));
                            }
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }

        self.sibling = Some(stmt);
        visitor::walk_stmt(self, stmt);
    }

    fn visit_body(&mut self, body: flat::Suite<'a>) {
        let sibling = self.sibling;
        self.sibling = None;
        visitor::walk_body(self, body);
        self.sibling = sibling;
    }
}
