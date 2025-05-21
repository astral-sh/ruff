use rustc_hash::FxHashSet;

use ruff_db::files::File;
use ruff_db::parsed::parsed_module;
use ruff_python_ast::name::Name;
use ruff_python_ast::statement_visitor::{StatementVisitor, walk_stmt};
use ruff_python_ast::{self as ast};

use crate::semantic_index::ast_ids::HasScopedExpressionId;
use crate::semantic_index::symbol::ScopeId;
use crate::semantic_index::{SemanticIndex, global_scope, semantic_index};
use crate::types::{Truthiness, Type, infer_expression_types};
use crate::{Db, ModuleName, resolve_module};

#[allow(clippy::ref_option)]
fn dunder_all_names_cycle_recover(
    _db: &dyn Db,
    _value: &Option<FxHashSet<Name>>,
    _count: u32,
    _file: File,
) -> salsa::CycleRecoveryAction<Option<FxHashSet<Name>>> {
    salsa::CycleRecoveryAction::Iterate
}

fn dunder_all_names_cycle_initial(_db: &dyn Db, _file: File) -> Option<FxHashSet<Name>> {
    None
}

/// Returns a set of names in the `__all__` variable for `file`, [`None`] if it is not defined or
/// if it contains invalid elements.
#[salsa::tracked(returns(as_ref), cycle_fn=dunder_all_names_cycle_recover, cycle_initial=dunder_all_names_cycle_initial)]
pub(crate) fn dunder_all_names(db: &dyn Db, file: File) -> Option<FxHashSet<Name>> {
    let _span = tracing::trace_span!("dunder_all_names", file=?file.path(db)).entered();

    let module = parsed_module(db.upcast(), file);
    let index = semantic_index(db, file);
    let mut collector = DunderAllNamesCollector::new(db, file, index);
    collector.visit_body(module.suite());
    collector.into_names()
}

/// A visitor that collects the names in the `__all__` variable of a module.
struct DunderAllNamesCollector<'db> {
    db: &'db dyn Db,
    file: File,

    /// The scope in which the `__all__` names are being collected from.
    ///
    /// This is always going to be the global scope of the module.
    scope: ScopeId<'db>,

    /// The semantic index for the module.
    index: &'db SemanticIndex<'db>,

    /// The origin of the `__all__` variable in the current module, [`None`] if it is not defined.
    origin: Option<DunderAllOrigin>,

    /// A flag indicating whether the module uses unrecognized `__all__` idioms or there are any
    /// invalid elements in `__all__`.
    invalid: bool,

    /// A set of names found in `__all__` for the current module.
    names: FxHashSet<Name>,
}

impl<'db> DunderAllNamesCollector<'db> {
    fn new(db: &'db dyn Db, file: File, index: &'db SemanticIndex<'db>) -> Self {
        Self {
            db,
            file,
            scope: global_scope(db, file),
            index,
            origin: None,
            invalid: false,
            names: FxHashSet::default(),
        }
    }

    /// Updates the origin of `__all__` in the current module.
    ///
    /// This will clear existing names if the origin is changed to mimic the behavior of overriding
    /// `__all__` in the current module.
    fn update_origin(&mut self, origin: DunderAllOrigin) {
        if self.origin.is_some() {
            self.names.clear();
        }
        self.origin = Some(origin);
    }

    /// Extends the current set of names with the names from the given expression which can be
    /// either a list/tuple/set of string-literal names or a module's `__all__` variable.
    ///
    /// Returns `true` if the expression is a valid list/tuple/set or module `__all__`, `false` otherwise.
    fn extend(&mut self, expr: &ast::Expr) -> bool {
        match expr {
            // `__all__ += [...]`
            // `__all__.extend([...])`
            ast::Expr::List(ast::ExprList { elts, .. })
            | ast::Expr::Tuple(ast::ExprTuple { elts, .. })
            | ast::Expr::Set(ast::ExprSet { elts, .. }) => self.add_names(elts),

            // `__all__ += module.__all__`
            // `__all__.extend(module.__all__)`
            ast::Expr::Attribute(ast::ExprAttribute { value, attr, .. }) => {
                if attr != "__all__" {
                    return false;
                }
                let Type::ModuleLiteral(module_literal) = self.standalone_expression_type(value)
                else {
                    return false;
                };
                let Some(module_dunder_all_names) = module_literal
                    .module(self.db)
                    .file()
                    .and_then(|file| dunder_all_names(self.db, file))
                else {
                    // The module either does not have a `__all__` variable or it is invalid.
                    return false;
                };
                self.names.extend(module_dunder_all_names.iter().cloned());
                true
            }

            _ => false,
        }
    }

