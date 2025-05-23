//! [`SemanticSyntaxChecker`] for AST-based syntax errors.
//!
//! This checker is not responsible for traversing the AST itself. Instead, its
//! [`SemanticSyntaxChecker::visit_stmt`] and [`SemanticSyntaxChecker::visit_expr`] methods should
//! be called in a parent `Visitor`'s `visit_stmt` and `visit_expr` methods, respectively.
use std::fmt::Display;

use ruff_python_ast::{
    self as ast, Expr, ExprContext, IrrefutablePatternKind, Pattern, PythonVersion, Stmt, StmtExpr,
    StmtImportFrom,
    comparable::ComparableExpr,
    visitor::{Visitor, walk_expr},
};
use ruff_text_size::{Ranged, TextRange, TextSize};
use rustc_hash::{FxBuildHasher, FxHashSet};

#[derive(Debug, Default)]
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

    /// The checker has traversed past the module docstring boundary (i.e. seen any statement in the
    /// module).
    seen_module_docstring_boundary: bool,
}

impl SemanticSyntaxChecker {
    pub fn new() -> Self {
        Self::default()
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
                for case in &match_stmt.cases {
                    let mut visitor = MatchPatternVisitor {
                        names: FxHashSet::default(),
                        ctx,
                    };
                    visitor.visit_pattern(&case.pattern);
                }
            }
            Stmt::FunctionDef(ast::StmtFunctionDef {
                type_params,
                parameters,
                ..
            }) => {
                if let Some(type_params) = type_params {
                    Self::duplicate_type_parameter_name(type_params, ctx);
                }
                Self::duplicate_parameter_name(parameters, ctx);
            }
            Stmt::ClassDef(ast::StmtClassDef { type_params, .. })
            | Stmt::TypeAlias(ast::StmtTypeAlias { type_params, .. }) => {
                if let Some(type_params) = type_params {
                    Self::duplicate_type_parameter_name(type_params, ctx);
                }
            }
            Stmt::Assign(ast::StmtAssign { targets, value, .. }) => {
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

                // test_ok assign_stmt_starred_expr_value
                // _ = 4
                // _ = [4]
                // _ = (*[1],)
                // _ = *[1],

                // test_err assign_stmt_starred_expr_value
                // _ = *[42]
                // _ = *{42}
                // _ = *list()
                // _ = *(p + q)
                Self::invalid_star_expression(value, ctx);
            }
            Stmt::Return(ast::StmtReturn { value, range }) => {
                if let Some(value) = value {
                    // test_err single_star_return
                    // def f(): return *x
                    Self::invalid_star_expression(value, ctx);
                }
                if !ctx.in_function_scope() {
                    Self::add_error(ctx, SemanticSyntaxErrorKind::ReturnOutsideFunction, *range);
                }
            }
            Stmt::For(ast::StmtFor {
                target,
                iter,
                is_async,
                ..
            }) => {
                // test_err single_star_for
                // for _ in *x: ...
                // for *x in xs: ...
                Self::invalid_star_expression(target, ctx);
                Self::invalid_star_expression(iter, ctx);
                if *is_async {
                    Self::await_outside_async_function(
                        ctx,
                        stmt,
                        AwaitOutsideAsyncFunctionKind::AsyncFor,
                    );
                }
            }
            Stmt::With(ast::StmtWith { is_async: true, .. }) => {
                Self::await_outside_async_function(
                    ctx,
                    stmt,
                    AwaitOutsideAsyncFunctionKind::AsyncWith,
                );
            }
            Stmt::Nonlocal(ast::StmtNonlocal { range, .. }) => {
                // test_ok nonlocal_declaration_at_module_level
                // def _():
                //     nonlocal x

                // test_err nonlocal_declaration_at_module_level
                // nonlocal x
                // nonlocal x, y
                if ctx.in_module_scope() {
                    Self::add_error(
                        ctx,
                        SemanticSyntaxErrorKind::NonlocalDeclarationAtModuleLevel,
                        *range,
                    );
                }
            }
            _ => {}
        }

        Self::debug_shadowing(stmt, ctx);
        Self::check_annotation(stmt, ctx);
    }

    fn check_annotation<Ctx: SemanticSyntaxContext>(stmt: &ast::Stmt, ctx: &Ctx) {
        match stmt {
            Stmt::AnnAssign(ast::StmtAnnAssign { annotation, .. }) => {
                if ctx.python_version() > PythonVersion::PY313 {
                    // test_ok valid_annotation_py313
                    // # parse_options: {"target-version": "3.13"}
                    // a: (x := 1)
                    // def outer():
                    //     b: (yield 1)
                    //     c: (yield from 1)
                    // async def outer():
                    //     d: (await 1)

                    // test_err invalid_annotation_py314
                    // # parse_options: {"target-version": "3.14"}
                    // a: (x := 1)
                    // def outer():
                    //     b: (yield 1)
                    //     c: (yield from 1)
                    // async def outer():
                    //     d: (await 1)
                    let mut visitor = InvalidExpressionVisitor {
                        position: InvalidExpressionPosition::TypeAnnotation,
                        ctx,
                    };
                    visitor.visit_expr(annotation);
                }
            }
            Stmt::FunctionDef(ast::StmtFunctionDef {
                type_params,
                parameters,
                returns,
                ..
            }) => {
                // test_ok valid_annotation_function_py313
                // # parse_options: {"target-version": "3.13"}
                // def f() -> (y := 3): ...
                // def g(arg: (x := 1)): ...
                // def outer():
                //     def i(x: (yield 1)): ...
                //     def k() -> (yield 1): ...
                //     def m(x: (yield from 1)): ...
                //     def o() -> (yield from 1): ...
                // async def outer():
                //     def f() -> (await 1): ...
                //     def g(arg: (await 1)): ...

                // test_err invalid_annotation_function_py314
                // # parse_options: {"target-version": "3.14"}
                // def f() -> (y := 3): ...
                // def g(arg: (x := 1)): ...
                // def outer():
                //     def i(x: (yield 1)): ...
                //     def k() -> (yield 1): ...
                //     def m(x: (yield from 1)): ...
                //     def o() -> (yield from 1): ...
                // async def outer():
                //     def f() -> (await 1): ...
                //     def g(arg: (await 1)): ...

                // test_err invalid_annotation_function
                // def d[T]() -> (await 1): ...
                // def e[T](arg: (await 1)): ...
                // def f[T]() -> (y := 3): ...
                // def g[T](arg: (x := 1)): ...
                // def h[T](x: (yield 1)): ...
                // def j[T]() -> (yield 1): ...
                // def l[T](x: (yield from 1)): ...
                // def n[T]() -> (yield from 1): ...
                // def p[T: (yield 1)](): ...      # yield in TypeVar bound
                // def q[T = (yield 1)](): ...     # yield in TypeVar default
                // def r[*Ts = (yield 1)](): ...   # yield in TypeVarTuple default
                // def s[**Ts = (yield 1)](): ...  # yield in ParamSpec default
                // def t[T: (x := 1)](): ...       # named expr in TypeVar bound
                // def u[T = (x := 1)](): ...      # named expr in TypeVar default
                // def v[*Ts = (x := 1)](): ...    # named expr in TypeVarTuple default
                // def w[**Ts = (x := 1)](): ...   # named expr in ParamSpec default
                // def t[T: (await 1)](): ...       # await in TypeVar bound
                // def u[T = (await 1)](): ...      # await in TypeVar default
                // def v[*Ts = (await 1)](): ...    # await in TypeVarTuple default
                // def w[**Ts = (await 1)](): ...   # await in ParamSpec default
                let mut visitor = InvalidExpressionVisitor {
                    position: InvalidExpressionPosition::TypeAnnotation,
                    ctx,
                };
                if let Some(type_params) = type_params {
                    visitor.visit_type_params(type_params);
                }
                // the __future__ annotation error takes precedence over the generic error
                if ctx.future_annotations_or_stub() || ctx.python_version() > PythonVersion::PY313 {
                    visitor.position = InvalidExpressionPosition::TypeAnnotation;
                } else if type_params.is_some() {
                    visitor.position = InvalidExpressionPosition::GenericDefinition;
                } else {
                    return;
                }
                for param in parameters
                    .iter()
                    .filter_map(ast::AnyParameterRef::annotation)
                {
                    visitor.visit_expr(param);
                }
                if let Some(returns) = returns {
                    visitor.visit_expr(returns);
                }
            }
            Stmt::ClassDef(ast::StmtClassDef {
                type_params: Some(type_params),
                arguments,
                ..
            }) => {
                // test_ok valid_annotation_class
                // class F(y := list): ...
                // def f():
                //     class G((yield 1)): ...
                //     class H((yield from 1)): ...
                // async def f():
                //     class G((await 1)): ...

                // test_err invalid_annotation_class
                // class F[T](y := list): ...
                // class I[T]((yield 1)): ...
                // class J[T]((yield from 1)): ...
                // class K[T: (yield 1)]: ...      # yield in TypeVar
                // class L[T: (x := 1)]: ...       # named expr in TypeVar
                // class M[T]((await 1)): ...
                // class N[T: (await 1)]: ...
                let mut visitor = InvalidExpressionVisitor {
                    position: InvalidExpressionPosition::TypeAnnotation,
                    ctx,
                };
                visitor.visit_type_params(type_params);
                if let Some(arguments) = arguments {
                    visitor.position = InvalidExpressionPosition::GenericDefinition;
                    visitor.visit_arguments(arguments);
                }
            }
            Stmt::TypeAlias(ast::StmtTypeAlias {
                type_params, value, ..
            }) => {
                // test_err invalid_annotation_type_alias
                // type X[T: (yield 1)] = int      # TypeVar bound
                // type X[T = (yield 1)] = int     # TypeVar default
                // type X[*Ts = (yield 1)] = int   # TypeVarTuple default
                // type X[**Ts = (yield 1)] = int  # ParamSpec default
                // type Y = (yield 1)              # yield in value
                // type Y = (x := 1)               # named expr in value
                // type Y[T: (await 1)] = int      # await in bound
                // type Y = (await 1)              # await in value
                let mut visitor = InvalidExpressionVisitor {
                    position: InvalidExpressionPosition::TypeAlias,
                    ctx,
                };
                visitor.visit_expr(value);
                if let Some(type_params) = type_params {
                    visitor.visit_type_params(type_params);
                }
            }
            _ => {}
        }
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

    fn duplicate_parameter_name<Ctx: SemanticSyntaxContext>(
        parameters: &ast::Parameters,
        ctx: &Ctx,
    ) {
        if parameters.len() < 2 {
            return;
        }

        let mut all_arg_names =
            FxHashSet::with_capacity_and_hasher(parameters.len(), FxBuildHasher);

        for parameter in parameters {
            let range = parameter.name().range();
            let param_name = parameter.name().as_str();
            if !all_arg_names.insert(param_name) {
                // test_err params_duplicate_names
                // def foo(a, a=10, *a, a, a: str, **a): ...
                Self::add_error(
                    ctx,
                    SemanticSyntaxErrorKind::DuplicateParameter(param_name.to_string()),
                    range,
                );
            }
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

    /// Check `stmt` for semantic syntax errors and update the checker's internal state.
    ///
    /// Note that this method should only be called when traversing `stmt` *and* its children. For
    /// example, if traversal of function bodies needs to be deferred, avoid calling `visit_stmt` on
    /// the function itself until the deferred body is visited too. Failing to defer `visit_stmt` in
    /// this case will break any internal state that depends on function scopes, such as `async`
    /// context detection.
    pub fn visit_stmt<Ctx: SemanticSyntaxContext>(&mut self, stmt: &ast::Stmt, ctx: &Ctx) {
        // check for errors
        self.check_stmt(stmt, ctx);

        // update internal state
        match stmt {
            Stmt::Expr(StmtExpr { value, .. })
                if !self.seen_module_docstring_boundary && value.is_string_literal_expr() => {}
            Stmt::ImportFrom(StmtImportFrom { module, .. }) => {
                // Allow __future__ imports until we see a non-__future__ import.
                if !matches!(module.as_deref(), Some("__future__")) {
                    self.seen_futures_boundary = true;
                }
            }
            Stmt::FunctionDef(_) => {
                self.seen_futures_boundary = true;
            }
            _ => {
                self.seen_futures_boundary = true;
            }
        }

        self.seen_module_docstring_boundary = true;
    }

    /// Check `expr` for semantic syntax errors and update the checker's internal state.
    pub fn visit_expr<Ctx: SemanticSyntaxContext>(&mut self, expr: &Expr, ctx: &Ctx) {
        match expr {
            Expr::ListComp(ast::ExprListComp {
                elt, generators, ..
            })
            | Expr::SetComp(ast::ExprSetComp {
                elt, generators, ..
            }) => {
                Self::check_generator_expr(elt, generators, ctx);
                Self::async_comprehension_in_sync_comprehension(ctx, generators);
                for generator in generators.iter().filter(|g| g.is_async) {
                    Self::await_outside_async_function(
                        ctx,
                        generator,
                        AwaitOutsideAsyncFunctionKind::AsyncComprehension,
                    );
                }
            }
            Expr::DictComp(ast::ExprDictComp {
                key,
                value,
                generators,
                ..
            }) => {
                Self::check_generator_expr(key, generators, ctx);
                Self::check_generator_expr(value, generators, ctx);
                Self::async_comprehension_in_sync_comprehension(ctx, generators);
                for generator in generators.iter().filter(|g| g.is_async) {
                    Self::await_outside_async_function(
                        ctx,
                        generator,
                        AwaitOutsideAsyncFunctionKind::AsyncComprehension,
                    );
                }
            }
            Expr::Generator(ast::ExprGenerator {
                elt, generators, ..
            }) => {
                Self::check_generator_expr(elt, generators, ctx);
                // Note that `await_outside_async_function` is not called here because generators
                // are evaluated lazily. See the note in the function for more details.
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
                    }
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
            Expr::Yield(ast::ExprYield { value, .. }) => {
                if let Some(value) = value {
                    // test_err single_star_yield
                    // def f(): yield *x
                    Self::invalid_star_expression(value, ctx);
                }
                Self::yield_outside_function(ctx, expr, YieldOutsideFunctionKind::Yield);
            }
            Expr::YieldFrom(_) => {
                Self::yield_outside_function(ctx, expr, YieldOutsideFunctionKind::YieldFrom);
            }
            Expr::Await(_) => {
                Self::yield_outside_function(ctx, expr, YieldOutsideFunctionKind::Await);
                Self::await_outside_async_function(ctx, expr, AwaitOutsideAsyncFunctionKind::Await);
            }
            Expr::Lambda(ast::ExprLambda {
                parameters: Some(parameters),
                ..
            }) => {
                Self::duplicate_parameter_name(parameters, ctx);
            }
            _ => {}
        }
    }

    /// PLE1142
    fn await_outside_async_function<Ctx: SemanticSyntaxContext, Node: Ranged>(
        ctx: &Ctx,
        node: Node,
        kind: AwaitOutsideAsyncFunctionKind,
    ) {
        if ctx.in_async_context() {
            return;
        }
        // `await` is allowed at the top level of a Jupyter notebook.
        // See: https://ipython.readthedocs.io/en/stable/interactive/autoawait.html.
        if ctx.in_module_scope() && ctx.in_notebook() {
            return;
        }
        // Generators are evaluated lazily, so you can use `await` in them. For example:
        //
        // ```python
        // # This is valid
        // def f():
        //     (await x for x in y)
        //     (x async for x in y)
        //
        // # This is invalid
        // def f():
        //     (x for x in await y)
        //     [await x for x in y]
        // ```
        //
        // This check is required in addition to avoiding calling this function in `visit_expr`
        // because the generator scope applies to nested parts of the `Expr::Generator` that are
        // visited separately.
        if ctx.in_generator_scope() {
            return;
        }
        Self::add_error(
            ctx,
            SemanticSyntaxErrorKind::AwaitOutsideAsyncFunction(kind),
            node.range(),
        );
    }

    /// F704
    fn yield_outside_function<Ctx: SemanticSyntaxContext>(
        ctx: &Ctx,
        expr: &Expr,
        kind: YieldOutsideFunctionKind,
    ) {
        // We are intentionally not inspecting the async status of the scope for now to mimic F704.
        // await-outside-async is PLE1142 instead, so we'll end up emitting both syntax errors for
        // cases that trigger F704

        if ctx.in_function_scope() {
            return;
        }

        if kind.is_await() {
            // `await` is allowed at the top level of a Jupyter notebook.
            // See: https://ipython.readthedocs.io/en/stable/interactive/autoawait.html.
            if ctx.in_module_scope() && ctx.in_notebook() {
                return;
            }
            if ctx.in_await_allowed_context() {
                return;
            }
        } else if ctx.in_yield_allowed_context() {
            return;
        }

        Self::add_error(
            ctx,
            SemanticSyntaxErrorKind::YieldOutsideFunction(kind),
            expr.range(),
        );
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

    fn async_comprehension_in_sync_comprehension<Ctx: SemanticSyntaxContext>(
        ctx: &Ctx,
        generators: &[ast::Comprehension],
    ) {
        let python_version = ctx.python_version();
        if python_version >= PythonVersion::PY311 {
            return;
        }
        // async allowed at notebook top-level
        if ctx.in_notebook() && ctx.in_module_scope() {
            return;
        }
        if !ctx.in_sync_comprehension() {
            return;
        }
        for generator in generators.iter().filter(|generator| generator.is_async) {
            // test_ok nested_async_comprehension_py311
            // # parse_options: {"target-version": "3.11"}
            // async def f(): return [[x async for x in foo(n)] for n in range(3)]    # list
            // async def g(): return [{x: 1 async for x in foo(n)} for n in range(3)] # dict
            // async def h(): return [{x async for x in foo(n)} for n in range(3)]    # set

            // test_ok nested_async_comprehension_py310
            // # parse_options: {"target-version": "3.10"}
            // async def f():
            //     [_ for n in range(3)]
            //     [_ async for n in range(3)]
            // async def f():
            //     def g(): ...
            //     [_ async for n in range(3)]

            // test_ok all_async_comprehension_py310
            // # parse_options: {"target-version": "3.10"}
            // async def test(): return [[x async for x in elements(n)] async for n in range(3)]

            // test_err nested_async_comprehension_py310
            // # parse_options: {"target-version": "3.10"}
            // async def f(): return [[x async for x in foo(n)] for n in range(3)]    # list
            // async def g(): return [{x: 1 async for x in foo(n)} for n in range(3)] # dict
            // async def h(): return [{x async for x in foo(n)} for n in range(3)]    # set
            // async def i(): return [([y async for y in range(1)], [z for z in range(2)]) for x in range(5)]
            // async def j(): return [([y for y in range(1)], [z async for z in range(2)]) for x in range(5)]
            Self::add_error(
                ctx,
                SemanticSyntaxErrorKind::AsyncComprehensionInSyncComprehension(python_version),
                generator.range,
            );
        }
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
                    write!(
                        f,
                        "cannot delete `__debug__` on Python {python_version} (syntax was removed in 3.9)"
                    )
                }
            },
            SemanticSyntaxErrorKind::InvalidExpression(kind, position) => {
                write!(f, "{kind} cannot be used within a {position}")
            }
            SemanticSyntaxErrorKind::DuplicateMatchKey(key) => {
                write!(
                    f,
                    "mapping pattern checks duplicate key `{}`",
                    EscapeDefault(key)
                )
            }
            SemanticSyntaxErrorKind::DuplicateMatchClassAttribute(name) => {
                write!(f, "attribute name `{name}` repeated in class pattern",)
            }
            SemanticSyntaxErrorKind::LoadBeforeGlobalDeclaration { name, start: _ } => {
                write!(f, "name `{name}` is used prior to global declaration")
            }
            SemanticSyntaxErrorKind::InvalidStarExpression => {
                f.write_str("Starred expression cannot be used here")
            }
            SemanticSyntaxErrorKind::AsyncComprehensionInSyncComprehension(python_version) => {
                write!(
                    f,
                    "cannot use an asynchronous comprehension inside of a synchronous comprehension \
                        on Python {python_version} (syntax was added in 3.11)",
                )
            }
            SemanticSyntaxErrorKind::YieldOutsideFunction(kind) => {
                write!(f, "`{kind}` statement outside of a function")
            }
            SemanticSyntaxErrorKind::ReturnOutsideFunction => {
                f.write_str("`return` statement outside of a function")
            }
            SemanticSyntaxErrorKind::AwaitOutsideAsyncFunction(kind) => {
                write!(f, "{kind} outside of an asynchronous function")
            }
            SemanticSyntaxErrorKind::DuplicateParameter(name) => {
                write!(f, r#"Duplicate parameter "{name}""#)
            }
            SemanticSyntaxErrorKind::NonlocalDeclarationAtModuleLevel => {
                write!(f, "nonlocal declaration not allowed at module level")
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

    /// Represents the use of an invalid expression kind in one of several locations.
    ///
    /// The kinds include `yield` and `yield from` expressions and named expressions, and locations
    /// include type parameter bounds and defaults, type annotations, type aliases, and base class
    /// lists.
    ///
    /// ## Examples
    ///
    /// ```python
    /// type X[T: (yield 1)] = int
    /// type Y = (yield 1)
    /// def f[T](x: int) -> (y := 3): return x
    /// ```
    InvalidExpression(InvalidExpressionKind, InvalidExpressionPosition),

    /// Represents a duplicate key in a `match` mapping pattern.
    ///
    /// The [CPython grammar] allows keys in mapping patterns to be literals or attribute accesses:
    ///
    /// ```text
    /// key_value_pattern:
    ///     | (literal_expr | attr) ':' pattern
    /// ```
    ///
    /// But only literals are checked for duplicates:
    ///
    /// ```pycon
    /// >>> match x:
    /// ...     case {"x": 1, "x": 2}: ...
    /// ...
    ///   File "<python-input-160>", line 2
    ///     case {"x": 1, "x": 2}: ...
    ///          ^^^^^^^^^^^^^^^^
    /// SyntaxError: mapping pattern checks duplicate key ('x')
    /// >>> match x:
    /// ...     case {x.a: 1, x.a: 2}: ...
    /// ...
    /// >>>
    /// ```
    ///
    /// ## Examples
    ///
    /// ```python
    /// match x:
    ///     case {"x": 1, "x": 2}: ...
    /// ```
    ///
    /// [CPython grammar]: https://docs.python.org/3/reference/grammar.html
    DuplicateMatchKey(String),

    /// Represents a duplicate attribute name in a `match` class pattern.
    ///
    /// ## Examples
    ///
    /// ```python
    /// match x:
    ///     case Class(x=1, x=2): ...
    /// ```
    DuplicateMatchClassAttribute(ast::name::Name),

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
    ///
    /// ## Known Issues
    ///
    /// Note that the order in which the parts of a `try` statement are visited was changed in 3.13,
    /// as tracked in Python issue [#111123]. For example, this code was valid on Python 3.12:
    ///
    /// ```python
    /// a = 10
    /// def g():
    ///     try:
    ///         1 / 0
    ///     except:
    ///         a = 1
    ///     else:
    ///         global a
    /// ```
    ///
    /// While this more intuitive behavior aligned with the textual order was a syntax error:
    ///
    /// ```python
    /// a = 10
    /// def f():
    ///     try:
    ///         pass
    ///     except:
    ///         global a
    ///     else:
    ///         a = 1  # SyntaxError: name 'a' is assigned to before global declaration
    /// ```
    ///
    /// This was reversed in version 3.13 to make the second case valid and the first case a syntax
    /// error. We intentionally enforce the 3.13 ordering, regardless of the Python version, which
    /// will lead to both false positives and false negatives on 3.12 code that takes advantage of
    /// the old behavior. However, as mentioned in the Python issue, we expect code relying on this
    /// to be very rare and not worth the additional complexity to detect.
    ///
    /// [#111123]: https://github.com/python/cpython/issues/111123
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

    /// Represents the use of an asynchronous comprehension inside of a synchronous comprehension
    /// before Python 3.11.
    ///
    /// ## Examples
    ///
    /// Before Python 3.11, code like this produces a syntax error because of the implicit function
    /// scope introduced by the outer comprehension:
    ///
    /// ```python
    /// async def elements(n): yield n
    ///
    /// async def test(): return { n: [x async for x in elements(n)] for n in range(3)}
    /// ```
    ///
    /// This was discussed in [BPO 33346] and fixed in Python 3.11.
    ///
    /// [BPO 33346]: https://github.com/python/cpython/issues/77527
    AsyncComprehensionInSyncComprehension(PythonVersion),

    /// Represents the use of `yield`, `yield from`, or `await` outside of a function scope.
    ///
    ///
    /// ## Examples
    ///
    /// `yield` and `yield from` are only allowed if the immediately-enclosing scope is a function
    /// or lambda and not allowed otherwise:
    ///
    /// ```python
    /// yield 1  # error
    ///
    /// def f():
    ///     [(yield 1) for x in y]  # error
    /// ```
    ///
    /// `await` is additionally allowed in comprehensions, if the comprehension itself is in a
    /// function scope:
    ///
    /// ```python
    /// await 1  # error
    ///
    /// async def f():
    ///     await 1  # okay
    ///     [await 1 for x in y]  # also okay
    /// ```
    ///
    /// This last case _is_ an error, but it has to do with the lambda not being an async function.
    /// For the sake of this error kind, this is okay.
    ///
    /// ## References
    ///
    /// See [PEP 255] for details on `yield`, [PEP 380] for the extension to `yield from`, [PEP 492]
    /// for async-await syntax, and [PEP 530] for async comprehensions.
    ///
    /// [PEP 255]: https://peps.python.org/pep-0255/
    /// [PEP 380]: https://peps.python.org/pep-0380/
    /// [PEP 492]: https://peps.python.org/pep-0492/
    /// [PEP 530]: https://peps.python.org/pep-0530/
    YieldOutsideFunction(YieldOutsideFunctionKind),

    /// Represents the use of `return` outside of a function scope.
    ReturnOutsideFunction,

    /// Represents the use of `await`, `async for`, or `async with` outside of an asynchronous
    /// function.
    ///
    /// ## Examples
    ///
    /// ```python
    /// def f():
    ///     await 1                # error
    ///     async for x in y: ...  # error
    ///     async with x: ...      # error
    /// ```
    AwaitOutsideAsyncFunction(AwaitOutsideAsyncFunctionKind),

    /// Represents a duplicate parameter name in a function or lambda expression.
    ///
    /// ## Examples
    ///
    /// ```python
    /// def f(x, x): ...
    /// lambda x, x: ...
    /// ```
    DuplicateParameter(String),

    /// Represents a nonlocal declaration at module level
    NonlocalDeclarationAtModuleLevel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AwaitOutsideAsyncFunctionKind {
    Await,
    AsyncFor,
    AsyncWith,
    AsyncComprehension,
}

impl Display for AwaitOutsideAsyncFunctionKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            AwaitOutsideAsyncFunctionKind::Await => "`await`",
            AwaitOutsideAsyncFunctionKind::AsyncFor => "`async for`",
            AwaitOutsideAsyncFunctionKind::AsyncWith => "`async with`",
            AwaitOutsideAsyncFunctionKind::AsyncComprehension => "asynchronous comprehension",
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum YieldOutsideFunctionKind {
    Yield,
    YieldFrom,
    Await,
}

impl YieldOutsideFunctionKind {
    pub fn is_await(&self) -> bool {
        matches!(self, Self::Await)
    }
}

impl Display for YieldOutsideFunctionKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            YieldOutsideFunctionKind::Yield => "yield",
            YieldOutsideFunctionKind::YieldFrom => "yield from",
            YieldOutsideFunctionKind::Await => "await",
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InvalidExpressionPosition {
    TypeVarBound,
    TypeVarDefault,
    TypeVarTupleDefault,
    ParamSpecDefault,
    TypeAnnotation,
    GenericDefinition,
    TypeAlias,
}

impl Display for InvalidExpressionPosition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            InvalidExpressionPosition::TypeVarBound => "TypeVar bound",
            InvalidExpressionPosition::TypeVarDefault => "TypeVar default",
            InvalidExpressionPosition::TypeVarTupleDefault => "TypeVarTuple default",
            InvalidExpressionPosition::ParamSpecDefault => "ParamSpec default",
            InvalidExpressionPosition::TypeAnnotation => "type annotation",
            InvalidExpressionPosition::GenericDefinition => "generic definition",
            InvalidExpressionPosition::TypeAlias => "type alias",
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum InvalidExpressionKind {
    Yield,
    NamedExpr,
    Await,
}

impl Display for InvalidExpressionKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            InvalidExpressionKind::Yield => "yield expression",
            InvalidExpressionKind::NamedExpr => "named expression",
            InvalidExpressionKind::Await => "await expression",
        })
    }
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
            }
        }
        walk_expr(self, expr);
    }
}

