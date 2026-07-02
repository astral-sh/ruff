//! A visitor and query to find all global-scope symbols that are exported from a module
//! when a wildcard import is used.
//!
//! For example, if a module `foo` contains `from bar import *`, which symbols from the global
//! scope of `bar` are imported into the global namespace of `foo`?
//!
//! ## Why is this a separate query rather than a part of semantic indexing?
//!
//! This query is called by the [`super::SemanticIndexBuilder`] in order to add the correct
//! [`super::Definition`]s to the semantic index of a module `foo` if `foo` has a
//! `from bar import *` statement in its global namespace. Adding the correct `Definition`s to
//! `foo`'s [`super::SemanticIndex`] requires knowing which symbols are exported from `bar`.
//!
//! If we determined the set of exported names during semantic indexing rather than as a
//! separate query, we would need to complete semantic indexing on `bar` in order to
//! complete analysis of the global namespace of `foo`. Since semantic indexing is somewhat
//! expensive, this would be undesirable. A separate query allows us to avoid this issue.
//!
//! An additional concern is that the recursive nature of this query means that it must be able
//! to handle cycles. We do this using fixpoint iteration; adding fixpoint iteration to the
//! whole [`super::semantic_index()`] query would probably be prohibitively expensive.

use ruff_db::{files::File, parsed::parsed_module};
use ruff_python_ast::{
    self as ast,
    name::{Name, UnqualifiedName},
    visitor::{Visitor, walk_expr, walk_pattern, walk_stmt},
};
use rustc_hash::{FxHashMap, FxHashSet};
use ty_module_resolver::{ModuleName, resolve_module};

use crate::Db;

#[salsa::tracked(
    returns(deref),
    cycle_initial=|_, _, _| Box::default(),
    heap_size=ruff_memory_usage::heap_size)
]
pub(super) fn exported_names(db: &dyn Db, file: File) -> Box<[Name]> {
    let module = parsed_module(db, file).load(db);
    let mut finder = ExportFinder::new(db, file);
    finder.visit_body(module.suite());

    let mut exports = finder.resolve_exports();

    // Sort the exports to ensure convergence regardless of hash map
    // or insertion order. See <https://github.com/astral-sh/ty/issues/444>
    exports.sort_unstable();
    exports.into()
}

/// Syntax-only analysis of a module's `__all__`.
#[derive(Clone, Debug, Default, PartialEq, Eq, get_size2::GetSize)]
pub struct DunderAllNames {
    /// An over-approximation used by IDE features that prefer extra suggestions over omissions.
    possible: Option<FxHashSet<Name>>,
    /// Names known to be in `__all__`, used when false positives would be incorrect.
    definite: Box<[Name]>,
}

impl DunderAllNames {
    pub fn possible(&self) -> Option<&FxHashSet<Name>> {
        self.possible.as_ref()
    }

    pub fn is_definitely_exported(&self, name: &str) -> bool {
        self.definite.iter().any(|export| export == name)
    }
}

/// Returns syntax-only `__all__` information without depending on type inference.
#[salsa::tracked(
    returns(ref),
    cycle_initial=|_, _, _| DunderAllNames::default(),
    heap_size=ruff_memory_usage::heap_size,
)]
pub fn syntactic_dunder_all_names(db: &dyn Db, file: File) -> DunderAllNames {
    let module = parsed_module(db, file).load(db);
    let mut possible = PossibleDunderAllNames::new(db, file);
    possible.visit_body(module.suite());

    DunderAllNames {
        possible: possible.into_names(),
        definite: definitely_exported_dunder_all_names(module.suite()),
    }
}

fn definitely_exported_dunder_all_names(statements: &[ast::Stmt]) -> Box<[Name]> {
    for statement in statements.iter().rev() {
        if let Some(value) = dunder_all_assignment_value(statement) {
            return dunder_all_names_from_value(value).unwrap_or_default();
        }

        let mut modification_finder = DunderAllModificationFinder::default();
        modification_finder.visit_stmt(statement);
        if modification_finder.found {
            return Box::default();
        }
    }

    Box::default()
}

