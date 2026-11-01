use ruff_db::parsed::parsed_module;
use ruff_python_ast::name::Name;
use ruff_python_ast::statement_visitor::{StatementVisitor, walk_stmt};
use ruff_python_ast::{
    self as ast, Arguments, Expr, ExprAttribute, ExprCall, ExprContext, Identifier, NodeList,
    Operator, Stmt, StmtImportFrom,
};

use rustc_hash::FxHashSet;
use ty_module_resolver::{ImportingFile, resolve_module_for_import_from};

use crate::types::{Type, TypeContext, infer_expression_types};
use crate::{Db, ProgramEnvironment};
use ty_python_core::static_dunder_all::static_dunder_all;
use ty_python_core::{ProgramFile, SemanticIndex, Truthiness, semantic_index};

/// Returns a set of names in the `__all__` variable for `file`, [`None`] if it is not defined or
/// if it contains invalid elements.
#[salsa::tracked(returns(as_ref), cycle_initial=|_, _, _| None, heap_size=ruff_memory_usage::heap_size)]
pub(crate) fn dunder_all_names(db: &dyn Db, file: ProgramFile<'_>) -> Option<FxHashSet<Name>> {
    if let Some(names) = static_dunder_all(db, file) {
        return Some(names.iter().cloned().collect());
    }

    let source_file = file.file(db);
    let _span = tracing::trace_span!("dunder_all_names", file=?source_file.path(db)).entered();

    let module = parsed_module(db, file.python_file(db)).load(db);
    let index = semantic_index(db, file);
    let mut collector = DunderAllNamesCollector::new(db, file, index);
    collector.visit_body(module.suite());
    collector.into_names()
}

