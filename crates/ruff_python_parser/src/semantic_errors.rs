//! [`SemanticSyntaxChecker`] for AST-based syntax errors.
//!
//! This checker is not responsible for traversing the AST itself. Instead, its
//! [`SemanticSyntaxChecker::visit_stmt`] and [`SemanticSyntaxChecker::visit_expr`] methods should
//! be called in a parent `Visitor`'s `visit_stmt` and `visit_expr` methods, respectively.

use std::fmt::Display;

use ruff_python_ast::{
    self as ast,
    visitor::{walk_expr, Visitor},
    Expr, ExprContext, IrrefutablePatternKind, Pattern, PythonVersion, Stmt, StmtExpr,
    StmtImportFrom,
};
use ruff_text_size::{Ranged, TextRange, TextSize};
use rustc_hash::FxHashSet;

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
        match stmt {
            Stmt::ImportFrom(StmtImportFrom { range, module, .. }) => {
                if self.seen_futures_boundary && matches!(module.as_deref(), Some("__future__")) {
                    Self::add_error(ctx, SemanticSyntaxErrorKind::LateFutureImport, *range);
                }
            }
            Stmt::Match(match_stmt) => {
                Self::irrefutable_match_case(match_stmt, ctx);
                Self::multiple_case_assignment(match_stmt, ctx);
            }
            Stmt::FunctionDef(ast::StmtFunctionDef { type_params, .. })
            | Stmt::ClassDef(ast::StmtClassDef { type_params, .. })
            | Stmt::TypeAlias(ast::StmtTypeAlias { type_params, .. }) => {
                if let Some(type_params) = type_params {
                    Self::duplicate_type_parameter_name(type_params, ctx);
                }
            }
            Stmt::Assign(ast::StmtAssign { targets, .. }) => {
                if let [Expr::Starred(ast::ExprStarred { range, .. })] = targets.as_slice() {
                    // test_ok single_starred_assignment_target
                    // (*a,) = (1,)
                    // *a, = (1,)
                    // [*a] = (1,)

                    // test_err single_starred_assignment_target
                    // *a = (1,)
                    Self::add_error(
                        ctx,
                        SemanticSyntaxErrorKind::SingleStarredAssignment,
                        *range,
                    );
                }
            }
            Stmt::Return(ast::StmtReturn {
                value: Some(value), ..
            }) => {
                // test_err single_star_return
                // def f(): return *x
                Self::invalid_star_expression(value, ctx);
            }
            Stmt::For(ast::StmtFor { target, iter, .. }) => {
                // test_err single_star_for
                // for _ in *x: ...
                // for *x in xs: ...
                Self::invalid_star_expression(target, ctx);
                Self::invalid_star_expression(iter, ctx);
            }
            _ => {}
        }

        Self::debug_shadowing(stmt, ctx);
    }

    /// Emit a [`SemanticSyntaxErrorKind::InvalidStarExpression`] if `expr` is starred.
    fn invalid_star_expression<Ctx: SemanticSyntaxContext>(expr: &Expr, ctx: &Ctx) {
        // test_ok single_star_in_tuple
        // def f(): yield (*x,)
        // def f(): return (*x,)
        // for _ in (*x,): ...
        // for (*x,) in xs: ...
        if expr.is_starred_expr() {
            Self::add_error(
                ctx,
                SemanticSyntaxErrorKind::InvalidStarExpression,
                expr.range(),
            );
        }
    }

    /// Check for [`SemanticSyntaxErrorKind::WriteToDebug`] in `stmt`.
    fn debug_shadowing<Ctx: SemanticSyntaxContext>(stmt: &ast::Stmt, ctx: &Ctx) {
        match stmt {
            Stmt::FunctionDef(ast::StmtFunctionDef {
                name,
                type_params,
                parameters,
                ..
            }) => {
                // test_err debug_shadow_function
                // def __debug__(): ...  # function name
                // def f[__debug__](): ...  # type parameter name
                // def f(__debug__): ...  # parameter name
                Self::check_identifier(name, ctx);
                if let Some(type_params) = type_params {
                    for type_param in type_params.iter() {
                        Self::check_identifier(type_param.name(), ctx);
                    }
                }
                for parameter in parameters {
                    Self::check_identifier(parameter.name(), ctx);
                }
            }
            Stmt::ClassDef(ast::StmtClassDef {
                name, type_params, ..
            }) => {
                // test_err debug_shadow_class
                // class __debug__: ...  # class name
                // class C[__debug__]: ...  # type parameter name
                Self::check_identifier(name, ctx);
                if let Some(type_params) = type_params {
                    for type_param in type_params.iter() {
                        Self::check_identifier(type_param.name(), ctx);
                    }
                }
            }
            Stmt::TypeAlias(ast::StmtTypeAlias {
                type_params: Some(type_params),
                ..
            }) => {
                // test_err debug_shadow_type_alias
                // type __debug__ = list[int]  # visited as an Expr but still flagged
                // type Debug[__debug__] = str
                for type_param in type_params.iter() {
                    Self::check_identifier(type_param.name(), ctx);
                }
            }
            Stmt::Import(ast::StmtImport { names, .. })
            | Stmt::ImportFrom(ast::StmtImportFrom { names, .. }) => {
                // test_err debug_shadow_import
                // import __debug__
                // import debug as __debug__
                // from x import __debug__
                // from x import debug as __debug__

                // test_ok debug_rename_import
                // import __debug__ as debug
                // from __debug__ import Some
                // from x import __debug__ as debug
                for name in names {
                    match &name.asname {
                        Some(asname) => Self::check_identifier(asname, ctx),
                        None => Self::check_identifier(&name.name, ctx),
                    }
                }
            }
            Stmt::Try(ast::StmtTry { handlers, .. }) => {
                // test_err debug_shadow_try
                // try: ...
                // except Exception as __debug__: ...
                for handler in handlers
                    .iter()
                    .filter_map(ast::ExceptHandler::as_except_handler)
                {
                    if let Some(name) = &handler.name {
                        Self::check_identifier(name, ctx);
                    }
                }
            }
            // test_err debug_shadow_with
            // with open("foo.txt") as __debug__: ...
            _ => {}
        }
    }

    /// Check if `ident` is equal to `__debug__` and emit a
    /// [`SemanticSyntaxErrorKind::WriteToDebug`] if so.
    fn check_identifier<Ctx: SemanticSyntaxContext>(ident: &ast::Identifier, ctx: &Ctx) {
        if ident.id == "__debug__" {
            Self::add_error(
                ctx,
                SemanticSyntaxErrorKind::WriteToDebug(WriteToDebugKind::Store),
                ident.range,
            );
        }
    }

    fn duplicate_type_parameter_name<Ctx: SemanticSyntaxContext>(
        type_params: &ast::TypeParams,
        ctx: &Ctx,
    ) {
        if type_params.len() < 2 {
            return;
        }

        for (i, type_param) in type_params.iter().enumerate() {
            if type_params
                .iter()
                .take(i)
                .any(|t| t.name().id == type_param.name().id)
            {
                // test_ok non_duplicate_type_parameter_names
                // type Alias[T] = list[T]
                // def f[T](t: T): ...
                // class C[T]: ...
                // class C[T, U, V]: ...
                // type Alias[T, U: str, V: (str, bytes), *Ts, **P, D = default] = ...

                // test_err duplicate_type_parameter_names
                // type Alias[T, T] = ...
                // def f[T, T](t: T): ...
                // class C[T, T]: ...
                // type Alias[T, U: str, V: (str, bytes), *Ts, **P, T = default] = ...
                // def f[T, T, T](): ...  # two errors
                // def f[T, *T](): ...    # star is still duplicate
                // def f[T, **T](): ...   # as is double star
                Self::add_error(
                    ctx,
                    SemanticSyntaxErrorKind::DuplicateTypeParameter,
                    type_param.range(),
                );
            }
        }
    }

    fn multiple_case_assignment<Ctx: SemanticSyntaxContext>(stmt: &ast::StmtMatch, ctx: &Ctx) {
        for case in &stmt.cases {
            let mut visitor = MultipleCaseAssignmentVisitor {
                names: FxHashSet::default(),
                ctx,
            };
            visitor.visit_pattern(&case.pattern);
        }
    }

    fn irrefutable_match_case<Ctx: SemanticSyntaxContext>(stmt: &ast::StmtMatch, ctx: &Ctx) {
        // test_ok irrefutable_case_pattern_at_end
        // match x:
        //     case 2: ...
        //     case var: ...
        // match x:
        //     case 2: ...
        //     case _: ...
        // match x:
        //     case var if True: ...  # don't try to refute a guarded pattern
        //     case 2: ...

        // test_err irrefutable_case_pattern
        // match x:
        //     case var: ...  # capture pattern
        //     case 2: ...
        // match x:
        //     case _: ...
        //     case 2: ...    # wildcard pattern
        // match x:
        //     case var1 as var2: ...  # as pattern with irrefutable left-hand side
        //     case 2: ...
        // match x:
        //     case enum.variant | var: ...  # or pattern with irrefutable part
        //     case 2: ...
        for case in stmt
            .cases
            .iter()
            .rev()
            .skip(1)
            .filter_map(|case| match case.guard {
                Some(_) => None,
                None => case.pattern.irrefutable_pattern(),
            })
        {
            Self::add_error(
                ctx,
                SemanticSyntaxErrorKind::IrrefutableCasePattern(case.kind),
                case.range,
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
            Expr::Name(ast::ExprName {
                range,
                id,
                ctx: expr_ctx,
            }) => {
                // test_err write_to_debug_expr
                // del __debug__
                // del x, y, __debug__, z
                // __debug__ = 1
                // x, y, __debug__, z = 1, 2, 3, 4

                // test_err del_debug_py39
                // # parse_options: {"target-version": "3.9"}
                // del __debug__

                // test_ok del_debug_py38
                // # parse_options: {"target-version": "3.8"}
                // del __debug__

                // test_ok read_from_debug
                // if __debug__: ...
                // x = __debug__
                if id == "__debug__" {
                    match expr_ctx {
                        ExprContext::Store => Self::add_error(
                            ctx,
                            SemanticSyntaxErrorKind::WriteToDebug(WriteToDebugKind::Store),
                            *range,
                        ),
                        ExprContext::Del => {
                            let version = ctx.python_version();
                            if version >= PythonVersion::PY39 {
                                Self::add_error(
                                    ctx,
                                    SemanticSyntaxErrorKind::WriteToDebug(
                                        WriteToDebugKind::Delete(version),
                                    ),
                                    *range,
                                );
                            }
                        }
                        _ => {}
                    };
                }

                // PLE0118
                if let Some(stmt) = ctx.global(id) {
                    let start = stmt.start();
                    if expr.start() < start {
                        Self::add_error(
                            ctx,
                            SemanticSyntaxErrorKind::LoadBeforeGlobalDeclaration {
                                name: id.to_string(),
                                start,
                            },
                            expr.range(),
                        );
                    }
                }
            }
            Expr::Yield(ast::ExprYield {
                value: Some(value), ..
            }) => {
                // test_err single_star_yield
                // def f(): yield *x
                Self::invalid_star_expression(value, ctx);
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
        match &self.kind {
            SemanticSyntaxErrorKind::LateFutureImport => {
                f.write_str("__future__ imports must be at the top of the file")
            }
            SemanticSyntaxErrorKind::ReboundComprehensionVariable => {
                f.write_str("assignment expression cannot rebind comprehension variable")
            }
            SemanticSyntaxErrorKind::DuplicateTypeParameter => {
                f.write_str("duplicate type parameter")
            }
            SemanticSyntaxErrorKind::MultipleCaseAssignment(name) => {
                write!(f, "multiple assignments to name `{name}` in pattern")
            }
            SemanticSyntaxErrorKind::IrrefutableCasePattern(kind) => match kind {
                // These error messages are taken from CPython's syntax errors
                IrrefutablePatternKind::Name(name) => {
                    write!(
                        f,
                        "name capture `{name}` makes remaining patterns unreachable"
                    )
                }
                IrrefutablePatternKind::Wildcard => {
                    f.write_str("wildcard makes remaining patterns unreachable")
                }
            },
            SemanticSyntaxErrorKind::SingleStarredAssignment => {
                f.write_str("starred assignment target must be in a list or tuple")
            }
            SemanticSyntaxErrorKind::WriteToDebug(kind) => match kind {
                WriteToDebugKind::Store => f.write_str("cannot assign to `__debug__`"),
                WriteToDebugKind::Delete(python_version) => {
                    write!(f, "cannot delete `__debug__` on Python {python_version} (syntax was removed in 3.9)")
                }
            },
            SemanticSyntaxErrorKind::LoadBeforeGlobalDeclaration { name, start: _ } => {
                write!(f, "name `{name}` is used prior to global declaration")
            }
            SemanticSyntaxErrorKind::InvalidStarExpression => {
                f.write_str("can't use starred expression here")
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
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

    /// Represents a duplicate binding in a `case` pattern of a `match` statement.
    ///
    /// ## Examples
    ///
    /// ```python
    /// match x:
    ///     case [x, y, x]: ...
    ///     case x as x: ...
    ///     case Class(x=1, x=2): ...
    /// ```
    MultipleCaseAssignment(ast::name::Name),

    /// Represents an irrefutable `case` pattern before the last `case` in a `match` statement.
    ///
    /// According to the [Python reference], "a match statement may have at most one irrefutable
    /// case block, and it must be last."
    ///
    /// ## Examples
    ///
    /// ```python
    /// match x:
    ///     case value: ...  # irrefutable capture pattern
    ///     case other: ...
    ///
    /// match x:
    ///     case _: ...      # irrefutable wildcard pattern
    ///     case other: ...
    /// ```
    ///
    /// [Python reference]: https://docs.python.org/3/reference/compound_stmts.html#irrefutable-case-blocks
    IrrefutableCasePattern(IrrefutablePatternKind),

    /// Represents a single starred assignment target outside of a tuple or list.
    ///
    /// ## Examples
    ///
    /// ```python
    /// *a = (1,)  # SyntaxError
    /// ```
    ///
    /// A starred assignment target can only occur within a tuple or list:
    ///
    /// ```python
    /// b, *a = 1, 2, 3
    /// (*a,) = 1, 2, 3
    /// [*a] = 1, 2, 3
    /// ```
    SingleStarredAssignment,

    /// Represents a write to `__debug__`. This includes simple assignments and deletions as well
    /// other kinds of statements that can introduce bindings, such as type parameters in functions,
    /// classes, and aliases, `match` arms, and imports, among others.
    ///
    /// ## Examples
    ///
    /// ```python
    /// del __debug__
    /// __debug__ = False
    /// def f(__debug__): ...
    /// class C[__debug__]: ...
    /// ```
    ///
    /// See [BPO 45000] for more information.
    ///
    /// [BPO 45000]: https://github.com/python/cpython/issues/89163
    WriteToDebug(WriteToDebugKind),

    /// Represents the use of a `global` variable before its `global` declaration.
    ///
    /// ## Examples
    ///
    /// ```python
    /// counter = 1
    /// def increment():
    ///     print(f"Adding 1 to {counter}")
    ///     global counter
    ///     counter += 1
    /// ```
    LoadBeforeGlobalDeclaration { name: String, start: TextSize },

    /// Represents the use of a starred expression in an invalid location, such as a `return` or
    /// `yield` statement.
    ///
    /// ## Examples
    ///
    /// ```python
    /// def f(): return *x
    /// def f(): yield *x
    /// for _ in *x: ...
    /// for *x in xs: ...
    /// ```
    InvalidStarExpression,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum WriteToDebugKind {
    Store,
    Delete(PythonVersion),
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

struct MultipleCaseAssignmentVisitor<'a, Ctx> {
    names: FxHashSet<&'a ast::name::Name>,
    ctx: &'a Ctx,
}

impl<'a, Ctx: SemanticSyntaxContext> MultipleCaseAssignmentVisitor<'a, Ctx> {
    fn visit_pattern(&mut self, pattern: &'a Pattern) {
        // test_err multiple_assignment_in_case_pattern
        // match 2:
        //     case [y, z, y]: ...  # MatchSequence
        //     case [y, z, *y]: ...  # MatchSequence
        //     case [y, y, y]: ...  # MatchSequence multiple
        //     case {1: x, 2: x}: ...  # MatchMapping duplicate pattern
        //     case {1: x, **x}: ...  # MatchMapping duplicate in **rest
        //     case Class(x, x): ...  # MatchClass positional
        //     case Class(x=1, x=2): ...  # MatchClass keyword
        //     case [x] | {1: x} | Class(x=1, x=2): ...  # MatchOr
        //     case x as x: ...  # MatchAs
        match pattern {
            Pattern::MatchValue(_) | Pattern::MatchSingleton(_) => {}
            Pattern::MatchStar(ast::PatternMatchStar { name, .. }) => {
                if let Some(name) = name {
                    self.insert(name);
                }
            }
            Pattern::MatchSequence(ast::PatternMatchSequence { patterns, .. }) => {
                for pattern in patterns {
                    self.visit_pattern(pattern);
                }
            }
            Pattern::MatchMapping(ast::PatternMatchMapping { patterns, rest, .. }) => {
                for pattern in patterns {
                    self.visit_pattern(pattern);
                }
                if let Some(rest) = rest {
                    self.insert(rest);
                }
            }
            Pattern::MatchClass(ast::PatternMatchClass { arguments, .. }) => {
                for pattern in &arguments.patterns {
                    self.visit_pattern(pattern);
                }
                for keyword in &arguments.keywords {
                    self.insert(&keyword.attr);
                    self.visit_pattern(&keyword.pattern);
                }
            }
            Pattern::MatchAs(ast::PatternMatchAs { pattern, name, .. }) => {
                if let Some(pattern) = pattern {
                    self.visit_pattern(pattern);
                }
                if let Some(name) = name {
                    self.insert(name);
                }
            }
            Pattern::MatchOr(ast::PatternMatchOr { patterns, .. }) => {
                // each of these patterns should be visited separately because patterns can only be
                // duplicated within a single arm of the or pattern. For example, the case below is
                // a valid pattern.

                // test_ok multiple_assignment_in_case_pattern
                // match 2:
                //     case Class(x) | [x] | x: ...
                for pattern in patterns {
                    let mut visitor = Self {
                        names: FxHashSet::default(),
                        ctx: self.ctx,
                    };
                    visitor.visit_pattern(pattern);
                }
            }
        }
    }

    /// Add an identifier to the set of visited names in `self` and emit a [`SemanticSyntaxError`]
    /// if `ident` has already been seen.
    fn insert(&mut self, ident: &'a ast::Identifier) {
        if !self.names.insert(&ident.id) {
            SemanticSyntaxChecker::add_error(
                self.ctx,
                SemanticSyntaxErrorKind::MultipleCaseAssignment(ident.id.clone()),
                ident.range(),
            );
        }
        // test_err debug_shadow_match
        // match x:
        //     case __debug__: ...
        SemanticSyntaxChecker::check_identifier(ident, self.ctx);
    }
}

pub trait SemanticSyntaxContext {
    /// Returns `true` if a module's docstring boundary has been passed.
    fn seen_docstring_boundary(&self) -> bool;

    /// The target Python version for detecting backwards-incompatible syntax changes.
    fn python_version(&self) -> PythonVersion;

    /// Return the [`TextRange`] at which a name is declared as `global` in the current scope.
    fn global(&self, name: &str) -> Option<TextRange>;

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
