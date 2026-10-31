use crate::StringImports;
use ruff_python_ast::flat::visitor::source_order::{
    SourceOrderVisitor, walk_expr, walk_module, walk_stmt,
};
use ruff_python_ast::flat::{self, fields as ast};
use ruff_python_ast::{Expr, Mod, Stmt};
use ty_module_resolver::ModuleName;

/// Collect all imports for a given Python file.
#[derive(Default, Debug)]
pub(crate) struct Collector<'a> {
    /// The path to the current module.
    module_path: Option<&'a [String]>,
    /// Whether to detect imports from string literals.
    string_imports: StringImports,
    /// The collected imports from the Python AST.
    imports: Vec<CollectedImport>,
    /// Whether to detect type checking imports
    type_checking_imports: bool,
}

impl<'a> Collector<'a> {
    pub(crate) fn new(
        module_path: Option<&'a [String]>,
        string_imports: StringImports,
        type_checking_imports: bool,
    ) -> Self {
        Self {
            module_path,
            string_imports,
            imports: Vec::new(),
            type_checking_imports,
        }
    }

    #[must_use]
    pub(crate) fn collect(mut self, module: flat::View<'_, Mod>) -> Vec<CollectedImport> {
        walk_module(&mut self, module);
        self.imports
    }
}

impl<'ast> SourceOrderVisitor<'ast> for Collector<'_> {
    fn visit_stmt(&mut self, stmt: flat::View<'ast, Stmt>) {
        match stmt.fields() {
            ast::Stmt::ImportFrom(ast::StmtImportFrom {
                names,
                module,
                level,
                is_lazy: _,
                range: _,
                node: _,
            }) => {
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
                        components.extend(module.as_str().split('.'));
                    }

                    // Add the alias name, unless it's a wildcard import.
                    if alias.name().as_str() != "*" {
                        components.push(alias.name().as_str());
                    }

                    if let Some(module_name) = ModuleName::from_components(components) {
                        self.imports.push(CollectedImport::ImportFrom(module_name));
                    }
                }
            }
            ast::Stmt::Import(ast::StmtImport {
                names,
                is_lazy: _,
                range: _,
                node: _,
            }) => {
                for alias in names {
                    if let Some(module_name) = ModuleName::new(alias.name().as_str()) {
                        self.imports.push(CollectedImport::Import(module_name));
                    }
                }
            }
            ast::Stmt::If(ast::StmtIf {
                test,
                body,
                elif_else_clauses,
                range: _,
                node: _,
            }) => {
                // Skip TYPE_CHECKING blocks if not requested
                if self.type_checking_imports || !is_type_checking_condition(test) {
                    self.visit_body(body);
                }

                for clause in elif_else_clauses {
                    self.visit_elif_else_clause(clause);
                }
            }
            ast::Stmt::FunctionDef(_)
            | ast::Stmt::ClassDef(_)
            | ast::Stmt::While(_)
            | ast::Stmt::With(_)
            | ast::Stmt::Match(_)
            | ast::Stmt::Try(_)
            | ast::Stmt::For(_) => {
                // Always traverse into compound statements.
                walk_stmt(self, stmt);
            }

            ast::Stmt::Return(_)
            | ast::Stmt::Delete(_)
            | ast::Stmt::Assign(_)
            | ast::Stmt::AugAssign(_)
            | ast::Stmt::AnnAssign(_)
            | ast::Stmt::TypeAlias(_)
            | ast::Stmt::Raise(_)
            | ast::Stmt::Assert(_)
            | ast::Stmt::Global(_)
            | ast::Stmt::Nonlocal(_)
            | ast::Stmt::Expr(_)
            | ast::Stmt::Pass(_)
            | ast::Stmt::Break(_)
            | ast::Stmt::Continue(_)
            | ast::Stmt::IpyEscapeCommand(_) => {
                // Only traverse simple statements when string imports is enabled.
                if self.string_imports.enabled {
                    walk_stmt(self, stmt);
                }
            }
        }
    }

    fn visit_expr(&mut self, expr: flat::View<'ast, Expr>) {
        if self.string_imports.enabled {
            if let ast::Expr::StringLiteral(ast::ExprStringLiteral {
                value,
                range: _,
                node: _,
            }) = expr.fields()
            {
                let value = value.to_str();
                // Determine whether the string literal "looks like" an import statement: contains
                // the requisite number of dots, and consists solely of valid Python identifiers.
                if self.string_imports.min_dots == 0
                    || memchr::memchr_iter(b'.', value.as_bytes()).count()
                        >= self.string_imports.min_dots
                {
                    if let Some(module_name) = ModuleName::new(value) {
                        self.imports.push(CollectedImport::StringImport(
                            module_name,
                            self.string_imports.min_dots,
                        ));
                    }
                }
            }

            walk_expr(self, expr);
        }
    }
}

/// Check if an expression is a `TYPE_CHECKING` condition.
///
/// Returns `true` for:
/// - `TYPE_CHECKING`
/// - `typing.TYPE_CHECKING`
///
/// NOTE: Aliased `TYPE_CHECKING`, i.e. `import typing.TYPE_CHECKING as TC; if TC: ...`
/// will not be detected!
fn is_type_checking_condition(expr: flat::View<'_, Expr>) -> bool {
    match expr.fields() {
        // `if TYPE_CHECKING:`
        ast::Expr::Name(ast::ExprName { id, .. }) => id.as_str() == "TYPE_CHECKING",
        // `if typing.TYPE_CHECKING:`
        ast::Expr::Attribute(ast::ExprAttribute { value, attr, .. }) => {
            attr.as_str() == "TYPE_CHECKING"
                && matches!(
                    value.fields(),
                    ast::Expr::Name(ast::ExprName { id, .. }) if id.as_str() == "typing"
                )
        }
        _ => false,
    }
}

#[derive(Debug)]
pub(crate) enum CollectedImport {
    /// The import was part of an `import` statement.
    Import(ModuleName),
    /// The import was part of an `import from` statement.
    ImportFrom(ModuleName),
    /// The import was detected from a string literal (e.g., `"a.b.c.MyClass"`).
    ///
    /// Unlike regular imports, the trailing components may refer to attributes rather than
    /// modules. The resolver will try progressively shorter prefixes until one resolves.
    StringImport(ModuleName, usize),
}
