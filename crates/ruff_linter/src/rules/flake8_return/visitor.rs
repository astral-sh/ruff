use std::ops::Add;

use ruff_diagnostics::Edit;
use ruff_python_ast::{self as ast, ElifElseClause, Expr, Identifier, Stmt};
use ruff_text_size::{Ranged, TextRange, TextSize};
use rustc_hash::FxHashSet;

use ruff_python_ast::visitor;
use ruff_python_ast::visitor::Visitor;
use ruff_python_trivia::is_python_whitespace;

use anyhow::Result;
use std::fmt::Debug;

use crate::checkers::ast::Checker;

#[derive(Default)]
pub(super) struct Stack<'a> {
    /// The `return` statements in the current function.
    pub(super) returns: Vec<&'a ast::StmtReturn>,
    /// The `elif` or `else` statements in the current function.
    pub(super) elifs_elses: Vec<(&'a [Stmt], &'a ElifElseClause)>,
    /// The non-local variables in the current function.
    pub(super) non_locals: FxHashSet<&'a str>,
    /// Whether the current function is a generator.
    pub(super) is_generator: bool,
    /// The `assignment`-to-`return` statement pairs in the current function.
    /// TODO(charlie): Remove the extra [`Stmt`] here, which is necessary to support statement
    /// removal for the `return` statement.
    pub(super) assignment_return: Vec<(UnifiedAssignStatement<'a>, &'a ast::StmtReturn, &'a Stmt)>,
}

#[derive(Default)]
pub(super) struct ReturnVisitor<'a> {
    /// The current stack of nodes.
    pub(super) stack: Stack<'a>,
    /// The preceding sibling of the current node.
    sibling: Option<&'a Stmt>,
    /// The parent nodes of the current node.
    parents: Vec<&'a Stmt>,
}

