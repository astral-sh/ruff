use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicUsize, Ordering};

use rustpython_parser::ast::{
    Arg, Arguments, Constant, Excepthandler, ExcepthandlerKind, Expr, ExprContext, ExprKind,
    Location, Stmt, StmtKind, Suite,
};
use rustpython_parser::parser;

use crate::builtins::{BUILTINS, MAGIC_GLOBALS};
use crate::check_ast::ScopeKind::{Class, Function, Generator, Module};
use crate::checks::{Check, CheckCode, CheckKind};
use crate::settings::Settings;
use crate::visitor;
use crate::visitor::{walk_excepthandler, Visitor};

fn id() -> usize {
    static COUNTER: AtomicUsize = AtomicUsize::new(1);
    COUNTER.fetch_add(1, Ordering::Relaxed)
}

enum ScopeKind {
    Class,
    Function,
    Generator,
    Module,
}

struct Scope {
    id: usize,
    kind: ScopeKind,
    values: BTreeMap<String, Binding>,
}

impl Scope {
    fn new(kind: ScopeKind) -> Self {
        Scope {
            id: id(),
            kind,
            values: BTreeMap::new(),
        }
    }
}

#[derive(Clone)]
enum BindingKind {
    Argument,
    Assignment,
    Definition,
    ClassDefinition,
    Builtin,
    FutureImportation,
    Importation(String),
    StarImportation,
    SubmoduleImportation(String),
}

#[derive(Clone)]
struct Binding {
    kind: BindingKind,
    location: Location,
    used: Option<usize>,
}

struct Checker<'a> {
    settings: &'a Settings,
    checks: Vec<Check>,
    scopes: Vec<Scope>,
    dead_scopes: Vec<Scope>,
    deferred: Vec<String>,
    in_f_string: bool,
    in_annotation: bool,
}

impl Checker<'_> {
    pub fn new(settings: &Settings) -> Checker {
        Checker {
            settings,
            checks: vec![],
            scopes: vec![],
            dead_scopes: vec![],
            deferred: vec![],
            in_f_string: false,
            in_annotation: false,
        }
    }
}