struct PossibleDunderAllNames<'db, 'ast> {
    db: &'db dyn Db,
    file: File,
    origin: Option<DunderAllOrigin>,
    names: FxHashSet<Name>,
    invalid: bool,
    imports: Imports<'ast>,
}

impl<'db> PossibleDunderAllNames<'db, '_> {
    fn new(db: &'db dyn Db, file: File) -> Self {
        Self {
            db,
            file,
            origin: None,
            names: FxHashSet::default(),
            invalid: false,
            imports: Imports::default(),
        }
    }

    fn into_names(mut self) -> Option<FxHashSet<Name>> {
        if self.origin.is_none() || self.invalid {
            return None;
        }
        self.names.shrink_to_fit();
        Some(self.names)
    }

    fn update_from_assignment(&mut self, targets: &[ast::Expr], value: Option<&ast::Expr>) {
        let Some(target) = targets.first() else {
            return;
        };
        if !is_dunder_all(target) {
            return;
        }

        let Some(value) = value else {
            return;
        };
        let Some(names) = dunder_all_names_from_value(value) else {
            self.invalid = true;
            return;
        };

        self.update_origin(DunderAllOrigin::CurrentModule);
        self.names.extend(names);
    }

    fn extend(&mut self, expression: &ast::Expr) -> bool {
        match expression {
            ast::Expr::List(ast::ExprList { elts, .. })
            | ast::Expr::Tuple(ast::ExprTuple { elts, .. })
            | ast::Expr::Set(ast::ExprSet { elts, .. }) => self.add_names(elts),
            ast::Expr::Attribute(_) => {
                let Some(unqualified) = UnqualifiedName::from_expr(expression) else {
                    return false;
                };
                let Some((&attr, rest)) = unqualified.segments().split_last() else {
                    return false;
                };
                if attr != "__all__" {
                    return false;
                }
                let module_name = Name::new(rest.join("."));
                let Some(names) = self
                    .imports
                    .get_module_dunder_all(self.db, self.file, &module_name)
                    .and_then(DunderAllNames::possible)
                else {
                    return false;
                };
                self.names.extend(names.iter().cloned());
                true
            }
            _ => false,
        }
    }

    fn update_by_call(&mut self, name: &ast::Identifier, arguments: &ast::Arguments) -> bool {
        if arguments.len() != 1 {
            return false;
        }
        let Some(argument) = arguments.find_positional(0) else {
            return false;
        };
        match name.as_str() {
            "extend" => self.extend(argument),
            "append" => {
                let Some(name) = dunder_all_name(argument) else {
                    return false;
                };
                self.names.insert(name);
                true
            }
            "remove" => {
                let Some(name) = dunder_all_name(argument) else {
                    return false;
                };
                self.names.remove(&name);
                true
            }
            _ => false,
        }
    }

    fn add_all_from_star_import(&mut self, import_from: &ast::StmtImportFrom) {
        let Some(names) = self
            .dunder_all_for_import_from(import_from)
            .and_then(DunderAllNames::possible)
        else {
            self.invalid = true;
            return;
        };
        if names.contains("__all__") {
            self.update_origin(DunderAllOrigin::StarImport);
            self.names.extend(names.iter().cloned());
        }
    }

    fn add_all_from_import(&mut self, import_from: &ast::StmtImportFrom) {
        let Some(names) = self
            .dunder_all_for_import_from(import_from)
            .and_then(DunderAllNames::possible)
        else {
            self.invalid = true;
            return;
        };
        self.update_origin(DunderAllOrigin::ExternalModule);
        self.names.extend(names.iter().cloned());
    }

    fn dunder_all_for_import_from(
        &self,
        import_from: &ast::StmtImportFrom,
    ) -> Option<&'db DunderAllNames> {
        let module_name =
            ModuleName::from_import_statement(self.db, self.file, import_from).ok()?;
        let module = resolve_module(self.db, self.file, &module_name)?;
        Some(syntactic_dunder_all_names(self.db, module.file(self.db)?))
    }

    fn add_names(&mut self, expressions: &[ast::Expr]) -> bool {
        for expression in expressions {
            let Some(name) = dunder_all_name(expression) else {
                return false;
            };
            self.names.insert(name);
        }
        true
    }

    fn update_origin(&mut self, origin: DunderAllOrigin) {
        // The possible-name policy intentionally unions conditional assignments to avoid omitting
        // auto-import suggestions when we cannot evaluate control flow without type inference.
        if !(matches!(self.origin, Some(DunderAllOrigin::CurrentModule))
            && matches!(origin, DunderAllOrigin::CurrentModule))
        {
            self.names.clear();
        }
        self.origin = Some(origin);
    }
}

