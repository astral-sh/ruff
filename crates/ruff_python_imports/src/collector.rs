use ruff_db::system::SystemPath;
use ruff_python_ast::helpers::is_dotted_name;
use ruff_python_ast::visitor::source_order::{
    SourceOrderVisitor, walk_expr, walk_module, walk_stmt,
};
use ruff_python_ast::{self as ast, Expr, Mod, Stmt};
use ruff_text_size::Ranged;
use ty_module_resolver::ModuleName;

use crate::{ImportKind, RawImportOccurrence, StringImports};

/// Collect all imports for a given Python file.
#[derive(Debug)]
pub(crate) struct Collector<'a> {
    importer: &'a SystemPath,
    module_path: Option<&'a [String]>,
    string_imports: StringImports,
    imports: Vec<CollectedImport>,
    type_checking_imports: bool,
    type_checking_nesting: usize,
}

impl<'a> Collector<'a> {
    pub(crate) fn new(
        importer: &'a SystemPath,
        module_path: Option<&'a [String]>,
        string_imports: StringImports,
        type_checking_imports: bool,
    ) -> Self {
        Self {
            importer,
            module_path,
            string_imports,
            imports: Vec::new(),
            type_checking_imports,
            type_checking_nesting: 0,
        }
    }

    #[must_use]
    pub(crate) fn collect(mut self, module: &Mod) -> Vec<CollectedImport> {
        walk_module(&mut self, module);
        self.imports
    }

    fn push_import(
        &mut self,
        requested: ModuleName,
        range: ruff_text_size::TextRange,
        kind: ImportKind,
        is_relative: bool,
    ) {
        self.imports.push(CollectedImport {
            occurrence: RawImportOccurrence {
                importer: self.importer.to_path_buf(),
                kind,
                requested,
                range,
                in_type_checking: self.type_checking_nesting > 0,
                is_relative,
            },
        });
    }

    fn visit_body_with_type_checking(&mut self, body: &[Stmt], in_type_checking: bool) {
        if in_type_checking {
            self.type_checking_nesting += 1;
        }
        self.visit_body(body);
        if in_type_checking {
            self.type_checking_nesting -= 1;
        }
    }

    fn in_type_checking(&self) -> bool {
        self.type_checking_nesting > 0
    }
}

impl<'ast> SourceOrderVisitor<'ast> for Collector<'_> {
    fn visit_stmt(&mut self, stmt: &'ast Stmt) {
        match stmt {
            Stmt::ImportFrom(ast::StmtImportFrom {
                names,
                module,
                level,
                is_lazy: _,
                range: _,
                node_index: _,
            }) => {
                let module = module.as_deref();
                let level = *level;
                for alias in names {
                    let mut components = vec![];

                    if level > 0 {
                        let Some(module_path) = self.module_path else {
                            return;
                        };

                        components.extend(module_path.iter().map(String::as_str));

                        for _ in 0..level {
                            if components.is_empty() {
                                return;
                            }
                            components.pop();
                        }
                    }

                    if let Some(module) = module {
                        components.extend(module.split('.'));
                    }

                    if alias.name.as_str() != "*" {
                        components.push(alias.name.as_str());
                    }

                    if let Some(module_name) = ModuleName::from_components(components) {
                        self.push_import(
                            module_name,
                            alias.name.range(),
                            ImportKind::ImportFrom,
                            level > 0,
                        );
                    }
                }
            }
            Stmt::Import(ast::StmtImport {
                names,
                is_lazy: _,
                range: _,
                node_index: _,
            }) => {
                for alias in names {
                    if let Some(module_name) = ModuleName::new(alias.name.as_str()) {
                        self.push_import(
                            module_name,
                            alias.name.range(),
                            ImportKind::Import,
                            false,
                        );
                    }
                }
            }
            Stmt::If(ast::StmtIf {
                test,
                body,
                elif_else_clauses,
                range: _,
                node_index: _,
            }) => {
                self.visit_expr(test);

                let outer_in_type_checking = self.in_type_checking();
                let mut in_not_type_checking_chain = is_not_type_checking_condition(test);
                let body_is_type_checking =
                    outer_in_type_checking || is_type_checking_condition(test);
                if self.type_checking_imports || !body_is_type_checking {
                    self.visit_body_with_type_checking(body, body_is_type_checking);
                }

                for clause in elif_else_clauses {
                    if let Some(test) = clause.test.as_ref() {
                        self.visit_expr(test);
                    }

                    let clause_is_type_checking = if let Some(test) = clause.test.as_ref() {
                        if is_type_checking_condition(test) {
                            true
                        } else if is_not_type_checking_condition(test) {
                            in_not_type_checking_chain = true;
                            false
                        } else {
                            in_not_type_checking_chain
                        }
                    } else {
                        in_not_type_checking_chain
                    };

                    let body_is_type_checking = outer_in_type_checking || clause_is_type_checking;
                    if self.type_checking_imports || !body_is_type_checking {
                        self.visit_body_with_type_checking(&clause.body, body_is_type_checking);
                    }
                }
            }
            Stmt::FunctionDef(_)
            | Stmt::ClassDef(_)
            | Stmt::While(_)
            | Stmt::With(_)
            | Stmt::Match(_)
            | Stmt::Try(_)
            | Stmt::For(_) => {
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
                if self.string_imports.enabled {
                    walk_stmt(self, stmt);
                }
            }
        }
    }

    fn visit_expr(&mut self, expr: &'ast Expr) {
        if self.string_imports.enabled {
            if let Expr::StringLiteral(ast::ExprStringLiteral {
                value,
                range,
                node_index: _,
            }) = expr
            {
                let value = value.to_str();
                if self.string_imports.min_dots == 0
                    || memchr::memchr_iter(b'.', value.as_bytes()).count()
                        >= self.string_imports.min_dots
                {
                    if let Some(module_name) = ModuleName::new(value) {
                        self.push_import(
                            module_name,
                            *range,
                            ImportKind::StringImport {
                                min_dots: self.string_imports.min_dots,
                            },
                            false,
                        );
                    }
                }
            }

            walk_expr(self, expr);
        }
    }
}

fn is_type_checking_condition(expr: &Expr) -> bool {
    match expr {
        Expr::Name(ast::ExprName { id, .. }) => id.as_str() == "TYPE_CHECKING",
        Expr::Attribute(ast::ExprAttribute { value, attr, .. }) => {
            attr.as_str() == "TYPE_CHECKING" && is_dotted_name(value)
        }
        _ => false,
    }
}

fn is_not_type_checking_condition(expr: &Expr) -> bool {
    matches!(
        expr,
        Expr::UnaryOp(ast::ExprUnaryOp {
            op: ast::UnaryOp::Not,
            operand,
            ..
        }) if is_type_checking_condition(operand)
    )
}

#[derive(Debug)]
pub(crate) struct CollectedImport {
    pub(crate) occurrence: RawImportOccurrence,
}
