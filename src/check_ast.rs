use std::collections::HashSet;

use rustpython_parser::ast::{Arg, Arguments, ExprKind, Stmt, StmtKind, Suite};

use crate::checks::{Check, CheckKind};
use crate::visitor::{walk_arguments, walk_stmt, Visitor};

#[derive(Default)]
struct Checker {
    checks: Vec<Check>,
}

impl Visitor for Checker {
    fn visit_stmt(&mut self, stmt: &Stmt) {
        match &stmt.node {
            StmtKind::ImportFrom { names, .. } => {
                for alias in names {
                    if alias.name == "*" {
                        self.checks.push(Check {
                            kind: CheckKind::ImportStarUsage,
                            location: stmt.location,
                        });
                    }
                }
            }
            StmtKind::If { test, .. } => {
                if let ExprKind::Tuple { .. } = test.node {
                    self.checks.push(Check {
                        kind: CheckKind::IfTuple,
                        location: stmt.location,
                    });
                }
            }
            _ => {}
        }

        walk_stmt(self, stmt);
    }

    fn visit_arguments(&mut self, arguments: &Arguments) {
        // Collect all the arguments into a single vector.
        let mut all_arguments: Vec<&Arg> = arguments
            .posonlyargs
            .iter()
            .chain(arguments.kwonlyargs.iter())
            .chain(arguments.args.iter())
            .collect();
        if let Some(arg) = &arguments.vararg {
            all_arguments.push(arg);
        }
        if let Some(arg) = &arguments.kwarg {
            all_arguments.push(arg);
        }

        // Search for duplicates.
        let mut idents: HashSet<String> = HashSet::new();
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

        walk_arguments(self, arguments);
    }
}

pub fn check_ast(python_ast: &Suite) -> Vec<Check> {
    python_ast
        .iter()
        .flat_map(|stmt| {
            let mut checker: Checker = Default::default();
            checker.visit_stmt(stmt);
            checker.checks
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use rustpython_parser::ast::{Alias, Location, Stmt, StmtKind};

    use crate::check_ast::Checker;
    use crate::checks::Check;
    use crate::checks::CheckKind::ImportStarUsage;
    use crate::visitor::Visitor;

    #[test]
    fn import_star_usage() {
        let mut checker: Checker = Default::default();
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
        for i in 1..actual.len() {
            assert_eq!(actual[i], expected[i]);
        }
    }
}
