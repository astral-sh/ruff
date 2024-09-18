use ruff_python_ast::helpers::collect_import_from_member;
use ruff_python_ast::name::QualifiedName;
use ruff_python_ast::visitor::source_order::walk_module;
use ruff_python_ast::visitor::source_order::{walk_expr, walk_stmt, SourceOrderVisitor};
use ruff_python_ast::{self as ast, Expr, Mod, Stmt};
use ruff_python_stdlib::identifiers::is_identifier;

/// Collect all imports for a given Python file.
#[derive(Default, Debug)]
pub(crate) struct Collector<'ast> {
    /// Whether to detect imports from string literals.
    string_imports: bool,
    /// The collected imports from the Python AST.
    imports: Vec<CollectedImport<'ast>>,
}

impl<'ast> Collector<'ast> {
    pub(crate) fn new(string_imports: bool) -> Self {
        Self {
            string_imports,
            imports: Vec::new(),
        }
    }

    #[must_use]
    pub(crate) fn collect(mut self, module: &'ast Mod) -> Vec<CollectedImport<'ast>> {
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
                    self.imports
                        .push(CollectedImport::ImportFrom(qualified_name));
                }
            }
            Stmt::Import(ast::StmtImport { names, range: _ }) => {
                for alias in names {
                    let qualified_name = QualifiedName::user_defined(&alias.name);
                    self.imports.push(CollectedImport::Import(qualified_name));
                }
            }
            _ => {
                walk_stmt(self, stmt);
            }
        }
    }

    fn visit_expr(&mut self, expr: &'ast Expr) {
        if self.string_imports {
            if let Expr::StringLiteral(ast::ExprStringLiteral { value, range: _ }) = expr {
                // Determine whether the string literal "looks like" an import statement: contains
                // a dot, and consists solely of valid Python identifiers.
                let value = value.to_str();
                if value.split('.').all(is_identifier) {
                    let qualified_name = QualifiedName::from_dotted_name(value);
                    self.imports.push(CollectedImport::Import(qualified_name));
                }
            }
            walk_expr(self, expr);
        }
    }
}

#[derive(Debug)]
pub(crate) enum CollectedImport<'ast> {
    /// The import was part of an `import` statement.
    Import(QualifiedName<'ast>),
    /// The import was part of an `import from` statement.
    ImportFrom(QualifiedName<'ast>),
}