impl<'ast> Visitor<'ast> for PossibleDunderAllNames<'_, 'ast> {
    fn visit_stmt(&mut self, statement: &'ast ast::Stmt) {
        match statement {
            ast::Stmt::FunctionDef(_) | ast::Stmt::ClassDef(_) => {}
            ast::Stmt::Assign(ast::StmtAssign { targets, value, .. }) => {
                self.update_from_assignment(targets, Some(value));
            }
            ast::Stmt::AnnAssign(ast::StmtAnnAssign { target, value, .. }) => {
                self.update_from_assignment(std::slice::from_ref(target), value.as_deref());
            }
            ast::Stmt::AugAssign(ast::StmtAugAssign {
                target, op, value, ..
            }) => {
                if self.origin.is_some() && is_dunder_all(target) {
                    if !matches!(op, ast::Operator::Add) || !self.extend(value) {
                        self.invalid = true;
                    }
                }
            }
            ast::Stmt::Expr(ast::StmtExpr { value, .. }) => {
                if self.origin.is_none() {
                    return;
                }
                let Some(ast::ExprCall {
                    func, arguments, ..
                }) = value.as_call_expr()
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
                if is_dunder_all(value) && !self.update_by_call(attr, arguments) {
                    self.invalid = true;
                }
            }
            ast::Stmt::Import(import) => self.imports.add_import(import),
            ast::Stmt::ImportFrom(import_from) => {
                self.imports.add_import_from(import_from);
                for alias in &import_from.names {
                    if &alias.name == "*" {
                        self.add_all_from_star_import(import_from);
                    } else if &alias.name == "__all__"
                        && alias
                            .asname
                            .as_ref()
                            .is_none_or(|asname| asname == "__all__")
                    {
                        self.add_all_from_import(import_from);
                    }
                }
            }
            _ => walk_stmt(self, statement),
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum DunderAllOrigin {
    CurrentModule,
    ExternalModule,
    StarImport,
}

/// Tracks module-scope imports used by syntax-only `__all__` analysis.
///
/// This recognizes idioms such as `__all__ += submodule.__all__` without type inference. The
/// approach is deliberately approximate because it does not track arbitrary redefinitions:
///
/// ```python
/// import numpy as np
/// from importlib import resources
/// np = resources
/// __all__ = []
/// __all__ += np.__all__
/// ```
///
/// Here, `np` is still resolved to `numpy`. Imports in `from` statements are treated as possible
/// modules and resolved on demand; unresolved candidates make `__all__` invalid.
///
/// Module names are stored as their constituent AST pieces because only a small fraction of
/// imports are later used to update `__all__`.
#[derive(Clone, Debug, Default)]
struct Imports<'ast> {
    module_names: FxHashMap<&'ast str, ImportModuleKind<'ast>>,
}

impl<'ast> Imports<'ast> {
    fn add_import(&mut self, import: &'ast ast::StmtImport) {
        for alias in &import.names {
            let asname = alias
                .asname
                .as_ref()
                .map(|identifier| &identifier.id)
                .unwrap_or(&alias.name.id);
            self.module_names.insert(
                asname,
                ImportModuleKind::Definitive(ImportModuleName::Import(&alias.name.id)),
            );
        }
    }

    fn add_import_from(&mut self, import_from: &'ast ast::StmtImportFrom) {
        for alias in &import_from.names {
            if &alias.name == "*" {
                continue;
            }
            let asname = alias
                .asname
                .as_ref()
                .map(|identifier| &identifier.id)
                .unwrap_or(&alias.name.id);
            self.module_names.insert(
                asname,
                ImportModuleKind::Possible(ImportModuleName::ImportFrom {
                    parent: import_from,
                    child: &alias.name.id,
                }),
            );
        }
    }

    fn get_module_dunder_all<'db>(
        &self,
        db: &'db dyn Db,
        importing_file: File,
        name: &Name,
    ) -> Option<&'db DunderAllNames> {
        let module_name = match self.module_names.get(name.as_str())? {
            ImportModuleKind::Definitive(name) | ImportModuleKind::Possible(name) => {
                name.to_module_name(db, importing_file)?
            }
        };
        let module = resolve_module(db, importing_file, &module_name)?;
        Some(syntactic_dunder_all_names(db, module.file(db)?))
    }
}