impl<'a> Visitor<'a> for ReturnVisitor<'a> {
    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        match stmt {
            Stmt::ClassDef(ast::StmtClassDef { decorator_list, .. }) => {
                // Visit the decorators, etc.
                self.sibling = Some(stmt);
                self.parents.push(stmt);
                for decorator in decorator_list {
                    visitor::walk_decorator(self, decorator);
                }
                self.parents.pop();

                // But don't recurse into the body.
                return;
            }
            Stmt::FunctionDef(ast::StmtFunctionDef {
                parameters,
                decorator_list,
                returns,
                ..
            }) => {
                // Visit the decorators, etc.
                self.sibling = Some(stmt);
                self.parents.push(stmt);
                for decorator in decorator_list {
                    visitor::walk_decorator(self, decorator);
                }
                if let Some(returns) = returns {
                    visitor::walk_expr(self, returns);
                }
                visitor::walk_parameters(self, parameters);
                self.parents.pop();

                // But don't recurse into the body.
                return;
            }
            Stmt::Global(ast::StmtGlobal { names, range: _ })
            | Stmt::Nonlocal(ast::StmtNonlocal { names, range: _ }) => {
                self.stack
                    .non_locals
                    .extend(names.iter().map(Identifier::as_str));
            }
            Stmt::Return(stmt_return) => {
                // If the `return` statement is preceded by a statement in the `assignment` family, then
                // the statement may be redundant.
                if let Some(sibling) = self.sibling {
                    match sibling {
                        // Example:
                        // ```python
                        // def foo():
                        //     x = 1
                        //     return x
                        // ```
                        Stmt::Assign(stmt_assign) => {
                            let unified = UnifiedAssignStatement::Assign(stmt_assign);
                            self.stack
                                .assignment_return
                                .push((unified, stmt_return, stmt));
                        }
                        Stmt::AugAssign(stmt_aug_assign) => {
                            let unified = UnifiedAssignStatement::AugAssign(stmt_aug_assign);
                            self.stack
                                .assignment_return
                                .push((unified, stmt_return, stmt));
                        }
                        Stmt::AnnAssign(stmt_ann_assign) => {
                            let unified = UnifiedAssignStatement::AnnAssign(stmt_ann_assign);
                            self.stack
                                .assignment_return
                                .push((unified, stmt_return, stmt));
                        }
                        Stmt::With(ast::StmtWith { body, .. }) => {
                            let mut unified_opt = None;
                            let last = body.last();

                            // Example:
                            // ```python
                            // def foo():
                            //     with open("foo.txt", "r") as f:
                            //         x = f.read()
                            //     return x
                            // ```
                            if let Some(stmt_assign) = last.and_then(Stmt::as_assign_stmt) {
                                unified_opt = Some(UnifiedAssignStatement::Assign(stmt_assign));
                            }

                            // Example:
                            // ```python
                            // def foo():
                            //     with open("foo.txt", "r") as f:
                            //         x = f.read()
                            //         x += 1
                            //     return x
                            // ```
                            if let Some(stmt_aug_assign) = last.and_then(Stmt::as_aug_assign_stmt) {
                                unified_opt =
                                    Some(UnifiedAssignStatement::AugAssign(stmt_aug_assign));
                            }

                            // Example:
                            // ```python
                            // def foo():
                            //     with open("foo.txt", "r") as f:
                            //         x: int = f.read()
                            //     return x
                            // ```
                            if let Some(stmt_ann_assign) = last.and_then(Stmt::as_ann_assign_stmt) {
                                unified_opt =
                                    Some(UnifiedAssignStatement::AnnAssign(stmt_ann_assign));
                            }

                            if let Some(unified) = unified_opt {
                                self.stack
                                    .assignment_return
                                    .push((unified, stmt_return, stmt));
                            }
                        }
                        _ => {}
                    }
                }

                self.stack.returns.push(stmt_return);
            }
            Stmt::If(ast::StmtIf {
                body,
                elif_else_clauses,
                ..
            }) => {
                if let Some(first) = elif_else_clauses.first() {
                    self.stack.elifs_elses.push((body, first));
                }
            }
            _ => {}
        }

        self.sibling = Some(stmt);
        self.parents.push(stmt);
        visitor::walk_stmt(self, stmt);
        self.parents.pop();
    }

    fn visit_expr(&mut self, expr: &'a Expr) {
        match expr {
            Expr::YieldFrom(_) | Expr::Yield(_) => {
                self.stack.is_generator = true;
            }
            _ => visitor::walk_expr(self, expr),
        }
    }

    fn visit_body(&mut self, body: &'a [Stmt]) {
        let sibling = self.sibling;
        self.sibling = None;
        visitor::walk_body(self, body);
        self.sibling = sibling;
    }
}

