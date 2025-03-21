//! [`SemanticSyntaxChecker`] for AST-based syntax errors.
//!
//! This checker is not responsible for traversing the AST itself. Instead, its
//! [`SemanticSyntaxChecker::visit_stmt`] and [`SemanticSyntaxChecker::visit_expr`] methods should
//! be called in a parent `Visitor`'s `visit_stmt` and `visit_expr` methods, respectively.

use std::fmt::Display;

use itertools::Itertools;
use ruff_python_ast::{
    self as ast,
    visitor::{walk_expr, Visitor},
    Expr, PythonVersion, Stmt, StmtExpr, StmtImportFrom,
};
use ruff_text_size::{Ranged, TextRange};

#[derive(Debug)]
pub struct SemanticSyntaxChecker {
    /// The checker has traversed past the `__future__` import boundary.
    ///
    /// For example, the checker could be visiting `x` in:
    ///
    /// ```python
    /// from __future__ import annotations
    ///
    /// import os
    ///
    /// x: int = 1
    /// ```
    ///
    /// Python considers it a syntax error to import from `__future__` after any other
    /// non-`__future__`-importing statements.
    seen_futures_boundary: bool,
}

impl SemanticSyntaxChecker {
    pub fn new() -> Self {
        Self {
            seen_futures_boundary: false,
        }
    }
}

impl SemanticSyntaxChecker {
    fn add_error<Ctx: SemanticSyntaxContext>(
        context: &Ctx,
        kind: SemanticSyntaxErrorKind,
        range: TextRange,
    ) {
        context.report_semantic_error(SemanticSyntaxError {
            kind,
            range,
            python_version: context.python_version(),
        });
    }

    fn check_stmt<Ctx: SemanticSyntaxContext>(&mut self, stmt: &ast::Stmt, ctx: &Ctx) {
        if let Stmt::ImportFrom(StmtImportFrom { range, module, .. }) = stmt {
            if self.seen_futures_boundary && matches!(module.as_deref(), Some("__future__")) {
                Self::add_error(ctx, SemanticSyntaxErrorKind::LateFutureImport, *range);
            }
        }

        Self::duplicate_type_parameter_name(stmt, ctx);
    }

    fn duplicate_type_parameter_name<Ctx: SemanticSyntaxContext>(stmt: &ast::Stmt, ctx: &Ctx) {
        let (Stmt::FunctionDef(ast::StmtFunctionDef { type_params, .. })
        | Stmt::ClassDef(ast::StmtClassDef { type_params, .. })
        | Stmt::TypeAlias(ast::StmtTypeAlias { type_params, .. })) = stmt
        else {
            return;
        };

        let Some(type_params) = type_params else {
            return;
        };

        if type_params.len() < 2 {
            return;
        }

        for type_param in type_params
            .iter()
            .rev()
            .sorted_by_key(|t| &t.name().id)
            .dedup_by_with_count(|t1, t2| t1.name().id == t2.name().id)
            .filter_map(|(count, type_param)| if count > 1 { Some(type_param) } else { None })
        {
            // test_err duplicate_type_parameter_names
            // type Alias[T, T] = ...
            // def f[T, T](t: T): ...
            // class C[T, T]: ...
            // type Alias[T, U: str, V: (str, bytes), *Ts, **P, T = default] = ...
            Self::add_error(
                ctx,
                SemanticSyntaxErrorKind::DuplicateTypeParameter,
                type_param.range(),
            );
        }
    }

    pub fn visit_stmt<Ctx: SemanticSyntaxContext>(&mut self, stmt: &ast::Stmt, ctx: &Ctx) {
        // update internal state
        match stmt {
            Stmt::Expr(StmtExpr { value, .. })
                if !ctx.seen_docstring_boundary() && value.is_string_literal_expr() => {}
            Stmt::ImportFrom(StmtImportFrom { module, .. }) => {
                // Allow __future__ imports until we see a non-__future__ import.
                if !matches!(module.as_deref(), Some("__future__")) {
                    self.seen_futures_boundary = true;
                }
            }
            _ => {
                self.seen_futures_boundary = true;
            }
        }

        // check for errors
        self.check_stmt(stmt, ctx);
    }

    pub fn visit_expr<Ctx: SemanticSyntaxContext>(&mut self, expr: &Expr, ctx: &Ctx) {
        match expr {
            Expr::ListComp(ast::ExprListComp {
                elt, generators, ..
            })
            | Expr::SetComp(ast::ExprSetComp {
                elt, generators, ..
            })
            | Expr::Generator(ast::ExprGenerator {
                elt, generators, ..
            }) => Self::check_generator_expr(elt, generators, ctx),
            Expr::DictComp(ast::ExprDictComp {
                key,
                value,
                generators,
                ..
            }) => {
                Self::check_generator_expr(key, generators, ctx);
                Self::check_generator_expr(value, generators, ctx);
            }
            _ => {}
        }
    }

