use std::collections::{BTreeMap, BTreeSet};
use std::ops::Deref;
use std::path::Path;

use log::error;
use once_cell::sync::Lazy;
use regex::Regex;
use rustpython_parser::ast::{
    Arg, Arguments, Constant, Excepthandler, ExcepthandlerKind, Expr, ExprContext, ExprKind,
    KeywordData, Operator, Stmt, StmtKind, Suite,
};
use rustpython_parser::parser;

use crate::ast::operations::{extract_all_names, SourceCodeLocator};
use crate::ast::relocate::relocate_expr;
use crate::ast::types::{
    Binding, BindingContext, BindingKind, CheckLocator, FunctionScope, ImportKind, Range, Scope,
    ScopeKind,
};
use crate::ast::visitor::{walk_excepthandler, Visitor};
use crate::ast::{checks, operations, visitor};
use crate::autofix::{fixer, fixes};
use crate::checks::{Check, CheckCode, CheckKind};
use crate::plugins;
use crate::python::builtins::{BUILTINS, MAGIC_GLOBALS};
use crate::python::future::ALL_FEATURE_NAMES;
use crate::python::typing;
use crate::settings::{PythonVersion, Settings};

pub const GLOBAL_SCOPE_INDEX: usize = 0;

static DUNDER_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"__[^\s]+__").unwrap());

