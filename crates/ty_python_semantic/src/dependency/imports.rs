use ruff_db::parsed::parsed_module;
use ruff_db::source::source_text;
use ruff_python_ast as ast;
use ruff_python_ast::visitor::{Visitor, walk_expr, walk_stmt};
use ty_module_resolver::{ImportingFile, Module, ModuleName, resolve_module};
use ty_python_core::{ProgramFile, SemanticIndex, semantic_index};

use crate::types::{KnownFunction, Type, infer_definition_types};
use crate::{Db, FxIndexSet, HasType, SemanticModel};

/// The modules imported anywhere in one file, including imports used only for typing.
#[derive(Debug, Default, PartialEq, Eq, get_size2::GetSize, salsa::SalsaValue)]
pub struct ImportedModules<'db> {
    pub modules: Box<[Module<'db>]>,
    /// Imports whose names are known but whose modules could not be resolved.
    pub unresolved: Box<[ModuleName]>,
    /// Whether an unreadable file, invalid syntax, or dynamic import prevents complete analysis.
    pub incomplete: bool,
}

/// Collect imports separately from inference results, so ordinary type inference does not retain
/// dependency-usage sets. The result changes only when the file's imported modules change.
#[salsa::tracked(returns(ref), heap_size=ruff_memory_usage::heap_size)]
pub fn imported_modules<'db>(db: &'db dyn Db, file: ProgramFile<'db>) -> ImportedModules<'db> {
    if source_text(db, file.file(db)).read_error().is_some() {
        return ImportedModules {
            incomplete: true,
            ..ImportedModules::default()
        };
    }

    let parsed = parsed_module(db, file.python_file(db)).load(db);
    if !parsed.errors().is_empty() {
        return ImportedModules {
            incomplete: true,
            ..ImportedModules::default()
        };
    }

    let mut collector = ImportCollector {
        model: SemanticModel::new(db, file),
        index: semantic_index(db, file),
        modules: FxIndexSet::default(),
        unresolved: FxIndexSet::default(),
        incomplete: false,
    };
    collector.visit_body(parsed.suite());
    ImportedModules {
        modules: collector.modules.into_iter().collect(),
        unresolved: collector.unresolved.into_iter().collect(),
        incomplete: collector.incomplete,
    }
}

struct ImportCollector<'db> {
    model: SemanticModel<'db>,
    index: &'db SemanticIndex<'db>,
    modules: FxIndexSet<Module<'db>>,
    unresolved: FxIndexSet<ModuleName>,
    incomplete: bool,
}

impl<'db> ImportCollector<'db> {
    fn resolve(&mut self, name: &ModuleName, include_namespace: bool) -> Option<Module<'db>> {
        let db = self.model.db();
        let importing_file = ImportingFile::File(
            self.model.file(),
            self.model.program_file().resolver_environment(db),
        );
        let module = resolve_module(db, importing_file, name);
        if let Some(module) = module {
            if include_namespace || module.search_path(db).is_some() {
                self.modules.insert(module);
            }
        } else {
            self.unresolved.insert(name.clone());
        }

        // Importing a child also executes its parent packages. This matters for local package
        // initializers and for namespace packages whose children have different distributions.
        for parent in name.ancestors().skip(1) {
            if let Some(parent) = resolve_module(db, importing_file, &parent)
                && parent.search_path(db).is_some()
            {
                self.modules.insert(parent);
            }
        }
        module
    }

    fn import_from(&mut self, import: &ast::StmtImportFrom) {
        let db = self.model.db();
        let importing_file = ImportingFile::File(
            self.model.file(),
            self.model.program_file().resolver_environment(db),
        );
        let Ok(name) = ModuleName::from_import_statement(db, importing_file, import) else {
            self.incomplete = true;
            return;
        };
        let Some(parent) = self.resolve(&name, false) else {
            return;
        };

        for alias in &import.names {
            if &alias.name == "*" {
                self.modules.insert(parent);
                continue;
            }
            let Some(definitions) = self.index.try_definitions(ast::AnyNodeRef::Alias(alias))
            else {
                self.incomplete = true;
                continue;
            };
            let mut found_child = false;
            for definition in definitions {
                // Follow the same attribute-versus-submodule decision as import inference.
                // A value re-exported from another distribution does not directly import it.
                for ty in infer_definition_types(db, *definition).declaration_types() {
                    if let Type::ModuleLiteral(literal) = ty.inner_type()
                        && let child = literal.module(db)
                        && let child_name = child.name(db)
                        && child_name.parent().as_ref() == Some(parent.name(db))
                        && child_name.components().next_back() == Some(alias.name.as_str())
                    {
                        self.modules.insert(child);
                        found_child = true;
                    }
                }
            }
            if !found_child {
                self.modules.insert(parent);
            }
        }
    }

    fn dynamic_import(&mut self, call: &ast::ExprCall) {
        // Import functions can be aliased, so use the callee's inferred identity rather than its
        // spelling. An unknown module name means we cannot prove any dependency is unused.
        let Some(Type::FunctionLiteral(function)) = call.func.inferred_type(&self.model) else {
            return;
        };
        if !matches!(
            function.known(self.model.db()),
            Some(KnownFunction::DunderImport | KnownFunction::ImportModule)
        ) {
            return;
        }

        if let [ast::Expr::StringLiteral(name)] = call.arguments.args.as_ref()
            && call.arguments.keywords.is_empty()
            && let Some(name) = ModuleName::new(name.value.to_str())
        {
            self.resolve(&name, true);
        } else {
            self.incomplete = true;
        }
    }
}