// Unifies the `StmtAssign` family of AST statements into one struct
#[derive(Debug)]
pub(super) enum UnifiedAssignStatement<'a> {
    // i.e. `x = 1`
    Assign(&'a ast::StmtAssign),
    // With a type assignment, i.e. `x: int = 1`
    AnnAssign(&'a ast::StmtAnnAssign),
    // With an augmentation, i.e. `x += 1`
    AugAssign(&'a ast::StmtAugAssign),
}

impl<'a> UnifiedAssignStatement<'a> {
    // Get the `target`(s) of the assignment expression
    pub(super) fn targets(&self) -> Vec<&Expr> {
        match self {
            Self::Assign(stmt) => stmt.targets.iter().collect::<Vec<&Expr>>(),
            Self::AugAssign(stmt) => vec![stmt.target.as_ref()],
            Self::AnnAssign(stmt) => vec![stmt.target.as_ref()],
        }
    }

    // Generate the arguments for `Fix::unsafe_edits` required to fix the respective
    // `UnifiedAssignStatement` variants
    pub(super) fn create_edit(&self, checker: &mut Checker) -> Result<Edit> {
        let content = checker.locator().slice(self);
        let equals_index = content
            .find('=')
            .ok_or(anyhow::anyhow!("expected '=' in assignment statement"))?;

        match self {
            Self::Assign(_) | Self::AnnAssign(_) => {
                // Replace the `x = 1` statement with `return 1`. This also works with `x: int = 1`
                // since the type annotation is between the start of the assignment statement
                // and the equals sign.

                Ok(Edit::range_replacement(
                    Self::whitespaced_return(content, equals_index),
                    // Replace from the start of the assignment statement to the end of the equals
                    // sign.
                    TextRange::new(
                        self.start(),
                        self.range()
                            .start()
                            .add(TextSize::try_from(equals_index + 1)?),
                    ),
                ))
            }
            Self::AugAssign(stmt) => {
                // Python's AST doesn't give us the index of the operator character directly, so we
                // have to find it manually.
                let op_char_opt = content
                    .chars()
                    .rev()
                    .skip(content.len() - equals_index)
                    .find(|c| !is_python_whitespace(*c));

                let Some(op_char) = op_char_opt else {
                    return Err(anyhow::anyhow!(
                        "Augmented assignment statement with no operator"
                    ));
                };

                let ast::StmtAugAssign { target, value, .. } = stmt;
                // SAFETY - safe because we never would've tried to generate a diagnostic if the
                // statement didn't have a target
                let target_str = match **target {
                    Expr::Name(ast::ExprName {
                        id: ref id_name, ..
                    }) => Some(id_name.to_string()),
                    _ => None,
                }
                .unwrap();

                let value_str_opt = match **value {
                    Expr::NumberLiteral(ast::ExprNumberLiteral {
                        value: ref value_value,
                        ..
                    }) => match value_value {
                        ast::Number::Int(i) => Some(i.to_string()),
                        ast::Number::Float(f) => Some(f.to_string()),
                        // TODO(evan): handle complex number addition
                        ast::Number::Complex { .. } => None,
                    },
                    Expr::Name(ast::ExprName {
                        id: ref id_name, ..
                    }) => Some(id_name.clone()),
                    _ => None,
                };

                let Some(value_str) = value_str_opt else {
                    return Err(anyhow::anyhow!(
                        "Augmented assignment statement with no value"
                    ));
                };

                Ok(Edit::range_replacement(
                    [
                        Self::whitespaced_return(content, equals_index),
                        target_str,
                        op_char.to_string(),
                        value_str,
                    ]
                    .join(" "),
                    self.range(),
                ))
            }
        }
    }

    fn whitespaced_return(content: &str, equals_index: usize) -> String {
        if content[equals_index + 1..]
            .chars()
            .next()
            .is_some_and(is_python_whitespace)
        {
            "return".to_string()
        } else {
            "return ".to_string()
        }
    }
}

impl<'a> TryFrom<UnifiedAssignStatement<'a>> for &'a ast::StmtAssign {
    type Error = UnifiedAssignStatement<'a>;

    fn try_from(value: UnifiedAssignStatement<'a>) -> std::result::Result<Self, Self::Error> {
        match value {
            UnifiedAssignStatement::Assign(inner) => Ok(inner),
            other => Err(other),
        }
    }
}

impl<'a> TryFrom<UnifiedAssignStatement<'a>> for &'a ast::StmtAnnAssign {
    type Error = UnifiedAssignStatement<'a>;

    fn try_from(value: UnifiedAssignStatement<'a>) -> std::result::Result<Self, Self::Error> {
        match value {
            UnifiedAssignStatement::AnnAssign(inner) => Ok(inner),
            other => Err(other),
        }
    }
}

impl<'a> TryFrom<UnifiedAssignStatement<'a>> for &'a ast::StmtAugAssign {
    type Error = UnifiedAssignStatement<'a>;

    fn try_from(value: UnifiedAssignStatement<'a>) -> std::result::Result<Self, Self::Error> {
        match value {
            UnifiedAssignStatement::AugAssign(inner) => Ok(inner),
            other => Err(other),
        }
    }
}

impl<'a> Ranged for UnifiedAssignStatement<'a> {
    fn range(&self) -> TextRange {
        match self {
            Self::Assign(s) => s.range(),
            Self::AnnAssign(s) => s.range(),
            Self::AugAssign(s) => s.range(),
        }
    }

    fn start(&self) -> TextSize {
        match self {
            Self::Assign(s) => s.start(),
            Self::AnnAssign(s) => s.start(),
            Self::AugAssign(s) => s.start(),
        }
    }

    fn end(&self) -> TextSize {
        match self {
            Self::Assign(s) => s.end(),
            Self::AnnAssign(s) => s.end(),
            Self::AugAssign(s) => s.end(),
        }
    }
}