/// Whether an import is definitely a module (`import foo`) or possibly one
/// (`from package import foo`).
#[derive(Debug, Clone, Copy)]
enum ImportModuleKind<'ast> {
    Definitive(ImportModuleName<'ast>),
    Possible(ImportModuleName<'ast>),
}

/// Lazily materialized pieces of a module name referenced by an import.
#[derive(Debug, Clone, Copy)]
enum ImportModuleName<'ast> {
    Import(&'ast Name),
    ImportFrom {
        parent: &'ast ast::StmtImportFrom,
        child: &'ast Name,
    },
}

impl ImportModuleName<'_> {
    fn to_module_name(self, db: &dyn Db, importing_file: File) -> Option<ModuleName> {
        match self {
            ImportModuleName::Import(name) => ModuleName::new(name),
            ImportModuleName::ImportFrom { parent, child } => {
                let mut module_name =
                    ModuleName::from_import_statement(db, importing_file, parent).ok()?;
                module_name.extend(&ModuleName::new(child)?);
                Some(module_name)
            }
        }
    }
}

fn dunder_all_assignment_value(statement: &ast::Stmt) -> Option<&ast::Expr> {
    match statement {
        ast::Stmt::Assign(ast::StmtAssign { targets, value, .. }) => {
            let [target] = targets.as_slice() else {
                return None;
            };
            is_dunder_all(target).then_some(value)
        }
        ast::Stmt::AnnAssign(ast::StmtAnnAssign {
            target,
            value: Some(value),
            ..
        }) => is_dunder_all(target).then_some(value),
        _ => None,
    }
}

fn dunder_all_names_from_value(value: &ast::Expr) -> Option<Box<[Name]>> {
    let (ast::Expr::List(ast::ExprList { elts: elements, .. })
    | ast::Expr::Tuple(ast::ExprTuple { elts: elements, .. })) = value
    else {
        return None;
    };

    elements
        .iter()
        .map(dunder_all_name)
        .collect::<Option<Vec<_>>>()
        .map(Vec::into_boxed_slice)
}

fn dunder_all_name(expression: &ast::Expr) -> Option<Name> {
    Some(Name::new(
        expression.as_string_literal_expr()?.value.to_str(),
    ))
}

fn is_dunder_all(expression: &ast::Expr) -> bool {
    matches!(expression, ast::Expr::Name(ast::ExprName { id, .. }) if id == "__all__")
}

#[derive(Default)]
struct DunderAllModificationFinder {
    found: bool,
}

impl<'db> Visitor<'db> for DunderAllModificationFinder {
    fn visit_stmt(&mut self, statement: &'db ast::Stmt) {
        if self.found {
            return;
        }

        match statement {
            ast::Stmt::FunctionDef(ast::StmtFunctionDef { name, .. })
            | ast::Stmt::ClassDef(ast::StmtClassDef { name, .. }) => {
                self.found = name == "__all__";
            }
            ast::Stmt::Import(ast::StmtImport { names, .. }) => {
                self.found = names.iter().any(|alias| {
                    alias.asname.as_ref().map_or_else(
                        || alias.name.id.split('.').next(),
                        |name| Some(name.as_str()),
                    ) == Some("__all__")
                });
            }
            ast::Stmt::ImportFrom(ast::StmtImportFrom { names, .. }) => {
                self.found = names.iter().any(|alias| {
                    &alias.name == "*" || alias.asname.as_ref().unwrap_or(&alias.name) == "__all__"
                });
            }
            _ => walk_stmt(self, statement),
        }
    }