pub struct Checker<'a> {
    // Input data.
    path: &'a Path,
    // TODO(charlie): Separate immutable from mutable state. (None of these should ever change.)
    pub(crate) locator: SourceCodeLocator<'a>,
    pub(crate) settings: &'a Settings,
    pub(crate) autofix: &'a fixer::Mode,
    // Computed checks.
    checks: Vec<Check>,
    // Edit tracking.
    // TODO(charlie): Instead of exposing deletions, wrap in a public API.
    pub(crate) deletions: BTreeSet<usize>,
    // Retain all scopes and parent nodes, along with a stack of indexes to track which are active
    // at various points in time.
    pub(crate) parents: Vec<&'a Stmt>,
    pub(crate) parent_stack: Vec<usize>,
    scopes: Vec<Scope>,
    scope_stack: Vec<usize>,
    dead_scopes: Vec<usize>,
    deferred_string_annotations: Vec<(Range, &'a str)>,
    deferred_annotations: Vec<(&'a Expr, Vec<usize>, Vec<usize>)>,
    deferred_functions: Vec<(&'a Stmt, Vec<usize>, Vec<usize>)>,
    deferred_lambdas: Vec<(&'a Expr, Vec<usize>, Vec<usize>)>,
    deferred_assignments: Vec<usize>,
    // Internal, derivative state.
    in_f_string: Option<Range>,
    in_annotation: bool,
    in_literal: bool,
    seen_non_import: bool,
    seen_docstring: bool,
    futures_allowed: bool,
    annotations_future_enabled: bool,
}

impl<'a> Checker<'a> {
    pub fn new(
        settings: &'a Settings,
        autofix: &'a fixer::Mode,
        path: &'a Path,
        content: &'a str,
    ) -> Checker<'a> {
        Checker {
            settings,
            autofix,
            path,
            locator: SourceCodeLocator::new(content),
            checks: vec![],
            parents: vec![],
            parent_stack: vec![],
            scopes: vec![],
            scope_stack: vec![],
            dead_scopes: vec![],
            deferred_string_annotations: vec![],
            deferred_annotations: vec![],
            deferred_functions: vec![],
            deferred_lambdas: vec![],
            deferred_assignments: vec![],
            in_f_string: None,
            in_annotation: false,
            in_literal: false,
            seen_non_import: false,
            seen_docstring: false,
            futures_allowed: true,
            annotations_future_enabled: false,
            deletions: Default::default(),
        }
    }
}

fn match_name_or_attr(expr: &Expr, target: &str) -> bool {
    match &expr.node {
        ExprKind::Attribute { attr, .. } => target == attr,
        ExprKind::Name { id, .. } => target == id,
        _ => false,
    }
}

enum SubscriptKind {
    AnnotatedSubscript,
    PEP593AnnotatedSubscript,
}

fn match_annotated_subscript(expr: &Expr) -> Option<SubscriptKind> {
    match &expr.node {
        ExprKind::Attribute { attr, .. } => {
            if typing::is_annotated_subscript(attr) {
                Some(SubscriptKind::AnnotatedSubscript)
            } else if typing::is_pep593_annotated_subscript(attr) {
                Some(SubscriptKind::PEP593AnnotatedSubscript)
            } else {
                None
            }
        }
        ExprKind::Name { id, .. } => {
            if typing::is_annotated_subscript(id) {
                Some(SubscriptKind::AnnotatedSubscript)
            } else if typing::is_pep593_annotated_subscript(id) {
                Some(SubscriptKind::PEP593AnnotatedSubscript)
            } else {
                None
            }
        }
        _ => None,
    }
}

fn is_assignment_to_a_dunder(node: &StmtKind) -> bool {
    // Check whether it's an assignment to a dunder, with or without a type annotation.
    // This is what pycodestyle (as of 2.9.1) does.
    match node {
        StmtKind::Assign {
            targets,
            value: _,
            type_comment: _,
        } => {
            if targets.len() != 1 {
                return false;
            }
            match &targets[0].node {
                ExprKind::Name { id, ctx: _ } => DUNDER_REGEX.is_match(id),
                _ => false,
            }
        }
        StmtKind::AnnAssign {
            target,
            annotation: _,
            value: _,
            simple: _,
        } => match &target.node {
            ExprKind::Name { id, ctx: _ } => DUNDER_REGEX.is_match(id),
            _ => false,
        },
        _ => false,
    }
}

impl<'a, 'b> Visitor<'b> for Checker<'a>
where
    'b: 'a,
{
    fn visit_stmt(&mut self, stmt: &'b Stmt) {
        self.push_parent(stmt);

        // Track whether we've seen docstrings, non-imports, etc.
        match &stmt.node {
            StmtKind::ImportFrom { module, .. } => {
                // Allow __future__ imports until we see a non-__future__ import.
                if self.futures_allowed {
                    if let Some(module) = module {
                        if module != "__future__" {
                            self.futures_allowed = false;
                        }
                    }
                }
            }
            StmtKind::Import { .. } => {
                self.futures_allowed = false;
            }
            StmtKind::Expr { value } => {
                if self.seen_docstring
                    && !self.seen_non_import
                    && !operations::in_nested_block(&self.parent_stack, &self.parents)
                {
                    self.seen_non_import = true;
                }

                if !self.seen_docstring
                    && !operations::in_nested_block(&self.parent_stack, &self.parents)
                    && matches!(
                        &value.node,
                        ExprKind::Constant {
                            value: Constant::Str(_),
                            ..
                        },
                    )
                {
                    self.seen_docstring = true;
                }

                // Allow docstrings to interrupt __future__ imports.
                if self.futures_allowed
                    && !matches!(
                        &value.node,
                        ExprKind::Constant {
                            value: Constant::Str(_),
                            ..
                        },
                    )
                {
                    self.futures_allowed = false;
                }
            }
            node => {
                self.futures_allowed = false;

                if !self.seen_non_import
                    && !is_assignment_to_a_dunder(node)
                    && !operations::in_nested_block(&self.parent_stack, &self.parents)
                {
                    self.seen_non_import = true;
                }
            }
        }

        // Pre-visit.
        match &stmt.node {
            StmtKind::Global { names } | StmtKind::Nonlocal { names } => {
                let global_scope_id = self.scopes[GLOBAL_SCOPE_INDEX].id;

                let current_scope = self.current_scope();
                let current_scope_id = current_scope.id;
                if current_scope_id != global_scope_id {
                    for name in names {
                        for scope in self.scopes.iter_mut().skip(GLOBAL_SCOPE_INDEX + 1) {
                            scope.values.insert(
                                name.to_string(),
                                Binding {
                                    kind: BindingKind::Assignment,
                                    used: Some((global_scope_id, Range::from_located(stmt))),
                                    range: Range::from_located(stmt),
                                },
                            );
                        }
                    }
                }

                if self.settings.enabled.contains(&CheckCode::E741) {
                    let location = self.locate_check(Range::from_located(stmt));
                    self.checks.extend(
                        names.iter().filter_map(|name| {
                            checks::check_ambiguous_variable_name(name, location)
                        }),
                    );
                }
            }
            StmtKind::Break => {
                if self.settings.enabled.contains(&CheckCode::F701) {
                    if let Some(check) = checks::check_break_outside_loop(
                        stmt,
                        &self.parents,
                        &self.parent_stack,
                        self,
                    ) {
                        self.checks.push(check);
                    }
                }
            }
            StmtKind::Continue => {
                if self.settings.enabled.contains(&CheckCode::F702) {
                    if let Some(check) = checks::check_continue_outside_loop(
                        stmt,
                        &self.parents,
                        &self.parent_stack,
                        self,
                    ) {
                        self.checks.push(check);
                    }
                }
            }
            StmtKind::FunctionDef {
                name,
                decorator_list,
                returns,
                args,
                ..
            }
            | StmtKind::AsyncFunctionDef {
                name,
                decorator_list,
                returns,
                args,
                ..
            } => {
                if self.settings.enabled.contains(&CheckCode::E743) {
                    if let Some(check) = checks::check_ambiguous_function_name(
                        name,
                        self.locate_check(Range::from_located(stmt)),
                    ) {
                        self.checks.push(check);
                    }
                }

                self.check_builtin_shadowing(name, Range::from_located(stmt), true);

                // Visit the decorators and arguments, but avoid the body, which will be deferred.
                for expr in decorator_list {
                    self.visit_expr(expr);
                }
                for arg in &args.posonlyargs {
                    if let Some(expr) = &arg.node.annotation {
                        self.visit_annotation(expr);
                    }
                }
                for arg in &args.args {
                    if let Some(expr) = &arg.node.annotation {
                        self.visit_annotation(expr);
                    }
                }
                if let Some(arg) = &args.vararg {
                    if let Some(expr) = &arg.node.annotation {
                        self.visit_annotation(expr);
                    }
                }
                for arg in &args.kwonlyargs {
                    if let Some(expr) = &arg.node.annotation {
                        self.visit_annotation(expr);
                    }
                }
                if let Some(arg) = &args.kwarg {
                    if let Some(expr) = &arg.node.annotation {
                        self.visit_annotation(expr);
                    }
                }
                for expr in returns {
                    self.visit_annotation(expr);
                }
                for expr in &args.kw_defaults {
                    self.visit_expr(expr);
                }
                for expr in &args.defaults {
                    self.visit_expr(expr);
                }
                self.add_binding(
                    name.to_string(),
                    Binding {
                        kind: BindingKind::Definition,
                        used: None,
                        range: Range::from_located(stmt),
                    },
                );
            }
            StmtKind::Return { .. } => {
                if self
                    .settings
                    .enabled
                    .contains(CheckKind::ReturnOutsideFunction.code())
                {
                    if let Some(scope_index) = self.scope_stack.last().cloned() {
                        match self.scopes[scope_index].kind {
                            ScopeKind::Class | ScopeKind::Module => {
                                self.checks.push(Check::new(
                                    CheckKind::ReturnOutsideFunction,
                                    self.locate_check(Range::from_located(stmt)),
                                ));
                            }
                            _ => {}
                        }
                    }
                }
            }
            StmtKind::ClassDef {
                name,
                bases,
                keywords,
                decorator_list,
                ..
            } => {
                if self.settings.enabled.contains(&CheckCode::U004) {
                    plugins::useless_object_inheritance(self, stmt, name, bases, keywords);
                }

                if self.settings.enabled.contains(&CheckCode::E742) {
                    if let Some(check) = checks::check_ambiguous_class_name(
                        name,
                        self.locate_check(Range::from_located(stmt)),
                    ) {
                        self.checks.push(check);
                    }
                }

                self.check_builtin_shadowing(
                    name,
                    self.locate_check(Range::from_located(stmt)),
                    false,
                );

                for expr in bases {
                    self.visit_expr(expr)
                }
                for keyword in keywords {
                    self.visit_keyword(keyword)
                }
                for expr in decorator_list {
                    self.visit_expr(expr)
                }
                self.push_scope(Scope::new(ScopeKind::Class))
            }
            StmtKind::Import { names } => {
                if self
                    .settings
                    .enabled
                    .contains(CheckKind::ModuleImportNotAtTopOfFile.code())
                    && self.seen_non_import
                    && stmt.location.column() == 1
                {
                    self.checks.push(Check::new(
                        CheckKind::ModuleImportNotAtTopOfFile,
                        self.locate_check(Range::from_located(stmt)),
                    ));
                }

                for alias in names {
                    if alias.node.name.contains('.') && alias.node.asname.is_none() {
                        // TODO(charlie): Multiple submodule imports with the same parent module
                        // will be merged into a single binding.
                        self.add_binding(
                            alias.node.name.split('.').next().unwrap().to_string(),
                            Binding {
                                kind: BindingKind::SubmoduleImportation(
                                    alias.node.name.to_string(),
                                    self.binding_context(),
                                ),
                                used: None,
                                range: Range::from_located(stmt),
                            },
                        )
                    } else {
                        if let Some(asname) = &alias.node.asname {
                            self.check_builtin_shadowing(asname, Range::from_located(stmt), false);
                        }

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
                                    self.binding_context(),
                                ),
                                used: None,
                                range: Range::from_located(stmt),
                            },
                        )
                    }
                }
            }
            StmtKind::ImportFrom {
                names,
                module,
                level,
            } => {
                if self
                    .settings
                    .enabled
                    .contains(CheckKind::ModuleImportNotAtTopOfFile.code())
                    && self.seen_non_import
                    && stmt.location.column() == 1
                {
                    self.checks.push(Check::new(
                        CheckKind::ModuleImportNotAtTopOfFile,
                        self.locate_check(Range::from_located(stmt)),
                    ));
                }

                for alias in names {
                    let name = alias
                        .node
                        .asname
                        .clone()
                        .unwrap_or_else(|| alias.node.name.clone());
                    if let Some("__future__") = module.as_deref() {
                        self.add_binding(
                            name,
                            Binding {
                                kind: BindingKind::FutureImportation,
                                used: Some((
                                    self.scopes[*(self
                                        .scope_stack
                                        .last()
                                        .expect("No current scope found."))]
                                    .id,
                                    Range::from_located(stmt),
                                )),
                                range: Range::from_located(stmt),
                            },
                        );

                        if alias.node.name == "annotations" {
                            self.annotations_future_enabled = true;
                        }

                        if self.settings.enabled.contains(&CheckCode::F407)
                            && !ALL_FEATURE_NAMES.contains(&alias.node.name.deref())
                        {
                            self.checks.push(Check::new(
                                CheckKind::FutureFeatureNotDefined(alias.node.name.to_string()),
                                self.locate_check(Range::from_located(stmt)),
                            ));
                        }

                        if self.settings.enabled.contains(&CheckCode::F404) && !self.futures_allowed
                        {
                            self.checks.push(Check::new(
                                CheckKind::LateFutureImport,
                                self.locate_check(Range::from_located(stmt)),
                            ));
                        }
                    } else if alias.node.name == "*" {
                        let module_name = format!(
                            "{}{}",
                            ".".repeat(level.unwrap_or_default()),
                            module.clone().unwrap_or_else(|| "module".to_string()),
                        );

                        self.add_binding(
                            module_name.to_string(),
                            Binding {
                                kind: BindingKind::StarImportation,
                                used: None,
                                range: Range::from_located(stmt),
                            },
                        );

                        if self.settings.enabled.contains(&CheckCode::F406) {
                            let scope = &self.scopes
                                [*(self.scope_stack.last().expect("No current scope found."))];
                            if !matches!(scope.kind, ScopeKind::Module) {
                                self.checks.push(Check::new(
                                    CheckKind::ImportStarNotPermitted(module_name.to_string()),
                                    self.locate_check(Range::from_located(stmt)),
                                ));
                            }
                        }

                        if self.settings.enabled.contains(&CheckCode::F403) {
                            self.checks.push(Check::new(
                                CheckKind::ImportStarUsed(module_name.to_string()),
                                self.locate_check(Range::from_located(stmt)),
                            ));
                        }

                        let scope = &mut self.scopes[*(self
                            .scope_stack
                            .last_mut()
                            .expect("No current scope found."))];
                        scope.import_starred = true;
                    } else {
                        if let Some(asname) = &alias.node.asname {
                            self.check_builtin_shadowing(asname, Range::from_located(stmt), false);
                        }

                        let binding = Binding {
                            kind: BindingKind::FromImportation(
                                match module {
                                    None => name.clone(),
                                    Some(parent) => format!("{}.{}", parent, name),
                                },
                                self.binding_context(),
                            ),
                            used: None,
                            range: Range::from_located(stmt),
                        };
                        self.add_binding(name, binding)
                    }
                }
            }
            StmtKind::Raise { exc, .. } => {
                if self.settings.enabled.contains(&CheckCode::F901) {
                    if let Some(expr) = exc {
                        if let Some(check) = checks::check_raise_not_implemented(expr) {
                            self.checks.push(check);
                        }
                    }
                }
            }
            StmtKind::AugAssign { target, .. } => {
                self.handle_node_load(target);
            }
            StmtKind::If { test, .. } => {
                if self.settings.enabled.contains(&CheckCode::F634) {
                    plugins::if_tuple(self, stmt, test);
                }
            }
            StmtKind::Assert { test, .. } => {
                if self.settings.enabled.contains(&CheckCode::F631) {
                    plugins::assert_tuple(self, stmt, test);
                }
            }
            StmtKind::Try { handlers, .. } => {
                if self.settings.enabled.contains(&CheckCode::F707) {
                    if let Some(check) = checks::check_default_except_not_last(handlers) {
                        self.checks.push(check);
                    }
                }
            }
            StmtKind::Assign { targets, value, .. } => {
                if self.settings.enabled.contains(&CheckCode::E731) {
                    if let Some(check) = checks::check_do_not_assign_lambda(
                        value,
                        self.locate_check(Range::from_located(stmt)),
                    ) {
                        self.checks.push(check);
                    }
                }
                if self.settings.enabled.contains(&CheckCode::U001) {
                    plugins::useless_metaclass_type(self, stmt, value, targets);
                }
            }
            StmtKind::AnnAssign { value, .. } => {
                if self.settings.enabled.contains(&CheckCode::E731) {
                    if let Some(value) = value {
                        if let Some(check) = checks::check_do_not_assign_lambda(
                            value,
                            self.locate_check(Range::from_located(stmt)),
                        ) {
                            self.checks.push(check);
                        }
                    }
                }
            }
            StmtKind::Delete { .. } => {}
            _ => {}
        }

        // Recurse.
        match &stmt.node {
            StmtKind::FunctionDef { .. } | StmtKind::AsyncFunctionDef { .. } => {
                self.deferred_functions.push((
                    stmt,
                    self.scope_stack.clone(),
                    self.parent_stack.clone(),
                ));
            }
            StmtKind::ClassDef { body, .. } => {
                for stmt in body {
                    self.visit_stmt(stmt);
                }
            }
            _ => visitor::walk_stmt(self, stmt),
        };

        // Post-visit.
        if let StmtKind::ClassDef { name, .. } = &stmt.node {
            self.pop_scope();
            self.add_binding(
                name.to_string(),
                Binding {
                    kind: BindingKind::ClassDefinition,
                    used: None,
                    range: Range::from_located(stmt),
                },
            );
        };

        self.pop_parent();
    }

    fn visit_annotation(&mut self, expr: &'b Expr) {
        let prev_in_annotation = self.in_annotation;
        self.in_annotation = true;
        self.visit_expr(expr);
        self.in_annotation = prev_in_annotation;
    }

    fn visit_expr(&mut self, expr: &'b Expr) {
        let prev_in_f_string = self.in_f_string;
        let prev_in_literal = self.in_literal;
        let prev_in_annotation = self.in_annotation;

        if self.in_annotation && self.annotations_future_enabled {
            self.deferred_annotations.push((
                expr,
                self.scope_stack.clone(),
                self.parent_stack.clone(),
            ));
            visitor::walk_expr(self, expr);
            return;
        }

        // Pre-visit.
        match &expr.node {
            ExprKind::Subscript { value, .. } => {
                if match_name_or_attr(value, "Literal") {
                    self.in_literal = true;
                }
            }
            ExprKind::Tuple { elts, ctx } | ExprKind::List { elts, ctx } => {
                if matches!(ctx, ExprContext::Store) {
                    let check_too_many_expressions =
                        self.settings.enabled.contains(&CheckCode::F621);
                    let check_two_starred_expressions =
                        self.settings.enabled.contains(&CheckCode::F622);
                    if let Some(check) = checks::check_starred_expressions(
                        elts,
                        check_too_many_expressions,
                        check_two_starred_expressions,
                        self.locate_check(Range::from_located(expr)),
                    ) {
                        self.checks.push(check);
                    }
                }
            }
            ExprKind::Name { id, ctx } => match ctx {
                ExprContext::Load => self.handle_node_load(expr),
                ExprContext::Store => {
                    if self.settings.enabled.contains(&CheckCode::E741) {
                        if let Some(check) = checks::check_ambiguous_variable_name(
                            id,
                            self.locate_check(Range::from_located(expr)),
                        ) {
                            self.checks.push(check);
                        }
                    }

                    self.check_builtin_shadowing(id, Range::from_located(expr), true);

                    self.handle_node_store(expr, self.current_parent());
                }
                ExprContext::Del => self.handle_node_delete(expr),
            },
            ExprKind::Call {
                func,
                args,
                keywords,
                ..
            } => {
                if self.settings.enabled.contains(&CheckCode::U005) {
                    plugins::assert_equals(self, func);
                }

                // flake8-super
                if self.settings.enabled.contains(&CheckCode::SPR001) {
                    plugins::super_call_with_parameters(self, expr, func, args);
                }

                // flake8-print
                if self.settings.enabled.contains(&CheckCode::T201)
                    || self.settings.enabled.contains(&CheckCode::T203)
                {
                    plugins::print_call(self, expr, func);
                }

                // flake8-comprehensions
                if self.settings.enabled.contains(&CheckCode::C400) {
                    if let Some(check) = checks::unnecessary_generator_list(expr, func, args) {
                        self.checks.push(check);
                    };
                }

                if self.settings.enabled.contains(&CheckCode::C401) {
                    if let Some(check) = checks::unnecessary_generator_set(expr, func, args) {
                        self.checks.push(check);
                    };
                }

                if self.settings.enabled.contains(&CheckCode::C402) {
                    if let Some(check) = checks::unnecessary_generator_dict(expr, func, args) {
                        self.checks.push(check);
                    };
                }

                if self.settings.enabled.contains(&CheckCode::C403) {
                    if let Some(check) =
                        checks::unnecessary_list_comprehension_set(expr, func, args)
                    {
                        self.checks.push(check);
                    };
                }

                if self.settings.enabled.contains(&CheckCode::C404) {
                    if let Some(check) =
                        checks::unnecessary_list_comprehension_dict(expr, func, args)
                    {
                        self.checks.push(check);
                    };
                }

                if self.settings.enabled.contains(&CheckCode::C405) {
                    if let Some(check) = checks::unnecessary_literal_set(expr, func, args) {
                        self.checks.push(check);
                    };
                }

                if self.settings.enabled.contains(&CheckCode::C406) {
                    if let Some(check) = checks::unnecessary_literal_dict(expr, func, args) {
                        self.checks.push(check);
                    };
                }

                if self.settings.enabled.contains(&CheckCode::C408) {
                    if let Some(check) =
                        checks::unnecessary_collection_call(expr, func, args, keywords)
                    {
                        self.checks.push(check);
                    };
                }

                // pyupgrade
                if self.settings.enabled.contains(&CheckCode::U002)
                    && self.settings.target_version >= PythonVersion::Py310
                {
                    plugins::unnecessary_abspath(self, expr, func, args);
                }

                if self.settings.enabled.contains(&CheckCode::U003) {
                    plugins::type_of_primitive(self, expr, func, args);
                }

                if let ExprKind::Name { id, ctx } = &func.node {
                    if id == "locals" && matches!(ctx, ExprContext::Load) {
                        let scope = &mut self.scopes[*(self
                            .scope_stack
                            .last_mut()
                            .expect("No current scope found."))];
                        if matches!(
                            scope.kind,
                            ScopeKind::Function(FunctionScope { uses_locals: false })
                        ) {
                            scope.kind = ScopeKind::Function(FunctionScope { uses_locals: true });
                        }
                    }
                }
            }
            ExprKind::Dict { keys, .. } => {
                let check_repeated_literals = self.settings.enabled.contains(&CheckCode::F601);
                let check_repeated_variables = self.settings.enabled.contains(&CheckCode::F602);
                if check_repeated_literals || check_repeated_variables {
                    self.checks.extend(checks::check_repeated_keys(
                        keys,
                        check_repeated_literals,
                        check_repeated_variables,
                        self,
                    ));
                }
            }
            ExprKind::Yield { .. } | ExprKind::YieldFrom { .. } | ExprKind::Await { .. } => {
                let scope = self.current_scope();
                if self
                    .settings
                    .enabled
                    .contains(CheckKind::YieldOutsideFunction.code())
                    && matches!(scope.kind, ScopeKind::Class | ScopeKind::Module)
                {
                    self.checks.push(Check::new(
                        CheckKind::YieldOutsideFunction,
                        self.locate_check(Range::from_located(expr)),
                    ));
                }
            }
            ExprKind::JoinedStr { values } => {
                if self.in_f_string.is_none()
                    && self
                        .settings
                        .enabled
                        .contains(CheckKind::FStringMissingPlaceholders.code())
                    && !values
                        .iter()
                        .any(|value| matches!(value.node, ExprKind::FormattedValue { .. }))
                {
                    self.checks.push(Check::new(
                        CheckKind::FStringMissingPlaceholders,
                        self.locate_check(Range::from_located(expr)),
                    ));
                }
                self.in_f_string = Some(Range::from_located(expr));
            }
            ExprKind::BinOp {
                left,
                op: Operator::RShift,
                ..
            } => {
                if self.settings.enabled.contains(&CheckCode::F633) {
                    plugins::invalid_print_syntax(self, left);
                }
            }
            ExprKind::UnaryOp { op, operand } => {
                let check_not_in = self.settings.enabled.contains(&CheckCode::E713);
                let check_not_is = self.settings.enabled.contains(&CheckCode::E714);
                if check_not_in || check_not_is {
                    self.checks.extend(checks::check_not_tests(
                        op,
                        operand,
                        check_not_in,
                        check_not_is,
                        self,
                    ));
                }
            }
            ExprKind::Compare {
                left,
                ops,
                comparators,
            } => {
                let check_none_comparisons = self.settings.enabled.contains(&CheckCode::E711);
                let check_true_false_comparisons = self.settings.enabled.contains(&CheckCode::E712);
                if check_none_comparisons || check_true_false_comparisons {
                    self.checks.extend(checks::check_literal_comparisons(
                        left,
                        ops,
                        comparators,
                        check_none_comparisons,
                        check_true_false_comparisons,
                        self,
                    ));
                }

                if self.settings.enabled.contains(&CheckCode::F632) {
                    self.checks.extend(checks::check_is_literal(
                        left,
                        ops,
                        comparators,
                        self.locate_check(Range::from_located(expr)),
                    ));
                }

                if self.settings.enabled.contains(&CheckCode::E721) {
                    self.checks.extend(checks::check_type_comparison(
                        ops,
                        comparators,
                        self.locate_check(Range::from_located(expr)),
                    ));
                }
            }
            ExprKind::Constant {
                value: Constant::Str(value),
                ..
            } => {
                if self.in_annotation && !self.in_literal {
                    self.deferred_string_annotations
                        .push((Range::from_located(expr), value));
                }
            }
            ExprKind::Lambda { args, .. } => {
                // Visit the arguments, but avoid the body, which will be deferred.
                for arg in &args.posonlyargs {
                    if let Some(expr) = &arg.node.annotation {
                        self.visit_annotation(expr);
                    }
                }
                for arg in &args.args {
                    if let Some(expr) = &arg.node.annotation {
                        self.visit_annotation(expr);
                    }
                }
                if let Some(arg) = &args.vararg {
                    if let Some(expr) = &arg.node.annotation {
                        self.visit_annotation(expr);
                    }
                }
                for arg in &args.kwonlyargs {
                    if let Some(expr) = &arg.node.annotation {
                        self.visit_annotation(expr);
                    }
                }
                if let Some(arg) = &args.kwarg {
                    if let Some(expr) = &arg.node.annotation {
                        self.visit_annotation(expr);
                    }
                }
                for expr in &args.kw_defaults {
                    self.visit_expr(expr);
                }
                for expr in &args.defaults {
                    self.visit_expr(expr);
                }
            }
            ExprKind::GeneratorExp { .. }
            | ExprKind::ListComp { .. }
            | ExprKind::DictComp { .. }
            | ExprKind::SetComp { .. } => self.push_scope(Scope::new(ScopeKind::Generator)),
            _ => {}
        };

        // Recurse.
        match &expr.node {
            ExprKind::Lambda { .. } => {
                self.deferred_lambdas.push((
                    expr,
                    self.scope_stack.clone(),
                    self.parent_stack.clone(),
                ));
            }
            ExprKind::Call {
                func,
                args,
                keywords,
            } => {
                if match_name_or_attr(func, "ForwardRef") {
                    self.visit_expr(func);
                    for expr in args {
                        self.visit_annotation(expr);
                    }
                } else if match_name_or_attr(func, "cast") {
                    self.visit_expr(func);
                    if !args.is_empty() {
                        self.visit_annotation(&args[0]);
                    }
                    for expr in args.iter().skip(1) {
                        self.visit_expr(expr);
                    }
                } else if match_name_or_attr(func, "NewType") {
                    self.visit_expr(func);
                    for expr in args.iter().skip(1) {
                        self.visit_annotation(expr);
                    }
                } else if match_name_or_attr(func, "TypeVar") {
                    self.visit_expr(func);
                    for expr in args.iter().skip(1) {
                        self.visit_annotation(expr);
                    }
                    for keyword in keywords {
                        let KeywordData { arg, value } = &keyword.node;
                        if let Some(id) = arg {
                            if id == "bound" {
                                self.visit_annotation(value);
                            } else {
                                self.in_annotation = false;
                                self.visit_expr(value);
                                self.in_annotation = prev_in_annotation;
                            }
                        }
                    }
                } else if match_name_or_attr(func, "NamedTuple") {
                    self.visit_expr(func);

                    // Ex) NamedTuple("a", [("a", int)])
                    if args.len() > 1 {
                        match &args[1].node {
                            ExprKind::List { elts, .. } | ExprKind::Tuple { elts, .. } => {
                                for elt in elts {
                                    match &elt.node {
                                        ExprKind::List { elts, .. }
                                        | ExprKind::Tuple { elts, .. } => {
                                            if elts.len() == 2 {
                                                self.in_annotation = false;
                                                self.visit_expr(&elts[0]);
                                                self.in_annotation = prev_in_annotation;

                                                self.visit_annotation(&elts[1]);
                                            }
                                        }
                                        _ => {}
                                    }
                                }
                            }
                            _ => {}
                        }
                    }

                    // Ex) NamedTuple("a", a=int)
                    for keyword in keywords {
                        let KeywordData { value, .. } = &keyword.node;
                        self.visit_annotation(value);
                    }
                } else if match_name_or_attr(func, "TypedDict") {
                    self.visit_expr(func);

                    // Ex) TypedDict("a", {"a": int})
                    if args.len() > 1 {
                        if let ExprKind::Dict { keys, values } = &args[1].node {
                            for key in keys {
                                self.in_annotation = false;
                                self.visit_expr(key);
                                self.in_annotation = prev_in_annotation;
                            }
                            for value in values {
                                self.visit_annotation(value);
                            }
                        }
                    }

                    // Ex) TypedDict("a", a=int)
                    for keyword in keywords {
                        let KeywordData { value, .. } = &keyword.node;
                        self.visit_annotation(value);
                    }
                } else {
                    visitor::walk_expr(self, expr);
                }
            }
            ExprKind::Subscript { value, slice, ctx } => {
                match match_annotated_subscript(value) {
                    Some(subscript) => match subscript {
                        // Ex) Optional[int]
                        SubscriptKind::AnnotatedSubscript => {
                            self.visit_expr(value);
                            self.visit_annotation(slice);
                            self.visit_expr_context(ctx);
                        }
                        // Ex) Annotated[int, "Hello, world!"]
                        SubscriptKind::PEP593AnnotatedSubscript => {
                            // First argument is a type (including forward references); the rest are
                            // arbitrary Python objects.
                            self.visit_expr(value);
                            if let ExprKind::Tuple { elts, ctx } = &slice.node {
                                if let Some(expr) = elts.first() {
                                    self.visit_expr(expr);
                                    self.in_annotation = false;
                                    for expr in elts.iter().skip(1) {
                                        self.visit_expr(expr);
                                    }
                                    self.in_annotation = true;
                                    self.visit_expr_context(ctx);
                                }
                            } else {
                                error!("Found non-ExprKind::Tuple argument to PEP 593 Annotation.")
                            }
                        }
                    },
                    None => visitor::walk_expr(self, expr),
                }
            }
            _ => visitor::walk_expr(self, expr),
        }

        // Post-visit.
        match &expr.node {
            ExprKind::GeneratorExp { .. }
            | ExprKind::ListComp { .. }
            | ExprKind::DictComp { .. }
            | ExprKind::SetComp { .. } => {
                self.pop_scope();
            }
            _ => {}
        };

        self.in_annotation = prev_in_annotation;
        self.in_literal = prev_in_literal;
        self.in_f_string = prev_in_f_string;
    }

    fn visit_excepthandler(&mut self, excepthandler: &'b Excepthandler) {
        match &excepthandler.node {
            ExcepthandlerKind::ExceptHandler { type_, name, .. } => {
                if self.settings.enabled.contains(&CheckCode::E722) && type_.is_none() {
                    self.checks.push(Check::new(
                        CheckKind::DoNotUseBareExcept,
                        Range::from_located(excepthandler),
                    ));
                }
                match name {
                    Some(name) => {
                        if self.settings.enabled.contains(&CheckCode::E741) {
                            if let Some(check) = checks::check_ambiguous_variable_name(
                                name,
                                self.locate_check(Range::from_located(excepthandler)),
                            ) {
                                self.checks.push(check);
                            }
                        }

                        self.check_builtin_shadowing(
                            name,
                            Range::from_located(excepthandler),
                            false,
                        );

                        if self.current_scope().values.contains_key(name) {
                            self.handle_node_store(
                                &Expr::new(
                                    excepthandler.location,
                                    excepthandler.end_location,
                                    ExprKind::Name {
                                        id: name.to_string(),
                                        ctx: ExprContext::Store,
                                    },
                                ),
                                self.current_parent(),
                            );
                        }

                        let definition = self.current_scope().values.get(name).cloned();
                        self.handle_node_store(
                            &Expr::new(
                                excepthandler.location,
                                excepthandler.end_location,
                                ExprKind::Name {
                                    id: name.to_string(),
                                    ctx: ExprContext::Store,
                                },
                            ),
                            self.current_parent(),
                        );

                        walk_excepthandler(self, excepthandler);

                        let scope = &mut self.scopes
                            [*(self.scope_stack.last().expect("No current scope found."))];
                        if let Some(binding) = &scope.values.remove(name) {
                            if self.settings.enabled.contains(&CheckCode::F841)
                                && binding.used.is_none()
                            {
                                self.checks.push(Check::new(
                                    CheckKind::UnusedVariable(name.to_string()),
                                    Range::from_located(excepthandler),
                                ));
                            }
                        }

                        if let Some(binding) = definition {
                            scope.values.insert(name.to_string(), binding);
                        }
                    }
                    None => walk_excepthandler(self, excepthandler),
                }
            }
        }
    }

    fn visit_arguments(&mut self, arguments: &'b Arguments) {
        if self.settings.enabled.contains(&CheckCode::F831) {
            self.checks
                .extend(checks::check_duplicate_arguments(arguments));
        }

        // Bind, but intentionally avoid walking default expressions, as we handle them upstream.
        for arg in &arguments.posonlyargs {
            self.visit_arg(arg);
        }
        for arg in &arguments.args {
            self.visit_arg(arg);
        }
        if let Some(arg) = &arguments.vararg {
            self.visit_arg(arg);
        }
        for arg in &arguments.kwonlyargs {
            self.visit_arg(arg);
        }
        if let Some(arg) = &arguments.kwarg {
            self.visit_arg(arg);
        }
    }

    fn visit_arg(&mut self, arg: &'b Arg) {
        // Bind, but intentionally avoid walking the annotation, as we handle it upstream.
        self.add_binding(
            arg.node.arg.to_string(),
            Binding {
                kind: BindingKind::Argument,
                used: None,
                range: Range::from_located(arg),
            },
        );

        if self.settings.enabled.contains(&CheckCode::E741) {
            if let Some(check) = checks::check_ambiguous_variable_name(
                &arg.node.arg,
                self.locate_check(Range::from_located(arg)),
            ) {
                self.checks.push(check);
            }
        }

        self.check_builtin_arg_shadowing(&arg.node.arg, Range::from_located(arg));
    }
}

