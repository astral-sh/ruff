use ruff_python_ast::helpers::collect_import_from_member;
use ruff_python_ast::identifier::Identifier;
use ruff_python_ast::name::QualifiedName;
use ruff_python_ast::visitor::source_order::walk_module;
use ruff_python_ast::visitor::source_order::{walk_stmt, SourceOrderVisitor};
use ruff_python_ast::{self as ast, Mod, Stmt};

/// Collect all imports for a given Python file.
#[derive(Default, Debug)]
pub struct Collector<'ast> {
    imports: Vec<QualifiedName<'ast>>,
}

impl<'ast> Collector<'ast> {
    #[must_use]
    pub fn collect(mut self, module: &'ast Mod) -> Vec<QualifiedName<'ast>> {
        walk_module(&mut self, module);
        self.imports
    }
}

impl<'ast> SourceOrderVisitor<'ast> for Collector<'ast> {
    fn visit_stmt(&mut self, stmt: &'ast Stmt) {
        match stmt {
            Stmt::ImportFrom(ast::StmtImportFrom {
                names,
                module,
                level,
                range: _,
            }) => {
                let module = module.as_deref();
                let level = *level;
                for alias in names {
                    let qualified_name = collect_import_from_member(level, module, &alias.name);
                    self.imports.push(qualified_name);
                }
            }
            Stmt::Import(ast::StmtImport { range, names }) => {
                for alias in names {
                    let qualified_name = QualifiedName::user_defined(&alias.name);
                    self.imports.push(qualified_name);
                }
            }
            _ => {
                walk_stmt(self, stmt);
            }
        }
    }
}
