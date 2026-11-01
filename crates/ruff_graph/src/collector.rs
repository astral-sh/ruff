use crate::StringImports;
use ruff_python_ast::visitor::source_order::{
    SourceOrderVisitor, walk_expr, walk_module, walk_stmt,
};
use ruff_python_ast::{self as ast, Expr, Mod, Stmt};
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
    pub(crate) fn collect(mut self, module: Mod<'_>) -> Vec<CollectedImport> {
        walk_module(&mut self, module);
        self.imports
    }
}

impl<'ast> SourceOrderVisitor<'ast> for Collector<'_> {
    fn visit_stmt(&mut self, stmt: Stmt<'ast>) {
        match stmt.fields() {
            ast::StmtFields::ImportFrom(ast::StmtImportFromFields {
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
            ast::StmtFields::Import(ast::StmtImportFields {
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
            ast::StmtFields::If(ast::StmtIfFields {
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
            ast::StmtFields::FunctionDef(_)
            | ast::StmtFields::ClassDef(_)
            | ast::StmtFields::While(_)
            | ast::StmtFields::With(_)
            | ast::StmtFields::Match(_)
            | ast::StmtFields::Try(_)
            | ast::StmtFields::For(_) => {
                // Always traverse into compound statements.
                walk_stmt(self, stmt);
            }

            ast::StmtFields::Return(_)
            | ast::StmtFields::Delete(_)
            | ast::StmtFields::Assign(_)
            | ast::StmtFields::AugAssign(_)
            | ast::StmtFields::AnnAssign(_)
            | ast::StmtFields::TypeAlias(_)
            | ast::StmtFields::Raise(_)
            | ast::StmtFields::Assert(_)
            | ast::StmtFields::Global(_)
            | ast::StmtFields::Nonlocal(_)
            | ast::StmtFields::Expr(_)
            | ast::StmtFields::Pass(_)
            | ast::StmtFields::Break(_)
            | ast::StmtFields::Continue(_)
            | ast::StmtFields::IpyEscapeCommand(_) => {
                // Only traverse simple statements when string imports is enabled.
                if self.string_imports.enabled {
                    walk_stmt(self, stmt);
                }
            }
        }
    }

    fn visit_expr(&mut self, expr: Expr<'ast>) {
        if self.string_imports.enabled {
            if let ast::ExprFields::StringLiteral(ast::ExprStringLiteralFields {
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
fn is_type_checking_condition(expr: Expr<'_>) -> bool {
    match expr.fields() {
        // `if TYPE_CHECKING:`
        ast::ExprFields::Name(ast::ExprNameFields { id, .. }) => id.as_str() == "TYPE_CHECKING",
        // `if typing.TYPE_CHECKING:`
        ast::ExprFields::Attribute(ast::ExprAttributeFields { value, attr, .. }) => {
            attr.as_str() == "TYPE_CHECKING"
                && matches!(
                    value.fields(),
                    ast::ExprFields::Name(ast::ExprNameFields { id, .. }) if id.as_str() == "typing"
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
