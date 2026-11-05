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
        walk_module(&mut self, module.classify());
        self.imports
    }
}

impl<'ast> SourceOrderVisitor<'ast> for Collector<'_> {
    fn visit_stmt(&mut self, stmt: Stmt<'ast>) {
        let kind = stmt.classify();
        match kind {
            ast::StmtKind::ImportFrom(node) => {
                let ast::StmtImportFromFields {
                    names,
                    module,
                    level,
                    ..
                } = node.fields();
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
            ast::StmtKind::Import(import) => {
                for alias in import.names() {
                    if let Some(module_name) = ModuleName::new(alias.name().as_str()) {
                        self.imports.push(CollectedImport::Import(module_name));
                    }
                }
            }
            ast::StmtKind::If(node) => {
                let ast::StmtIfFields {
                    test,
                    body,
                    elif_else_clauses,
                    ..
                } = node.fields();
                // Skip TYPE_CHECKING blocks if not requested
                if self.type_checking_imports || !is_type_checking_condition(test) {
                    self.visit_body(body);
                }

                for clause in elif_else_clauses {
                    self.visit_elif_else_clause(clause);
                }
            }
            ast::StmtKind::FunctionDef(_)
            | ast::StmtKind::ClassDef(_)
            | ast::StmtKind::While(_)
            | ast::StmtKind::With(_)
            | ast::StmtKind::Match(_)
            | ast::StmtKind::Try(_)
            | ast::StmtKind::For(_) => {
                // Always traverse into compound statements.
                walk_stmt(self, kind);
            }

            ast::StmtKind::Return(_)
            | ast::StmtKind::Delete(_)
            | ast::StmtKind::Assign(_)
            | ast::StmtKind::AugAssign(_)
            | ast::StmtKind::AnnAssign(_)
            | ast::StmtKind::TypeAlias(_)
            | ast::StmtKind::Raise(_)
            | ast::StmtKind::Assert(_)
            | ast::StmtKind::Global(_)
            | ast::StmtKind::Nonlocal(_)
            | ast::StmtKind::Expr(_)
            | ast::StmtKind::Pass(_)
            | ast::StmtKind::Break(_)
            | ast::StmtKind::Continue(_)
            | ast::StmtKind::IpyEscapeCommand(_) => {
                // Only traverse simple statements when string imports is enabled.
                if self.string_imports.enabled {
                    walk_stmt(self, kind);
                }
            }
        }
    }

    fn visit_expr(&mut self, expr: Expr<'ast>) {
        if self.string_imports.enabled {
            let kind = expr.classify();
            if let ast::ExprKind::StringLiteral(literal) = kind {
                let value = literal.value().to_str();
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

            walk_expr(self, kind);
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
    match expr.classify() {
        // `if TYPE_CHECKING:`
        ast::ExprKind::Name(name) => {
            let id = name.id();
            id.as_str() == "TYPE_CHECKING"
        }
        // `if typing.TYPE_CHECKING:`
        ast::ExprKind::Attribute(attribute) => {
            let value = attribute.value();
            let attr = attribute.attr();
            attr.as_str() == "TYPE_CHECKING"
                && value
                    .as_name_expr()
                    .is_some_and(|name| name.id().as_str() == "typing")
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