    fn visit_expr(&mut self, expression: &'db ast::Expr) {
        if self.found {
            return;
        }

        match expression {
            ast::Expr::Name(ast::ExprName { id, ctx, .. }) if id == "__all__" && !ctx.is_load() => {
                self.found = true;
            }
            ast::Expr::Call(ast::ExprCall { func, .. })
                if func
                    .as_attribute_expr()
                    .is_some_and(|attribute| is_dunder_all(&attribute.value)) =>
            {
                self.found = true;
            }
            _ => walk_expr(self, expression),
        }
    }
}

struct ExportFinder<'db> {
    db: &'db dyn Db,
    file: File,
    visiting_stub_file: bool,
    exports: FxHashMap<&'db Name, PossibleExportKind>,
    dunder_all: DunderAll,
}

impl<'db> ExportFinder<'db> {
    fn new(db: &'db dyn Db, file: File) -> Self {
        Self {
            db,
            file,
            visiting_stub_file: file.is_stub(db),
            exports: FxHashMap::default(),
            dunder_all: DunderAll::NotPresent,
        }
    }

    fn possibly_add_export(&mut self, export: &'db Name, kind: PossibleExportKind) {
        self.exports.insert(export, kind);

        if export == "__all__" {
            self.dunder_all = DunderAll::Present;
        }
    }

    fn resolve_exports(self) -> Vec<Name> {
        match self.dunder_all {
            DunderAll::NotPresent => self
                .exports
                .into_iter()
                .filter_map(|(name, kind)| {
                    if kind == PossibleExportKind::StubImportWithoutRedundantAlias {
                        return None;
                    }
                    if name.starts_with('_') {
                        return None;
                    }
                    Some(name.clone())
                })
                .collect(),
            DunderAll::Present => self.exports.into_keys().cloned().collect(),
        }
    }
}