    /// Add a [`SyntaxErrorKind::ReboundComprehensionVariable`] if `expr` rebinds an iteration
    /// variable in `generators`.
    fn check_generator_expr<Ctx: SemanticSyntaxContext>(
        expr: &Expr,
        comprehensions: &[ast::Comprehension],
        ctx: &Ctx,
    ) {
        let rebound_variables = {
            let mut visitor = ReboundComprehensionVisitor {
                comprehensions,
                rebound_variables: Vec::new(),
            };
            visitor.visit_expr(expr);
            visitor.rebound_variables
        };

        // TODO(brent) with multiple diagnostic ranges, we could mark both the named expr (current)
        // and the name expr being rebound
        for range in rebound_variables {
            // test_err rebound_comprehension_variable
            // [(a := 0) for a in range(0)]
            // {(a := 0) for a in range(0)}
            // {(a := 0): val for a in range(0)}
            // {key: (a := 0) for a in range(0)}
            // ((a := 0) for a in range(0))
            // [[(a := 0)] for a in range(0)]
            // [(a := 0) for b in range (0) for a in range(0)]
            // [(a := 0) for a in range (0) for b in range(0)]
            // [((a := 0), (b := 1)) for a in range (0) for b in range(0)]

            // test_ok non_rebound_comprehension_variable
            // [a := 0 for x in range(0)]
            Self::add_error(
                ctx,
                SemanticSyntaxErrorKind::ReboundComprehensionVariable,
                range,
            );
        }
    }
}

impl Default for SemanticSyntaxChecker {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SemanticSyntaxError {
    pub kind: SemanticSyntaxErrorKind,
    pub range: TextRange,
    pub python_version: PythonVersion,
}

impl Display for SemanticSyntaxError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.kind {
            SemanticSyntaxErrorKind::LateFutureImport => {
                f.write_str("__future__ imports must be at the top of the file")
            }
            SemanticSyntaxErrorKind::ReboundComprehensionVariable => {
                f.write_str("assignment expression cannot rebind comprehension variable")
            }
            SemanticSyntaxErrorKind::DuplicateTypeParameter => {
                f.write_str("duplicate type parameter")
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SemanticSyntaxErrorKind {
    /// Represents the use of a `__future__` import after the beginning of a file.
    ///
    /// ## Examples
    ///
    /// ```python
    /// from pathlib import Path
    ///
    /// from __future__ import annotations
    /// ```
    ///
    /// This corresponds to the [`late-future-import`] (`F404`) rule in ruff.
    ///
    /// [`late-future-import`]: https://docs.astral.sh/ruff/rules/late-future-import/
    LateFutureImport,

    /// Represents the rebinding of the iteration variable of a list, set, or dict comprehension or
    /// a generator expression.
    ///
    /// ## Examples
    ///
    /// ```python
    /// [(a := 0) for a in range(0)]
    /// {(a := 0) for a in range(0)}
    /// {(a := 0): val for a in range(0)}
    /// {key: (a := 0) for a in range(0)}
    /// ((a := 0) for a in range(0))
    /// ```
    ReboundComprehensionVariable,

    /// Represents a duplicate type parameter name in a function definition, class definition, or
    /// type alias statement.
    ///
    /// ## Examples
    ///
    /// ```python
    /// type Alias[T, T] = ...
    /// def f[T, T](t: T): ...
    /// class C[T, T]: ...
    /// ```
    DuplicateTypeParameter,
}

/// Searches for the first named expression (`x := y`) rebinding one of the `iteration_variables` in
/// a comprehension or generator expression.
struct ReboundComprehensionVisitor<'a> {
    comprehensions: &'a [ast::Comprehension],
    rebound_variables: Vec<TextRange>,
}

impl Visitor<'_> for ReboundComprehensionVisitor<'_> {
    fn visit_expr(&mut self, expr: &Expr) {
        if let Expr::Named(ast::ExprNamed { target, .. }) = expr {
            if let Expr::Name(ast::ExprName { id, range, .. }) = &**target {
                if self.comprehensions.iter().any(|comp| {
                    comp.target
                        .as_name_expr()
                        .is_some_and(|name| name.id == *id)
                }) {
                    self.rebound_variables.push(*range);
                }
            };
        }
        walk_expr(self, expr);
    }
}

pub trait SemanticSyntaxContext {
    /// Returns `true` if a module's docstring boundary has been passed.
    fn seen_docstring_boundary(&self) -> bool;

    /// The target Python version for detecting backwards-incompatible syntax changes.
    fn python_version(&self) -> PythonVersion;

    fn report_semantic_error(&self, error: SemanticSyntaxError);
}

#[derive(Default)]
pub struct SemanticSyntaxCheckerVisitor<Ctx> {
    checker: SemanticSyntaxChecker,
    context: Ctx,
}

impl<Ctx> SemanticSyntaxCheckerVisitor<Ctx> {
    pub fn new(context: Ctx) -> Self {
        Self {
            checker: SemanticSyntaxChecker::new(),
            context,
        }
    }

    pub fn into_context(self) -> Ctx {
        self.context
    }
}

impl<Ctx> Visitor<'_> for SemanticSyntaxCheckerVisitor<Ctx>
where
    Ctx: SemanticSyntaxContext,
{
    fn visit_stmt(&mut self, stmt: &'_ Stmt) {
        self.checker.visit_stmt(stmt, &self.context);
        ruff_python_ast::visitor::walk_stmt(self, stmt);
    }

    fn visit_expr(&mut self, expr: &'_ Expr) {
        self.checker.visit_expr(expr, &self.context);
        ruff_python_ast::visitor::walk_expr(self, expr);
    }
}