impl Visitor for Checker<'_> {
    fn visit_stmt(&mut self, stmt: &Stmt) {
        match &stmt.node {
            StmtKind::Global { names } | StmtKind::Nonlocal { names } => {
                // TODO(charlie): Handle doctests.
                let global_scope_index = 0;
                let global_scope_id = self.scopes[global_scope_index].id;
                let current_scope_id = self.scopes.last().expect("No current scope found.").id;
                if current_scope_id != global_scope_id {
                    for name in names {
                        for scope in self.scopes.iter_mut().skip(global_scope_index + 1) {
                            scope.values.insert(
                                name.to_string(),
                                Binding {
                                    kind: BindingKind::Assignment,
                                    used: Some(global_scope_id),
                                    location: stmt.location,
                                },
                            );
                        }
                    }
                }
            }
            StmtKind::FunctionDef { name, .. } => {
                self.add_binding(
                    name.to_string(),
                    Binding {
                        kind: BindingKind::Definition,
                        used: None,
                        location: stmt.location,
                    },
                );
                self.push_scope(Scope::new(Function));
            }
            StmtKind::AsyncFunctionDef { name, .. } => {
                self.add_binding(
                    name.to_string(),
                    Binding {
                        kind: BindingKind::Definition,
                        used: None,
                        location: stmt.location,
                    },
                );
                self.push_scope(Scope::new(Function));
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
            StmtKind::ClassDef { .. } => self.push_scope(Scope::new(Class)),
            StmtKind::Import { names } => {
                for alias in names {
                    if alias.node.name.contains('.') && alias.node.asname.is_none() {
                        // TODO(charlie): Multiple submodule imports with the same parent module
                        // will be merged into a single binding.
                        self.add_binding(
                            alias.node.name.split('.').next().unwrap().to_string(),
                            Binding {
                                kind: BindingKind::SubmoduleImportation(
                                    alias.node.name.to_string(),
                                ),
                                used: None,
                                location: stmt.location,
                            },
                        )
                    } else {
                        self.add_binding(
                            alias
                                .node
                                .asname
                                .clone()
                                .unwrap_or_else(|| alias.node.name.clone()),
                            Binding {
                                kind: BindingKind::Importation(
                                    alias
                                        .node
                                        .asname
                                        .clone()
                                        .unwrap_or_else(|| alias.node.name.clone()),
                                ),
                                used: None,
                                location: stmt.location,
                            },
                        )
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
                    if let Some("future") = module.as_deref() {
                        self.add_binding(
                            name,
                            Binding {
                                kind: BindingKind::FutureImportation,

                                used: Some(self.scopes.last().expect("No current scope found.").id),
                                location: stmt.location,
                            },
                        );
                    } else if alias.node.name == "*" {
                        self.add_binding(
                            name,
                            Binding {
                                kind: BindingKind::StarImportation,

                                used: None,
                                location: stmt.location,
                            },
                        );

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
                        let binding = Binding {
                            kind: BindingKind::Importation(match module {
                                None => name.clone(),
                                Some(parent) => format!("{}.{}", parent, name.clone()),
                            }),
                            used: None,
                            location: stmt.location,
                        };
                        self.add_binding(name, binding)
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
            StmtKind::AugAssign { target, .. } => self.handle_node_load(target),
            _ => {}
        }

        visitor::walk_stmt(self, stmt);

        match &stmt.node {
            StmtKind::ClassDef { .. } => {
                if let Some(scope) = self.scopes.pop() {
                    self.dead_scopes.push(scope);
                }
            }
            StmtKind::FunctionDef { .. } | StmtKind::AsyncFunctionDef { .. } => {
                let scope = self.scopes.last().expect("No current scope found.");
                for (name, binding) in scope.values.iter() {
                    // TODO(charlie): Ignore if using `locals`.
                    if self.settings.select.contains(&CheckCode::F841)
                        && binding.used.is_none()
                        && name != "_"
                        && name != "__tracebackhide__"
                        && name != "__traceback_info__"
                        && name != "__traceback_supplement__"
                        && matches!(binding.kind, BindingKind::Assignment)
                    {
                        self.checks.push(Check {
                            kind: CheckKind::UnusedVariable(name.to_string()),
                            location: binding.location,
                        });
                    }
                }

                if let Some(scope) = self.scopes.pop() {
                    self.dead_scopes.push(scope);
                }
            }
            _ => {}
        };

        if let StmtKind::ClassDef { name, .. } = &stmt.node {
            self.add_binding(
                name.to_string(),
                Binding {
                    kind: BindingKind::ClassDefinition,
                    used: None,
                    location: stmt.location,
                },
            );
        }
    }
    fn visit_annotation(&mut self, expr: &Expr) {
        let initial = self.in_annotation;
        self.in_annotation = true;
        self.visit_expr(expr);
        self.in_annotation = initial;
    }

    fn visit_expr(&mut self, expr: &Expr) {
        let initial = self.in_f_string;
        match &expr.node {
            ExprKind::Name { ctx, .. } => match ctx {
                ExprContext::Load => self.handle_node_load(expr),
                ExprContext::Store => self.handle_node_store(expr),
                ExprContext::Del => self.handle_node_delete(expr),
            },
            ExprKind::GeneratorExp { .. }
            | ExprKind::ListComp { .. }
            | ExprKind::DictComp { .. }
            | ExprKind::SetComp { .. } => self.push_scope(Scope::new(Generator)),
            ExprKind::Lambda { .. } => self.push_scope(Scope::new(Function)),
            ExprKind::Yield { .. } | ExprKind::YieldFrom { .. } => {
                let scope = self.scopes.last().expect("No current scope found.");
                if self
                    .settings
                    .select
                    .contains(CheckKind::YieldOutsideFunction.code())
                    && matches!(scope.kind, ScopeKind::Class)
                    || matches!(scope.kind, ScopeKind::Module)
                {
                    self.checks.push(Check {
                        kind: CheckKind::YieldOutsideFunction,
                        location: expr.location,
                    });
                }
            }
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
            ExprKind::Constant {
                value: Constant::Str(value),
                ..
            } if self.in_annotation => self.deferred.push(value.to_string()),
            _ => {}
        };

        visitor::walk_expr(self, expr);

        match &expr.node {
            ExprKind::GeneratorExp { .. }
            | ExprKind::ListComp { .. }
            | ExprKind::DictComp { .. }
            | ExprKind::SetComp { .. }
            | ExprKind::Lambda { .. } => {
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

    fn visit_excepthandler(&mut self, excepthandler: &Excepthandler) {
        match &excepthandler.node {
            ExcepthandlerKind::ExceptHandler { name, .. } => match name {
                Some(name) => {
                    let scope = self.scopes.last().expect("No current scope found.");
                    if scope.values.contains_key(name) {
                        self.handle_node_store(&Expr::new(
                            excepthandler.location,
                            ExprKind::Name {
                                id: name.to_string(),
                                ctx: ExprContext::Store,
                            },
                        ));
                    }

                    let scope = self.scopes.last().expect("No current scope found.");
                    let prev_definition = scope.values.get(name).cloned();
                    self.handle_node_store(&Expr::new(
                        excepthandler.location,
                        ExprKind::Name {
                            id: name.to_string(),
                            ctx: ExprContext::Store,
                        },
                    ));

                    walk_excepthandler(self, excepthandler);

                    let scope = self.scopes.last_mut().expect("No current scope found.");
                    if let Some(binding) = scope.values.remove(name) {
                        if self.settings.select.contains(&CheckCode::F841) && binding.used.is_none()
                        {
                            self.checks.push(Check {
                                kind: CheckKind::UnusedVariable(name.to_string()),
                                location: excepthandler.location,
                            });
                        }
                    }

                    if let Some(binding) = prev_definition {
                        scope.values.insert(name.to_string(), binding);
                    }
                }
                None => walk_excepthandler(self, excepthandler),
            },
        }
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
            let mut idents: BTreeSet<&str> = BTreeSet::new();
            for arg in all_arguments {
                let ident = &arg.node.arg;
                if idents.contains(ident.as_str()) {
                    self.checks.push(Check {
                        kind: CheckKind::DuplicateArgumentName,
                        location: arg.location,
                    });
                    break;
                }
                idents.insert(ident);
            }
        }

        visitor::walk_arguments(self, arguments);
    }

    fn visit_arg(&mut self, arg: &Arg) {
        self.add_binding(
            arg.node.arg.to_string(),
            Binding {
                kind: BindingKind::Argument,
                used: None,
                location: arg.location,
            },
        );
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

    fn bind_builtins(&mut self) {
        for builtin in BUILTINS {
            self.add_binding(
                builtin.to_string(),
                Binding {
                    kind: BindingKind::Builtin,
                    location: Default::default(),
                    used: None,
                },
            )
        }
        for builtin in MAGIC_GLOBALS {
            self.add_binding(
                builtin.to_string(),
                Binding {
                    kind: BindingKind::Builtin,
                    location: Default::default(),
                    used: None,
                },
            )
        }
    }

    fn add_binding(&mut self, name: String, binding: Binding) {
        let scope = self.scopes.last_mut().expect("No current scope found.");

        // TODO(charlie): Don't treat annotations as assignments if there is an existing value.
        let binding = match scope.values.get(&name) {
            None => binding,
            Some(existing) => Binding {
                kind: binding.kind,
                location: binding.location,
                used: existing.used,
            },
        };
        scope.values.insert(name, binding);
    }

    fn handle_node_load(&mut self, expr: &Expr) {
        if let ExprKind::Name { id, .. } = &expr.node {
            let scope_id = self.scopes.last_mut().expect("No current scope found.").id;
            let mut first_iter = true;
            let mut in_generators = false;
            for scope in self.scopes.iter_mut().rev() {
                if matches!(scope.kind, Class) {
                    if id == "__class__" {
                        return;
                    } else if !first_iter && !in_generators {
                        continue;
                    }
                }
                if let Some(binding) = scope.values.get_mut(id) {
                    binding.used = Some(scope_id);
                    return;
                }

                first_iter = false;
                in_generators = matches!(scope.kind, Generator);
            }

            if self.settings.select.contains(&CheckCode::F821) {
                self.checks.push(Check {
                    kind: CheckKind::UndefinedName(id.clone()),
                    location: expr.location,
                })
            }
        }
    }

    fn handle_node_store(&mut self, expr: &Expr) {
        if let ExprKind::Name { id, .. } = &expr.node {
            if self.settings.select.contains(&CheckCode::F832) {
                let current = self.scopes.last().expect("No current scope found.");
                if matches!(current.kind, ScopeKind::Function) && !current.values.contains_key(id) {
                    for scope in self.scopes.iter().rev().skip(1) {
                        if matches!(scope.kind, ScopeKind::Function) || matches!(scope.kind, Module)
                        {
                            let used = scope
                                .values
                                .get(id)
                                .map(|binding| binding.used)
                                .unwrap_or_default();
                            if let Some(scope_id) = used {
                                if scope_id == current.id {
                                    self.checks.push(Check {
                                        kind: CheckKind::UndefinedLocal(id.clone()),
                                        location: expr.location,
                                    });
                                }
                            }
                        }
                    }
                }
            }

            // TODO(charlie): Handle alternate binding types (like `Annotation`).
            self.add_binding(
                id.to_string(),
                Binding {
                    kind: BindingKind::Assignment,
                    used: None,
                    location: expr.location,
                },
            );
        }
    }

    fn handle_node_delete(&mut self, expr: &Expr) {
        if let ExprKind::Name { id, .. } = &expr.node {
            let current = self.scopes.last_mut().expect("No current scope found.");
            if current.values.remove(id).is_none()
                && self.settings.select.contains(&CheckCode::F821)
            {
                self.checks.push(Check {
                    kind: CheckKind::UndefinedName(id.clone()),
                    location: expr.location,
                })
            }
        }
    }

    fn check_deferred(&mut self, path: &str) {
        for value in self.deferred.clone() {
            if let Ok(expr) = &parser::parse_expression(&value, path) {
                self.visit_expr(expr);
            }
        }
    }

    fn check_dead_scopes(&mut self) {
        if self.settings.select.contains(&CheckCode::F401) {
            // TODO(charlie): Handle `__all__`.
            for scope in &self.dead_scopes {
                for (_, binding) in scope.values.iter().rev() {
                    if binding.used.is_none() {
                        match &binding.kind {
                            BindingKind::Importation(full_name)
                            | BindingKind::SubmoduleImportation(full_name) => {
                                self.checks.push(Check {
                                    kind: CheckKind::UnusedImport(full_name.to_string()),
                                    location: binding.location,
                                });
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
    }
}

pub fn check_ast(python_ast: &Suite, settings: &Settings, path: &str) -> Vec<Check> {
    let mut checker = Checker::new(settings);
    checker.push_scope(Scope::new(Module));
    checker.bind_builtins();

    for stmt in python_ast {
        checker.visit_stmt(stmt);
    }
    checker.check_deferred(path);

    checker.pop_scope();
    checker.check_dead_scopes();
    checker.checks
}