impl CheckLocator for Checker<'_> {
    fn locate_check(&self, default: Range) -> Range {
        self.in_f_string.unwrap_or(default)
    }
}

impl<'a> Checker<'a> {
    pub fn add_check(&mut self, check: Check) {
        self.checks.push(check);
    }

    fn push_parent(&mut self, parent: &'a Stmt) {
        self.parent_stack.push(self.parents.len());
        self.parents.push(parent);
    }

    fn pop_parent(&mut self) {
        self.parent_stack
            .pop()
            .expect("Attempted to pop without scope.");
    }

    fn push_scope(&mut self, scope: Scope) {
        self.scope_stack.push(self.scopes.len());
        self.scopes.push(scope);
    }

    fn pop_scope(&mut self) {
        self.dead_scopes.push(
            self.scope_stack
                .pop()
                .expect("Attempted to pop without scope."),
        );
    }

    fn bind_builtins(&mut self) {
        let scope = &mut self.scopes[*(self.scope_stack.last().expect("No current scope found."))];

        for builtin in BUILTINS {
            scope.values.insert(
                (*builtin).to_string(),
                Binding {
                    kind: BindingKind::Builtin,
                    range: Default::default(),
                    used: None,
                },
            );
        }
        for builtin in MAGIC_GLOBALS {
            scope.values.insert(
                (*builtin).to_string(),
                Binding {
                    kind: BindingKind::Builtin,
                    range: Default::default(),
                    used: None,
                },
            );
        }
    }

