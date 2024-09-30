use red_knot_python_semantic::ModuleName;
use ruff_python_ast::visitor::source_order::{
    walk_expr, walk_module, walk_stmt, SourceOrderVisitor,
};
use ruff_python_ast::{self as ast, Expr, Mod, Stmt};

/// Collect all imports for a given Python file.
#[derive(Default, Debug)]
pub(crate) struct Collector<'a> {
    /// The path to the current module.
    module_path: Option<&'a [String]>,
    /// Whether to detect imports from string literals.
    string_imports: bool,
    /// The collected imports from the Python AST.
    imports: Vec<CollectedImport>,
}

impl<'a> Collector<'a> {
    pub(crate) fn new(module_path: Option<&'a [String]>, string_imports: bool) -> Self {
        Self {
            module_path,
            string_imports,
            imports: Vec::new(),
        }
    }

    #[must_use]
    pub(crate) fn collect(mut self, module: &Mod) -> Vec<CollectedImport> {
        walk_module(&mut self, module);
        self.imports
    }
}

impl<'ast> SourceOrderVisitor<'ast> for Collector<'_> {
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
                    let mut components = vec![];

                    if level > 0 {
                        // If we're resolving a relative import, we must have a module path.
                        let Some(module_path) = self.module_path else {
                            return;
                        };

                        // Start with the containing module.
                        components.extend(module_path.iter().map(String::as_str));

                        // Remove segments based on the number of dots.
                        for _ in 0..level {
                            if components.is_empty() {
                                return;
                            }
                            components.pop();
                        }
                    }

                    // Add the module path.
                    if let Some(module) = module {
                        components.extend(module.split('.'));
                    }

                    // Add the alias name, unless it's a wildcard import.
                    if alias.name.as_str() != "*" {
                        components.push(alias.name.as_str());
                    }

                    if let Some(module_name) = ModuleName::from_components(components) {
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
            Stmt::FunctionDef(_)
            | Stmt::ClassDef(_)
            | Stmt::While(_)
            | Stmt::If(_)
            | Stmt::With(_)
            | Stmt::Match(_)
            | Stmt::Try(_)
            | Stmt::For(_) => {
                // Always traverse into compound statements.
                walk_stmt(self, stmt);
            }

            Stmt::Return(_)
            | Stmt::Delete(_)
            | Stmt::Assign(_)
            | Stmt::AugAssign(_)
            | Stmt::AnnAssign(_)
            | Stmt::TypeAlias(_)
            | Stmt::Raise(_)
            | Stmt::Assert(_)
            | Stmt::Global(_)
            | Stmt::Nonlocal(_)
            | Stmt::Expr(_)
            | Stmt::Pass(_)
            | Stmt::Break(_)
            | Stmt::Continue(_)
            | Stmt::IpyEscapeCommand(_) => {
                // Only traverse simple statements when string imports is enabled.
                if self.string_imports {
                    walk_stmt(self, stmt);
                }
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
