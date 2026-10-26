use ruff_db::parsed::parsed_module;
use ruff_db::source::source_text;
use ruff_python_ast::visitor::{self, Visitor};
use ruff_python_ast::{self as ast, Expr, Stmt};
use ty_module_resolver::{
    ImportingFile, Module, ModuleName, SearchPath, resolve_module, resolve_real_shadowable_module,
};
use ty_python_core::{ProgramFile, semantic_index};

use crate::types::{
    KnownFunction, Type, TypeContext, binding_type, infer_same_file_expression_type,
};
use crate::{Db, HasType, SemanticModel};

/// The module an import refers to at runtime and during type checking.
#[derive(Debug, Clone, Copy, PartialEq, Eq, get_size2::GetSize, salsa::SalsaValue)]
pub struct ImportedModule<'db> {
    /// The runtime module, or the module selected for typing if runtime resolution failed.
    pub target: Module<'db>,
    /// A separate module selected for typing, for example a local helper's `.pyi` file.
    pub typing_target: Option<Module<'db>>,
}

impl<'db> ImportedModule<'db> {
    pub(super) fn resolve(
        db: &'db dyn Db,
        importing_file: ImportingFile<'db>,
        name: &ModuleName,
        typing: Option<Module<'db>>,
    ) -> Option<Self> {
        let target = resolve_real_shadowable_module(db, importing_file, name).or(typing)?;
        Some(Self {
            target,
            typing_target: typing.filter(|module| *module != target),
        })
    }
}

#[derive(Debug, Default, PartialEq, Eq, get_size2::GetSize, salsa::SalsaValue)]
pub struct ImportedModules<'db> {
    pub modules: Box<[ImportedModule<'db>]>,
    /// Imports that ty couldn't resolve but uv may still identify by name.
    pub unresolved: Box<[ModuleName]>,
    /// An unreadable file, syntax error, or unknown import target can hide uses of any package.
    pub incomplete: bool,
}

/// Collect imports from all scopes, including imports used only for typing.
#[salsa::tracked(returns(ref), heap_size=ruff_memory_usage::heap_size)]
pub fn imported_modules<'db>(db: &'db dyn Db, file: ProgramFile<'db>) -> ImportedModules<'db> {
    let parsed = parsed_module(db, file.python_file(db)).load_then_clear_on_drop(db);
    if parsed.has_syntax_errors() || source_text(db, file.file(db)).read_error().is_some() {
        return ImportedModules {
            incomplete: true,
            ..ImportedModules::default()
        };
    }

    let mut collector = ImportCollector {
        model: SemanticModel::new(db, file),
        importing_file: ImportingFile::File(file.file(db), file.resolver_environment(db)),
        modules: Vec::new(),
        unresolved: Vec::new(),
        incomplete: false,
    };
    collector.visit_body(&parsed.syntax().body);
    collector
        .modules
        .sort_unstable_by_key(|module| module.target.name(db));
    collector.modules.dedup();
    collector.unresolved.sort_unstable();
    collector.unresolved.dedup();
    ImportedModules {
        modules: collector.modules.into_boxed_slice(),
        unresolved: collector.unresolved.into_boxed_slice(),
        incomplete: collector.incomplete,
    }
}

struct ImportCollector<'db> {
    model: SemanticModel<'db>,
    importing_file: ImportingFile<'db>,
    modules: Vec<ImportedModule<'db>>,
    unresolved: Vec<ModuleName>,
    incomplete: bool,
}