    /// Processes a call idiom for `__all__` and updates the set of names accordingly.
    ///
    /// Returns `true` if the call idiom is recognized and valid, `false` otherwise.
    fn process_call_idiom(
        &mut self,
        function_name: &ast::Identifier,
        arguments: &ast::Arguments,
    ) -> bool {
        if arguments.len() != 1 {
            return false;
        }
        let Some(argument) = arguments.find_positional(0) else {
            return false;
        };
        match function_name.as_str() {
            // `__all__.extend([...])`
            // `__all__.extend(module.__all__)`
            "extend" => {
                if !self.extend(argument) {
                    return false;
                }
            }

            // `__all__.append(...)`
            "append" => {
                let Some(name) = create_name(argument) else {
                    return false;
                };
                self.names.insert(name);
            }

            // `__all__.remove(...)`
            "remove" => {
                let Some(name) = create_name(argument) else {
                    return false;
                };
                self.names.remove(&name);
            }

            _ => return false,
        }
        true
    }

    /// Returns the names in `__all__` from the module imported from the given `import_from`
    /// statement.
    ///
    /// Returns [`None`] if module resolution fails, invalid syntax, or if the module does not have
    /// a `__all__` variable.
    fn dunder_all_names_for_import_from(
        &self,
        import_from: &ast::StmtImportFrom,
    ) -> Option<&'db FxHashSet<Name>> {
        let module_name =
            ModuleName::from_import_statement(self.db, self.file, import_from).ok()?;
        let module = resolve_module(self.db, &module_name)?;
        dunder_all_names(self.db, module.file()?)
    }

    /// Infer the type of a standalone expression.
    ///
    /// # Panics
    ///
    /// This function panics if `expr` was not marked as a standalone expression during semantic indexing.
    fn standalone_expression_type(&self, expr: &ast::Expr) -> Type<'db> {
        infer_expression_types(self.db, self.index.expression(expr))
            .expression_type(expr.scoped_expression_id(self.db, self.scope))
    }

    /// Evaluate the given expression and return its truthiness.
    ///
    /// Returns [`None`] if the expression type doesn't implement `__bool__` correctly.
    fn evaluate_test_expr(&self, expr: &ast::Expr) -> Option<Truthiness> {
        self.standalone_expression_type(expr).try_bool(self.db).ok()
    }

    /// Add valid names to the set.
    ///
    /// Returns `false` if any of the names are invalid.
    fn add_names(&mut self, exprs: &[ast::Expr]) -> bool {
        for expr in exprs {
            let Some(name) = create_name(expr) else {
                return false;
            };
            self.names.insert(name);
        }
        true
    }

    /// Consumes `self` and returns the collected set of names.
    ///
    /// Returns [`None`] if `__all__` is not defined in the current module or if it contains
    /// invalid elements.
    fn into_names(self) -> Option<FxHashSet<Name>> {
        if self.origin.is_none() {
            None
        } else if self.invalid {
            tracing::debug!("Invalid `__all__` in `{}`", self.file.path(self.db));
            None
        } else {
            Some(self.names)
        }
    }
}

