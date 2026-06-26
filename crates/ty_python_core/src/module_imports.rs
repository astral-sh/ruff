//! A syntax-only query for finding modules imported by a file.

use ruff_db::{files::File, parsed::parsed_module};
use ruff_python_ast::{
    self as ast,
    statement_visitor::{StatementVisitor, walk_stmt},
};
use ty_module_resolver::ModuleName;

use crate::Db;

/// Returns the modules imported by `import` statements in `file`, including their ancestors.
///
/// Imports are collected across all scopes and control-flow branches. `from ... import ...`
/// statements are intentionally excluded; see `ModuleLiteralType::available_submodule_attributes`
/// for why this analysis is limited to direct imports.
#[salsa::tracked(returns(deref), heap_size=ruff_memory_usage::heap_size)]
pub fn imported_modules(db: &dyn Db, file: File) -> Box<[ModuleName]> {
    let module = parsed_module(db, file).load(db);
    let mut finder = ImportedModulesFinder::default();
    finder.visit_body(module.suite());

    finder.modules.sort_unstable();
    finder.modules.dedup();
    finder.modules.into_boxed_slice()
}

#[derive(Default)]
struct ImportedModulesFinder {
    modules: Vec<ModuleName>,
}

impl<'ast> StatementVisitor<'ast> for ImportedModulesFinder {
    fn visit_stmt(&mut self, stmt: &'ast ast::Stmt) {
        if let ast::Stmt::Import(import) = stmt {
            for alias in &import.names {
                if let Some(module_name) = ModuleName::new(&alias.name) {
                    self.modules.extend(module_name.ancestors());
                }
            }
        }

        walk_stmt(self, stmt);
    }
}

#[cfg(test)]
mod tests {
    use ruff_db::files::system_path_to_file;
    use ty_module_resolver::ModuleName;

    use super::imported_modules;
    use crate::db::tests::TestDbBuilder;

    #[test]
    fn collects_direct_imports_and_their_ancestors() {
        const FILENAME: &str = "test.py";
        let db = TestDbBuilder::new()
            .with_file(
                FILENAME,
                r#"
import alpha
import beta.gamma.delta as beta_delta
from ignored.module import value

if condition:
    import nested.module

def function():
    import function.module
"#,
            )
            .build()
            .unwrap();
        let file = system_path_to_file(&db, FILENAME).unwrap();

        let modules = imported_modules(&db, file)
            .iter()
            .map(ModuleName::as_str)
            .collect::<Vec<_>>();

        assert_eq!(
            modules,
            [
                "alpha",
                "beta",
                "beta.gamma",
                "beta.gamma.delta",
                "function",
                "function.module",
                "nested",
                "nested.module",
            ]
        );
    }
}
