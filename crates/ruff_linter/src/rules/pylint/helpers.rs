use std::fmt;

use ruff_python_ast as ast;
use ruff_python_ast::visitor::Visitor;
use ruff_python_ast::{visitor, Arguments, CmpOp, Expr, Stmt};
use ruff_python_semantic::analyze::function_type;
use ruff_python_semantic::{ScopeKind, SemanticModel};
use ruff_text_size::TextRange;

use crate::settings::LinterSettings;

/// Returns the value of the `name` parameter to, e.g., a `TypeVar` constructor.
pub(super) fn type_param_name(arguments: &Arguments) -> Option<&str> {
    // Handle both `TypeVar("T")` and `TypeVar(name="T")`.
    let name_param = arguments.find_argument("name", 0)?;
    if let Expr::StringLiteral(ast::ExprStringLiteral { value, .. }) = &name_param {
        Some(value.to_str())
    } else {
        None
    }
}

pub(super) fn in_dunder_method(
    dunder_name: &str,
    semantic: &SemanticModel,
    settings: &LinterSettings,
) -> bool {
    let scope = semantic.current_scope();
    let ScopeKind::Function(ast::StmtFunctionDef {
        name,
        decorator_list,
        ..
    }) = scope.kind
    else {
        return false;
    };
    if name != dunder_name {
        return false;
    }
    let Some(parent) = semantic.first_non_type_parent_scope(scope) else {
        return false;
    };

    if !matches!(
        function_type::classify(
            name,
            decorator_list,
            parent,
            semantic,
            &settings.pep8_naming.classmethod_decorators,
            &settings.pep8_naming.staticmethod_decorators,
        ),
        function_type::FunctionType::Method
    ) {
        return false;
    }
    true
}

/// A wrapper around [`CmpOp`] that implements `Display`.
#[derive(Debug)]
pub(super) struct CmpOpExt(CmpOp);

impl From<&CmpOp> for CmpOpExt {
    fn from(cmp_op: &CmpOp) -> Self {
        CmpOpExt(*cmp_op)
    }
}

impl fmt::Display for CmpOpExt {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let representation = match self.0 {
            CmpOp::Eq => "==",
            CmpOp::NotEq => "!=",
            CmpOp::Lt => "<",
            CmpOp::LtE => "<=",
            CmpOp::Gt => ">",
            CmpOp::GtE => ">=",
            CmpOp::Is => "is",
            CmpOp::IsNot => "is not",
            CmpOp::In => "in",
            CmpOp::NotIn => "not in",
        };
        write!(f, "{representation}")
    }
}

/// Visitor to track reads from an iterable in a loop.
#[derive(Debug)]
pub(crate) struct SequenceIndexVisitor<'a> {
    /// `letters`, given `for index, letter in enumerate(letters)`.
    sequence_name: &'a str,
    /// `index`, given `for index, letter in enumerate(letters)`.
    index_name: &'a str,
    /// `letter`, given `for index, letter in enumerate(letters)`.
    value_name: &'a str,
    /// The ranges of any `letters[index]` accesses.
    accesses: Vec<TextRange>,
    /// Whether any of the variables have been modified.
    modified: bool,
}

impl<'a> SequenceIndexVisitor<'a> {
    pub(crate) fn new(sequence_name: &'a str, index_name: &'a str, value_name: &'a str) -> Self {
        Self {
            sequence_name,
            index_name,
            value_name,
            accesses: Vec::new(),
            modified: false,
        }
    }

    pub(crate) fn into_accesses(self) -> Vec<TextRange> {
        self.accesses
    }
}

impl SequenceIndexVisitor<'_> {
    fn is_assignment(&self, expr: &Expr) -> bool {
        // If we see the sequence, a subscript, or the index being modified, we'll stop emitting
        // diagnostics.
        match expr {
            Expr::Name(ast::ExprName { id, .. }) => {
                id == self.sequence_name || id == self.index_name || id == self.value_name
            }
            Expr::Subscript(ast::ExprSubscript { value, slice, .. }) => {
                let Expr::Name(ast::ExprName { id, .. }) = value.as_ref() else {
                    return false;
                };
                if id == self.sequence_name {
                    let Expr::Name(ast::ExprName { id, .. }) = slice.as_ref() else {
                        return false;
                    };
                    if id == self.index_name {
                        return true;
                    }
                }
                false
            }
            _ => false,
        }
    }
}

impl<'a> Visitor<'_> for SequenceIndexVisitor<'a> {
    fn visit_stmt(&mut self, stmt: &Stmt) {
        if self.modified {
            return;
        }
        match stmt {
            Stmt::Assign(ast::StmtAssign { targets, value, .. }) => {
                self.modified = targets.iter().any(|target| self.is_assignment(target));
                self.visit_expr(value);
            }
            Stmt::AnnAssign(ast::StmtAnnAssign { target, value, .. }) => {
                if let Some(value) = value {
                    self.modified = self.is_assignment(target);
                    self.visit_expr(value);
                }
            }
            Stmt::AugAssign(ast::StmtAugAssign { target, value, .. }) => {
                self.modified = self.is_assignment(target);
                self.visit_expr(value);
            }
            Stmt::Delete(ast::StmtDelete { targets, .. }) => {
                self.modified = targets.iter().any(|target| self.is_assignment(target));
            }
            _ => visitor::walk_stmt(self, stmt),
        }
    }

    fn visit_expr(&mut self, expr: &Expr) {
        if self.modified {
            return;
        }
        match expr {
            Expr::Subscript(ast::ExprSubscript {
                value,
                slice,
                range,
                ..
            }) => {
                let Expr::Name(ast::ExprName { id, .. }) = value.as_ref() else {
                    return;
                };
                if id == self.sequence_name {
                    let Expr::Name(ast::ExprName { id, .. }) = slice.as_ref() else {
                        return;
                    };
                    if id == self.index_name {
                        self.accesses.push(*range);
                    }
                }
            }
            _ => visitor::walk_expr(self, expr),
        }
    }
}
