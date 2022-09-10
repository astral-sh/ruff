use std::path::Path;

use rustpython_parser::ast::{
    Arg, Arguments, Constant, Excepthandler, ExcepthandlerKind, Expr, ExprContext, ExprKind,
    KeywordData, Location, Stmt, StmtKind, Suite,
};
use rustpython_parser::parser;

use crate::ast::operations::{extract_all_names, SourceCodeLocator};
use crate::ast::relocate::relocate_expr;
use crate::ast::types::{Binding, BindingKind, Scope, ScopeKind};
use crate::ast::visitor::{walk_excepthandler, Visitor};
use crate::ast::{checks, visitor};
use crate::autofix::fixer;
use crate::checks::{Check, CheckCode, CheckKind};
use crate::python::builtins::{BUILTINS, MAGIC_GLOBALS};
use crate::python::typing;
use crate::settings::Settings;

pub const GLOBAL_SCOPE_INDEX: usize = 0;

struct Checker<'a> {
    // Input data.
    locator: SourceCodeLocator<'a>,
    settings: &'a Settings,
    autofix: &'a fixer::Mode,
    path: &'a str,
    // Computed checks.
    checks: Vec<Check>,
    // Retain all scopes and parent nodes, along with a stack of indexes to track which are active
    // at various points in time.
    parents: Vec<&'a Stmt>,
    parent_stack: Vec<usize>,
    scopes: Vec<Scope>,
    scope_stack: Vec<usize>,
    dead_scopes: Vec<usize>,
    deferred_annotations: Vec<(Location, &'a str)>,
    deferred_functions: Vec<(&'a Stmt, Vec<usize>, Vec<usize>)>,
    deferred_lambdas: Vec<(&'a Expr, Vec<usize>, Vec<usize>)>,
    // Derivative state.
    in_f_string: bool,
    in_annotation: bool,
    in_literal: bool,
    seen_non_import: bool,
    seen_docstring: bool,
}

impl<'a> Checker<'a> {
    pub fn new(
        settings: &'a Settings,
        autofix: &'a fixer::Mode,
        path: &'a str,
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
            deferred_annotations: vec![],
            deferred_functions: vec![],
            deferred_lambdas: vec![],
            in_f_string: false,
            in_annotation: false,
            in_literal: false,
            seen_non_import: false,
            seen_docstring: false,
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

fn is_annotated_subscript(expr: &Expr) -> bool {
    match &expr.node {
        ExprKind::Attribute { attr, .. } => typing::is_annotated_subscript(attr),
        ExprKind::Name { id, .. } => typing::is_annotated_subscript(id),
        _ => false,
    }
}

impl<'a, 'b> Visitor<'b> for Checker<'a>
where
    'b: 'a,
{
    fn visit_stmt(&mut self, stmt: &'b Stmt) {
        self.push_parent(stmt);

        // Pre-visit.
        match &stmt.node {
            StmtKind::Global { names } | StmtKind::Nonlocal { names } => {
                // TODO(charlie): Handle doctests.
                let global_scope_id = self.scopes[GLOBAL_SCOPE_INDEX].id;

                let current_scope =
                    &self.scopes[*(self.scope_stack.last().expect("No current scope found."))];
                let current_scope_id = current_scope.id;
                if current_scope_id != global_scope_id {
                    for name in names {
                        for scope in self.scopes.iter_mut().skip(GLOBAL_SCOPE_INDEX + 1) {
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
                        location: stmt.location,
                    },
                );
            }
            StmtKind::Return { .. } => {
                if self
                    .settings
                    .select
                    .contains(CheckKind::ReturnOutsideFunction.code())
                {
                    if let Some(scope_index) = self.scope_stack.last().cloned() {
                        match self.scopes[scope_index].kind {
                            ScopeKind::Class | ScopeKind::Module => {
                                self.checks.push(Check::new(
                                    CheckKind::ReturnOutsideFunction,
                                    stmt.location,
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
                if self.settings.select.contains(&CheckCode::R001) {
                    let scope =
                        &self.scopes[*(self.scope_stack.last().expect("No current scope found."))];
                    if let Some(check) = checks::check_useless_object_inheritance(
                        stmt,
                        name,
                        bases,
                        keywords,
                        scope,
                        &mut self.locator,
                        self.autofix,
                    ) {
                        self.checks.push(check);
                    }
                }

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
                    .select
                    .contains(CheckKind::ModuleImportNotAtTopOfFile.code())
                    && self.seen_non_import
                    && stmt.location.column() == 1
                {
                    self.checks.push(Check::new(
                        CheckKind::ModuleImportNotAtTopOfFile,
                        stmt.location,
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
                if self
                    .settings
                    .select
                    .contains(CheckKind::ModuleImportNotAtTopOfFile.code())
                    && self.seen_non_import
                    && stmt.location.column() == 1
                {
                    self.checks.push(Check::new(
                        CheckKind::ModuleImportNotAtTopOfFile,
                        stmt.location,
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
                                used: Some(
                                    self.scopes[*(self
                                        .scope_stack
                                        .last()
                                        .expect("No current scope found."))]
                                    .id,
                                ),
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

                        if self.settings.select.contains(&CheckCode::F403) {
                            self.checks
                                .push(Check::new(CheckKind::ImportStarUsage, stmt.location));
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
            StmtKind::Raise { exc, .. } => {
                if self.settings.select.contains(&CheckCode::F901) {
                    if let Some(expr) = exc {
                        if let Some(check) = checks::check_raise_not_implemented(expr) {
                            self.checks.push(check);
                        }
                    }
                }
            }
            StmtKind::AugAssign { target, .. } => {
                self.seen_non_import = true;
                self.handle_node_load(target);
            }
            StmtKind::If { test, .. } => {
                if self.settings.select.contains(&CheckCode::F634) {
                    if let Some(check) = checks::check_if_tuple(test, stmt.location) {
                        self.checks.push(check);
                    }
                }
            }
            StmtKind::Assert { test, .. } => {
                self.seen_non_import = true;
                if self.settings.select.contains(CheckKind::AssertTuple.code()) {
                    if let Some(check) = checks::check_assert_tuple(test, stmt.location) {
                        self.checks.push(check);
                    }
                }
            }
            StmtKind::Try { handlers, .. } => {
                if self.settings.select.contains(&CheckCode::F707) {
                    if let Some(check) = checks::check_default_except_not_last(handlers) {
                        self.checks.push(check);
                    }
                }
            }
            StmtKind::Expr { value } => {
                if !self.seen_docstring {
                    if let ExprKind::Constant {
                        value: Constant::Str(_),
                        ..
                    } = &value.node
                    {
                        self.seen_docstring = true;
                    }
                } else {
                    self.seen_non_import = true;
                }
            }
            StmtKind::Assign { value, .. } => {
                self.seen_non_import = true;
                if self.settings.select.contains(&CheckCode::E731) {
                    if let Some(check) = checks::check_do_not_assign_lambda(value, stmt.location) {
                        self.checks.push(check);
                    }
                }
            }
            StmtKind::AnnAssign { value, .. } => {
                self.seen_non_import = true;
                if self.settings.select.contains(&CheckCode::E731) {
                    if let Some(value) = value {
                        if let Some(check) =
                            checks::check_do_not_assign_lambda(value, stmt.location)
                        {
                            self.checks.push(check);
                        }
                    }
                }
            }
            StmtKind::Delete { .. } => {
                self.seen_non_import = true;
            }
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
                    location: stmt.location,
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

        // Pre-visit.
        match &expr.node {
            ExprKind::Subscript { value, .. } => {
                if match_name_or_attr(value, "Literal") {
                    self.in_literal = true;
                }
            }
            ExprKind::Tuple { elts, ctx } => {
                if matches!(ctx, ExprContext::Store) {
                    let check_too_many_expressions =
                        self.settings.select.contains(&CheckCode::F621);
                    let check_two_starred_expressions =
                        self.settings.select.contains(&CheckCode::F622);
                    if let Some(check) = checks::check_starred_expressions(
                        elts,
                        expr.location,
                        check_too_many_expressions,
                        check_two_starred_expressions,
                    ) {
                        self.checks.push(check);
                    }
                }
            }
            ExprKind::Name { ctx, .. } => match ctx {
                ExprContext::Load => self.handle_node_load(expr),
                ExprContext::Store => {
                    let parent =
                        self.parents[*(self.parent_stack.last().expect("No parent found."))];
                    self.handle_node_store(expr, Some(parent));
                }
                ExprContext::Del => self.handle_node_delete(expr),
            },
            ExprKind::Call { func, .. } => {
                if self.settings.select.contains(&CheckCode::R002) {
                    if let Some(check) = checks::check_assert_equals(func, self.autofix) {
                        self.checks.push(check)
                    }
                }
            }
            ExprKind::Dict { keys, .. } => {
                let check_repeated_literals = self.settings.select.contains(&CheckCode::F601);
                let check_repeated_variables = self.settings.select.contains(&CheckCode::F602);
                if check_repeated_literals || check_repeated_variables {
                    self.checks.extend(checks::check_repeated_keys(
                        keys,
                        check_repeated_literals,
                        check_repeated_variables,
                    ));
                }
            }
            ExprKind::Yield { .. } | ExprKind::YieldFrom { .. } | ExprKind::Await { .. } => {
                let scope =
                    &self.scopes[*(self.scope_stack.last().expect("No current scope found."))];
                if self
                    .settings
                    .select
                    .contains(CheckKind::YieldOutsideFunction.code())
                    && matches!(scope.kind, ScopeKind::Class)
                    || matches!(scope.kind, ScopeKind::Module)
                {
                    self.checks
                        .push(Check::new(CheckKind::YieldOutsideFunction, expr.location));
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
                    self.checks.push(Check::new(
                        CheckKind::FStringMissingPlaceholders,
                        expr.location,
                    ));
                }
                self.in_f_string = true;
            }
            ExprKind::UnaryOp { op, operand } => {
                let check_not_in = self.settings.select.contains(&CheckCode::E713);
                let check_not_is = self.settings.select.contains(&CheckCode::E714);
                if check_not_in || check_not_is {
                    self.checks.extend(checks::check_not_tests(
                        op,
                        operand,
                        check_not_in,
                        check_not_is,
                    ));
                }
            }
            ExprKind::Compare {
                left,
                ops,
                comparators,
            } => {
                let check_none_comparisons = self.settings.select.contains(&CheckCode::E711);
                let check_true_false_comparisons = self.settings.select.contains(&CheckCode::E712);
                if check_none_comparisons || check_true_false_comparisons {
                    self.checks.extend(checks::check_literal_comparisons(
                        left,
                        ops,
                        comparators,
                        check_none_comparisons,
                        check_true_false_comparisons,
                    ));
                }
            }
            ExprKind::Constant {
                value: Constant::Str(value),
                ..
            } if self.in_annotation && !self.in_literal => {
                self.deferred_annotations.push((expr.location, value));
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

                    // NamedTuple("a", [("a", int)])
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

                    // NamedTuple("a", a=int)
                    for keyword in keywords {
                        let KeywordData { value, .. } = &keyword.node;
                        self.visit_annotation(value);
                    }
                } else if match_name_or_attr(func, "TypedDict") {
                    self.visit_expr(func);

                    // TypedDict("a", {"a": int})
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

                    // TypedDict("a", a=int)
                    for keyword in keywords {
                        let KeywordData { value, .. } = &keyword.node;
                        self.visit_annotation(value);
                    }
                } else {
                    visitor::walk_expr(self, expr);
                }
            }
            ExprKind::Subscript { value, slice, ctx } => {
                if is_annotated_subscript(value) {
                    self.visit_expr(value);
                    self.visit_annotation(slice);
                    self.visit_expr_context(ctx);
                } else {
                    visitor::walk_expr(self, expr);
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
            ExcepthandlerKind::ExceptHandler { name, .. } => match name {
                Some(name) => {
                    let scope =
                        &self.scopes[*(self.scope_stack.last().expect("No current scope found."))];
                    if scope.values.contains_key(name) {
                        let parent =
                            self.parents[*(self.parent_stack.last().expect("No parent found."))];
                        self.handle_node_store(
                            &Expr::new(
                                excepthandler.location,
                                ExprKind::Name {
                                    id: name.to_string(),
                                    ctx: ExprContext::Store,
                                },
                            ),
                            Some(parent),
                        );
                        self.parents.push(parent);
                    }

                    let parent =
                        self.parents[*(self.parent_stack.last().expect("No parent found."))];
                    let scope =
                        &self.scopes[*(self.scope_stack.last().expect("No current scope found."))];
                    let definition = scope.values.get(name).cloned();
                    self.handle_node_store(
                        &Expr::new(
                            excepthandler.location,
                            ExprKind::Name {
                                id: name.to_string(),
                                ctx: ExprContext::Store,
                            },
                        ),
                        Some(parent),
                    );
                    self.parents.push(parent);

                    walk_excepthandler(self, excepthandler);

                    let scope = &mut self.scopes
                        [*(self.scope_stack.last().expect("No current scope found."))];
                    if let Some(binding) = &scope.values.remove(name) {
                        if self.settings.select.contains(&CheckCode::F841) && binding.used.is_none()
                        {
                            self.checks.push(Check::new(
                                CheckKind::UnusedVariable(name.to_string()),
                                excepthandler.location,
                            ));
                        }
                    }

                    if let Some(binding) = definition {
                        scope.values.insert(name.to_string(), binding);
                    }
                }
                None => walk_excepthandler(self, excepthandler),
            },
        }
    }

    fn visit_arguments(&mut self, arguments: &'b Arguments) {
        if self.settings.select.contains(&CheckCode::F831) {
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
                location: arg.location,
            },
        );
    }
}

impl<'a> Checker<'a> {
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
                builtin.to_string(),
                Binding {
                    kind: BindingKind::Builtin,
                    location: Default::default(),
                    used: None,
                },
            );
        }
        for builtin in MAGIC_GLOBALS {
            scope.values.insert(
                builtin.to_string(),
                Binding {
                    kind: BindingKind::Builtin,
                    location: Default::default(),
                    used: None,
                },
            );
        }
    }

    fn add_binding(&mut self, name: String, binding: Binding) {
        let scope = &mut self.scopes[*(self.scope_stack.last().expect("No current scope found."))];

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
            let scope_id =
                self.scopes[*(self.scope_stack.last().expect("No current scope found."))].id;

            let mut first_iter = true;
            let mut in_generator = false;
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
                    binding.used = Some(scope_id);
                    return;
                }

                first_iter = false;
                in_generator = matches!(scope.kind, ScopeKind::Generator);
            }

            if self.settings.select.contains(&CheckCode::F821) {
                self.checks.push(Check::new(
                    CheckKind::UndefinedName(id.clone()),
                    expr.location,
                ))
            }
        }
    }

    fn handle_node_store(&mut self, expr: &Expr, parent: Option<&Stmt>) {
        if let ExprKind::Name { id, .. } = &expr.node {
            let current =
                &self.scopes[*(self.scope_stack.last().expect("No current scope found."))];

            if self.settings.select.contains(&CheckCode::F823)
                && matches!(current.kind, ScopeKind::Function)
                && !current.values.contains_key(id)
            {
                for scope in self.scopes.iter().rev().skip(1) {
                    if matches!(scope.kind, ScopeKind::Function)
                        || matches!(scope.kind, ScopeKind::Module)
                    {
                        let used = scope
                            .values
                            .get(id)
                            .map(|binding| binding.used)
                            .unwrap_or_default();
                        if let Some(scope_id) = used {
                            if scope_id == current.id {
                                self.checks.push(Check::new(
                                    CheckKind::UndefinedLocal(id.clone()),
                                    expr.location,
                                ));
                            }
                        }
                    }
                }
            }

            // TODO(charlie): Handle alternate binding types (like `Annotation`).
            if id == "__all__"
                && matches!(current.kind, ScopeKind::Module)
                && parent
                    .map(|stmt| {
                        matches!(stmt.node, StmtKind::Assign { .. })
                            || matches!(stmt.node, StmtKind::AugAssign { .. })
                            || matches!(stmt.node, StmtKind::AnnAssign { .. })
                    })
                    .unwrap_or_default()
            {
                self.add_binding(
                    id.to_string(),
                    Binding {
                        kind: BindingKind::Export(extract_all_names(parent.unwrap(), current)),
                        used: None,
                        location: expr.location,
                    },
                );
            } else {
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
    }

    fn handle_node_delete(&mut self, expr: &Expr) {
        if let ExprKind::Name { id, .. } = &expr.node {
            let scope =
                &mut self.scopes[*(self.scope_stack.last().expect("No current scope found."))];
            if scope.values.remove(id).is_none() && self.settings.select.contains(&CheckCode::F821)
            {
                self.checks.push(Check::new(
                    CheckKind::UndefinedName(id.clone()),
                    expr.location,
                ))
            }
        }
    }

    fn check_deferred_annotations<'b>(&mut self, path: &str, allocator: &'b mut Vec<Expr>)
    where
        'b: 'a,
    {
        while !self.deferred_annotations.is_empty() {
            let (location, expression) = self.deferred_annotations.pop().unwrap();
            if let Ok(mut expr) = parser::parse_expression(expression, path) {
                relocate_expr(&mut expr, location);
                allocator.push(expr);
            }
        }
        for expr in allocator {
            self.visit_expr(expr);
        }
    }

    fn check_deferred_functions(&mut self) {
        while !self.deferred_functions.is_empty() {
            let (stmt, scopes, parents) = self.deferred_functions.pop().unwrap();

            self.parent_stack = parents;
            self.scope_stack = scopes;
            self.push_scope(Scope::new(ScopeKind::Function));

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

            if self.settings.select.contains(&CheckCode::F841) {
                let scope =
                    &self.scopes[*(self.scope_stack.last().expect("No current scope found."))];
                self.checks.extend(checks::check_unused_variables(scope));
            }

            self.pop_scope();
        }
    }

    fn check_deferred_lambdas(&mut self) {
        while !self.deferred_lambdas.is_empty() {
            let (expr, scopes, parents) = self.deferred_lambdas.pop().unwrap();

            self.parent_stack = parents;
            self.scope_stack = scopes;
            self.push_scope(Scope::new(ScopeKind::Function));

            if let ExprKind::Lambda { args, body } = &expr.node {
                self.visit_arguments(args);
                self.visit_expr(body);
            }

            if self.settings.select.contains(&CheckCode::F841) {
                let scope =
                    &self.scopes[*(self.scope_stack.last().expect("No current scope found."))];
                self.checks.extend(checks::check_unused_variables(scope));
            }

            self.pop_scope();
        }
    }

    fn check_dead_scopes(&mut self) {
        if !self.settings.select.contains(&CheckCode::F822)
            && !self.settings.select.contains(&CheckCode::F401)
        {
            return;
        }

        for index in self.dead_scopes.clone() {
            let scope = &self.scopes[index];

            let all_binding = scope.values.get("__all__");
            let all_names = all_binding.and_then(|binding| match &binding.kind {
                BindingKind::Export(names) => Some(names),
                _ => None,
            });

            if self.settings.select.contains(&CheckCode::F822)
                && !Path::new(self.path).ends_with("__init__.py")
            {
                if let Some(binding) = all_binding {
                    if let Some(names) = all_names {
                        for name in names {
                            if !scope.values.contains_key(name) {
                                self.checks.push(Check::new(
                                    CheckKind::UndefinedExport(name.to_string()),
                                    binding.location,
                                ));
                            }
                        }
                    }
                }
            }

            if self.settings.select.contains(&CheckCode::F401) {
                for (name, binding) in scope.values.iter().rev() {
                    let used = binding.used.is_some()
                        || all_names
                            .map(|names| names.contains(name))
                            .unwrap_or_default();

                    if !used {
                        match &binding.kind {
                            BindingKind::Importation(full_name)
                            | BindingKind::SubmoduleImportation(full_name) => {
                                self.checks.push(Check::new(
                                    CheckKind::UnusedImport(full_name.to_string()),
                                    binding.location,
                                ));
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
    }
}

pub fn check_ast(
    python_ast: &Suite,
    content: &str,
    settings: &Settings,
    autofix: &fixer::Mode,
    path: &str,
) -> Vec<Check> {
    let mut checker = Checker::new(settings, autofix, path, content);
    checker.push_scope(Scope::new(ScopeKind::Module));
    checker.bind_builtins();

    // Iterate over the AST.
    for stmt in python_ast {
        checker.visit_stmt(stmt);
    }

    // Check any deferred statements.
    checker.check_deferred_functions();
    checker.check_deferred_lambdas();
    let mut allocator = vec![];
    checker.check_deferred_annotations(path, &mut allocator);

    // Reset the scope to module-level, and check all consumed scopes.
    checker.scope_stack = vec![GLOBAL_SCOPE_INDEX];
    checker.pop_scope();
    checker.check_dead_scopes();

    checker.checks
}