    pub fn current_scope(&self) -> &Scope {
        &self.scopes[*(self.scope_stack.last().expect("No current scope found."))]
    }

    pub fn current_parent(&self) -> &'a Stmt {
        self.parents[*(self.parent_stack.last().expect("No parent found."))]
    }

    pub fn binding_context(&self) -> BindingContext {
        let mut rev = self.parent_stack.iter().rev().fuse();
        let defined_by = *rev.next().expect("Expected to bind within a statement.");
        let defined_in = rev.next().cloned();
        BindingContext {
            defined_by,
            defined_in,
        }
    }

    fn add_binding(&mut self, name: String, binding: Binding) {
        let scope = &mut self.scopes[*(self.scope_stack.last().expect("No current scope found."))];

        // TODO(charlie): Don't treat annotations as assignments if there is an existing value.
        let binding = match scope.values.get(&name) {
            None => binding,
            Some(existing) => {
                if self.settings.enabled.contains(&CheckCode::F402)
                    && matches!(binding.kind, BindingKind::LoopVar)
                    && matches!(
                        existing.kind,
                        BindingKind::Importation(_, _) | BindingKind::FromImportation(_, _)
                    )
                {
                    self.checks.push(Check::new(
                        CheckKind::ImportShadowedByLoopVar(
                            name.clone(),
                            existing.range.location.row(),
                        ),
                        binding.range,
                    ));
                }
                Binding {
                    kind: binding.kind,
                    range: binding.range,
                    used: existing.used,
                }
            }
        };

        scope.values.insert(name, binding);
    }

    fn handle_node_load(&mut self, expr: &Expr) {
        if let ExprKind::Name { id, .. } = &expr.node {
            let scope_id =
                self.scopes[*(self.scope_stack.last().expect("No current scope found."))].id;

            let mut first_iter = true;
            let mut in_generator = false;
            let mut import_starred = false;
            for scope_index in self.scope_stack.iter().rev() {
                let scope = &mut self.scopes[*scope_index];
                if matches!(scope.kind, ScopeKind::Class) {
                    if id == "__class__" {
                        return;
                    } else if !first_iter && !in_generator {
                        continue;
                    }
                }
                if let Some(binding) = scope.values.get_mut(id) {
                    binding.used = Some((scope_id, Range::from_located(expr)));
                    return;
                }

                first_iter = false;
                in_generator = matches!(scope.kind, ScopeKind::Generator);
                import_starred = import_starred || scope.import_starred;
            }

            if import_starred {
                if self.settings.enabled.contains(&CheckCode::F405) {
                    let mut from_list = vec![];
                    for scope_index in self.scope_stack.iter().rev() {
                        let scope = &self.scopes[*scope_index];
                        for (name, binding) in scope.values.iter() {
                            if matches!(binding.kind, BindingKind::StarImportation) {
                                from_list.push(name.to_string());
                            }
                        }
                    }
                    from_list.sort();

                    self.checks.push(Check::new(
                        CheckKind::ImportStarUsage(id.clone(), from_list),
                        self.locate_check(Range::from_located(expr)),
                    ));
                }
                return;
            }

            if self.settings.enabled.contains(&CheckCode::F821) {
                // Allow __path__.
                if self.path.ends_with("__init__.py") && id == "__path__" {
                    return;
                }
                self.checks.push(Check::new(
                    CheckKind::UndefinedName(id.clone()),
                    self.locate_check(Range::from_located(expr)),
                ))
            }
        }
    }

    fn handle_node_store(&mut self, expr: &Expr, parent: &Stmt) {
        if let ExprKind::Name { id, .. } = &expr.node {
            let current =
                &self.scopes[*(self.scope_stack.last().expect("No current scope found."))];

            if self.settings.enabled.contains(&CheckCode::F823)
                && matches!(current.kind, ScopeKind::Function(_))
                && !current.values.contains_key(id)
            {
                for scope in self.scopes.iter().rev().skip(1) {
                    if matches!(scope.kind, ScopeKind::Function(_) | ScopeKind::Module) {
                        if let Some(binding) = scope.values.get(id) {
                            if let Some((scope_id, location)) = binding.used {
                                if scope_id == current.id {
                                    self.checks.push(Check::new(
                                        CheckKind::UndefinedLocal(id.clone()),
                                        self.locate_check(location),
                                    ));
                                }
                            }
                        }
                    }
                }
            }

            if matches!(parent.node, StmtKind::AnnAssign { value: None, .. }) {
                self.add_binding(
                    id.to_string(),
                    Binding {
                        kind: BindingKind::Annotation,
                        used: None,
                        range: Range::from_located(expr),
                    },
                );
                return;
            }

            // TODO(charlie): Include comprehensions here.
            if matches!(
                parent.node,
                StmtKind::For { .. } | StmtKind::AsyncFor { .. }
            ) {
                self.add_binding(
                    id.to_string(),
                    Binding {
                        kind: BindingKind::LoopVar,
                        used: None,
                        range: Range::from_located(expr),
                    },
                );
                return;
            }

            if operations::is_unpacking_assignment(parent) {
                self.add_binding(
                    id.to_string(),
                    Binding {
                        kind: BindingKind::Binding,
                        used: None,
                        range: Range::from_located(expr),
                    },
                );
                return;
            }

            if id == "__all__"
                && matches!(current.kind, ScopeKind::Module)
                && matches!(
                    parent.node,
                    StmtKind::Assign { .. }
                        | StmtKind::AugAssign { .. }
                        | StmtKind::AnnAssign { .. }
                )
            {
                self.add_binding(
                    id.to_string(),
                    Binding {
                        kind: BindingKind::Export(extract_all_names(parent, current)),
                        used: None,
                        range: Range::from_located(expr),
                    },
                );
                return;
            }

            self.add_binding(
                id.to_string(),
                Binding {
                    kind: BindingKind::Assignment,
                    used: None,
                    range: Range::from_located(expr),
                },
            );
        }
    }

    fn handle_node_delete(&mut self, expr: &Expr) {
        if let ExprKind::Name { id, .. } = &expr.node {
            // Check if we're on a conditional branch.
            if operations::on_conditional_branch(&self.parent_stack, &self.parents) {
                return;
            }

            let scope =
                &mut self.scopes[*(self.scope_stack.last().expect("No current scope found."))];
            if scope.values.remove(id).is_none() && self.settings.enabled.contains(&CheckCode::F821)
            {
                self.checks.push(Check::new(
                    CheckKind::UndefinedName(id.clone()),
                    self.locate_check(Range::from_located(expr)),
                ))
            }
        }
    }

    fn check_deferred_annotations(&mut self) {
        while let Some((expr, scopes, parents)) = self.deferred_annotations.pop() {
            self.parent_stack = parents;
            self.scope_stack = scopes;
            self.visit_expr(expr);
        }
    }

    fn check_deferred_string_annotations<'b>(&mut self, allocator: &'b mut Vec<Expr>)
    where
        'b: 'a,
    {
        while let Some((location, expression)) = self.deferred_string_annotations.pop() {
            if let Ok(mut expr) = parser::parse_expression(expression, "<filename>") {
                relocate_expr(&mut expr, location);
                allocator.push(expr);
            } else if self.settings.enabled.contains(&CheckCode::F722) {
                self.checks.push(Check::new(
                    CheckKind::ForwardAnnotationSyntaxError(expression.to_string()),
                    self.locate_check(location),
                ));
            }
        }
        for expr in allocator {
            self.visit_expr(expr);
        }
    }

    fn check_deferred_functions(&mut self) {
        while let Some((stmt, scopes, parents)) = self.deferred_functions.pop() {
            self.parent_stack = parents;
            self.scope_stack = scopes;
            self.push_scope(Scope::new(ScopeKind::Function(Default::default())));

            match &stmt.node {
                StmtKind::FunctionDef { body, args, .. }
                | StmtKind::AsyncFunctionDef { body, args, .. } => {
                    self.visit_arguments(args);
                    for stmt in body {
                        self.visit_stmt(stmt);
                    }
                }
                _ => {}
            }

            self.deferred_assignments
                .push(*self.scope_stack.last().expect("No current scope found."));

            self.pop_scope();
        }
    }

    fn check_deferred_lambdas(&mut self) {
        while let Some((expr, scopes, parents)) = self.deferred_lambdas.pop() {
            self.parent_stack = parents;
            self.scope_stack = scopes;
            self.push_scope(Scope::new(ScopeKind::Function(Default::default())));

            if let ExprKind::Lambda { args, body } = &expr.node {
                self.visit_arguments(args);
                self.visit_expr(body);
            }

            self.deferred_assignments
                .push(*self.scope_stack.last().expect("No current scope found."));

            self.pop_scope();
        }
    }

    fn check_deferred_assignments(&mut self) {
        if self.settings.enabled.contains(&CheckCode::F841) {
            while let Some(index) = self.deferred_assignments.pop() {
                self.checks.extend(checks::check_unused_variables(
                    &self.scopes[index],
                    self,
                    &self.settings.dummy_variable_rgx,
                ));
            }
        }
    }

    fn check_dead_scopes(&mut self) {
        if !self.settings.enabled.contains(&CheckCode::F401)
            && !self.settings.enabled.contains(&CheckCode::F405)
            && !self.settings.enabled.contains(&CheckCode::F822)
        {
            return;
        }

        for index in self.dead_scopes.iter().copied() {
            let scope = &self.scopes[index];

            let all_binding = scope.values.get("__all__");
            let all_names = all_binding.and_then(|binding| match &binding.kind {
                BindingKind::Export(names) => Some(names),
                _ => None,
            });

            if self.settings.enabled.contains(&CheckCode::F822)
                && !scope.import_starred
                && !self.path.ends_with("__init__.py")
            {
                if let Some(all_binding) = all_binding {
                    if let Some(names) = all_names {
                        for name in names {
                            if !scope.values.contains_key(name) {
                                self.checks.push(Check::new(
                                    CheckKind::UndefinedExport(name.to_string()),
                                    self.locate_check(all_binding.range),
                                ));
                            }
                        }
                    }
                }
            }

            if self.settings.enabled.contains(&CheckCode::F405) && scope.import_starred {
                if let Some(all_binding) = all_binding {
                    if let Some(names) = all_names {
                        let mut from_list = vec![];
                        for (name, binding) in scope.values.iter() {
                            if matches!(binding.kind, BindingKind::StarImportation) {
                                from_list.push(name.to_string());
                            }
                        }
                        from_list.sort();

                        for name in names {
                            if !scope.values.contains_key(name) {
                                self.checks.push(Check::new(
                                    CheckKind::ImportStarUsage(name.clone(), from_list.clone()),
                                    self.locate_check(all_binding.range),
                                ));
                            }
                        }
                    }
                }
            }

            if self.settings.enabled.contains(&CheckCode::F401) {
                // Collect all unused imports by location. (Multiple unused imports at the same
                // location indicates an `import from`.)
                let mut unused: BTreeMap<(ImportKind, usize, Option<usize>), Vec<&str>> =
                    BTreeMap::new();

                for (name, binding) in scope.values.iter().rev() {
                    let used = binding.used.is_some()
                        || all_names
                            .map(|names| names.contains(name))
                            .unwrap_or_default();

                    if !used {
                        match &binding.kind {
                            BindingKind::FromImportation(full_name, context) => {
                                let full_names = unused
                                    .entry((
                                        ImportKind::ImportFrom,
                                        context.defined_by,
                                        context.defined_in,
                                    ))
                                    .or_default();
                                full_names.push(full_name);
                            }
                            BindingKind::Importation(full_name, context)
                            | BindingKind::SubmoduleImportation(full_name, context) => {
                                let full_names = unused
                                    .entry((
                                        ImportKind::Import,
                                        context.defined_by,
                                        context.defined_in,
                                    ))
                                    .or_default();
                                full_names.push(full_name);
                            }
                            _ => {}
                        }
                    }
                }

                for ((kind, defined_by, defined_in), full_names) in unused {
                    let child = self.parents[defined_by];
                    let parent = defined_in.map(|defined_in| self.parents[defined_in]);

                    let fix = if matches!(self.autofix, fixer::Mode::Generate | fixer::Mode::Apply)
                    {
                        let deleted: Vec<&Stmt> = self
                            .deletions
                            .iter()
                            .map(|index| self.parents[*index])
                            .collect();

                        let removal_fn = match kind {
                            ImportKind::Import => fixes::remove_unused_imports,
                            ImportKind::ImportFrom => fixes::remove_unused_import_froms,
                        };

                        match removal_fn(&mut self.locator, &full_names, child, parent, &deleted) {
                            Ok(fix) => Some(fix),
                            Err(e) => {
                                error!("Failed to fix unused imports: {}", e);
                                None
                            }
                        }
                    } else {
                        None
                    };

                    let mut check = Check::new(
                        CheckKind::UnusedImport(full_names.into_iter().map(String::from).collect()),
                        self.locate_check(Range::from_located(child)),
                    );
                    if let Some(fix) = fix {
                        check.amend(fix);
                    }

                    self.checks.push(check);
                }
            }
        }
    }

    fn check_builtin_shadowing(&mut self, name: &str, location: Range, is_attribute: bool) {
        let scope = self.current_scope();

        // flake8-builtins
        if is_attribute && matches!(scope.kind, ScopeKind::Class) {
            if self.settings.enabled.contains(&CheckCode::A003) {
                if let Some(check) = checks::check_builtin_shadowing(
                    name,
                    self.locate_check(location),
                    checks::ShadowingType::Attribute,
                ) {
                    self.checks.push(check);
                }
            }
        } else if self.settings.enabled.contains(&CheckCode::A001) {
            if let Some(check) = checks::check_builtin_shadowing(
                name,
                self.locate_check(location),
                checks::ShadowingType::Variable,
            ) {
                self.checks.push(check);
            }
        }
    }

    fn check_builtin_arg_shadowing(&mut self, name: &str, location: Range) {
        if self.settings.enabled.contains(&CheckCode::A002) {
            if let Some(check) = checks::check_builtin_shadowing(
                name,
                self.locate_check(location),
                checks::ShadowingType::Argument,
            ) {
                self.checks.push(check);
            }
        }
    }
}

pub fn check_ast(
    python_ast: &Suite,
    contents: &str,
    settings: &Settings,
    autofix: &fixer::Mode,
    path: &Path,
) -> Vec<Check> {
    let mut checker = Checker::new(settings, autofix, path, contents);
    checker.push_scope(Scope::new(ScopeKind::Module));
    checker.bind_builtins();

    // Iterate over the AST.
    for stmt in python_ast {
        checker.visit_stmt(stmt);
    }

    // Check any deferred statements.
    checker.check_deferred_functions();
    checker.check_deferred_lambdas();
    checker.check_deferred_assignments();
    checker.check_deferred_annotations();
    let mut allocator = vec![];
    checker.check_deferred_string_annotations(&mut allocator);

    // Reset the scope to module-level, and check all consumed scopes.
    checker.scope_stack = vec![GLOBAL_SCOPE_INDEX];
    checker.pop_scope();
    checker.check_dead_scopes();

    checker.checks
}