impl<'ast> Visitor<'ast> for ImportCollector<'_> {
    fn visit_stmt(&mut self, stmt: &'ast ast::Stmt) {
        match stmt {
            ast::Stmt::Import(import) => {
                for alias in &import.names {
                    if let Some(name) = ModuleName::new(&alias.name) {
                        self.resolve(&name, true);
                    }
                }
            }
            ast::Stmt::ImportFrom(import) => self.import_from(import),
            _ => walk_stmt(self, stmt),
        }
    }

    fn visit_expr(&mut self, expr: &'ast ast::Expr) {
        if let ast::Expr::Call(call) = expr {
            self.dynamic_import(call);
        }
        walk_expr(self, expr);
    }
}

#[cfg(test)]
mod tests {
    use ruff_db::files::system_path_to_file;
    use ruff_db::system::DbWithWritableSystem as _;

    use crate::Db;
    use crate::db::tests::TestDbBuilder;

    use super::imported_modules;

    #[test]
    fn import_sites_across_scopes() -> anyhow::Result<()> {
        let db = TestDbBuilder::new()
            .with_file(
                "/src/main.py",
                "from typing import TYPE_CHECKING\n\
                 from package import exported\n\
                 from namespace import child\n\
                 from empty import *\n\
                 if TYPE_CHECKING:\n    import typing_only\n\
                 def inner():\n    import package.child\n",
            )
            .with_file("/src/package/__init__.py", "import unrelated as exported\n")
            .with_file("/src/package/exported.py", "")
            .with_file("/src/package/child.py", "")
            .with_file("/src/unrelated.py", "")
            .with_file("/src/namespace/child.py", "")
            .with_file("/src/empty.py", "")
            .with_file("/src/typing_only.py", "")
            .build()?;
        let file = system_path_to_file(&db, "/src/main.py")?;
        let imports = imported_modules(&db, db.program_file(file));
        assert!(!imports.incomplete);
        assert!(imports.unresolved.is_empty());
        let names: Vec<_> = imports
            .modules
            .iter()
            .map(|module| module.name(&db).as_str())
            .collect();
        assert_eq!(
            names,
            [
                "typing",
                "package",
                "namespace.child",
                "empty",
                "typing_only",
                "package.child"
            ]
        );
        Ok(())
    }

    #[test]
    fn dynamic_import_identity_and_invalidation() -> anyhow::Result<()> {
        let mut db = TestDbBuilder::new()
            .with_file(
                "/src/main.py",
                "from importlib import import_module as load\nload('external')\n",
            )
            .with_file("/src/external.py", "")
            .build()?;
        let file = system_path_to_file(&db, "/src/main.py")?;
        let imports = imported_modules(&db, db.program_file(file));
        assert!(!imports.incomplete);
        assert!(
            imports
                .modules
                .iter()
                .any(|module| module.name(&db) == "external")
        );

        db.write_file(
            "/src/main.py",
            "from importlib import import_module as load\ndef run(name: str):\n    load(name)\n",
        )?;
        assert!(imported_modules(&db, db.program_file(file)).incomplete);

        db.write_file(
            "/src/main.py",
            "def load(name: str):\n    pass\nload('external')\n",
        )?;
        let imports = imported_modules(&db, db.program_file(file));
        assert!(!imports.incomplete);
        assert!(imports.modules.is_empty());
        Ok(())
    }
}
