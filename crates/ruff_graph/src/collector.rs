use red_knot_python_semantic::ModuleName;
use ruff_python_ast::helpers::collect_import_from_member;
use ruff_python_ast::visitor::source_order::{walk_body, walk_expr, walk_stmt, SourceOrderVisitor};
use ruff_python_ast::{self as ast, Expr, ModModule, Stmt};

/// Collect all imports for a given Python file.
#[derive(Default, Debug)]
pub(crate) struct Collector {
    /// Whether to detect imports from string literals.
    string_imports: bool,
    /// The collected imports from the Python AST.
    imports: Vec<CollectedImport>,
}

impl Collector {
    pub(crate) fn new(string_imports: bool) -> Self {
        Self {
            string_imports,
            imports: Vec::new(),
        }
    }

    #[must_use]
    pub(crate) fn collect(mut self, module: &ModModule) -> Vec<CollectedImport> {
        walk_body(&mut self, &module.body);
        self.imports
    }
}

impl<'ast> SourceOrderVisitor<'ast> for Collector {
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
                    if let Some(module_name) = ModuleName::from_components(
                        collect_import_from_member(level, module, &alias.name)
                            .segments()
                            .iter()
                            .cloned(),
                    ) {
                        self.imports.push(CollectedImport::ImportFrom(module_name));
                    }
                }
            }
            Stmt::Import(ast::StmtImport { names, range: _ }) => {
                for alias in names {
                    if let Some(module_name) = ModuleName::new(alias.name.as_str()) {
                        self.imports.push(CollectedImport::Import(module_name));
                    }
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
                if let Some(module_name) = ModuleName::new(value) {
                    self.imports.push(CollectedImport::Import(module_name));
                }
            }
            walk_expr(self, expr);
        }
    }
}

#[derive(Debug)]
pub(crate) enum CollectedImport {
    /// The import was part of an `import` statement.
    Import(ModuleName),
    /// The import was part of an `import from` statement.
    ImportFrom(ModuleName),
}
