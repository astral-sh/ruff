use std::collections::BTreeSet;

use crate::check_ast::ScopeKind::{Class, Function, Generator, Module};
use rustpython_parser::ast::{Arg, Arguments, Expr, ExprKind, Stmt, StmtKind, Suite};

use crate::checks::{Check, CheckKind};
use crate::settings::Settings;
use crate::visitor;
use crate::visitor::Visitor;

enum ScopeKind {
    Class,
    Function,
    Generator,
    Module,
}

struct Scope {
    kind: ScopeKind,
}

struct Checker<'a> {
    settings: &'a Settings,
    checks: Vec<Check>,
    scopes: Vec<Scope>,
}

impl Checker<'_> {
    pub fn new(settings: &Settings) -> Checker {
        Checker {
            settings,
            checks: vec![],
            scopes: vec![Scope { kind: Module }],
        }
    }
}

impl Visitor for Checker<'_> {
    fn visit_stmt(&mut self, stmt: &Stmt) {
        match &stmt.node {
            StmtKind::FunctionDef { .. } => self.scopes.push(Scope { kind: Function }),
            StmtKind::AsyncFunctionDef { .. } => self.scopes.push(Scope { kind: Function }),
            StmtKind::Return { .. } => {
                if self
                    .settings
                    .select
                    .contains(CheckKind::ReturnOutsideFunction.code())
                {
                    if let Some(scope) = self.scopes.last() {
                        match scope.kind {
                            Class | Module => {
                                self.checks.push(Check {
                                    kind: CheckKind::ReturnOutsideFunction,
                                    location: stmt.location,
                                });
                            }
                            _ => {}
                        }
                    }
                }
            }
            StmtKind::ClassDef { .. } => self.scopes.push(Scope { kind: Class }),
            StmtKind::ImportFrom { names, .. } => {
                if self
                    .settings
                    .select
                    .contains(CheckKind::ImportStarUsage.code())
                {
                    for alias in names {
                        if alias.name == "*" {
                            self.checks.push(Check {
                                kind: CheckKind::ImportStarUsage,
                                location: stmt.location,
                            });
                        }
                    }
                }
            }
            StmtKind::If { test, .. } => {
                if self.settings.select.contains(CheckKind::IfTuple.code()) {
                    if let ExprKind::Tuple { .. } = test.node {
                        self.checks.push(Check {
                            kind: CheckKind::IfTuple,
                            location: stmt.location,
                        });
                    }
                }
            }
            StmtKind::Raise { exc, .. } => {
                if self
                    .settings
                    .select
                    .contains(CheckKind::RaiseNotImplemented.code())
                {
                    if let Some(expr) = exc {
                        match &expr.node {
                            ExprKind::Call { func, .. } => {
                                if let ExprKind::Name { id, .. } = &func.node {
                                    if id == "NotImplemented" {
                                        self.checks.push(Check {
                                            kind: CheckKind::RaiseNotImplemented,
                                            location: stmt.location,
                                        });
                                    }
                                }
                            }
                            ExprKind::Name { id, .. } => {
                                if id == "NotImplemented" {
                                    self.checks.push(Check {
                                        kind: CheckKind::RaiseNotImplemented,
                                        location: stmt.location,
                                    });
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
            _ => {}
        }

        visitor::walk_stmt(self, stmt);

        match &stmt.node {
            StmtKind::ClassDef { .. }
            | StmtKind::FunctionDef { .. }
            | StmtKind::AsyncFunctionDef { .. } => {
                self.scopes.pop();
            }
            _ => {}
        };
    }

    fn visit_expr(&mut self, expr: &Expr) {
        match &expr.node {
            ExprKind::GeneratorExp { .. } => self.scopes.push(Scope { kind: Generator }),
            ExprKind::Lambda { .. } => self.scopes.push(Scope { kind: Function }),
            ExprKind::JoinedStr { values } => {
                if self
                    .settings
                    .select
                    .contains(CheckKind::FStringMissingPlaceholders.code())
                    && !values
                        .iter()
                        .any(|value| matches!(value.node, ExprKind::FormattedValue { .. }))
                {
                    self.checks.push(Check {
                        kind: CheckKind::FStringMissingPlaceholders,
                        location: expr.location,
                    });
                }
            }
            _ => {}
        };

        visitor::walk_expr(self, expr);

        match &expr.node {
            ExprKind::GeneratorExp { .. } | ExprKind::Lambda { .. } => {
                self.scopes.pop();
            }
            _ => {}
        };
    }

    fn visit_arguments(&mut self, arguments: &Arguments) {
        if self
            .settings
            .select
            .contains(CheckKind::DuplicateArgumentName.code())
        {
            // Collect all the arguments into a single vector.
            let mut all_arguments: Vec<&Arg> = arguments
                .args
                .iter()
                .chain(arguments.posonlyargs.iter())
                .chain(arguments.kwonlyargs.iter())
                .collect();
            if let Some(arg) = &arguments.vararg {
                all_arguments.push(arg);
            }
            if let Some(arg) = &arguments.kwarg {
                all_arguments.push(arg);
            }

            // Search for duplicates.
            let mut idents: BTreeSet<String> = BTreeSet::new();
            for arg in all_arguments {
                let ident = &arg.node.arg;
                if idents.contains(ident) {
                    self.checks.push(Check {
                        kind: CheckKind::DuplicateArgumentName,
                        location: arg.location,
                    });
                    break;
                }
                idents.insert(ident.clone());
            }
        }

        visitor::walk_arguments(self, arguments);
    }
}

pub fn check_ast(python_ast: &Suite, settings: &Settings) -> Vec<Check> {
    python_ast
        .iter()
        .flat_map(|stmt| {
            let mut checker = Checker::new(settings);
            checker.visit_stmt(stmt);
            checker.checks
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use rustpython_parser::ast::{Alias, Location, Stmt, StmtKind};

    use crate::check_ast::Checker;
    use crate::checks::CheckKind::ImportStarUsage;
    use crate::checks::{Check, CheckCode};
    use crate::settings::Settings;
    use crate::visitor::Visitor;

    #[test]
    fn import_star_usage() {
        let settings = Settings {
            line_length: 88,
            exclude: vec![],
            select: BTreeSet::from([CheckCode::F403]),
        };
        let mut checker = Checker::new(&settings);
        checker.visit_stmt(&Stmt {
            location: Location::new(1, 1),
            custom: (),
            node: StmtKind::ImportFrom {
                module: Some("bar".to_string()),
                names: vec![Alias {
                    name: "*".to_string(),
                    asname: None,
                }],
                level: 0,
            },
        });

        let actual = checker.checks;
        let expected = vec![Check {
            kind: ImportStarUsage,
            location: Location::new(1, 1),
        }];
        assert_eq!(actual.len(), expected.len());
        for i in 0..actual.len() {
            assert_eq!(actual[i], expected[i]);
        }
    }
}