impl<'db> Visitor<'db> for ExportFinder<'db> {
    fn visit_alias(&mut self, alias: &'db ast::Alias) {
        let ast::Alias {
            name,
            asname,
            range: _,
            node_index: _,
        } = alias;

        let name = &name.id;
        let asname = asname.as_ref().map(|asname| &asname.id);

        // If the source is a stub, names defined by imports are only exported
        // if they use the explicit `foo as foo` syntax:
        let kind = if self.visiting_stub_file && asname.is_none_or(|asname| asname != name) {
            PossibleExportKind::StubImportWithoutRedundantAlias
        } else {
            PossibleExportKind::Normal
        };

        self.possibly_add_export(asname.unwrap_or(name), kind);
    }

    fn visit_pattern(&mut self, pattern: &'db ast::Pattern) {
        match pattern {
            ast::Pattern::MatchAs(ast::PatternMatchAs {
                pattern,
                name,
                range: _,
                node_index: _,
            }) => {
                if let Some(pattern) = pattern {
                    self.visit_pattern(pattern);
                }
                if let Some(name) = name {
                    // Wildcard patterns (`case _:`) do not bind names.
                    // Currently `self.possibly_add_export()` just ignores
                    // all names with leading underscores, but this will not always be the case
                    // (in the future we will want to support modules with `__all__ = ['_']`).
                    if name != "_" {
                        self.possibly_add_export(&name.id, PossibleExportKind::Normal);
                    }
                }
            }
            ast::Pattern::MatchMapping(ast::PatternMatchMapping {
                patterns,
                rest,
                keys: _,
                range: _,
                node_index: _,
            }) => {
                for pattern in patterns {
                    self.visit_pattern(pattern);
                }
                if let Some(rest) = rest {
                    self.possibly_add_export(&rest.id, PossibleExportKind::Normal);
                }
            }
            ast::Pattern::MatchStar(ast::PatternMatchStar {
                name,
                range: _,
                node_index: _,
            }) => {
                if let Some(name) = name {
                    self.possibly_add_export(&name.id, PossibleExportKind::Normal);
                }
            }
            ast::Pattern::MatchSequence(_)
            | ast::Pattern::MatchOr(_)
            | ast::Pattern::MatchClass(_) => {
                walk_pattern(self, pattern);
            }
            ast::Pattern::MatchSingleton(_) | ast::Pattern::MatchValue(_) => {}
        }
    }

    fn visit_stmt(&mut self, stmt: &'db ast::Stmt) {
        match stmt {
            ast::Stmt::ClassDef(ast::StmtClassDef {
                name,
                decorator_list,
                arguments,
                type_params: _, // We don't want to visit the type params of the class
                body: _,        // We don't want to visit the body of the class
                range: _,
                node_index: _,
            }) => {
                self.possibly_add_export(&name.id, PossibleExportKind::Normal);
                for decorator in decorator_list {
                    self.visit_decorator(decorator);
                }
                if let Some(arguments) = arguments {
                    self.visit_arguments(arguments);
                }
            }

            ast::Stmt::FunctionDef(ast::StmtFunctionDef {
                name,
                decorator_list,
                parameters,
                returns,
                type_params: _, // We don't want to visit the type params of the function
                body: _,        // We don't want to visit the body of the function
                range: _,
                node_index: _,
                is_async: _,
            }) => {
                self.possibly_add_export(&name.id, PossibleExportKind::Normal);
                for decorator in decorator_list {
                    self.visit_decorator(decorator);
                }
                self.visit_parameters(parameters);
                if let Some(returns) = returns {
                    self.visit_expr(returns);
                }
            }

            ast::Stmt::AnnAssign(ast::StmtAnnAssign {
                target,
                value,
                annotation,
                simple: _,
                range: _,
                node_index: _,
            }) => {
                if value.is_some() || self.visiting_stub_file {
                    self.visit_expr(target);
                }
                self.visit_expr(annotation);
                if let Some(value) = value {
                    self.visit_expr(value);
                }
            }

            ast::Stmt::TypeAlias(ast::StmtTypeAlias {
                name,
                type_params: _,
                value: _,
                range: _,
                node_index: _,
            }) => {
                self.visit_expr(name);
                // Neither walrus expressions nor statements cannot appear in type aliases;
                // no need to recursively visit the `value` or `type_params`
            }

            ast::Stmt::ImportFrom(node) => {
                let mut found_star = false;
                for name in &node.names {
                    if &name.name.id == "*" {
                        if !found_star {
                            found_star = true;
                            for export in
                                ModuleName::from_import_statement(self.db, self.file, node)
                                    .ok()
                                    .and_then(|module_name| {
                                        resolve_module(self.db, self.file, &module_name)
                                    })
                                    .iter()
                                    .flat_map(|module| {
                                        module
                                            .file(self.db)
                                            .map(|file| exported_names(self.db, file))
                                            .unwrap_or_default()
                                    })
                            {
                                self.possibly_add_export(export, PossibleExportKind::Normal);
                            }
                        }
                    } else {
                        self.visit_alias(name);
                    }
                }
            }

            ast::Stmt::Import(_)
            | ast::Stmt::AugAssign(_)
            | ast::Stmt::While(_)
            | ast::Stmt::If(_)
            | ast::Stmt::With(_)
            | ast::Stmt::Assert(_)
            | ast::Stmt::Try(_)
            | ast::Stmt::Expr(_)
            | ast::Stmt::For(_)
            | ast::Stmt::Assign(_)
            | ast::Stmt::Match(_) => walk_stmt(self, stmt),

            ast::Stmt::Global(_)
            | ast::Stmt::Raise(_)
            | ast::Stmt::Return(_)
            | ast::Stmt::Break(_)
            | ast::Stmt::Continue(_)
            | ast::Stmt::IpyEscapeCommand(_)
            | ast::Stmt::Delete(_)
            | ast::Stmt::Nonlocal(_)
            | ast::Stmt::Pass(_) => {}
        }
    }

    fn visit_expr(&mut self, expr: &'db ast::Expr) {
        match expr {
            ast::Expr::Name(ast::ExprName {
                id,
                ctx,
                range: _,
                node_index: _,
            }) => {
                if ctx.is_store() {
                    self.possibly_add_export(id, PossibleExportKind::Normal);
                }
            }

            ast::Expr::Lambda(_)
            | ast::Expr::BooleanLiteral(_)
            | ast::Expr::NoneLiteral(_)
            | ast::Expr::NumberLiteral(_)
            | ast::Expr::BytesLiteral(_)
            | ast::Expr::EllipsisLiteral(_)
            | ast::Expr::StringLiteral(_) => {}

            // Walrus definitions "leak" from comprehension scopes into the comprehension's
            // enclosing scope; they thus need special handling
            ast::Expr::SetComp(_)
            | ast::Expr::ListComp(_)
            | ast::Expr::Generator(_)
            | ast::Expr::DictComp(_) => {
                let mut walrus_finder = WalrusFinder {
                    export_finder: self,
                };
                walk_expr(&mut walrus_finder, expr);
            }

            ast::Expr::BoolOp(_)
            | ast::Expr::Named(_)
            | ast::Expr::BinOp(_)
            | ast::Expr::UnaryOp(_)
            | ast::Expr::If(_)
            | ast::Expr::Attribute(_)
            | ast::Expr::Subscript(_)
            | ast::Expr::Starred(_)
            | ast::Expr::Call(_)
            | ast::Expr::Compare(_)
            | ast::Expr::Yield(_)
            | ast::Expr::YieldFrom(_)
            | ast::Expr::FString(_)
            | ast::Expr::TString(_)
            | ast::Expr::Tuple(_)
            | ast::Expr::List(_)
            | ast::Expr::Slice(_)
            | ast::Expr::IpyEscapeCommand(_)
            | ast::Expr::Dict(_)
            | ast::Expr::Set(_)
            | ast::Expr::Await(_) => walk_expr(self, expr),
        }
    }
}

