use std::collections::{BTreeMap, BTreeSet};

use rustpython_parser::ast::{
    Arg, Arguments, Expr, ExprContext, ExprKind, Location, Stmt, StmtKind, Suite,
};

use crate::check_ast::ScopeKind::{Class, Function, Generator, Module};
use crate::checks::{Check, CheckCode, CheckKind};
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
    values: BTreeMap<String, Binding>,
}

enum BindingKind {
    Argument,
    Assignment,
    ClassDefinition,
    Definition,
    FutureImportation,
    Importation(String),
    StarImportation,
    SubmoduleImportation,
}

struct Binding {
    kind: BindingKind,
    name: String,
    location: Location,
    used: bool,
}

struct Checker<'a> {
    settings: &'a Settings,
    checks: Vec<Check>,
    scopes: Vec<Scope>,
    dead_scopes: Vec<Scope>,
    in_f_string: bool,
}

impl Checker<'_> {
    pub fn new(settings: &Settings) -> Checker {
        Checker {
            settings,
            checks: vec![],
            scopes: vec![],
            dead_scopes: vec![],
            in_f_string: false,
        }
    }
}

impl Visitor for Checker<'_> {
    fn visit_stmt(&mut self, stmt: &Stmt) {
        match &stmt.node {
            StmtKind::FunctionDef { name, .. } => {
                self.push_scope(Scope {
                    kind: Function,
                    values: BTreeMap::new(),
                });
                self.add_binding(Binding {
                    kind: BindingKind::ClassDefinition,
                    name: name.clone(),
                    used: false,
                    location: stmt.location,
                })
            }
            StmtKind::AsyncFunctionDef { name, .. } => {
                self.push_scope(Scope {
                    kind: Function,
                    values: BTreeMap::new(),
                });
                self.add_binding(Binding {
                    kind: BindingKind::ClassDefinition,
                    name: name.clone(),
                    used: false,
                    location: stmt.location,
                })
            }
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
            StmtKind::ClassDef { .. } => self.push_scope(Scope {
                kind: Class,
                values: BTreeMap::new(),
            }),
            StmtKind::Import { names } => {
                for alias in names {
                    if alias.node.name.contains('.') && alias.node.asname.is_none() {
                        self.add_binding(Binding {
                            kind: BindingKind::SubmoduleImportation,
                            name: alias.node.name.clone(),
                            used: false,
                            location: stmt.location,
                        })
                    } else {
                        self.add_binding(Binding {
                            kind: BindingKind::Importation(
                                alias
                                    .node
                                    .asname
                                    .clone()
                                    .unwrap_or_else(|| alias.node.name.clone()),
                            ),
                            name: alias
                                .node
                                .asname
                                .clone()
                                .unwrap_or_else(|| alias.node.name.clone()),
                            used: false,
                            location: stmt.location,
                        })
                    }
                }
            }
            StmtKind::ImportFrom { names, module, .. } => {
                for alias in names {
                    let name = alias
                        .node
                        .asname
                        .clone()
                        .unwrap_or_else(|| alias.node.name.clone());
                    if module
                        .clone()
                        .map(|name| name == "future")
                        .unwrap_or_default()
                    {
                        self.add_binding(Binding {
                            kind: BindingKind::FutureImportation,
                            name,
                            used: true,
                            location: stmt.location,
                        });
                    } else if alias.node.name == "*" {
                        self.add_binding(Binding {
                            kind: BindingKind::StarImportation,
                            name,
                            used: false,
                            location: stmt.location,
                        });

                        if self
                            .settings
                            .select
                            .contains(CheckKind::ImportStarUsage.code())
                        {
                            self.checks.push(Check {
                                kind: CheckKind::ImportStarUsage,
                                location: stmt.location,
                            });
                        }
                    } else {
                        self.add_binding(Binding {
                            kind: BindingKind::Importation(match module {
                                None => name.clone(),
                                Some(parent) => format!("{}.{}", parent, name),
                            }),
                            name,
                            used: false,
                            location: stmt.location,
                        })
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
            StmtKind::AugAssign { target, .. } => {
                self.handle_node_load(target);
            }
            _ => {}
        }

        visitor::walk_stmt(self, stmt);

        match &stmt.node {
            StmtKind::ClassDef { .. }
            | StmtKind::FunctionDef { .. }
            | StmtKind::AsyncFunctionDef { .. } => {
                self.pop_scope();
            }
            _ => {}
        };

        if let StmtKind::ClassDef { name, .. } = &stmt.node {
            self.add_binding(Binding {
                kind: BindingKind::Definition,
                name: name.clone(),
                used: false,
                location: stmt.location,
            });
        }
    }

    fn visit_expr(&mut self, expr: &Expr) {
        let initial = self.in_f_string;
        match &expr.node {
            ExprKind::Name { ctx, .. } => match ctx {
                ExprContext::Load => self.handle_node_load(expr),
                ExprContext::Store => self.handle_node_store(expr),
                ExprContext::Del => {}
            },
            ExprKind::GeneratorExp { .. } => self.push_scope(Scope {
                kind: Generator,
                values: BTreeMap::new(),
            }),
            ExprKind::Lambda { .. } => self.push_scope(Scope {
                kind: Function,
                values: BTreeMap::new(),
            }),
            ExprKind::JoinedStr { values } => {
                if !self.in_f_string
                    && self
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
                self.in_f_string = true;
            }
            _ => {}
        };

        visitor::walk_expr(self, expr);

        match &expr.node {
            ExprKind::GeneratorExp { .. } | ExprKind::Lambda { .. } => {
                if let Some(scope) = self.scopes.pop() {
                    self.dead_scopes.push(scope);
                }
            }
            ExprKind::JoinedStr { .. } => {
                self.in_f_string = initial;
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

    fn visit_arg(&mut self, arg: &Arg) {
        self.add_binding(Binding {
            kind: BindingKind::Argument,
            name: arg.node.arg.clone(),
            used: false,
            location: arg.location,
        });
        visitor::walk_arg(self, arg);
    }
}

impl Checker<'_> {
    fn push_scope(&mut self, scope: Scope) {
        self.scopes.push(scope);
    }

    fn pop_scope(&mut self) {
        self.dead_scopes
            .push(self.scopes.pop().expect("Attempted to pop without scope."));
    }

    fn add_binding(&mut self, binding: Binding) {
        // TODO(charlie): Don't treat annotations as assignments if there is an existing value.
        let scope = self.scopes.last_mut().expect("No current scope found.");
        scope.values.insert(
            binding.name.clone(),
            match scope.values.get(&binding.name) {
                None => binding,
                Some(existing) => Binding {
                    kind: binding.kind,
                    name: binding.name,
                    location: binding.location,
                    used: existing.used,
                },
            },
        );
    }

    fn handle_node_load(&mut self, expr: &Expr) {
        if let ExprKind::Name { id, .. } = &expr.node {
            for scope in self.scopes.iter_mut().rev() {
                if matches!(scope.kind, Class) {
                    if id == "__class__" {
                        return;
                    } else {
                        continue;
                    }
                }
                if let Some(binding) = scope.values.get_mut(id) {
                    binding.used = true;
                }
            }
        }
    }

    fn handle_node_store(&mut self, expr: &Expr) {
        if let ExprKind::Name { id, .. } = &expr.node {
            // TODO(charlie): Handle alternate binding types (like `Annotation`).
            self.add_binding(Binding {
                kind: BindingKind::Assignment,
                name: id.to_string(),
                used: false,
                location: expr.location,
            });
        }
    }

    fn check_dead_scopes(&mut self) {
        if self.settings.select.contains(&CheckCode::F401) {
            // TODO(charlie): Handle `__all__`.
            for scope in &self.dead_scopes {
                for (_, binding) in scope.values.iter().rev() {
                    if !binding.used {
                        if let BindingKind::Importation(name) = &binding.kind {
                            self.checks.push(Check {
                                kind: CheckKind::UnusedImport(name.clone()),
                                location: binding.location,
                            });
                        }
                    }
                }
            }
        }
    }
}

pub fn check_ast(python_ast: &Suite, settings: &Settings) -> Vec<Check> {
    let mut checker = Checker::new(settings);
    checker.push_scope(Scope {
        kind: Module,
        values: BTreeMap::new(),
    });
    for stmt in python_ast {
        checker.visit_stmt(stmt);
    }
    checker.pop_scope();
    checker.check_dead_scopes();
    checker.checks
}