struct MatchPatternVisitor<'a, Ctx> {
    names: FxHashSet<&'a ast::name::Name>,
    ctx: &'a Ctx,
}

impl<'a, Ctx: SemanticSyntaxContext> MatchPatternVisitor<'a, Ctx> {
    fn visit_pattern(&mut self, pattern: &'a Pattern) {
        // test_ok class_keyword_in_case_pattern
        // match 2:
        //     case Class(x=x): ...

        // test_err multiple_assignment_in_case_pattern
        // match 2:
        //     case [y, z, y]: ...  # MatchSequence
        //     case [y, z, *y]: ...  # MatchSequence
        //     case [y, y, y]: ...  # MatchSequence multiple
        //     case {1: x, 2: x}: ...  # MatchMapping duplicate pattern
        //     case {1: x, **x}: ...  # MatchMapping duplicate in **rest
        //     case Class(x, x): ...  # MatchClass positional
        //     case Class(y=x, z=x): ...  # MatchClass keyword
        //     case [x] | {1: x} | Class(y=x, z=x): ...  # MatchOr
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
            Pattern::MatchMapping(ast::PatternMatchMapping {
                keys,
                patterns,
                rest,
                ..
            }) => {
                for pattern in patterns {
                    self.visit_pattern(pattern);
                }
                if let Some(rest) = rest {
                    self.insert(rest);
                }

                let mut seen = FxHashSet::default();
                for key in keys
                    .iter()
                    // complex numbers (`1 + 2j`) are allowed as keys but are not literals
                    // because they are represented as a `BinOp::Add` between a real number and
                    // an imaginary number
                    .filter(|key| key.is_literal_expr() || key.is_bin_op_expr())
                {
                    if !seen.insert(ComparableExpr::from(key)) {
                        let key_range = key.range();
                        let duplicate_key = self.ctx.source()[key_range].to_string();
                        // test_ok duplicate_match_key_attr
                        // match x:
                        //     case {x.a: 1, x.a: 2}: ...

                        // test_err duplicate_match_key
                        // match x:
                        //     case {"x": 1, "x": 2}: ...
                        //     case {b"x": 1, b"x": 2}: ...
                        //     case {0: 1, 0: 2}: ...
                        //     case {1.0: 1, 1.0: 2}: ...
                        //     case {1.0 + 2j: 1, 1.0 + 2j: 2}: ...
                        //     case {True: 1, True: 2}: ...
                        //     case {None: 1, None: 2}: ...
                        //     case {
                        //     """x
                        //     y
                        //     z
                        //     """: 1,
                        //     """x
                        //     y
                        //     z
                        //     """: 2}: ...
                        //     case {"x": 1, "x": 2, "x": 3}: ...
                        //     case {0: 1, "x": 1, 0: 2, "x": 2}: ...
                        //     case [{"x": 1, "x": 2}]: ...
                        //     case Foo(x=1, y={"x": 1, "x": 2}): ...
                        //     case [Foo(x=1), Foo(x=1, y={"x": 1, "x": 2})]: ...
                        SemanticSyntaxChecker::add_error(
                            self.ctx,
                            SemanticSyntaxErrorKind::DuplicateMatchKey(duplicate_key),
                            key_range,
                        );
                    }
                }
            }
            Pattern::MatchClass(ast::PatternMatchClass { arguments, .. }) => {
                for pattern in &arguments.patterns {
                    self.visit_pattern(pattern);
                }
                let mut seen = FxHashSet::default();
                for keyword in &arguments.keywords {
                    if !seen.insert(&keyword.attr.id) {
                        // test_err duplicate_match_class_attr
                        // match x:
                        //     case Class(x=1, x=2): ...
                        //     case [Class(x=1, x=2)]: ...
                        //     case {"x": x, "y": Foo(x=1, x=2)}: ...
                        //     case [{}, {"x": x, "y": Foo(x=1, x=2)}]: ...
                        //     case Class(x=1, d={"x": 1, "x": 2}, other=Class(x=1, x=2)): ...
                        SemanticSyntaxChecker::add_error(
                            self.ctx,
                            SemanticSyntaxErrorKind::DuplicateMatchClassAttribute(
                                keyword.attr.id.clone(),
                            ),
                            keyword.attr.range,
                        );
                    }
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

struct InvalidExpressionVisitor<'a, Ctx> {
    /// Context used for emitting errors.
    ctx: &'a Ctx,

    position: InvalidExpressionPosition,
}

impl<Ctx> Visitor<'_> for InvalidExpressionVisitor<'_, Ctx>
where
    Ctx: SemanticSyntaxContext,
{
    fn visit_expr(&mut self, expr: &Expr) {
        match expr {
            Expr::Named(ast::ExprNamed { range, .. }) => {
                SemanticSyntaxChecker::add_error(
                    self.ctx,
                    SemanticSyntaxErrorKind::InvalidExpression(
                        InvalidExpressionKind::NamedExpr,
                        self.position,
                    ),
                    *range,
                );
            }
            Expr::Yield(ast::ExprYield { range, .. })
            | Expr::YieldFrom(ast::ExprYieldFrom { range, .. }) => {
                SemanticSyntaxChecker::add_error(
                    self.ctx,
                    SemanticSyntaxErrorKind::InvalidExpression(
                        InvalidExpressionKind::Yield,
                        self.position,
                    ),
                    *range,
                );
            }
            Expr::Await(ast::ExprAwait { range, .. }) => {
                SemanticSyntaxChecker::add_error(
                    self.ctx,
                    SemanticSyntaxErrorKind::InvalidExpression(
                        InvalidExpressionKind::Await,
                        self.position,
                    ),
                    *range,
                );
            }
            _ => {}
        }
        ast::visitor::walk_expr(self, expr);
    }

    fn visit_type_param(&mut self, type_param: &ast::TypeParam) {
        match type_param {
            ast::TypeParam::TypeVar(ast::TypeParamTypeVar { bound, default, .. }) => {
                if let Some(expr) = bound {
                    self.position = InvalidExpressionPosition::TypeVarBound;
                    self.visit_expr(expr);
                }
                if let Some(expr) = default {
                    self.position = InvalidExpressionPosition::TypeVarDefault;
                    self.visit_expr(expr);
                }
            }
            ast::TypeParam::TypeVarTuple(ast::TypeParamTypeVarTuple { default, .. }) => {
                if let Some(expr) = default {
                    self.position = InvalidExpressionPosition::TypeVarTupleDefault;
                    self.visit_expr(expr);
                }
            }
            ast::TypeParam::ParamSpec(ast::TypeParamParamSpec { default, .. }) => {
                if let Some(expr) = default {
                    self.position = InvalidExpressionPosition::ParamSpecDefault;
                    self.visit_expr(expr);
                }
            }
        }
    }
}

/// Information needed from a parent visitor to emit semantic syntax errors.
///
/// Note that the `in_*_scope` methods should refer to the immediately-enclosing scope. For example,
/// `in_function_scope` should return true for this case:
///
/// ```python
/// def f():
///     x  # here
/// ```
///
/// but not for this case:
///
/// ```python
/// def f():
///     class C:
///         x  # here
/// ```
///
/// In contrast, the `in_*_context` methods should traverse parent scopes. For example,
/// `in_function_context` should return true for this case:
///
/// ```python
/// def f():
///     [x  # here
///         for x in range(3)]
/// ```
///
/// but not here:
///
/// ```python
/// def f():
///     class C:
///         x  # here, classes break function scopes
/// ```
pub trait SemanticSyntaxContext {
    /// Returns `true` if `__future__`-style type annotations are enabled.
    fn future_annotations_or_stub(&self) -> bool;

    /// The target Python version for detecting backwards-incompatible syntax changes.
    fn python_version(&self) -> PythonVersion;

    /// Returns the source text under analysis.
    fn source(&self) -> &str;

    /// Return the [`TextRange`] at which a name is declared as `global` in the current scope.
    fn global(&self, name: &str) -> Option<TextRange>;

    /// Returns `true` if the visitor is currently in an async context, i.e. an async function.
    fn in_async_context(&self) -> bool;

    /// Returns `true` if the visitor is currently in a context where the `await` keyword is
    /// allowed.
    ///
    /// Note that this is method is primarily used to report `YieldOutsideFunction` errors for
    /// `await` outside function scopes, irrespective of their async status. As such, this differs
    /// from `in_async_context` in two ways:
    ///
    /// 1. `await` is allowed in a lambda, despite it not being async
    /// 2. `await` is allowed in any function, regardless of its async status
    ///
    /// In short, only nested class definitions should cause this method to return `false`, for
    /// example:
    ///
    /// ```python
    /// def f():
    ///     await 1  # okay, in a function
    ///     class C:
    ///         await 1  # error
    /// ```
    ///
    /// See the trait-level documentation for more details.
    fn in_await_allowed_context(&self) -> bool;

    /// Returns `true` if the visitor is currently in a context where `yield` and `yield from`
    /// expressions are allowed.
    ///
    /// Yield expressions are allowed only in:
    /// 1. Function definitions
    /// 2. Lambda expressions
    ///
    /// Unlike `await`, yield is not allowed in:
    /// - Comprehensions (list, set, dict)
    /// - Generator expressions
    /// - Class definitions
    ///
    /// This method should traverse parent scopes to check if the closest relevant scope
    /// is a function or lambda, and that no disallowed context (class, comprehension, generator)
    /// intervenes. For example:
    ///
    /// ```python
    /// def f():
    ///     yield 1  # okay, in a function
    ///     lambda: (yield 1)  # okay, in a lambda
    ///
    ///     [(yield 1) for x in range(3)]  # error, in a comprehension
    ///     ((yield 1) for x in range(3))  # error, in a generator expression
    ///     class C:
    ///         yield 1  # error, in a class within a function
    /// ```
    ///
    fn in_yield_allowed_context(&self) -> bool;

    /// Returns `true` if the visitor is currently inside of a synchronous comprehension.
    ///
    /// This method is necessary because `in_async_context` only checks for the nearest, enclosing
    /// function to determine the (a)sync context. Instead, this method will search all enclosing
    /// scopes until it finds a sync comprehension. As a result, the two methods will typically be
    /// used together.
    fn in_sync_comprehension(&self) -> bool;

    /// Returns `true` if the visitor is at the top-level module scope.
    fn in_module_scope(&self) -> bool;

    /// Returns `true` if the visitor is in a function scope.
    fn in_function_scope(&self) -> bool;

    /// Returns `true` if the visitor is in a generator scope.
    ///
    /// Note that this refers to an `Expr::Generator` precisely, not to comprehensions more
    /// generally.
    fn in_generator_scope(&self) -> bool;

    /// Returns `true` if the source file is a Jupyter notebook.
    fn in_notebook(&self) -> bool;

    fn report_semantic_error(&self, error: SemanticSyntaxError);
}

/// Modified version of [`std::str::EscapeDefault`] that does not escape single or double quotes.
struct EscapeDefault<'a>(&'a str);

impl Display for EscapeDefault<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use std::fmt::Write;

        for c in self.0.chars() {
            match c {
                '\'' | '\"' => f.write_char(c)?,
                _ => write!(f, "{}", c.escape_default())?,
            }
        }
        Ok(())
    }
}
