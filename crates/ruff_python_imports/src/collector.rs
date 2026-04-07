use crate::{ImportKind, ImportOccurrence, StringImports};
use ruff_python_ast::helpers::is_dotted_name;
use ruff_python_ast::visitor::source_order::{
    SourceOrderVisitor, walk_expr, walk_module, walk_stmt,
};
use ruff_python_ast::{self as ast, Expr, Mod, Stmt};
use ruff_text_size::TextRange;
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
    /// Whether to include imports that only appear in type-checking contexts.
    include_type_checking_imports: bool,
    /// Whether the current traversal context is inside a type-checking-only branch.
    in_type_checking: bool,
}

impl<'a> Collector<'a> {
    pub(crate) fn new(
        module_path: Option<&'a [String]>,
        string_imports: StringImports,
        include_type_checking_imports: bool,
    ) -> Self {
        Self {
            module_path,
            string_imports,
            imports: Vec::new(),
            include_type_checking_imports,
            in_type_checking: false,
        }
    }

    #[must_use]
    pub(crate) fn collect(mut self, module: &Mod) -> Vec<CollectedImport> {
        walk_module(&mut self, module);
        self.imports
    }

    fn push_import(
        &mut self,
        kind: ImportKind,
        requested: ModuleName,
        range: TextRange,
        is_relative: bool,
        string_import_min_dots: Option<usize>,
    ) {
        if self.in_type_checking && !self.include_type_checking_imports {
            return;
        }

        self.imports.push(CollectedImport {
            occurrence: ImportOccurrence {
                importer: Default::default(),
                kind,
                requested,
                range: Some(range),
                in_type_checking: self.in_type_checking,
                is_relative,
            },
            string_import_min_dots,
        });
    }

    fn visit_body_with_type_checking_context<'ast>(
        &mut self,
        body: &'ast [Stmt],
        in_type_checking: bool,
    ) {
        let previous = self.in_type_checking;
        self.in_type_checking = previous || in_type_checking;
        self.visit_body(body);
        self.in_type_checking = previous;
    }
}

impl<'ast> SourceOrderVisitor<'ast> for Collector<'_> {
    fn visit_stmt(&mut self, stmt: &'ast Stmt) {
        match stmt {
            Stmt::ImportFrom(ast::StmtImportFrom {
                names,
                module,
                level,
                ..
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
                        self.push_import(
                            ImportKind::ImportFrom,
                            module_name,
                            alias.range,
                            level > 0,
                            None,
                        );
                    }
                }
            }
            Stmt::Import(ast::StmtImport { names, .. }) => {
                for alias in names {
                    if let Some(module_name) = ModuleName::new(alias.name.as_str()) {
                        self.push_import(ImportKind::Import, module_name, alias.range, false, None);
                    }
                }
            }
            Stmt::If(ast::StmtIf {
                test,
                body,
                elif_else_clauses,
                ..
            }) => {
                self.visit_expr(test);

                let outer_in_type_checking = self.in_type_checking;
                let mut in_not_type_checking_chain = is_if_not_type_checking(test);

                self.visit_body_with_type_checking_context(
                    body,
                    outer_in_type_checking || is_if_type_checking(test),
                );

                for clause in elif_else_clauses {
                    if let Some(test) = clause.test.as_ref() {
                        self.visit_expr(test);
                    }

                    let clause_in_type_checking = if let Some(test) = clause.test.as_ref() {
                        if is_if_type_checking(test) {
                            true
                        } else if is_if_not_type_checking(test) {
                            in_not_type_checking_chain = true;
                            false
                        } else {
                            in_not_type_checking_chain
                        }
                    } else {
                        in_not_type_checking_chain
                    };

                    self.visit_body_with_type_checking_context(
                        &clause.body,
                        outer_in_type_checking || clause_in_type_checking,
                    );
                }
            }
            Stmt::FunctionDef(_)
            | Stmt::ClassDef(_)
            | Stmt::While(_)
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
                // Only traverse simple statements when string imports are enabled.
                if self.string_imports.enabled {
                    walk_stmt(self, stmt);
                }
            }
        }
    }

    fn visit_expr(&mut self, expr: &'ast Expr) {
        if self.string_imports.enabled {
            if let Expr::StringLiteral(ast::ExprStringLiteral { value, range, .. }) = expr {
                let value = value.to_str();
                if self.string_imports.min_dots == 0
                    || memchr::memchr_iter(b'.', value.as_bytes()).count()
                        >= self.string_imports.min_dots
                {
                    if let Some(module_name) = ModuleName::new(value) {
                        self.push_import(
                            ImportKind::StringImport,
                            module_name,
                            *range,
                            false,
                            Some(self.string_imports.min_dots),
                        );
                    }
                }
            }

            walk_expr(self, expr);
        }
    }
}

#[derive(Debug)]
pub(crate) struct CollectedImport {
    pub(crate) occurrence: ImportOccurrence,
    pub(crate) string_import_min_dots: Option<usize>,
}

/// Returns if the expression is a `TYPE_CHECKING` expression.
fn is_if_type_checking(expr: &Expr) -> bool {
    match expr {
        Expr::Name(ast::ExprName { id, .. }) => id.as_str() == "TYPE_CHECKING",
        Expr::Attribute(ast::ExprAttribute { value, attr, .. }) => {
            attr.as_str() == "TYPE_CHECKING" && is_dotted_name(value)
        }
        _ => false,
    }
}

/// Returns if the expression is a `not TYPE_CHECKING` expression.
fn is_if_not_type_checking(expr: &Expr) -> bool {
    matches!(
        expr,
        Expr::UnaryOp(ast::ExprUnaryOp {
            op: ast::UnaryOp::Not,
            operand,
            ..
        }) if is_if_type_checking(operand)
    )
}