impl<'db> ImportCollector<'db> {
    fn record_parents(&mut self, name: &ModuleName) -> Option<&'db SearchPath> {
        let mut nearest_parent = None;
        // Concrete parent packages run their initializers. A namespace has no initializer, and
        // counting all of its owners would credit unrelated distributions for a child import.
        for (parent, path) in concrete_parents(self.model.db(), self.importing_file, name) {
            nearest_parent.get_or_insert(path);
            self.modules.push(parent);
        }
        nearest_parent
    }

    fn record(&mut self, name: Option<ModuleName>) {
        let Some(name) = name else {
            self.incomplete = true;
            return;
        };
        let parent = self.record_parents(&name);
        if let Some(import) = resolve(self.model.db(), self.importing_file, &name) {
            self.modules.push(import);
        } else if can_import_installed_child(parent) {
            self.unresolved.push(name);
        }
    }

    fn import_from(&mut self, import: &ast::StmtImportFrom) {
        let db = self.model.db();
        let Ok(name) = ModuleName::from_import_statement(db, self.importing_file, import) else {
            self.incomplete = true;
            return;
        };
        let ancestor = self.record_parents(&name);
        let Some(parent) = resolve(db, self.importing_file, &name) else {
            if can_import_installed_child(ancestor) {
                // Without a Python module or stub, ty can't distinguish an attribute of the
                // parent from a child module supplied by another distribution.
                self.unresolved.extend(
                    import
                        .names
                        .iter()
                        .filter_map(|alias| child_name(&name, alias)),
                );
                self.unresolved.push(name);
            }
            return;
        };
        let path = parent.target.search_path(db);
        let allow_unresolved_child = can_import_installed_child(path.or(ancestor));
        let mut uses_parent = path.is_some();
        for alias in &import.names {
            if !self.record_child(parent, alias, allow_unresolved_child) {
                uses_parent = true;
            }
        }
        if uses_parent {
            self.modules.push(parent);
        }
    }

    fn record_child(
        &mut self,
        parent: ImportedModule<'db>,
        alias: &ast::Alias,
        allow_unresolved: bool,
    ) -> bool {
        let db = self.model.db();
        let module = parent.typing_target.unwrap_or(parent.target);
        if !module.kind(db).is_package() {
            return false;
        }
        let Some(name) = child_name(module.name(db), alias) else {
            return false;
        };
        let child = resolve(db, self.importing_file, &name);
        if child.is_none() && !allow_unresolved {
            return false;
        }
        let Some(definition) = semantic_index(db, self.model.program_file()).try_definition(alias)
        else {
            self.incomplete = true;
            return false;
        };
        // A package attribute can take precedence over a submodule of the same name.
        match (child, binding_type(db, definition)) {
            (Some(child), Type::ModuleLiteral(literal)) if literal.module(db).name(db) == &name => {
                self.modules.push(child);
                true
            }
            // ty may not see a native extension. uv can identify its exact owner or, for a
            // namespace package, fall back to the distributions that provide its parent.
            (None, Type::Dynamic(_) | Type::Divergent(_)) => {
                self.unresolved.push(name);
                true
            }
            // An `Any` export or a cyclic binding doesn't settle which module was used.
            (Some(child), Type::Dynamic(_) | Type::Divergent(_)) => {
                self.modules.push(child);
                false
            }
            _ => false,
        }
    }

    fn import_call(&mut self, call: &ast::ExprCall) {
        let arguments = &call.arguments;
        let name = arguments.find_argument_value("name", 0);
        let unpacked = arguments.args.iter().any(Expr::is_starred_expr)
            || arguments
                .keywords
                .iter()
                .any(|keyword| keyword.arg.is_none());
        if (name.is_none() && !unpacked)
            || name.is_some_and(|name| name.is_literal_expr() && !name.is_string_literal_expr())
        {
            return;
        }
        match self.known_function(&call.func) {
            Some(KnownFunction::ImportModule) => {}
            Some(KnownFunction::DunderImport) => {
                let absolute = match arguments.find_argument_value("level", 4) {
                    None => !unpacked,
                    Some(Expr::NumberLiteral(ast::ExprNumberLiteral {
                        value: ast::Number::Int(level),
                        ..
                    })) => *level == 0,
                    Some(Expr::BooleanLiteral(level)) => !level.value,
                    _ => false,
                };
                if !absolute {
                    self.incomplete = true;
                    return;
                }
                // A nonempty fromlist can also import children of the named package.
                self.incomplete |= match arguments.find_argument_value("fromlist", 3) {
                    None => unpacked,
                    Some(Expr::NoneLiteral(_)) => false,
                    Some(Expr::List(list)) => !list.elts.is_empty(),
                    Some(Expr::Tuple(tuple)) => !tuple.elts.is_empty(),
                    _ => true,
                };
            }
            _ => return,
        }
        self.record(
            name.and_then(Expr::as_string_literal_expr)
                .and_then(|literal| ModuleName::new(literal.value.to_str())),
        );
    }

    fn known_function(&self, callee: &Expr) -> Option<KnownFunction> {
        // Import functions can be aliased or re-exported. Infer just the callee when it has its
        // own query, so ordinary calls needn't trigger inference for their entire scope.
        let db = self.model.db();
        let ty = if let Some(expr) =
            semantic_index(db, self.model.program_file()).try_expression(callee)
        {
            infer_same_file_expression_type(db, expr, TypeContext::default())
        } else {
            callee.inferred_type(&self.model)?
        };
        ty.as_function_literal()?.known(db)
    }
}

fn resolve<'db>(
    db: &'db dyn Db,
    importing_file: ImportingFile<'db>,
    name: &ModuleName,
) -> Option<ImportedModule<'db>> {
    let typing = resolve_module(db, importing_file, name);
    ImportedModule::resolve(db, importing_file, name, typing)
}

pub(super) fn concrete_parents<'db>(
    db: &'db dyn Db,
    importing_file: ImportingFile<'db>,
    name: &ModuleName,
) -> impl Iterator<Item = (ImportedModule<'db>, &'db SearchPath)> {
    name.ancestors().skip(1).filter_map(move |ancestor| {
        let parent = resolve(db, importing_file, &ancestor)?;
        Some((parent, parent.target.search_path(db)?))
    })
}

pub(super) fn can_import_installed_child(parent: Option<&SearchPath>) -> bool {
    // If a local `requests` wins, `import requests.missing` cannot use an installed
    // distribution's `requests.missing` module.
    parent.is_none_or(|path| path.is_site_packages() || path.is_editable())
}