impl<'db> StatementVisitor<'db> for DunderAllNamesCollector<'db> {
    fn visit_stmt(&mut self, stmt: &'db ast::Stmt) {
        if self.invalid {
            return;
        }

        match stmt {
            ast::Stmt::ImportFrom(import_from @ ast::StmtImportFrom { names, .. }) => {
                for ast::Alias { name, asname, .. } in names {
                    // `from module import *` where `module` is a module with a top-level `__all__`
                    // variable that contains the "__all__" element.
                    if name == "*" {
                        // Here, we need to use the `dunder_all_names` query instead of the
                        // `exported_names` query because a `*`-import does not import the
                        // `__all__` attribute unless it is explicitly included in the `__all__` of
                        // the module.
                        let Some(all_names) = self.dunder_all_names_for_import_from(import_from)
                        else {
                            self.invalid = true;
                            continue;
                        };

                        if all_names.contains(&Name::new_static("__all__")) {
                            self.update_origin(DunderAllOrigin::StarImport);
                            self.names.extend(all_names.iter().cloned());
                        }
                    } else {
                        // `from module import __all__`
                        // `from module import __all__ as __all__`
                        if name != "__all__"
                            || asname.as_ref().is_some_and(|asname| asname != "__all__")
                        {
                            continue;
                        }

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
                        let Some(all_names) = self.dunder_all_names_for_import_from(import_from)
                        else {
                            self.invalid = true;
                            continue;
                        };

                        self.update_origin(DunderAllOrigin::ExternalModule);
                        self.names.extend(all_names.iter().cloned());
                    }
                }
            }

            ast::Stmt::Assign(ast::StmtAssign { targets, value, .. }) => {
                let [target] = targets.as_slice() else {
                    return;
                };
                if !is_dunder_all(target) {
                    return;
                }
                match &**value {
                    // `__all__ = [...]`
                    // `__all__ = (...)`
                    ast::Expr::List(ast::ExprList { elts, .. })
                    | ast::Expr::Tuple(ast::ExprTuple { elts, .. }) => {
                        self.update_origin(DunderAllOrigin::CurrentModule);
                        if !self.add_names(elts) {
                            self.invalid = true;
                        }
                    }
                    _ => {
                        self.invalid = true;
                    }
                }
            }

            ast::Stmt::AugAssign(ast::StmtAugAssign {
                target,
                op: ast::Operator::Add,
                value,
                ..
            }) => {
                if self.origin.is_none() {
                    // We can't update `__all__` if it doesn't already exist.
                    return;
                }
                if !is_dunder_all(target) {
                    return;
                }
                if !self.extend(value) {
                    self.invalid = true;
                }
            }

            ast::Stmt::AnnAssign(ast::StmtAnnAssign {
                target,
                value: Some(value),
                ..
            }) => {
                if !is_dunder_all(target) {
                    return;
                }
                match &**value {
                    // `__all__: list[str] = [...]`
                    // `__all__: tuple[str, ...] = (...)`
                    ast::Expr::List(ast::ExprList { elts, .. })
                    | ast::Expr::Tuple(ast::ExprTuple { elts, .. }) => {
                        self.update_origin(DunderAllOrigin::CurrentModule);
                        if !self.add_names(elts) {
                            self.invalid = true;
                        }
                    }
                    _ => {
                        self.invalid = true;
                    }
                }
            }

            ast::Stmt::Expr(ast::StmtExpr { value: expr, .. }) => {
                if self.origin.is_none() {
                    // We can't update `__all__` if it doesn't already exist.
                    return;
                }
                let Some(ast::ExprCall {
                    func, arguments, ..
                }) = expr.as_call_expr()
                else {
                    return;
                };
                let Some(ast::ExprAttribute {
                    value,
                    attr,
                    ctx: ast::ExprContext::Load,
                    ..
                }) = func.as_attribute_expr()
                else {
                    return;
                };
                if !is_dunder_all(value) {
                    return;
                }
                if !self.process_call_idiom(attr, arguments) {
                    self.invalid = true;
                }
            }

            ast::Stmt::If(ast::StmtIf {
                test,
                body,
                elif_else_clauses,
                ..
            }) => match self.evaluate_test_expr(test) {
                Some(Truthiness::AlwaysTrue) => self.visit_body(body),
                Some(Truthiness::AlwaysFalse) => {
                    for ast::ElifElseClause { test, body, .. } in elif_else_clauses {
                        if let Some(test) = test {
                            match self.evaluate_test_expr(test) {
                                Some(Truthiness::AlwaysTrue) => {
                                    self.visit_body(body);
                                    break;
                                }
                                Some(Truthiness::AlwaysFalse) => {}
                                Some(Truthiness::Ambiguous) | None => {
                                    break;
                                }
                            }
                        } else {
                            self.visit_body(body);
                        }
                    }
                }
                Some(Truthiness::Ambiguous) | None => {}
            },

            ast::Stmt::For(..)
            | ast::Stmt::While(..)
            | ast::Stmt::With(..)
            | ast::Stmt::Match(..)
            | ast::Stmt::Try(..) => {
                walk_stmt(self, stmt);
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
}

#[derive(Debug, Clone)]
enum DunderAllOrigin {
    /// The `__all__` variable is defined in the current module.
    CurrentModule,

    /// The `__all__` variable is imported from another module.
    ExternalModule,

    /// The `__all__` variable is imported from a module via a `*`-import.
    StarImport,
}

/// Checks if the given expression is a name expression for `__all__`.
fn is_dunder_all(expr: &ast::Expr) -> bool {
    matches!(expr, ast::Expr::Name(ast::ExprName { id, .. }) if id == "__all__")
}

/// Create and return a [`Name`] from the given expression, [`None`] if it is an invalid expression
/// for a `__all__` element.
fn create_name(expr: &ast::Expr) -> Option<Name> {
    Some(Name::new(expr.as_string_literal_expr()?.value.to_str()))
}
