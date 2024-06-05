use std::fmt::Formatter;
use std::num::NonZeroU32;
use std::sync::Arc;

use countme::Count;
use rustc_hash::FxHashMap;
use salsa::database::AsSalsaDatabase;
use salsa::DebugWithDb;
use tracing::warn;

use ruff_python_ast::visitor::preorder;
use ruff_python_ast::visitor::preorder::{PreorderVisitor, TraversalSignal};
use ruff_python_ast::{AnyNodeRef, Stmt};

use crate::ast_ids::AstIds;
use crate::db::Upcast;
use crate::module::ModuleName;
use crate::salsa_db::hir::{module_body, Expression, Statement};
use crate::salsa_db::semantic::module::{
    file_to_module, Module, ModuleSearchPaths, ResolvedModule,
};
use crate::salsa_db::source::File;
use crate::salsa_db::{hir, source};
use crate::symbols::Dependency;
use crate::Name;

pub use self::module::resolve_module;

pub mod module;

#[salsa::tracked(jar=Jar)]
pub struct Symbol {
    #[id]
    #[returned_ref]
    pub name: Name,

    count: Count<Symbol>,
}

#[derive(Eq, PartialEq, Default)]
pub struct SymbolTable {
    symbols: FxHashMap<Name, Symbol>,
}

impl SymbolTable {
    fn insert(&mut self, name: Name, symbol: Symbol) {
        self.symbols.insert(name, symbol);
    }
}

impl std::fmt::Debug for SymbolTable {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        self.symbols.fmt(f)
    }
}

impl<Db> DebugWithDb<Db> for SymbolTable
where
    Db: AsSalsaDatabase + self::Db,
{
    fn fmt(&self, f: &mut Formatter<'_>, db: &Db) -> std::fmt::Result {
        let mut map = f.debug_map();

        for (name, symbol) in &self.symbols {
            map.entry(name, &symbol.debug(db));
        }
        map.finish()
    }
}

#[tracing::instrument(level = "debug", skip(db))]
#[salsa::tracked(jar=Jar)]
pub fn dependencies(db: &dyn Db, file: File) -> Arc<[ModuleName]> {
    struct DependenciesVisitor<'a> {
        db: &'a dyn Db,
        resolved_module: Option<ResolvedModule>,
        dependencies: Vec<ModuleName>,
    }

    // TODO support package imports
    impl PreorderVisitor<'_> for DependenciesVisitor<'_> {
        fn enter_node(&mut self, node: AnyNodeRef) -> TraversalSignal {
            // Don't traverse into expressions
            if node.is_expression() {
                return TraversalSignal::Skip;
            }

            TraversalSignal::Traverse
        }

        fn visit_stmt(&mut self, stmt: &Stmt) {
            match stmt {
                Stmt::Import(import) => {
                    for alias in &import.names {
                        self.dependencies.push(ModuleName::new(&alias.name));
                    }
                }

                Stmt::ImportFrom(from) => {
                    if let Some(level) = NonZeroU32::new(from.level) {
                        // FIXME how to handle dependencies if the current file isn't a module?
                        //  e.g. what if the current file is a jupyter notebook. We should still be able to resolve files somehow.
                        if let Some(resolved_module) = &self.resolved_module {
                            if let Some(dependency) = resolved_module.resolve_dependency(
                                self.db,
                                &Dependency::Relative {
                                    module: from
                                        .module
                                        .as_ref()
                                        .map(|module| ModuleName::new(module)),
                                    level,
                                },
                            ) {
                                self.dependencies.push(dependency);
                            };
                        }
                    } else {
                        let module = from.module.as_ref().unwrap();
                        self.dependencies.push(ModuleName::new(module));
                    }
                }
                _ => {}
            }
            preorder::walk_stmt(self, stmt);
        }
    }

    let parsed = source::parse(db.upcast(), file);

    let mut visitor = DependenciesVisitor {
        db,
        resolved_module: file_to_module(db, file),
        dependencies: Vec::new(),
    };

    // TODO change the visitor so that `visit_mod` accepts a `ModRef` node that we can construct from module.
    visitor.visit_body(&parsed.syntax().body);

    Arc::from(visitor.dependencies)

    // TODO we should extract the names of dependencies during parsing to avoid an extra traversal here.
}

#[tracing::instrument(level = "debug", skip(db))]
#[salsa::tracked(jar=Jar)]
pub fn symbol_table(db: &dyn Db, file_id: File) -> Arc<SymbolTable> {
    let module_body = module_body(db.upcast(), file_id);

    let module_ast = module_body.ast(db.upcast());
    let mut symbol_table = SymbolTable::default();

    for statement in module_ast.statements() {
        match statement {
            Statement::Assignment { targets, .. } => {
                for target in &module_ast[targets.clone()] {
                    if let Expression::Name(name) = target {
                        symbol_table.insert(
                            name.clone(),
                            Symbol::new(db, name.clone(), Count::default()),
                        );
                    }
                }
            }
            _ => {}
        }
    }

    Arc::new(symbol_table)
}

#[tracing::instrument(level = "debug", skip(db))]
#[salsa::tracked(jar=Jar)]
pub fn ast_ids(db: &dyn Db, file: File) -> Arc<AstIds> {
    let parsed = source::parse(db.upcast(), file);

    Arc::new(AstIds::from_module(parsed.syntax()))
}

#[salsa::jar(db=Db)]
pub struct Jar(
    ModuleSearchPaths,
    Symbol,
    Module,
    dependencies,
    symbol_table,
    ast_ids,
    resolve_module,
    file_to_module,
);

pub trait Db: hir::Db + salsa::DbWithJar<Jar> + Upcast<dyn hir::Db> {}