fn child_name(parent: &ModuleName, alias: &ast::Alias) -> Option<ModuleName> {
    let mut name = parent.clone();
    name.extend(&ModuleName::new(&alias.name)?);
    Some(name)
}

impl<'ast> Visitor<'ast> for ImportCollector<'_> {
    fn visit_stmt(&mut self, stmt: &'ast Stmt) {
        match stmt {
            Stmt::Import(import) => {
                for alias in &import.names {
                    self.record(ModuleName::new(&alias.name));
                }
            }
            Stmt::ImportFrom(import) => self.import_from(import),
            _ => visitor::walk_stmt(self, stmt),
        }
    }

    fn visit_expr(&mut self, expr: &'ast Expr) {
        if let Expr::Call(call) = expr {
            self.import_call(call);
        }
        visitor::walk_expr(self, expr);
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use anyhow::Context;
    use ruff_db::files::system_path_to_file;
    use ruff_db::system::DbWithWritableSystem as _;
    use ty_module_resolver::ModuleName;

    use crate::Db as _;
    use crate::db::tests::TestDbBuilder;
    use crate::dependency::DependencyMetadata;

    use super::imported_modules;

    #[test]
    fn import_targets_follow_bindings_across_scopes() -> anyhow::Result<()> {
        let db = TestDbBuilder::new()
            .with_third_party_packages()
            .with_file(
                "/.venv/lib/python3.13/site-packages/ext/__init__.py",
                "from typing import Any\nmaybe: Any\n",
            )
            .with_file("/.venv/lib/python3.13/site-packages/ext/maybe.py", "")
            .with_file("/src/pkg/__init__.py", "import other as child\n")
            .with_file("/src/pkg/child.py", "")
            .with_file("/src/pkg/ns/present.py", "")
            .with_file("/src/ns/sub.py", "")
            .with_file("/src/other.py", "")
            .with_file(
                "/src/pkg/main.py",
                concat!(
                    "from typing import TYPE_CHECKING\n",
                    "if TYPE_CHECKING:\n    from ns import sub\n",
                    "def load():\n    from . import child\n",
                    "import pkg.missing\nfrom pkg import missing\n",
                    "from pkg.ns import missing\n",
                    "from ns import native_child\nfrom ext import native_child\n",
                    "from ext import maybe\n",
                    "from native import child\n",
                ),
            )
            .build()?;
        let file = system_path_to_file(&db, "/src/pkg/main.py")?;
        let imports = imported_modules(&db, db.program_file(file));
        assert!(!imports.incomplete);
        let names: Vec<_> = imports
            .modules
            .iter()
            .map(|import| import.target.name(&db).as_str())
            .collect();
        assert_eq!(
            names,
            ["ext", "ext.maybe", "ns.sub", "pkg", "pkg.ns", "typing"]
        );
        assert_eq!(
            imports
                .unresolved
                .iter()
                .map(ModuleName::as_str)
                .collect::<Vec<_>>(),
            [
                "ext.native_child",
                "native",
                "native.child",
                "ns.native_child"
            ]
        );
        let shadowed_name = ModuleName::new_static("pkg.ns").context("valid module name")?;
        let metadata = DependencyMetadata {
            distributions: BTreeMap::new(),
            projects: Box::default(),
            module_owners: [(shadowed_name, Box::from(["installed".into()]))].into(),
        };
        assert!(imports.modules.iter().all(|import| {
            metadata
                .import_owners(&db, db.program_file(file), import.target)
                .is_empty()
        }));
        Ok(())
    }

    #[test]
    fn dynamic_imports_distinguish_literal_and_unknown_names() -> anyhow::Result<()> {
        let mut db = TestDbBuilder::new()
            .with_file("/src/helper.py", "")
            .with_file("/src/other.py", "")
            .with_file(
                "/src/loader.py",
                "from importlib import import_module as load\n",
            )
            .with_file(
                "/src/main.py",
                concat!(
                    "from loader import load\n",
                    "load(name='helper', package=__package__)\n",
                    "__import__('other', globals(), locals(), [], 0)\n",
                    "def import_module(name): pass\nimport_module(unknown_name)\n",
                ),
            )
            .build()?;
        let file = system_path_to_file(&db, "/src/main.py")?;
        let imports = imported_modules(&db, db.program_file(file));
        assert!(!imports.incomplete);
        assert_eq!(
            imports
                .modules
                .iter()
                .map(|import| import.target.name(&db).as_str())
                .collect::<Vec<_>>(),
            ["helper", "loader", "other"]
        );

        db.write_file("/src/main.py", "__import__('other', fromlist=children)\n")?;
        let imports = imported_modules(&db, db.program_file(file));
        assert!(imports.incomplete);
        assert_eq!(imports.modules[0].target.name(&db), "other");

        db.write_file("/src/main.py", "__import__(unknown_name)\n")?;
        assert!(imported_modules(&db, db.program_file(file)).incomplete);
        Ok(())
    }
}