struct WalrusFinder<'a, 'db> {
    export_finder: &'a mut ExportFinder<'db>,
}

impl<'db> Visitor<'db> for WalrusFinder<'_, 'db> {
    fn visit_expr(&mut self, expr: &'db ast::Expr) {
        match expr {
            // It's important for us to short-circuit here for lambdas specifically,
            // as walruses cannot leak out of the body of a lambda function.
            ast::Expr::Lambda(_)
            | ast::Expr::BooleanLiteral(_)
            | ast::Expr::NoneLiteral(_)
            | ast::Expr::NumberLiteral(_)
            | ast::Expr::BytesLiteral(_)
            | ast::Expr::EllipsisLiteral(_)
            | ast::Expr::StringLiteral(_)
            | ast::Expr::Name(_) => {}

            ast::Expr::Named(ast::ExprNamed {
                target,
                value: _,
                range: _,
                node_index: _,
            }) => {
                if let ast::Expr::Name(ast::ExprName {
                    id,
                    ctx: ast::ExprContext::Store,
                    range: _,
                    node_index: _,
                }) = &**target
                {
                    self.export_finder
                        .possibly_add_export(id, PossibleExportKind::Normal);
                }
            }

            // We must recurse inside nested comprehensions,
            // as even a walrus inside a comprehension inside a comprehension in the global scope
            // will leak out into the global scope
            ast::Expr::DictComp(_)
            | ast::Expr::SetComp(_)
            | ast::Expr::ListComp(_)
            | ast::Expr::Generator(_)
            | ast::Expr::BoolOp(_)
            | ast::Expr::BinOp(_)
            | ast::Expr::UnaryOp(_)
            | ast::Expr::If(_)
            | ast::Expr::Attribute(_)
            | ast::Expr::Subscript(_)
            | ast::Expr::Starred(_)
            | ast::Expr::Call(_)
            | ast::Expr::Compare(_)
            | ast::Expr::Yield(_)
            | ast::Expr::YieldFrom(_)
            | ast::Expr::FString(_)
            | ast::Expr::TString(_)
            | ast::Expr::Tuple(_)
            | ast::Expr::List(_)
            | ast::Expr::Slice(_)
            | ast::Expr::IpyEscapeCommand(_)
            | ast::Expr::Dict(_)
            | ast::Expr::Set(_)
            | ast::Expr::Await(_) => walk_expr(self, expr),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PossibleExportKind {
    Normal,
    StubImportWithoutRedundantAlias,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DunderAll {
    NotPresent,
    Present,
}
