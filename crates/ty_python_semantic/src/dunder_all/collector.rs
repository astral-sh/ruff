use std::convert::Infallible;

use ruff_python_ast as ast;
use ruff_python_ast::name::Name;
use rustc_hash::FxHashSet;
use ty_mapping_probe_macros::shared_semantic_family;
use ty_module_resolver::{ImportingFile, resolve_module_for_import_from};
use ty_python_core::{ProgramFile, SemanticIndex, Truthiness};

use super::dunder_all_names;
use crate::types::{ModuleLiteralType, Type, TypeContext, infer_expression_types};
use crate::{Db, ProgramEnvironment};

pub(crate) struct DunderAllFacts;

/// Pending visits borrow the prepared module and never own nested continuations.
pub(crate) enum Frame<'ast> {
    Body(&'ast [ast::Stmt]),
    Statement(&'ast ast::Stmt),
    Assignment(&'ast ast::Expr),
    ElifElse(&'ast [ast::ElifElseClause]),
    ImportAliases(&'ast ast::StmtImportFrom, &'ast [ast::Alias]),
    AddNames(&'ast [ast::Expr]),
    Extend(&'ast ast::Expr),
    MatchCases(&'ast [ast::MatchCase]),
    Handlers(&'ast [ast::ExceptHandler]),
}

/// Retains the collected names and pending visits across child queries.
#[derive(Default)]
pub(crate) struct Collector<'ast> {
    pub(crate) frames: Vec<Frame<'ast>>,

    /// The origin of the `__all__` variable in the current module, [`None`] if it is not defined.
    pub(crate) origin: Option<DunderAllOrigin>,

    /// A flag indicating whether the module uses unrecognized `__all__` idioms or there are any
    /// invalid elements in `__all__`.
    pub(crate) invalid: bool,

    /// A set of names found in `__all__` for the current module.
    pub(crate) names: FxHashSet<Name>,

    /// High-water table-slot bound for disposal admission. Removals and clearing can reduce
    /// reported capacity without releasing the backing allocation, so they retain this bound.
    pub(crate) names_backing: usize,
}

#[derive(Clone, Copy)]
pub(crate) enum DunderAllOrigin {
    /// The `__all__` variable is defined in the current module.
    CurrentModule,

    /// The `__all__` variable is imported from another module.
    ExternalModule,

    /// The `__all__` variable is imported from a module via a `*`-import.
    StarImport,
}

pub(super) struct OrdinaryDunderAllEffects<'db> {
    pub(super) db: &'db dyn Db,
    pub(super) env: ProgramEnvironment<'db>,
    pub(super) file: ProgramFile<'db>,
    pub(super) index: &'db SemanticIndex<'db>,
}

shared_semantic_family! {
    #[synchronous(SynchronousDunderAllEffects)]
    pub(crate) trait DunderAllEffects<'db, 'ast> {
        type Error;

        // Storage effects admit growth and disposal before mutation, including state retained
        // while a child query is suspended or refuses to complete.
        #[operation(local)]
        #[progress]
        async fn next(&self, state: &mut Collector<'ast>) -> Result<Option<Frame<'ast>>, Self::Error>;
        #[operation(local)]
        async fn push(&self, state: &mut Collector<'ast>, frame: Frame<'ast>) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn clear_names(&self, state: &mut Collector<'ast>) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn add_name(&self, state: &mut Collector<'ast>, expr: &ast::ExprStringLiteral) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn remove_name(&self, state: &mut Collector<'ast>, expr: &ast::ExprStringLiteral) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn extend_names(&self, state: &mut Collector<'ast>, names: &FxHashSet<Name>) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn contains_dunder_all(&self, names: &FxHashSet<Name>) -> Result<bool, Self::Error>;
        #[operation(local)]
        async fn finish(&self, state: &mut Collector<'ast>) -> Result<FxHashSet<Name>, Self::Error>;
        #[operation(local)]
        async fn discard(&self, state: &mut Collector<'ast>) -> Result<(), Self::Error>;

        #[operation(child)]
        async fn imported_names(&self, import: &ast::StmtImportFrom) -> Result<Option<&'db FxHashSet<Name>>, Self::Error>;
        #[operation(child)]
        async fn module_names(&self, module: ModuleLiteralType<'db>) -> Result<Option<&'db FxHashSet<Name>>, Self::Error>;
        #[operation(child)]
        async fn expression_type(&self, expr: &'ast ast::Expr) -> Result<Type<'db>, Self::Error>;
        #[operation(source)]
        async fn truthiness(&self, ty: Type<'db>) -> Result<Option<Truthiness>, Self::Error>;
    }

    #[finite_capability]
    impl DunderAllFacts {
        fn invalid(&self, state: &Collector<'_>) -> bool {
            state.invalid
        }

        fn invalidate(&self, state: &mut Collector<'_>) {
            state.invalid = true;
        }

        fn has_origin(&self, state: &Collector<'_>) -> bool {
            state.origin.is_some()
        }

        fn set_origin(&self, state: &mut Collector<'_>, origin: DunderAllOrigin) {
            state.origin = Some(origin);
        }

        fn statement<'ast>(&self, body: &'ast [ast::Stmt]) -> Option<(&'ast ast::Stmt, &'ast [ast::Stmt])> {
            body.split_first()
        }

        fn clause<'ast>(&self, clauses: &'ast [ast::ElifElseClause]) -> Option<(&'ast ast::ElifElseClause, &'ast [ast::ElifElseClause])> {
            clauses.split_first()
        }

        fn alias<'ast>(&self, aliases: &'ast [ast::Alias]) -> Option<(&'ast ast::Alias, &'ast [ast::Alias])> {
            aliases.split_first()
        }

        fn expression<'ast>(&self, expressions: &'ast [ast::Expr]) -> Option<(&'ast ast::Expr, &'ast [ast::Expr])> {
            expressions.split_first()
        }

        fn match_case<'ast>(&self, cases: &'ast [ast::MatchCase]) -> Option<(&'ast ast::MatchCase, &'ast [ast::MatchCase])> {
            cases.split_first()
        }

        fn handler<'ast>(&self, handlers: &'ast [ast::ExceptHandler]) -> Option<(&'ast ast::ExceptHandler, &'ast [ast::ExceptHandler])> {
            handlers.split_first()
        }

        /// Checks if the given expression is a name expression for `__all__`.
        fn is_dunder_all(&self, expr: &ast::Expr) -> bool {
            matches!(expr, ast::Expr::Name(ast::ExprName { id, .. }) if id == "__all__")
        }

        fn single_target<'ast>(&self, targets: &'ast [ast::Expr]) -> Option<&'ast ast::Expr> {
            match targets {
                [target] => Some(target),
                _ => None,
            }
        }

        fn is_dunder_all_attribute(&self, attr: &ast::Identifier) -> bool {
            attr == "__all__"
        }

        fn import_origin(&self, alias: &ast::Alias) -> Option<DunderAllOrigin> {
            if alias.name.as_str() == "*" {
                Some(DunderAllOrigin::StarImport)
            } else if alias.name.as_str() == "__all__"
                && !alias.asname.as_ref().is_some_and(|asname| asname != "__all__")
            {
                Some(DunderAllOrigin::ExternalModule)
            } else {
                None
            }
        }

        fn call_idiom<'ast>(&self, expr: &'ast ast::Expr) -> Option<CallIdiom<'ast>> {
            let ast::ExprCall { func, arguments, .. } = expr.as_call_expr()?;
            let ast::ExprAttribute { value, attr, ctx: ast::ExprContext::Load, .. } = func.as_attribute_expr()? else {
                return None;
            };
            if !self.is_dunder_all(value) {
                return None;
            }
            if arguments.len() != 1 {
                return Some(CallIdiom::Invalid);
            }
            let Some(argument) = arguments.find_positional(0) else {
                return Some(CallIdiom::Invalid);
            };
            Some(match attr.as_str() {
                "extend" => CallIdiom::Extend(argument),
                "append" => CallIdiom::Append(argument),
                "remove" => CallIdiom::Remove(argument),
                _ => CallIdiom::Invalid,
            })
        }
    }

    #[synchronous(collect_sync)]
    #[capabilities(effects = DunderAllEffects, facts = DunderAllFacts)]
    #[passive_values(Frame::Body, Frame::Statement, Frame::Assignment, Frame::ElifElse, Frame::ImportAliases, Frame::AddNames, Frame::Extend, Frame::MatchCases, Frame::Handlers, DunderAllOrigin::CurrentModule)]
    pub(crate) async fn collect_with<'db, 'ast, E: DunderAllEffects<'db, 'ast>>(
        state: &mut Collector<'ast>,
        body: &'ast [ast::Stmt],
        facts: DunderAllFacts,
        effects: &E,
    ) -> Result<Option<FxHashSet<Name>>, E::Error> {
        effects.push(state, Frame::Body(body)).await?;
        #[cursor_loop]
        while let Some(frame) = effects.next(state).await? {
            match frame {
                Frame::Body(body) => {
                    if let Some((statement, rest)) = facts.statement(body) {
                        effects.push(state, Frame::Body(rest)).await?;
                        effects.push(state, Frame::Statement(statement)).await?;
                    }
                }
                Frame::Statement(statement) => {
                    if facts.invalid(state) {
                        continue;
                    }
                    match statement {
                        ast::Stmt::ImportFrom(import_from) => {
                            effects.push(state, Frame::ImportAliases(import_from, &import_from.names)).await?;
                        }
                        ast::Stmt::Assign(ast::StmtAssign { targets, value, .. }) => {
                            if let Some(target) = facts.single_target(targets)
                                && facts.is_dunder_all(target)
                            {
                                effects.push(state, Frame::Assignment(value)).await?;
                            }
                        }
                        ast::Stmt::AnnAssign(ast::StmtAnnAssign { target, value: Some(value), .. }) => {
                            if facts.is_dunder_all(target) {
                                effects.push(state, Frame::Assignment(value)).await?;
                            }
                        }
                        ast::Stmt::AugAssign(ast::StmtAugAssign { target, op: ast::Operator::Add, value, .. }) => {
                            if !facts.has_origin(state) {
                                // We can't update `__all__` if it doesn't already exist.
                                continue;
                            }
                            if facts.is_dunder_all(target) {
                                effects.push(state, Frame::Extend(value)).await?;
                            }
                        }
                        ast::Stmt::Expr(ast::StmtExpr { value: expr, .. }) => {
                            if !facts.has_origin(state) {
                                // We can't update `__all__` if it doesn't already exist.
                                continue;
                            }
                            match facts.call_idiom(expr) {
                                Some(CallIdiom::Extend(argument)) => {
                                    // `__all__.extend([...])`
                                    // `__all__.extend(module.__all__)`
                                    effects.push(state, Frame::Extend(argument)).await?;
                                }
                                Some(CallIdiom::Append(argument)) => {
                                    // `__all__.append(...)`
                                    if let ast::Expr::StringLiteral(literal) = argument {
                                        effects.add_name(state, literal).await?;
                                    } else {
                                        facts.invalidate(state);
                                    }
                                }
                                Some(CallIdiom::Remove(argument)) => {
                                    // `__all__.remove(...)`
                                    if let ast::Expr::StringLiteral(literal) = argument {
                                        effects.remove_name(state, literal).await?;
                                    } else {
                                        facts.invalidate(state);
                                    }
                                }
                                Some(CallIdiom::Invalid) => facts.invalidate(state),
                                None => {}
                            }
                        }
                        ast::Stmt::If(ast::StmtIf { test, body, elif_else_clauses, .. }) => {
                            let ty = effects.expression_type(test).await?;
                            match effects.truthiness(ty).await? {
                                Some(Truthiness::AlwaysTrue) => {
                                    effects.push(state, Frame::Body(body)).await?;
                                }
                                Some(Truthiness::AlwaysFalse) => {
                                    effects.push(state, Frame::ElifElse(elif_else_clauses)).await?;
                                }
                                Some(Truthiness::Ambiguous) | None => {}
                            }
                        }
                        ast::Stmt::For(ast::StmtFor { body, orelse, .. })
                        | ast::Stmt::While(ast::StmtWhile { body, orelse, .. }) => {
                            effects.push(state, Frame::Body(orelse)).await?;
                            effects.push(state, Frame::Body(body)).await?;
                        }
                        ast::Stmt::With(ast::StmtWith { body, .. }) => {
                            effects.push(state, Frame::Body(body)).await?;
                        }
                        ast::Stmt::Match(ast::StmtMatch { cases, .. }) => {
                            effects.push(state, Frame::MatchCases(cases)).await?;
                        }
                        ast::Stmt::Try(ast::StmtTry { body, handlers, orelse, finalbody, .. }) => {
                            effects.push(state, Frame::Body(finalbody)).await?;
                            effects.push(state, Frame::Body(orelse)).await?;
                            effects.push(state, Frame::Handlers(handlers)).await?;
                            effects.push(state, Frame::Body(body)).await?;
                        }
                        ast::Stmt::FunctionDef(..) | ast::Stmt::ClassDef(..) => {
                            // Avoid recursing into any nested scopes as `__all__` is only valid at the module
                            // level.
                        }
                        ast::Stmt::AugAssign(..)
                        | ast::Stmt::AnnAssign(..)
                        | ast::Stmt::Delete(..)
                        | ast::Stmt::Return(..)
                        | ast::Stmt::Raise(..)
                        | ast::Stmt::Assert(..)
                        | ast::Stmt::Import(..)
                        | ast::Stmt::Global(..)
                        | ast::Stmt::Nonlocal(..)
                        | ast::Stmt::TypeAlias(..)
                        | ast::Stmt::Pass(..)
                        | ast::Stmt::Break(..)
                        | ast::Stmt::Continue(..)
                        | ast::Stmt::IpyEscapeCommand(..) => {}
                    }
                }
                Frame::Assignment(value) => {
                    match value {
                        // `__all__ = [...]`
                        // `__all__ = (...)`
                        // `__all__: list[str] = [...]`
                        // `__all__: tuple[str, ...] = (...)`
                        ast::Expr::List(ast::ExprList { elts, .. })
                        | ast::Expr::Tuple(ast::ExprTuple { elts, .. }) => {
                            if facts.has_origin(state) {
                                effects.clear_names(state).await?;
                            }
                            facts.set_origin(state, DunderAllOrigin::CurrentModule);
                            effects.push(state, Frame::AddNames(elts)).await?;
                        }
                        _ => facts.invalidate(state),
                    }
                }
                Frame::ImportAliases(import_from, aliases) => {
                    let Some((alias, rest)) = facts.alias(aliases) else {
                        continue;
                    };
                    effects.push(state, Frame::ImportAliases(import_from, rest)).await?;
                    let Some(origin) = facts.import_origin(alias) else {
                        continue;
                    };
                    // We could do the `__all__` lookup lazily in case it's not needed. This would
                    // happen if a `__all__` is imported from another module but then the module
                    // redefines it. For example:
                    //
                    // ```python
                    // from module import __all__ as __all__
                    //
                    // __all__ = ["a", "b"]
                    // ```
                    //
                    // I'm avoiding this for now because it doesn't seem likely to happen in
                    // practice.
                    let Some(all_names) = effects.imported_names(import_from).await? else {
                        facts.invalidate(state);
                        continue;
                    };
                    if matches!(origin, DunderAllOrigin::StarImport) {
                        // `from module import *` where `module` is a module with a top-level `__all__`
                        // variable that contains the "__all__" element.
                        // Here, we need to use the `dunder_all_names` query instead of the
                        // `exported_names` query because a `*`-import does not import the
                        // `__all__` attribute unless it is explicitly included in the `__all__` of
                        // the module.
                        if !effects.contains_dunder_all(all_names).await? {
                            continue;
                        }
                    }
                    if facts.has_origin(state) {
                        effects.clear_names(state).await?;
                    }
                    facts.set_origin(state, origin);
                    effects.extend_names(state, all_names).await?;
                }
                Frame::AddNames(expressions) => {
                    if let Some((expression, rest)) = facts.expression(expressions) {
                        if let ast::Expr::StringLiteral(literal) = expression {
                            effects.add_name(state, literal).await?;
                            effects.push(state, Frame::AddNames(rest)).await?;
                        } else {
                            facts.invalidate(state);
                        }
                    }
                }
                Frame::Extend(expression) => {
                    match expression {
                        // `__all__ += [...]`
                        // `__all__.extend([...])`
                        ast::Expr::List(ast::ExprList { elts, .. })
                        | ast::Expr::Tuple(ast::ExprTuple { elts, .. })
                        | ast::Expr::Set(ast::ExprSet { elts, .. }) => {
                            effects.push(state, Frame::AddNames(elts)).await?;
                        }
                        // `__all__ += module.__all__`
                        // `__all__.extend(module.__all__)`
                        ast::Expr::Attribute(ast::ExprAttribute { value, attr, .. }) => {
                            if !facts.is_dunder_all_attribute(attr) {
                                facts.invalidate(state);
                                continue;
                            }
                            let Type::ModuleLiteral(module) = effects.expression_type(value).await? else {
                                facts.invalidate(state);
                                continue;
                            };
                            let Some(names) = effects.module_names(module).await? else {
                                // The module either does not have a `__all__` variable or it is invalid.
                                facts.invalidate(state);
                                continue;
                            };
                            effects.extend_names(state, names).await?;
                        }
                        _ => facts.invalidate(state),
                    }
                }
                Frame::ElifElse(clauses) => {
                    let Some((clause, rest)) = facts.clause(clauses) else {
                        continue;
                    };
                    if let Some(test) = &clause.test {
                        let ty = effects.expression_type(test).await?;
                        match effects.truthiness(ty).await? {
                            Some(Truthiness::AlwaysTrue) => {
                                effects.push(state, Frame::Body(&clause.body)).await?;
                            }
                            Some(Truthiness::AlwaysFalse) => {
                                effects.push(state, Frame::ElifElse(rest)).await?;
                            }
                            Some(Truthiness::Ambiguous) | None => {}
                        }
                    } else {
                        effects.push(state, Frame::ElifElse(rest)).await?;
                        effects.push(state, Frame::Body(&clause.body)).await?;
                    }
                }
                Frame::MatchCases(cases) => {
                    if let Some((case, rest)) = facts.match_case(cases) {
                        effects.push(state, Frame::MatchCases(rest)).await?;
                        effects.push(state, Frame::Body(&case.body)).await?;
                    }
                }
                Frame::Handlers(handlers) => {
                    if let Some((ast::ExceptHandler::ExceptHandler(handler), rest)) = facts.handler(handlers) {
                        effects.push(state, Frame::Handlers(rest)).await?;
                        effects.push(state, Frame::Body(&handler.body)).await?;
                    }
                }
            }
        }
        if facts.has_origin(state) && !facts.invalid(state) {
            Ok(Some(effects.finish(state).await?))
        } else {
            effects.discard(state).await?;
            Ok(None)
        }
    }
}

enum CallIdiom<'ast> {
    Extend(&'ast ast::Expr),
    Append(&'ast ast::Expr),
    Remove(&'ast ast::Expr),
    Invalid,
}

impl<'db, 'ast> SynchronousDunderAllEffects<'db, 'ast> for OrdinaryDunderAllEffects<'db> {
    type Error = Infallible;

    fn next(&self, state: &mut Collector<'ast>) -> Result<Option<Frame<'ast>>, Infallible> {
        Ok(state.frames.pop())
    }

    fn push(&self, state: &mut Collector<'ast>, frame: Frame<'ast>) -> Result<(), Infallible> {
        state.frames.push(frame);
        Ok(())
    }

    fn clear_names(&self, state: &mut Collector<'ast>) -> Result<(), Infallible> {
        state.names.clear();
        Ok(())
    }

    fn add_name(
        &self,
        state: &mut Collector<'ast>,
        expr: &ast::ExprStringLiteral,
    ) -> Result<(), Infallible> {
        state.names.insert(create_name(expr));
        state.names_backing = state.names_backing.max(state.names.capacity());
        Ok(())
    }

    fn remove_name(
        &self,
        state: &mut Collector<'ast>,
        expr: &ast::ExprStringLiteral,
    ) -> Result<(), Infallible> {
        state.names.remove(&create_name(expr));
        Ok(())
    }

    fn extend_names(
        &self,
        state: &mut Collector<'ast>,
        names: &FxHashSet<Name>,
    ) -> Result<(), Infallible> {
        state.names.extend(names.iter().cloned());
        state.names_backing = state.names_backing.max(state.names.capacity());
        Ok(())
    }

    fn contains_dunder_all(&self, names: &FxHashSet<Name>) -> Result<bool, Infallible> {
        Ok(names.contains(&Name::new_static("__all__")))
    }

    fn finish(&self, state: &mut Collector<'ast>) -> Result<FxHashSet<Name>, Infallible> {
        state.names.shrink_to_fit();
        Ok(std::mem::take(&mut state.names))
    }

    fn discard(&self, state: &mut Collector<'ast>) -> Result<(), Infallible> {
        state.names = FxHashSet::default();
        Ok(())
    }

    fn imported_names(
        &self,
        import: &ast::StmtImportFrom,
    ) -> Result<Option<&'db FxHashSet<Name>>, Infallible> {
        let db = self.db;
        let importing_file =
            ImportingFile::File(self.file.file(db), self.env.resolver_environment(db));
        let Some(module) = resolve_module_for_import_from(db, importing_file, import) else {
            return Ok(None);
        };
        let Some(file) = module.file(db) else {
            return Ok(None);
        };
        Ok(dunder_all_names(
            db,
            ProgramFile::new(db, file, self.env.program(db)),
        ))
    }

    fn module_names(
        &self,
        module: ModuleLiteralType<'db>,
    ) -> Result<Option<&'db FxHashSet<Name>>, Infallible> {
        let db = self.db;
        let Some(file) = module.module(db).file(db) else {
            return Ok(None);
        };
        Ok(dunder_all_names(
            db,
            ProgramFile::new(db, file, self.env.program(db)),
        ))
    }

    fn expression_type(&self, expr: &'ast ast::Expr) -> Result<Type<'db>, Infallible> {
        Ok(
            infer_expression_types(self.db, self.index.expression(expr), TypeContext::default())
                .expression_type(expr),
        )
    }

    fn truthiness(&self, ty: Type<'db>) -> Result<Option<Truthiness>, Infallible> {
        Ok(ty.try_bool(self.db, &self.env).ok())
    }
}

/// Create and return a [`Name`] from the given string literal.
fn create_name(expr: &ast::ExprStringLiteral) -> Name {
    Name::new(expr.value.to_str())
}