/// A visitor that collects the names in the `__all__` variable of a module.
struct DunderAllNamesCollector<'db> {
    db: &'db dyn Db,
    env: ProgramEnvironment<'db>,
    file: ProgramFile<'db>,

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
    fn new(db: &'db dyn Db, file: ProgramFile<'db>, index: &'db SemanticIndex<'db>) -> Self {
        Self {
            db,
            env: ProgramEnvironment::from_file(file),
            file,
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
    fn extend(&mut self, expr: Expr<'_>) -> bool {
        let db = self.db;
        match expr.fields() {
            // `__all__ += [...]`
            // `__all__.extend([...])`
            ast::ExprFields::List(ast::ExprListFields { elts, .. })
            | ast::ExprFields::Tuple(ast::ExprTupleFields { elts, .. })
            | ast::ExprFields::Set(ast::ExprSetFields { elts, .. }) => self.add_names(elts),

            // `__all__ += module.__all__`
            // `__all__.extend(module.__all__)`
            ast::ExprFields::Attribute(ast::ExprAttributeFields { value, attr, .. }) => {
                if attr != "__all__" {
                    return false;
                }

                let Type::ModuleLiteral(module_literal) = self.standalone_expression_type(value)
                else {
                    return false;
                };
                let Some(module_dunder_all_names) = module_literal
                    .module(db)
                    .file(db)
                    .map(|file| ProgramFile::new(db, file, self.env.program(db)))
                    .and_then(|file| dunder_all_names(db, file))
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
        function_name: Identifier<'_>,
        arguments: Arguments<'_>,
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
        import_from: StmtImportFrom<'_>,
    ) -> Option<&'db FxHashSet<Name>> {
        let db = self.db;

        let importing_file =
            ImportingFile::File(self.file.file(db), self.env.resolver_environment(db));
        let module = resolve_module_for_import_from(db, importing_file, import_from)?;
        dunder_all_names(
            db,
            ProgramFile::new(db, module.file(db)?, self.env.program(db)),
        )
    }

    /// Infer the type of a standalone expression.
    ///
    /// # Panics
    ///
    /// This function panics if `expr` was not marked as a standalone expression during semantic indexing.
    fn standalone_expression_type(&self, expr: Expr<'_>) -> Type<'db> {
        let db = self.db;
        infer_expression_types(db, self.index.expression(expr), TypeContext::default())
            .expression_type(expr)
    }

    /// Evaluate the given expression and return its truthiness.
    ///
    /// Returns [`None`] if the expression type doesn't implement `__bool__` correctly.
    fn evaluate_test_expr(&self, expr: Expr<'_>) -> Option<Truthiness> {
        let db = self.db;
        self.standalone_expression_type(expr)
            .try_bool(db, &self.env)
            .ok()
    }

    /// Add valid names to the set.
    ///
    /// Returns `false` if any of the names are invalid.
    fn add_names(&mut self, exprs: NodeList<'_, ruff_python_ast::kind::Expr>) -> bool {
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
    fn into_names(mut self) -> Option<FxHashSet<Name>> {
        let db = self.db;
        if self.origin.is_none() {
            None
        } else if self.invalid {
            tracing::debug!("Invalid `__all__` in `{}`", self.file.file(db).path(db));
            None
        } else {
            self.names.shrink_to_fit();
            Some(self.names)
        }
    }
}

impl<'db> StatementVisitor<'db> for DunderAllNamesCollector<'db> {
    fn visit_stmt(&mut self, stmt: Stmt<'db>) {
        if self.invalid {
            return;
        }

        match stmt.fields() {
            ast::StmtFields::ImportFrom(ast::StmtImportFromFields {
                node: import_from,
                names,
                ..
            }) => {
                for alias in names {
                    let name = alias.name();
                    let asname = alias.asname();
                    // `from module import *` where `module` is a module with a top-level `__all__`
                    // variable that contains the "__all__" element.
                    if *name == *"*" {
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
                        if *name != *"__all__" || asname.is_some_and(|asname| *asname != *"__all__")
                        {
                            continue;
                        }

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

            ast::StmtFields::Assign(ast::StmtAssignFields { targets, value, .. }) => {
                let Some([target]) = targets.as_array() else {
                    return;
                };
                if !is_dunder_all(target) {
                    return;
                }
                match value.fields() {
                    // `__all__ = [...]`
                    // `__all__ = (...)`
                    ast::ExprFields::List(ast::ExprListFields { elts, .. })
                    | ast::ExprFields::Tuple(ast::ExprTupleFields { elts, .. }) => {
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

            ast::StmtFields::AugAssign(ast::StmtAugAssignFields {
                target,
                op: Operator::Add,
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

            ast::StmtFields::AnnAssign(ast::StmtAnnAssignFields {
                target,
                value: Some(value),
                ..
            }) => {
                if !is_dunder_all(target) {
                    return;
                }
                match value.fields() {
                    // `__all__: list[str] = [...]`
                    // `__all__: tuple[str, ...] = (...)`
                    ast::ExprFields::List(ast::ExprListFields { elts, .. })
                    | ast::ExprFields::Tuple(ast::ExprTupleFields { elts, .. }) => {
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

            ast::StmtFields::Expr(ast::StmtExprFields { value: expr, .. }) => {
                if self.origin.is_none() {
                    // We can't update `__all__` if it doesn't already exist.
                    return;
                }
                let Some(ast::ExprCallFields {
                    func, arguments, ..
                }) = expr.as_call_expr().map(ExprCall::fields)
                else {
                    return;
                };
                let Some(ast::ExprAttributeFields {
                    value,
                    attr,
                    ctx: ExprContext::Load,
                    ..
                }) = func.as_attribute_expr().map(ExprAttribute::fields)
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

            ast::StmtFields::If(ast::StmtIfFields {
                test,
                body,
                elif_else_clauses,
                ..
            }) => match self.evaluate_test_expr(test) {
                Some(Truthiness::AlwaysTrue) => self.visit_body(body),
                Some(Truthiness::AlwaysFalse) => {
                    for elif_else_clause in elif_else_clauses {
                        let test = elif_else_clause.test();
                        let body = elif_else_clause.body();
                        if let Some(test) = test {
                            match self.evaluate_test_expr(test) {
                                Some(Truthiness::AlwaysTrue) => {
                                    self.visit_body(body);
                                    break;
                                }
                                Some(Truthiness::AlwaysFalse) => {}
                                Some(Truthiness::Ambiguous | Truthiness::Uninhabited) | None => {
                                    break;
                                }
                            }
                        } else {
                            self.visit_body(body);
                        }
                    }
                }
                Some(Truthiness::Ambiguous | Truthiness::Uninhabited) | None => {}
            },

            ast::StmtFields::For(..)
            | ast::StmtFields::While(..)
            | ast::StmtFields::With(..)
            | ast::StmtFields::Match(..)
            | ast::StmtFields::Try(..) => {
                walk_stmt(self, stmt);
            }

            ast::StmtFields::FunctionDef(..) | ast::StmtFields::ClassDef(..) => {
                // Avoid recursing into any nested scopes as `__all__` is only valid at the module
                // level.
            }

            ast::StmtFields::AugAssign(..)
            | ast::StmtFields::AnnAssign(..)
            | ast::StmtFields::Delete(..)
            | ast::StmtFields::Return(..)
            | ast::StmtFields::Raise(..)
            | ast::StmtFields::Assert(..)
            | ast::StmtFields::Import(..)
            | ast::StmtFields::Global(..)
            | ast::StmtFields::Nonlocal(..)
            | ast::StmtFields::TypeAlias(..)
            | ast::StmtFields::Pass(..)
            | ast::StmtFields::Break(..)
            | ast::StmtFields::Continue(..)
            | ast::StmtFields::IpyEscapeCommand(..) => {}
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
fn is_dunder_all(expr: Expr<'_>) -> bool {
    matches!(expr.fields(), ast::ExprFields::Name(ast::ExprNameFields { id, .. }) if id == "__all__")
}

/// Create and return a [`Name`] from the given expression, [`None`] if it is an invalid expression
/// for a `__all__` element.
fn create_name(expr: Expr<'_>) -> Option<Name> {
    Some(Name::new(expr.as_string_literal_expr()?.value().to_str()))
}
