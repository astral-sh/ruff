//! Lint rules based on AST traversal.

use std::collections::BTreeMap;
use std::ops::Deref;
use std::path::Path;

use itertools::Itertools;
use log::error;
use rustc_hash::{FxHashMap, FxHashSet};
use rustpython_parser::ast::{
    Arg, Arguments, Constant, Excepthandler, ExcepthandlerKind, Expr, ExprContext, ExprKind,
    KeywordData, Operator, Stmt, StmtKind, Suite,
};
use rustpython_parser::parser;

use crate::ast::helpers::{
    collect_call_paths, dealias_call_path, extract_handler_names, match_call_path,
};
use crate::ast::operations::extract_all_names;
use crate::ast::relocate::relocate_expr;
use crate::ast::types::{
    Binding, BindingContext, BindingKind, ClassScope, ImportKind, Range, Scope, ScopeKind,
};
use crate::ast::visitor::{walk_excepthandler, Visitor};
use crate::ast::{helpers, operations, visitor};
use crate::autofix::fixer;
use crate::checks::{Check, CheckCode, CheckKind};
use crate::docstrings::definition::{Definition, DefinitionKind, Documentable};
use crate::python::builtins::{BUILTINS, MAGIC_GLOBALS};
use crate::python::future::ALL_FEATURE_NAMES;
use crate::python::typing;
use crate::python::typing::SubscriptKind;
use crate::settings::types::PythonVersion;
use crate::settings::Settings;
use crate::source_code_locator::SourceCodeLocator;
use crate::visibility::{module_visibility, transition_scope, Modifier, Visibility, VisibleScope};
use crate::{
    docstrings, flake8_2020, flake8_annotations, flake8_bandit, flake8_blind_except,
    flake8_boolean_trap, flake8_bugbear, flake8_builtins, flake8_comprehensions, flake8_print,
    flake8_tidy_imports, mccabe, pep8_naming, pycodestyle, pydocstyle, pyflakes, pyupgrade, rules,
};

const GLOBAL_SCOPE_INDEX: usize = 0;

pub struct Checker<'a> {
    // Input data.
    path: &'a Path,
    autofix: &'a fixer::Mode,
    pub(crate) settings: &'a Settings,
    pub(crate) locator: &'a SourceCodeLocator<'a>,
    // Computed checks.
    checks: Vec<Check>,
    // Function and class definition tracking (e.g., for docstring enforcement).
    definitions: Vec<(Definition<'a>, Visibility)>,
    // Edit tracking.
    // TODO(charlie): Instead of exposing deletions, wrap in a public API.
    pub(crate) deletions: FxHashSet<usize>,
    // Import tracking.
    pub(crate) from_imports: FxHashMap<&'a str, FxHashSet<&'a str>>,
    pub(crate) import_aliases: FxHashMap<&'a str, &'a str>,
    // Retain all scopes and parent nodes, along with a stack of indexes to track which are active
    // at various points in time.
    pub(crate) parents: Vec<&'a Stmt>,
    pub(crate) parent_stack: Vec<usize>,
    scopes: Vec<Scope<'a>>,
    scope_stack: Vec<usize>,
    dead_scopes: Vec<usize>,
    deferred_string_annotations: Vec<(Range, &'a str, Vec<usize>, Vec<usize>)>,
    deferred_annotations: Vec<(&'a Expr, Vec<usize>, Vec<usize>)>,
    deferred_functions: Vec<(&'a Stmt, Vec<usize>, Vec<usize>, VisibleScope)>,
    deferred_lambdas: Vec<(&'a Expr, Vec<usize>, Vec<usize>)>,
    deferred_assignments: Vec<usize>,
    // Internal, derivative state.
    visible_scope: VisibleScope,
    in_f_string: Option<Range>,
    in_annotation: bool,
    in_literal: bool,
    in_subscript: bool,
    seen_import_boundary: bool,
    futures_allowed: bool,
    annotations_future_enabled: bool,
    except_handlers: Vec<Vec<Vec<&'a str>>>,
}

impl<'a> Checker<'a> {
    pub fn new(
        settings: &'a Settings,
        autofix: &'a fixer::Mode,
        path: &'a Path,
        locator: &'a SourceCodeLocator,
    ) -> Checker<'a> {
        Checker {
            settings,
            autofix,
            path,
            locator,
            checks: Default::default(),
            definitions: Default::default(),
            deletions: Default::default(),
            from_imports: Default::default(),
            import_aliases: Default::default(),
            parents: Default::default(),
            parent_stack: Default::default(),
            scopes: Default::default(),
            scope_stack: Default::default(),
            dead_scopes: Default::default(),
            deferred_string_annotations: Default::default(),
            deferred_annotations: Default::default(),
            deferred_functions: Default::default(),
            deferred_lambdas: Default::default(),
            deferred_assignments: Default::default(),
            // Internal, derivative state.
            visible_scope: VisibleScope {
                modifier: Modifier::Module,
                visibility: module_visibility(path),
            },
            in_f_string: Default::default(),
            in_annotation: Default::default(),
            in_literal: Default::default(),
            in_subscript: Default::default(),
            seen_import_boundary: Default::default(),
            futures_allowed: true,
            annotations_future_enabled: Default::default(),
            except_handlers: Default::default(),
        }
    }

    /// Add a `Check` to the `Checker`.
    #[inline(always)]
    pub(crate) fn add_check(&mut self, check: Check) {
        // If we're in an f-string, override the location. RustPython doesn't produce
        // reliable locations for expressions within f-strings, so we use the
        // span of the f-string itself as a best-effort default.
        if let Some(range) = self.in_f_string {
            self.checks.push(Check {
                location: range.location,
                end_location: range.end_location,
                ..check
            });
        } else {
            self.checks.push(check);
        }
    }

    /// Add multiple `Check` items to the `Checker`.
    #[inline(always)]
    pub(crate) fn add_checks(&mut self, checks: impl Iterator<Item = Check>) {
        for check in checks {
            self.add_check(check);
        }
    }

    /// Return `true` if a patch should be generated under the given autofix
    /// `Mode`.
    pub fn patch(&self, code: &CheckCode) -> bool {
        // TODO(charlie): We can't fix errors in f-strings until RustPython adds
        // location data.
        self.autofix.patch() && self.in_f_string.is_none() && self.settings.fixable.contains(code)
    }

    /// Return `true` if the `Expr` is a reference to `typing.${target}`.
    pub fn match_typing_expr(&self, expr: &Expr, target: &str) -> bool {
        let call_path = dealias_call_path(collect_call_paths(expr), &self.import_aliases);
        self.match_typing_call_path(&call_path, target)
    }

    /// Return `true` if the call path is a reference to `typing.${target}`.
    pub fn match_typing_call_path(&self, call_path: &[&str], target: &str) -> bool {
        match_call_path(call_path, "typing", target, &self.from_imports)
            || (typing::in_extensions(target)
                && match_call_path(call_path, "typing_extensions", target, &self.from_imports))
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
            node => {
                self.futures_allowed = false;
                if !self.seen_import_boundary
                    && !helpers::is_assignment_to_a_dunder(node)
                    && !operations::in_nested_block(
                        &mut self
                            .parent_stack
                            .iter()
                            .rev()
                            .map(|index| self.parents[*index]),
                    )
                {
                    self.seen_import_boundary = true;
                }
            }
        }

        // Pre-visit.
        match &stmt.node {
            StmtKind::Global { names } => {
                let scope_index = *self.scope_stack.last().expect("No current scope found.");
                if scope_index != GLOBAL_SCOPE_INDEX {
                    let scope = &mut self.scopes[scope_index];
                    let usage = Some((scope.id, Range::from_located(stmt)));
                    for name in names {
                        // Add a binding to the current scope.
                        scope.values.insert(
                            name,
                            Binding {
                                kind: BindingKind::Global,
                                used: usage,
                                range: Range::from_located(stmt),
                            },
                        );
                    }

                    // Mark the binding in the global scope as used.
                    for name in names {
                        if let Some(mut existing) = self.scopes[GLOBAL_SCOPE_INDEX]
                            .values
                            .get_mut(&name.as_str())
                        {
                            existing.used = usage;
                        }
                    }
                }

                if self.settings.enabled.contains(&CheckCode::E741) {
                    let location = Range::from_located(stmt);
                    self.add_checks(
                        names
                            .iter()
                            .filter_map(|name| {
                                pycodestyle::checks::ambiguous_variable_name(name, location)
                            })
                            .into_iter(),
                    );
                }
            }
            StmtKind::Nonlocal { names } => {
                let scope_index = *self.scope_stack.last().expect("No current scope found.");
                if scope_index != GLOBAL_SCOPE_INDEX {
                    let scope = &mut self.scopes[scope_index];
                    let usage = Some((scope.id, Range::from_located(stmt)));
                    for name in names {
                        // Add a binding to the current scope.
                        scope.values.insert(
                            name,
                            Binding {
                                kind: BindingKind::Global,
                                used: usage,
                                range: Range::from_located(stmt),
                            },
                        );
                    }

                    // Mark the binding in the defining scopes as used too. (Skip the global scope
                    // and the current scope.)
                    for name in names {
                        for index in self.scope_stack.iter().skip(1).rev().skip(1) {
                            if let Some(mut existing) =
                                self.scopes[*index].values.get_mut(&name.as_str())
                            {
                                existing.used = usage;
                            }
                        }
                    }
                }

                if self.settings.enabled.contains(&CheckCode::E741) {
                    let location = Range::from_located(stmt);
                    self.add_checks(
                        names
                            .iter()
                            .filter_map(|name| {
                                pycodestyle::checks::ambiguous_variable_name(name, location)
                            })
                            .into_iter(),
                    );
                }
            }
            StmtKind::Break => {
                if self.settings.enabled.contains(&CheckCode::F701) {
                    if let Some(check) = pyflakes::checks::break_outside_loop(
                        stmt,
                        &self.parents,
                        &self.parent_stack,
                    ) {
                        self.add_check(check);
                    }
                }
            }
            StmtKind::Continue => {
                if self.settings.enabled.contains(&CheckCode::F702) {
                    if let Some(check) = pyflakes::checks::continue_outside_loop(
                        stmt,
                        &self.parents,
                        &self.parent_stack,
                    ) {
                        self.add_check(check);
                    }
                }
            }
            StmtKind::FunctionDef {
                name,
                decorator_list,
                returns,
                args,
                body,
                ..
            }
            | StmtKind::AsyncFunctionDef {
                name,
                decorator_list,
                returns,
                args,
                body,
                ..
            } => {
                if self.settings.enabled.contains(&CheckCode::E743) {
                    if let Some(check) = pycodestyle::checks::ambiguous_function_name(
                        name,
                        Range::from_located(stmt),
                    ) {
                        self.add_check(check);
                    }
                }

                if self.settings.enabled.contains(&CheckCode::N802) {
                    if let Some(check) = pep8_naming::checks::invalid_function_name(
                        stmt,
                        name,
                        &self.settings.pep8_naming,
                    ) {
                        self.add_check(check);
                    }
                }

                if self.settings.enabled.contains(&CheckCode::N804) {
                    if let Some(check) =
                        pep8_naming::checks::invalid_first_argument_name_for_class_method(
                            self.current_scope(),
                            name,
                            decorator_list,
                            args,
                            &self.from_imports,
                            &self.import_aliases,
                            &self.settings.pep8_naming,
                        )
                    {
                        self.add_check(check);
                    }
                }

                if self.settings.enabled.contains(&CheckCode::N805) {
                    if let Some(check) = pep8_naming::checks::invalid_first_argument_name_for_method(
                        self.current_scope(),
                        name,
                        decorator_list,
                        args,
                        &self.from_imports,
                        &self.import_aliases,
                        &self.settings.pep8_naming,
                    ) {
                        self.add_check(check);
                    }
                }

                if self.settings.enabled.contains(&CheckCode::N807) {
                    if let Some(check) =
                        pep8_naming::checks::dunder_function_name(self.current_scope(), stmt, name)
                    {
                        self.add_check(check);
                    }
                }

                if self.settings.enabled.contains(&CheckCode::U011)
                    && self.settings.target_version >= PythonVersion::Py38
                {
                    pyupgrade::plugins::unnecessary_lru_cache_params(self, decorator_list);
                }

                if self.settings.enabled.contains(&CheckCode::B018) {
                    flake8_bugbear::plugins::useless_expression(self, body);
                }
                if self.settings.enabled.contains(&CheckCode::B019) {
                    flake8_bugbear::plugins::cached_instance_method(self, decorator_list);
                }
                if self.settings.enabled.contains(&CheckCode::C901) {
                    if let Some(check) = mccabe::checks::function_is_too_complex(
                        stmt,
                        name,
                        body,
                        self.settings.mccabe.max_complexity,
                    ) {
                        self.add_check(check);
                    }
                }

                if self.settings.enabled.contains(&CheckCode::S107) {
                    self.add_checks(
                        flake8_bandit::plugins::hardcoded_password_default(args).into_iter(),
                    );
                }

                self.check_builtin_shadowing(name, Range::from_located(stmt), true);

                // Visit the decorators and arguments, but avoid the body, which will be
                // deferred.
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
                    name,
                    Binding {
                        kind: BindingKind::Definition,
                        used: None,
                        range: Range::from_located(stmt),
                    },
                );
            }
            StmtKind::Return { .. } => {
                if self.settings.enabled.contains(&CheckCode::F706) {
                    if let Some(&index) = self.scope_stack.last() {
                        if matches!(
                            self.scopes[index].kind,
                            ScopeKind::Class(_) | ScopeKind::Module
                        ) {
                            self.add_check(Check::new(
                                CheckKind::ReturnOutsideFunction,
                                Range::from_located(stmt),
                            ));
                        }
                    }
                }
            }
            StmtKind::ClassDef {
                name,
                bases,
                keywords,
                decorator_list,
                body,
            } => {
                if self.settings.enabled.contains(&CheckCode::U004) {
                    pyupgrade::plugins::useless_object_inheritance(
                        self, stmt, name, bases, keywords,
                    );
                }

                if self.settings.enabled.contains(&CheckCode::E742) {
                    if let Some(check) =
                        pycodestyle::checks::ambiguous_class_name(name, Range::from_located(stmt))
                    {
                        self.add_check(check);
                    }
                }

                if self.settings.enabled.contains(&CheckCode::N801) {
                    if let Some(check) = pep8_naming::checks::invalid_class_name(stmt, name) {
                        self.add_check(check);
                    }
                }

                if self.settings.enabled.contains(&CheckCode::N818) {
                    if let Some(check) =
                        pep8_naming::checks::error_suffix_on_exception_name(stmt, bases, name)
                    {
                        self.add_check(check);
                    }
                }

                if self.settings.enabled.contains(&CheckCode::B018) {
                    flake8_bugbear::plugins::useless_expression(self, body);
                }

                if self.settings.enabled.contains(&CheckCode::B024)
                    || self.settings.enabled.contains(&CheckCode::B027)
                {
                    flake8_bugbear::plugins::abstract_base_class(
                        self, stmt, name, bases, keywords, body,
                    );
                }

                self.check_builtin_shadowing(name, Range::from_located(stmt), false);

                for expr in bases {
                    self.visit_expr(expr)
                }
                for keyword in keywords {
                    self.visit_keyword(keyword)
                }
                for expr in decorator_list {
                    self.visit_expr(expr)
                }
                self.push_scope(Scope::new(ScopeKind::Class(ClassScope {
                    name,
                    bases,
                    keywords,
                    decorator_list,
                })))
            }
            StmtKind::Import { names } => {
                if self.settings.enabled.contains(&CheckCode::E402) {
                    if self.seen_import_boundary && stmt.location.column() == 0 {
                        self.add_check(Check::new(
                            CheckKind::ModuleImportNotAtTopOfFile,
                            Range::from_located(stmt),
                        ));
                    }
                }

                for alias in names {
                    if alias.node.name.contains('.') && alias.node.asname.is_none() {
                        // Given `import foo.bar`, `name` would be "foo", and `full_name` would be
                        // "foo.bar".
                        let name = alias.node.name.split('.').next().unwrap();
                        let full_name = &alias.node.name;
                        self.add_binding(
                            name,
                            Binding {
                                kind: BindingKind::SubmoduleImportation(
                                    name.to_string(),
                                    full_name.to_string(),
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

                        // Given `import foo`, `name` and `full_name` would both be `foo`.
                        // Given `import foo as bar`, `name` would be `bar` and `full_name` would
                        // be `foo`.
                        let name = alias.node.asname.as_ref().unwrap_or(&alias.node.name);
                        let full_name = &alias.node.name;
                        self.add_binding(
                            name,
                            Binding {
                                kind: BindingKind::Importation(
                                    name.to_string(),
                                    full_name.to_string(),
                                    self.binding_context(),
                                ),
                                // Treat explicit re-export as usage (e.g., `import applications
                                // as applications`).
                                used: if alias
                                    .node
                                    .asname
                                    .as_ref()
                                    .map(|asname| asname == &alias.node.name)
                                    .unwrap_or(false)
                                {
                                    Some((
                                        self.scopes[*(self
                                            .scope_stack
                                            .last()
                                            .expect("No current scope found."))]
                                        .id,
                                        Range::from_located(stmt),
                                    ))
                                } else {
                                    None
                                },
                                range: Range::from_located(stmt),
                            },
                        )
                    }

                    if let Some(asname) = &alias.node.asname {
                        for alias in names {
                            if let Some(asname) = &alias.node.asname {
                                self.import_aliases.insert(asname, &alias.node.name);
                            }
                        }

                        let name = alias.node.name.split('.').last().unwrap();
                        if self.settings.enabled.contains(&CheckCode::N811) {
                            if let Some(check) =
                                pep8_naming::checks::constant_imported_as_non_constant(
                                    stmt, name, asname,
                                )
                            {
                                self.add_check(check);
                            }
                        }

                        if self.settings.enabled.contains(&CheckCode::N812) {
                            if let Some(check) =
                                pep8_naming::checks::lowercase_imported_as_non_lowercase(
                                    stmt, name, asname,
                                )
                            {
                                self.add_check(check);
                            }
                        }

                        if self.settings.enabled.contains(&CheckCode::N813) {
                            if let Some(check) =
                                pep8_naming::checks::camelcase_imported_as_lowercase(
                                    stmt, name, asname,
                                )
                            {
                                self.add_check(check);
                            }
                        }

                        if self.settings.enabled.contains(&CheckCode::N814) {
                            if let Some(check) = pep8_naming::checks::camelcase_imported_as_constant(
                                stmt, name, asname,
                            ) {
                                self.add_check(check);
                            }
                        }

                        if self.settings.enabled.contains(&CheckCode::N817) {
                            if let Some(check) = pep8_naming::checks::camelcase_imported_as_acronym(
                                stmt, name, asname,
                            ) {
                                self.add_check(check);
                            }
                        }
                    }
                }
            }
            StmtKind::ImportFrom {
                names,
                module,
                level,
            } => {
                // Track `import from` statements, to ensure that we can correctly attribute
                // references like `from typing import Union`.
                if level.map(|level| level == 0).unwrap_or(true) {
                    if let Some(module) = module {
                        self.from_imports
                            .entry(module)
                            .or_insert_with(FxHashSet::default)
                            .extend(
                                names
                                    .iter()
                                    .filter(|alias| alias.node.asname.is_none())
                                    .map(|alias| alias.node.name.as_str()),
                            )
                    }
                    for alias in names {
                        if let Some(asname) = &alias.node.asname {
                            self.import_aliases.insert(asname, &alias.node.name);
                        }
                    }
                }

                if self.settings.enabled.contains(&CheckCode::E402) {
                    if self.seen_import_boundary && stmt.location.column() == 0 {
                        self.add_check(Check::new(
                            CheckKind::ModuleImportNotAtTopOfFile,
                            Range::from_located(stmt),
                        ));
                    }
                }

                if let Some("__future__") = module.as_deref() {
                    if self.settings.enabled.contains(&CheckCode::U010) {
                        pyupgrade::plugins::unnecessary_future_import(self, stmt, names);
                    }
                }

                for alias in names {
                    if let Some("__future__") = module.as_deref() {
                        let name = alias.node.asname.as_ref().unwrap_or(&alias.node.name);
                        self.add_binding(
                            name,
                            Binding {
                                kind: BindingKind::FutureImportation,
                                // Always mark `__future__` imports as used.
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

                        if self.settings.enabled.contains(&CheckCode::F407) {
                            if !ALL_FEATURE_NAMES.contains(&alias.node.name.deref()) {
                                self.add_check(Check::new(
                                    CheckKind::FutureFeatureNotDefined(alias.node.name.to_string()),
                                    Range::from_located(stmt),
                                ));
                            }
                        }

                        if self.settings.enabled.contains(&CheckCode::F404) && !self.futures_allowed
                        {
                            self.add_check(Check::new(
                                CheckKind::LateFutureImport,
                                Range::from_located(stmt),
                            ));
                        }
                    } else if alias.node.name == "*" {
                        self.add_binding(
                            "*",
                            Binding {
                                kind: BindingKind::StarImportation(*level, module.clone()),
                                used: None,
                                range: Range::from_located(stmt),
                            },
                        );

                        if self.settings.enabled.contains(&CheckCode::F406) {
                            let scope = &self.scopes
                                [*(self.scope_stack.last().expect("No current scope found."))];
                            if !matches!(scope.kind, ScopeKind::Module) {
                                self.add_check(Check::new(
                                    CheckKind::ImportStarNotPermitted(helpers::format_import_from(
                                        level.as_ref(),
                                        module.as_ref(),
                                    )),
                                    Range::from_located(stmt),
                                ));
                            }
                        }

                        if self.settings.enabled.contains(&CheckCode::F403) {
                            self.add_check(Check::new(
                                CheckKind::ImportStarUsed(helpers::format_import_from(
                                    level.as_ref(),
                                    module.as_ref(),
                                )),
                                Range::from_located(stmt),
                            ));
                        }

                        let scope = &mut self.scopes
                            [*(self.scope_stack.last().expect("No current scope found."))];
                        scope.import_starred = true;
                    } else {
                        if let Some(asname) = &alias.node.asname {
                            self.check_builtin_shadowing(asname, Range::from_located(stmt), false);
                        }

                        // Given `from foo import bar`, `name` would be "bar" and `full_name` would
                        // be "foo.bar". Given `from foo import bar as baz`, `name` would be "baz"
                        // and `full_name` would be "foo.bar".
                        let name = alias.node.asname.as_ref().unwrap_or(&alias.node.name);
                        let full_name = match module {
                            None => alias.node.name.to_string(),
                            Some(parent) => format!("{}.{}", parent, alias.node.name),
                        };
                        self.add_binding(
                            name,
                            Binding {
                                kind: BindingKind::FromImportation(
                                    name.to_string(),
                                    full_name,
                                    self.binding_context(),
                                ),
                                // Treat explicit re-export as usage (e.g., `from .applications
                                // import FastAPI as FastAPI`).
                                used: if alias
                                    .node
                                    .asname
                                    .as_ref()
                                    .map(|asname| asname == &alias.node.name)
                                    .unwrap_or(false)
                                {
                                    Some((
                                        self.scopes[*(self
                                            .scope_stack
                                            .last()
                                            .expect("No current scope found."))]
                                        .id,
                                        Range::from_located(stmt),
                                    ))
                                } else {
                                    None
                                },
                                range: Range::from_located(stmt),
                            },
                        )
                    }

                    if self.settings.enabled.contains(&CheckCode::I252) {
                        if let Some(check) = flake8_tidy_imports::checks::banned_relative_import(
                            stmt,
                            level.as_ref(),
                            &self.settings.flake8_tidy_imports.ban_relative_imports,
                        ) {
                            self.add_check(check);
                        }
                    }

                    if let Some(asname) = &alias.node.asname {
                        if self.settings.enabled.contains(&CheckCode::N811) {
                            if let Some(check) =
                                pep8_naming::checks::constant_imported_as_non_constant(
                                    stmt,
                                    &alias.node.name,
                                    asname,
                                )
                            {
                                self.add_check(check);
                            }
                        }

                        if self.settings.enabled.contains(&CheckCode::N812) {
                            if let Some(check) =
                                pep8_naming::checks::lowercase_imported_as_non_lowercase(
                                    stmt,
                                    &alias.node.name,
                                    asname,
                                )
                            {
                                self.add_check(check);
                            }
                        }

                        if self.settings.enabled.contains(&CheckCode::N813) {
                            if let Some(check) =
                                pep8_naming::checks::camelcase_imported_as_lowercase(
                                    stmt,
                                    &alias.node.name,
                                    asname,
                                )
                            {
                                self.add_check(check);
                            }
                        }

                        if self.settings.enabled.contains(&CheckCode::N814) {
                            if let Some(check) = pep8_naming::checks::camelcase_imported_as_constant(
                                stmt,
                                &alias.node.name,
                                asname,
                            ) {
                                self.add_check(check);
                            }
                        }

                        if self.settings.enabled.contains(&CheckCode::N817) {
                            if let Some(check) = pep8_naming::checks::camelcase_imported_as_acronym(
                                stmt,
                                &alias.node.name,
                                asname,
                            ) {
                                self.add_check(check);
                            }
                        }
                    }
                }
            }
            StmtKind::Raise { exc, .. } => {
                if self.settings.enabled.contains(&CheckCode::F901) {
                    if let Some(expr) = exc {
                        pyflakes::plugins::raise_not_implemented(self, expr);
                    }
                }
                if self.settings.enabled.contains(&CheckCode::B016) {
                    if let Some(exc) = exc {
                        flake8_bugbear::plugins::cannot_raise_literal(self, exc);
                    }
                }
            }
            StmtKind::AugAssign { target, .. } => {
                self.handle_node_load(target);
            }
            StmtKind::If { test, .. } => {
                if self.settings.enabled.contains(&CheckCode::F634) {
                    pyflakes::plugins::if_tuple(self, stmt, test);
                }
            }
            StmtKind::Assert { test, msg } => {
                if self.settings.enabled.contains(&CheckCode::F631) {
                    pyflakes::plugins::assert_tuple(self, stmt, test);
                }
                if self.settings.enabled.contains(&CheckCode::B011) {
                    flake8_bugbear::plugins::assert_false(
                        self,
                        stmt,
                        test,
                        msg.as_ref().map(|expr| expr.deref()),
                    );
                }
                if self.settings.enabled.contains(&CheckCode::S101) {
                    self.add_check(flake8_bandit::plugins::assert_used(stmt));
                }
            }
            StmtKind::With { items, .. } | StmtKind::AsyncWith { items, .. } => {
                if self.settings.enabled.contains(&CheckCode::B017) {
                    flake8_bugbear::plugins::assert_raises_exception(self, stmt, items);
                }
            }
            StmtKind::For {
                target, body, iter, ..
            } => {
                if self.settings.enabled.contains(&CheckCode::B007) {
                    flake8_bugbear::plugins::unused_loop_control_variable(self, target, body);
                }
                if self.settings.enabled.contains(&CheckCode::B020) {
                    flake8_bugbear::plugins::loop_variable_overrides_iterator(self, target, iter);
                }
            }
            StmtKind::Try { handlers, .. } => {
                if self.settings.enabled.contains(&CheckCode::F707) {
                    if let Some(check) = pyflakes::checks::default_except_not_last(handlers) {
                        self.add_check(check);
                    }
                }
                if self.settings.enabled.contains(&CheckCode::B014)
                    || self.settings.enabled.contains(&CheckCode::B025)
                {
                    flake8_bugbear::plugins::duplicate_exceptions(self, stmt, handlers);
                }
                if self.settings.enabled.contains(&CheckCode::B013) {
                    flake8_bugbear::plugins::redundant_tuple_in_exception_handler(self, handlers);
                }
                if self.settings.enabled.contains(&CheckCode::BLE001) {
                    flake8_blind_except::plugins::blind_except(self, handlers);
                }
            }
            StmtKind::Assign { targets, value, .. } => {
                if self.settings.enabled.contains(&CheckCode::E731) {
                    if let [target] = &targets[..] {
                        pycodestyle::plugins::do_not_assign_lambda(self, target, value, stmt)
                    }
                }
                if self.settings.enabled.contains(&CheckCode::U001) {
                    pyupgrade::plugins::useless_metaclass_type(self, stmt, value, targets);
                }
                if self.settings.enabled.contains(&CheckCode::B003) {
                    flake8_bugbear::plugins::assignment_to_os_environ(self, targets);
                }
                if self.settings.enabled.contains(&CheckCode::S105) {
                    if let Some(check) =
                        flake8_bandit::plugins::assign_hardcoded_password_string(value, targets)
                    {
                        self.add_check(check);
                    }
                }
                if self.settings.enabled.contains(&CheckCode::U013) {
                    pyupgrade::plugins::convert_typed_dict_functional_to_class(
                        self, stmt, targets, value,
                    );
                }
                if self.settings.enabled.contains(&CheckCode::U014) {
                    pyupgrade::plugins::convert_named_tuple_functional_to_class(
                        self, stmt, targets, value,
                    );
                }
            }
            StmtKind::AnnAssign { target, value, .. } => {
                if self.settings.enabled.contains(&CheckCode::E731) {
                    if let Some(value) = value {
                        pycodestyle::plugins::do_not_assign_lambda(self, target, value, stmt);
                    }
                }
            }
            StmtKind::Delete { .. } => {}
            StmtKind::Expr { value, .. } => {
                if self.settings.enabled.contains(&CheckCode::B015) {
                    flake8_bugbear::plugins::useless_comparison(self, value)
                }
            }
            _ => {}
        }

        // Recurse.
        let prev_visible_scope = self.visible_scope.clone();
        match &stmt.node {
            StmtKind::FunctionDef { body, .. } | StmtKind::AsyncFunctionDef { body, .. } => {
                if self.settings.enabled.contains(&CheckCode::B021) {
                    flake8_bugbear::plugins::f_string_docstring(self, body);
                }
                let definition = docstrings::extraction::extract(
                    &self.visible_scope,
                    stmt,
                    body,
                    &Documentable::Function,
                );
                let scope = transition_scope(&self.visible_scope, stmt, &Documentable::Function);
                self.definitions
                    .push((definition, scope.visibility.clone()));
                self.visible_scope = scope;

                self.deferred_functions.push((
                    stmt,
                    self.scope_stack.clone(),
                    self.parent_stack.clone(),
                    self.visible_scope.clone(),
                ));
            }
            StmtKind::ClassDef { body, .. } => {
                if self.settings.enabled.contains(&CheckCode::B021) {
                    flake8_bugbear::plugins::f_string_docstring(self, body);
                }
                let definition = docstrings::extraction::extract(
                    &self.visible_scope,
                    stmt,
                    body,
                    &Documentable::Class,
                );
                let scope = transition_scope(&self.visible_scope, stmt, &Documentable::Class);
                self.definitions
                    .push((definition, scope.visibility.clone()));
                self.visible_scope = scope;

                for stmt in body {
                    self.visit_stmt(stmt);
                }
            }
            StmtKind::Try {
                body,
                handlers,
                orelse,
                finalbody,
            } => {
                self.except_handlers.push(extract_handler_names(handlers));
                if self.settings.enabled.contains(&CheckCode::B012) {
                    flake8_bugbear::plugins::jump_statement_in_finally(self, finalbody);
                }
                for stmt in body {
                    self.visit_stmt(stmt);
                }
                self.except_handlers.pop();
                for excepthandler in handlers {
                    self.visit_excepthandler(excepthandler)
                }
                for stmt in orelse {
                    self.visit_stmt(stmt);
                }
                for stmt in finalbody {
                    self.visit_stmt(stmt);
                }
            }
            _ => visitor::walk_stmt(self, stmt),
        };
        self.visible_scope = prev_visible_scope;

        // Post-visit.
        if let StmtKind::ClassDef { name, .. } = &stmt.node {
            self.pop_scope();
            self.add_binding(
                name,
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
            if let ExprKind::Constant {
                value: Constant::Str(value),
                ..
            } = &expr.node
            {
                self.deferred_string_annotations.push((
                    Range::from_located(expr),
                    value,
                    self.scope_stack.clone(),
                    self.parent_stack.clone(),
                ));
            } else {
                self.deferred_annotations.push((
                    expr,
                    self.scope_stack.clone(),
                    self.parent_stack.clone(),
                ));
            }
            return;
        }

        // Pre-visit.
        match &expr.node {
            ExprKind::Subscript { value, slice, .. } => {
                // Ex) typing.List[...]
                if self.settings.enabled.contains(&CheckCode::U007)
                    && self.settings.target_version >= PythonVersion::Py310
                {
                    pyupgrade::plugins::use_pep604_annotation(self, expr, value, slice);
                }

                if self.match_typing_expr(value, "Literal") {
                    self.in_literal = true;
                }

                if self.settings.enabled.contains(&CheckCode::YTT101)
                    || self.settings.enabled.contains(&CheckCode::YTT102)
                    || self.settings.enabled.contains(&CheckCode::YTT301)
                    || self.settings.enabled.contains(&CheckCode::YTT303)
                {
                    flake8_2020::plugins::subscript(self, value, slice);
                }
            }
            ExprKind::Tuple { elts, ctx } | ExprKind::List { elts, ctx } => {
                if matches!(ctx, ExprContext::Store) {
                    let check_too_many_expressions =
                        self.settings.enabled.contains(&CheckCode::F621);
                    let check_two_starred_expressions =
                        self.settings.enabled.contains(&CheckCode::F622);
                    if let Some(check) = pyflakes::checks::starred_expressions(
                        elts,
                        check_too_many_expressions,
                        check_two_starred_expressions,
                        Range::from_located(expr),
                    ) {
                        self.add_check(check);
                    }
                }
            }
            ExprKind::Name { id, ctx } => {
                match ctx {
                    ExprContext::Load => {
                        // Ex) List[...]
                        if self.settings.enabled.contains(&CheckCode::U006)
                            && self.settings.target_version >= PythonVersion::Py39
                            && typing::is_pep585_builtin(
                                expr,
                                &self.from_imports,
                                &self.import_aliases,
                            )
                        {
                            pyupgrade::plugins::use_pep585_annotation(self, expr, id);
                        }

                        self.handle_node_load(expr);
                    }
                    ExprContext::Store => {
                        if self.settings.enabled.contains(&CheckCode::E741) {
                            if let Some(check) = pycodestyle::checks::ambiguous_variable_name(
                                id,
                                Range::from_located(expr),
                            ) {
                                self.add_check(check);
                            }
                        }

                        self.check_builtin_shadowing(id, Range::from_located(expr), true);

                        self.handle_node_store(id, expr, self.current_parent());
                    }
                    ExprContext::Del => self.handle_node_delete(expr),
                }

                if self.settings.enabled.contains(&CheckCode::YTT202) {
                    flake8_2020::plugins::name_or_attribute(self, expr);
                }
            }
            ExprKind::Attribute { attr, .. } => {
                // Ex) typing.List[...]
                if self.settings.enabled.contains(&CheckCode::U006)
                    && self.settings.target_version >= PythonVersion::Py39
                    && typing::is_pep585_builtin(expr, &self.from_imports, &self.import_aliases)
                {
                    pyupgrade::plugins::use_pep585_annotation(self, expr, attr);
                }

                if self.settings.enabled.contains(&CheckCode::YTT202) {
                    flake8_2020::plugins::name_or_attribute(self, expr);
                }
            }
            ExprKind::Call {
                func,
                args,
                keywords,
            } => {
                if self.settings.enabled.contains(&CheckCode::U005) {
                    pyupgrade::plugins::deprecated_unittest_alias(self, func);
                }

                // flake8-super
                if self.settings.enabled.contains(&CheckCode::U008) {
                    pyupgrade::plugins::super_call_with_parameters(self, expr, func, args);
                }

                if self.settings.enabled.contains(&CheckCode::U012) {
                    pyupgrade::plugins::unnecessary_encode_utf8(self, expr, func, args, keywords);
                }

                // flake8-print
                if self.settings.enabled.contains(&CheckCode::T201)
                    || self.settings.enabled.contains(&CheckCode::T203)
                {
                    flake8_print::plugins::print_call(self, expr, func);
                }

                if self.settings.enabled.contains(&CheckCode::B004) {
                    flake8_bugbear::plugins::unreliable_callable_check(self, expr, func, args);
                }
                if self.settings.enabled.contains(&CheckCode::B005) {
                    flake8_bugbear::plugins::strip_with_multi_characters(self, expr, func, args);
                }
                if self.settings.enabled.contains(&CheckCode::B009) {
                    flake8_bugbear::plugins::getattr_with_constant(self, expr, func, args);
                }
                if self.settings.enabled.contains(&CheckCode::B010) {
                    if !self
                        .scope_stack
                        .iter()
                        .rev()
                        .any(|index| matches!(self.scopes[*index].kind, ScopeKind::Lambda))
                    {
                        flake8_bugbear::plugins::setattr_with_constant(self, expr, func, args);
                    }
                }
                if self.settings.enabled.contains(&CheckCode::B022) {
                    flake8_bugbear::plugins::useless_contextlib_suppress(self, expr, args);
                }
                if self.settings.enabled.contains(&CheckCode::B026) {
                    flake8_bugbear::plugins::star_arg_unpacking_after_keyword_arg(
                        self, args, keywords,
                    );
                }
                if self.settings.enabled.contains(&CheckCode::S102) {
                    if let Some(check) = flake8_bandit::plugins::exec_used(expr, func) {
                        self.add_check(check);
                    }
                }
                if self.settings.enabled.contains(&CheckCode::S106) {
                    self.add_checks(
                        flake8_bandit::plugins::hardcoded_password_func_arg(keywords).into_iter(),
                    );
                }

                // flake8-comprehensions
                if self.settings.enabled.contains(&CheckCode::C400) {
                    if let Some(check) = flake8_comprehensions::checks::unnecessary_generator_list(
                        expr,
                        func,
                        args,
                        keywords,
                        self.locator,
                        self.patch(&CheckCode::C400),
                        Range::from_located(expr),
                    ) {
                        self.add_check(check);
                    };
                }

                if self.settings.enabled.contains(&CheckCode::C401) {
                    if let Some(check) = flake8_comprehensions::checks::unnecessary_generator_set(
                        expr,
                        func,
                        args,
                        keywords,
                        self.locator,
                        self.patch(&CheckCode::C401),
                        Range::from_located(expr),
                    ) {
                        self.add_check(check);
                    };
                }

                if self.settings.enabled.contains(&CheckCode::C402) {
                    if let Some(check) = flake8_comprehensions::checks::unnecessary_generator_dict(
                        expr,
                        func,
                        args,
                        keywords,
                        self.locator,
                        self.patch(&CheckCode::C402),
                        Range::from_located(expr),
                    ) {
                        self.add_check(check);
                    };
                }

                if self.settings.enabled.contains(&CheckCode::C403) {
                    if let Some(check) =
                        flake8_comprehensions::checks::unnecessary_list_comprehension_set(
                            expr,
                            func,
                            args,
                            keywords,
                            self.locator,
                            self.patch(&CheckCode::C403),
                            Range::from_located(expr),
                        )
                    {
                        self.add_check(check);
                    };
                }

                if self.settings.enabled.contains(&CheckCode::C404) {
                    if let Some(check) =
                        flake8_comprehensions::checks::unnecessary_list_comprehension_dict(
                            expr,
                            func,
                            args,
                            keywords,
                            self.locator,
                            self.patch(&CheckCode::C404),
                            Range::from_located(expr),
                        )
                    {
                        self.add_check(check);
                    };
                }

                if self.settings.enabled.contains(&CheckCode::C405) {
                    if let Some(check) = flake8_comprehensions::checks::unnecessary_literal_set(
                        expr,
                        func,
                        args,
                        keywords,
                        self.locator,
                        self.patch(&CheckCode::C405),
                        Range::from_located(expr),
                    ) {
                        self.add_check(check);
                    };
                }

                if self.settings.enabled.contains(&CheckCode::C406) {
                    if let Some(check) = flake8_comprehensions::checks::unnecessary_literal_dict(
                        expr,
                        func,
                        args,
                        keywords,
                        self.locator,
                        self.patch(&CheckCode::C406),
                        Range::from_located(expr),
                    ) {
                        self.add_check(check);
                    };
                }

                if self.settings.enabled.contains(&CheckCode::C408) {
                    if let Some(check) = flake8_comprehensions::checks::unnecessary_collection_call(
                        expr,
                        func,
                        args,
                        keywords,
                        self.locator,
                        self.patch(&CheckCode::C408),
                        Range::from_located(expr),
                    ) {
                        self.add_check(check);
                    };
                }

                if self.settings.enabled.contains(&CheckCode::C409) {
                    if let Some(check) =
                        flake8_comprehensions::checks::unnecessary_literal_within_tuple_call(
                            expr,
                            func,
                            args,
                            self.locator,
                            self.patch(&CheckCode::C409),
                            Range::from_located(expr),
                        )
                    {
                        self.add_check(check);
                    };
                }

                if self.settings.enabled.contains(&CheckCode::C410) {
                    if let Some(check) =
                        flake8_comprehensions::checks::unnecessary_literal_within_list_call(
                            expr,
                            func,
                            args,
                            self.locator,
                            self.patch(&CheckCode::C410),
                            Range::from_located(expr),
                        )
                    {
                        self.add_check(check);
                    };
                }

                if self.settings.enabled.contains(&CheckCode::C411) {
                    if let Some(check) = flake8_comprehensions::checks::unnecessary_list_call(
                        expr,
                        func,
                        args,
                        self.locator,
                        self.patch(&CheckCode::C411),
                        Range::from_located(expr),
                    ) {
                        self.add_check(check);
                    };
                }

                if self.settings.enabled.contains(&CheckCode::C413) {
                    if let Some(check) =
                        flake8_comprehensions::checks::unnecessary_call_around_sorted(
                            expr,
                            func,
                            args,
                            self.locator,
                            self.patch(&CheckCode::C413),
                            Range::from_located(expr),
                        )
                    {
                        self.add_check(check);
                    };
                }

                if self.settings.enabled.contains(&CheckCode::C414) {
                    if let Some(check) =
                        flake8_comprehensions::checks::unnecessary_double_cast_or_process(
                            func,
                            args,
                            Range::from_located(expr),
                        )
                    {
                        self.add_check(check);
                    };
                }

                if self.settings.enabled.contains(&CheckCode::C415) {
                    if let Some(check) =
                        flake8_comprehensions::checks::unnecessary_subscript_reversal(
                            func,
                            args,
                            Range::from_located(expr),
                        )
                    {
                        self.add_check(check);
                    };
                }

                if self.settings.enabled.contains(&CheckCode::C417) {
                    if let Some(check) = flake8_comprehensions::checks::unnecessary_map(
                        func,
                        args,
                        Range::from_located(expr),
                    ) {
                        self.add_check(check);
                    };
                }

                // pyupgrade
                if self.settings.enabled.contains(&CheckCode::U003) {
                    pyupgrade::plugins::type_of_primitive(self, expr, func, args);
                }

                // flake8-boolean-trap
                if self.settings.enabled.contains(&CheckCode::FBT003) {
                    flake8_boolean_trap::plugins::check_boolean_positional_value_in_function_call(
                        self, args,
                    );
                }
                if let ExprKind::Name { id, ctx } = &func.node {
                    if id == "locals" && matches!(ctx, ExprContext::Load) {
                        let scope = &mut self.scopes
                            [*(self.scope_stack.last().expect("No current scope found."))];
                        if let ScopeKind::Function(inner) = &mut scope.kind {
                            inner.uses_locals = true;
                        }
                    }
                }

                // Ruff
                if self.settings.enabled.contains(&CheckCode::RUF101) {
                    rules::plugins::convert_exit_to_sys_exit(self, func);
                }
            }
            ExprKind::Dict { keys, .. } => {
                let check_repeated_literals = self.settings.enabled.contains(&CheckCode::F601);
                let check_repeated_variables = self.settings.enabled.contains(&CheckCode::F602);
                if check_repeated_literals || check_repeated_variables {
                    self.add_checks(
                        pyflakes::checks::repeated_keys(
                            keys,
                            check_repeated_literals,
                            check_repeated_variables,
                        )
                        .into_iter(),
                    );
                }
            }
            ExprKind::Yield { .. } | ExprKind::YieldFrom { .. } | ExprKind::Await { .. } => {
                let scope = self.current_scope();
                if self.settings.enabled.contains(&CheckCode::F704) {
                    if matches!(scope.kind, ScopeKind::Class(_) | ScopeKind::Module) {
                        self.add_check(Check::new(
                            CheckKind::YieldOutsideFunction,
                            Range::from_located(expr),
                        ));
                    }
                }
            }
            ExprKind::JoinedStr { values } => {
                if self.settings.enabled.contains(&CheckCode::F541) {
                    if self.in_f_string.is_none()
                        && !values
                            .iter()
                            .any(|value| matches!(value.node, ExprKind::FormattedValue { .. }))
                    {
                        self.add_check(Check::new(
                            CheckKind::FStringMissingPlaceholders,
                            Range::from_located(expr),
                        ));
                    }
                }
                self.in_f_string = Some(Range::from_located(expr));
            }
            ExprKind::BinOp {
                left,
                op: Operator::RShift,
                ..
            } => {
                if self.settings.enabled.contains(&CheckCode::F633) {
                    pyflakes::plugins::invalid_print_syntax(self, left);
                }
            }
            ExprKind::UnaryOp { op, operand } => {
                let check_not_in = self.settings.enabled.contains(&CheckCode::E713);
                let check_not_is = self.settings.enabled.contains(&CheckCode::E714);
                if check_not_in || check_not_is {
                    pycodestyle::plugins::not_tests(
                        self,
                        expr,
                        op,
                        operand,
                        check_not_in,
                        check_not_is,
                    );
                }

                if self.settings.enabled.contains(&CheckCode::B002) {
                    flake8_bugbear::plugins::unary_prefix_increment(self, expr, op, operand);
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
                    pycodestyle::plugins::literal_comparisons(
                        self,
                        expr,
                        left,
                        ops,
                        comparators,
                        check_none_comparisons,
                        check_true_false_comparisons,
                    )
                }

                if self.settings.enabled.contains(&CheckCode::F632) {
                    pyflakes::plugins::invalid_literal_comparison(
                        self,
                        left,
                        ops,
                        comparators,
                        Range::from_located(expr),
                    );
                }

                if self.settings.enabled.contains(&CheckCode::E721) {
                    self.add_checks(
                        pycodestyle::checks::type_comparison(
                            ops,
                            comparators,
                            Range::from_located(expr),
                        )
                        .into_iter(),
                    );
                }

                if self.settings.enabled.contains(&CheckCode::YTT103)
                    || self.settings.enabled.contains(&CheckCode::YTT201)
                    || self.settings.enabled.contains(&CheckCode::YTT203)
                    || self.settings.enabled.contains(&CheckCode::YTT204)
                    || self.settings.enabled.contains(&CheckCode::YTT302)
                {
                    flake8_2020::plugins::compare(self, left, ops, comparators);
                }

                if self.settings.enabled.contains(&CheckCode::S105) {
                    self.add_checks(
                        flake8_bandit::plugins::compare_to_hardcoded_password_string(
                            left,
                            comparators,
                        )
                        .into_iter(),
                    );
                }
            }
            ExprKind::Constant {
                value: Constant::Str(value),
                ..
            } => {
                if self.in_annotation && !self.in_literal {
                    self.deferred_string_annotations.push((
                        Range::from_located(expr),
                        value,
                        self.scope_stack.clone(),
                        self.parent_stack.clone(),
                    ));
                }
                if self.settings.enabled.contains(&CheckCode::S104) {
                    if let Some(check) = flake8_bandit::plugins::hardcoded_bind_all_interfaces(
                        value,
                        &Range::from_located(expr),
                    ) {
                        self.add_check(check);
                    }
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
                self.push_scope(Scope::new(ScopeKind::Lambda))
            }
            ExprKind::ListComp { elt, generators } | ExprKind::SetComp { elt, generators } => {
                if self.settings.enabled.contains(&CheckCode::C416) {
                    if let Some(check) = flake8_comprehensions::checks::unnecessary_comprehension(
                        expr,
                        elt,
                        generators,
                        self.locator,
                        self.patch(&CheckCode::C416),
                        Range::from_located(expr),
                    ) {
                        self.add_check(check);
                    };
                }
                self.push_scope(Scope::new(ScopeKind::Generator))
            }
            ExprKind::GeneratorExp { .. } | ExprKind::DictComp { .. } => {
                self.push_scope(Scope::new(ScopeKind::Generator))
            }
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
                let call_path = dealias_call_path(collect_call_paths(func), &self.import_aliases);
                if self.match_typing_call_path(&call_path, "ForwardRef") {
                    self.visit_expr(func);
                    for expr in args {
                        self.visit_annotation(expr);
                    }
                } else if self.match_typing_call_path(&call_path, "cast") {
                    self.visit_expr(func);
                    if !args.is_empty() {
                        self.visit_annotation(&args[0]);
                    }
                    for expr in args.iter().skip(1) {
                        self.visit_expr(expr);
                    }
                } else if self.match_typing_call_path(&call_path, "NewType") {
                    self.visit_expr(func);
                    for expr in args.iter().skip(1) {
                        self.visit_annotation(expr);
                    }
                } else if self.match_typing_call_path(&call_path, "TypeVar") {
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
                } else if self.match_typing_call_path(&call_path, "NamedTuple") {
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
                } else if self.match_typing_call_path(&call_path, "TypedDict") {
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
                // Only allow annotations in `ExprContext::Load`. If we have, e.g.,
                // `obj["foo"]["bar"]`, we need to avoid treating the `obj["foo"]`
                // portion as an annotation, despite having `ExprContext::Load`. Thus, we track
                // the `ExprContext` at the top-level.
                let prev_in_subscript = self.in_subscript;
                if self.in_subscript {
                    visitor::walk_expr(self, expr);
                } else if matches!(ctx, ExprContext::Store | ExprContext::Del) {
                    self.in_subscript = true;
                    visitor::walk_expr(self, expr);
                } else {
                    self.in_subscript = true;
                    match typing::match_annotated_subscript(
                        value,
                        &self.from_imports,
                        &self.import_aliases,
                    ) {
                        Some(subscript) => {
                            match subscript {
                                // Ex) Optional[int]
                                SubscriptKind::AnnotatedSubscript => {
                                    self.visit_expr(value);
                                    self.visit_annotation(slice);
                                    self.visit_expr_context(ctx);
                                }
                                // Ex) Annotated[int, "Hello, world!"]
                                SubscriptKind::PEP593AnnotatedSubscript => {
                                    // First argument is a type (including forward references); the
                                    // rest are arbitrary Python
                                    // objects.
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
                                        error!(
                                            "Found non-ExprKind::Tuple argument to PEP 593 \
                                             Annotation."
                                        )
                                    }
                                }
                            }
                        }
                        None => visitor::walk_expr(self, expr),
                    }
                }
                self.in_subscript = prev_in_subscript;
            }
            _ => visitor::walk_expr(self, expr),
        }

        // Post-visit.
        match &expr.node {
            ExprKind::Lambda { .. }
            | ExprKind::GeneratorExp { .. }
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
                    self.add_check(Check::new(
                        CheckKind::DoNotUseBareExcept,
                        Range::from_located(excepthandler),
                    ));
                }
                match name {
                    Some(name) => {
                        if self.settings.enabled.contains(&CheckCode::E741) {
                            if let Some(check) = pycodestyle::checks::ambiguous_variable_name(
                                name,
                                Range::from_located(excepthandler),
                            ) {
                                self.add_check(check);
                            }
                        }

                        self.check_builtin_shadowing(
                            name,
                            Range::from_located(excepthandler),
                            false,
                        );

                        if self.current_scope().values.contains_key(&name.as_str()) {
                            self.handle_node_store(
                                name,
                                &Expr::new(
                                    excepthandler.location,
                                    excepthandler.end_location.unwrap(),
                                    ExprKind::Name {
                                        id: name.to_string(),
                                        ctx: ExprContext::Store,
                                    },
                                ),
                                self.current_parent(),
                            );
                        }

                        let definition = self.current_scope().values.get(&name.as_str()).cloned();
                        self.handle_node_store(
                            name,
                            &Expr::new(
                                excepthandler.location,
                                excepthandler.end_location.unwrap(),
                                ExprKind::Name {
                                    id: name.to_string(),
                                    ctx: ExprContext::Store,
                                },
                            ),
                            self.current_parent(),
                        );

                        walk_excepthandler(self, excepthandler);

                        if let Some(binding) = {
                            let scope = &mut self.scopes
                                [*(self.scope_stack.last().expect("No current scope found."))];
                            &scope.values.remove(&name.as_str())
                        } {
                            if binding.used.is_none() {
                                if self.settings.enabled.contains(&CheckCode::F841) {
                                    self.add_check(Check::new(
                                        CheckKind::UnusedVariable(name.to_string()),
                                        Range::from_located(excepthandler),
                                    ));
                                }
                            }
                        }

                        if let Some(binding) = definition {
                            let scope = &mut self.scopes
                                [*(self.scope_stack.last().expect("No current scope found."))];
                            scope.values.insert(name, binding);
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
                .extend(pyflakes::checks::duplicate_arguments(arguments));
        }
        if self.settings.enabled.contains(&CheckCode::B006) {
            flake8_bugbear::plugins::mutable_argument_default(self, arguments)
        }
        if self.settings.enabled.contains(&CheckCode::B008) {
            flake8_bugbear::plugins::function_call_argument_default(self, arguments)
        }

        // flake8-boolean-trap
        if self.settings.enabled.contains(&CheckCode::FBT001) {
            flake8_boolean_trap::plugins::check_positional_boolean_in_def(self, arguments);
        }
        if self.settings.enabled.contains(&CheckCode::FBT002) {
            flake8_boolean_trap::plugins::check_boolean_default_value_in_function_definition(
                self, arguments,
            );
        }

        // Bind, but intentionally avoid walking default expressions, as we handle them
        // upstream.
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
        // Bind, but intentionally avoid walking the annotation, as we handle it
        // upstream.
        self.add_binding(
            &arg.node.arg,
            Binding {
                kind: BindingKind::Argument,
                used: None,
                range: Range::from_located(arg),
            },
        );

        if self.settings.enabled.contains(&CheckCode::E741) {
            if let Some(check) = pycodestyle::checks::ambiguous_variable_name(
                &arg.node.arg,
                Range::from_located(arg),
            ) {
                self.add_check(check);
            }
        }

        if self.settings.enabled.contains(&CheckCode::N803) {
            if let Some(check) =
                pep8_naming::checks::invalid_argument_name(&arg.node.arg, Range::from_located(arg))
            {
                self.add_check(check);
            }
        }

        self.check_builtin_arg_shadowing(&arg.node.arg, Range::from_located(arg));
    }
}

fn try_mark_used(scope: &mut Scope, scope_id: usize, id: &str, expr: &Expr) -> bool {
    let alias = if let Some(binding) = scope.values.get_mut(id) {
        // Mark the binding as used.
        binding.used = Some((scope_id, Range::from_located(expr)));

        // If the name of the sub-importation is the same as an alias of another
        // importation and the alias is used, that sub-importation should be
        // marked as used too.
        //
        // This handles code like:
        //   import pyarrow as pa
        //   import pyarrow.csv
        //   print(pa.csv.read_csv("test.csv"))
        if let BindingKind::Importation(name, full_name, _)
        | BindingKind::FromImportation(name, full_name, _)
        | BindingKind::SubmoduleImportation(name, full_name, _) = &binding.kind
        {
            let has_alias = full_name
                .split('.')
                .last()
                .map(|segment| segment != name)
                .unwrap_or_default();
            if has_alias {
                // Clone the alias. (We'll mutate it below.)
                full_name.to_string()
            } else {
                return true;
            }
        } else {
            return true;
        }
    } else {
        return false;
    };

    // Mark the sub-importation as used.
    if let Some(binding) = scope.values.get_mut(alias.as_str()) {
        binding.used = Some((scope_id, Range::from_located(expr)));
    }
    true
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

    fn push_scope(&mut self, scope: Scope<'a>) {
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
                builtin,
                Binding {
                    kind: BindingKind::Builtin,
                    range: Default::default(),
                    used: None,
                },
            );
        }
        for builtin in MAGIC_GLOBALS {
            scope.values.insert(
                builtin,
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

    pub fn current_scopes(&self) -> impl Iterator<Item = &Scope> {
        self.scope_stack.iter().rev().map(|s| &self.scopes[*s])
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

    fn add_binding<'b>(&mut self, name: &'b str, binding: Binding)
    where
        'b: 'a,
    {
        if self.settings.enabled.contains(&CheckCode::F402) {
            let scope = self.current_scope();
            if let Some(existing) = scope.values.get(&name) {
                if matches!(binding.kind, BindingKind::LoopVar)
                    && matches!(
                        existing.kind,
                        BindingKind::Importation(..)
                            | BindingKind::FromImportation(..)
                            | BindingKind::SubmoduleImportation(..)
                            | BindingKind::StarImportation(..)
                            | BindingKind::FutureImportation
                    )
                {
                    self.add_check(Check::new(
                        CheckKind::ImportShadowedByLoopVar(
                            name.to_string(),
                            existing.range.location.row(),
                        ),
                        binding.range,
                    ));
                }
            }
        }

        // TODO(charlie): Don't treat annotations as assignments if there is an existing
        // value.
        let scope = self.current_scope();
        let binding = match scope.values.get(&name) {
            None => binding,
            Some(existing) => Binding {
                kind: binding.kind,
                range: binding.range,
                used: existing.used,
            },
        };

        let scope = &mut self.scopes[*(self.scope_stack.last().expect("No current scope found."))];
        scope.values.insert(name, binding);
    }

    fn handle_node_load(&mut self, expr: &Expr) {
        if let ExprKind::Name { id, .. } = &expr.node {
            let scope_id = self.current_scope().id;

            let mut first_iter = true;
            let mut in_generator = false;
            let mut import_starred = false;
            for scope_index in self.scope_stack.iter().rev() {
                let scope = &mut self.scopes[*scope_index];

                if matches!(scope.kind, ScopeKind::Class(_)) {
                    if id == "__class__" {
                        return;
                    } else if !first_iter && !in_generator {
                        continue;
                    }
                }

                if try_mark_used(scope, scope_id, id, expr) {
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
                        for binding in scope.values.values() {
                            if let BindingKind::StarImportation(level, module) = &binding.kind {
                                from_list.push(helpers::format_import_from(
                                    level.as_ref(),
                                    module.as_ref(),
                                ));
                            }
                        }
                    }
                    from_list.sort();

                    self.add_check(Check::new(
                        CheckKind::ImportStarUsage(id.to_string(), from_list),
                        Range::from_located(expr),
                    ));
                }
                return;
            }

            if self.settings.enabled.contains(&CheckCode::F821) {
                // Allow __path__.
                if self.path.ends_with("__init__.py") && id == "__path__" {
                    return;
                }

                // Avoid flagging if NameError is handled.
                if let Some(handler_names) = self.except_handlers.last() {
                    if handler_names
                        .iter()
                        .any(|call_path| call_path.len() == 1 && call_path[0] == "NameError")
                    {
                        return;
                    }
                }

                self.add_check(Check::new(
                    CheckKind::UndefinedName(id.clone()),
                    Range::from_located(expr),
                ))
            }
        }
    }

    fn handle_node_store<'b>(&mut self, id: &'b str, expr: &Expr, parent: &Stmt)
    where
        'b: 'a,
    {
        if self.settings.enabled.contains(&CheckCode::F823) {
            let scopes: Vec<&Scope> = self
                .scope_stack
                .iter()
                .map(|index| &self.scopes[*index])
                .collect();
            if let Some(check) = pyflakes::checks::undefined_local(&scopes, id) {
                self.add_check(check);
            }
        }

        if self.settings.enabled.contains(&CheckCode::N806) {
            if matches!(self.current_scope().kind, ScopeKind::Function(..)) {
                // Ignore globals.
                if !self
                    .current_scope()
                    .values
                    .get(id)
                    .map(|binding| matches!(binding.kind, BindingKind::Global))
                    .unwrap_or(false)
                {
                    pep8_naming::plugins::non_lowercase_variable_in_function(self, expr, parent, id)
                }
            }
        }

        if self.settings.enabled.contains(&CheckCode::N815) {
            if matches!(self.current_scope().kind, ScopeKind::Class(..)) {
                pep8_naming::plugins::mixed_case_variable_in_class_scope(self, expr, parent, id)
            }
        }

        if self.settings.enabled.contains(&CheckCode::N816) {
            if matches!(self.current_scope().kind, ScopeKind::Module) {
                pep8_naming::plugins::mixed_case_variable_in_global_scope(self, expr, parent, id)
            }
        }

        if matches!(parent.node, StmtKind::AnnAssign { value: None, .. }) {
            self.add_binding(
                id,
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
                id,
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
                id,
                Binding {
                    kind: BindingKind::Binding,
                    used: None,
                    range: Range::from_located(expr),
                },
            );
            return;
        }

        let current = self.current_scope();
        if id == "__all__"
            && matches!(current.kind, ScopeKind::Module)
            && matches!(
                parent.node,
                StmtKind::Assign { .. } | StmtKind::AugAssign { .. } | StmtKind::AnnAssign { .. }
            )
        {
            self.add_binding(
                id,
                Binding {
                    kind: BindingKind::Export(extract_all_names(parent, current)),
                    used: None,
                    range: Range::from_located(expr),
                },
            );
            return;
        }

        self.add_binding(
            id,
            Binding {
                kind: BindingKind::Assignment,
                used: None,
                range: Range::from_located(expr),
            },
        );
    }

    fn handle_node_delete<'b>(&mut self, expr: &'b Expr)
    where
        'b: 'a,
    {
        if let ExprKind::Name { id, .. } = &expr.node {
            if operations::on_conditional_branch(
                &mut self
                    .parent_stack
                    .iter()
                    .rev()
                    .map(|index| self.parents[*index]),
            ) {
                return;
            }

            let scope =
                &mut self.scopes[*(self.scope_stack.last().expect("No current scope found."))];
            if scope.values.remove(&id.as_str()).is_none()
                && self.settings.enabled.contains(&CheckCode::F821)
            {
                self.add_check(Check::new(
                    CheckKind::UndefinedName(id.to_string()),
                    Range::from_located(expr),
                ))
            }
        }
    }

    fn visit_docstring<'b>(&mut self, python_ast: &'b Suite) -> bool
    where
        'b: 'a,
    {
        if self.settings.enabled.contains(&CheckCode::B021) {
            flake8_bugbear::plugins::f_string_docstring(self, python_ast);
        }
        let docstring = docstrings::extraction::docstring_from(python_ast);
        self.definitions.push((
            Definition {
                kind: if self.path.ends_with("__init__.py") {
                    DefinitionKind::Package
                } else {
                    DefinitionKind::Module
                },
                docstring,
            },
            self.visible_scope.visibility.clone(),
        ));
        docstring.is_some()
    }

    fn check_deferred_annotations(&mut self) {
        while let Some((expr, scopes, parents)) = self.deferred_annotations.pop() {
            self.scope_stack = scopes;
            self.parent_stack = parents;
            self.visit_expr(expr);
        }
    }

    fn check_deferred_string_annotations<'b>(&mut self, allocator: &'b mut Vec<Expr>)
    where
        'b: 'a,
    {
        let mut stacks = vec![];
        while let Some((range, expression, scopes, parents)) =
            self.deferred_string_annotations.pop()
        {
            if let Ok(mut expr) = parser::parse_expression(expression, "<filename>") {
                relocate_expr(&mut expr, range);
                allocator.push(expr);
                stacks.push((scopes, parents));
            } else {
                if self.settings.enabled.contains(&CheckCode::F722) {
                    self.add_check(Check::new(
                        CheckKind::ForwardAnnotationSyntaxError(expression.to_string()),
                        range,
                    ));
                }
            }
        }
        for (expr, (scopes, parents)) in allocator.iter().zip(stacks) {
            self.scope_stack = scopes;
            self.parent_stack = parents;
            self.visit_expr(expr);
        }
    }

    fn check_deferred_functions(&mut self) {
        while let Some((stmt, scopes, parents, visibility)) = self.deferred_functions.pop() {
            self.parent_stack = parents;
            self.scope_stack = scopes;
            self.visible_scope = visibility;
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
            self.push_scope(Scope::new(ScopeKind::Lambda));

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
                self.add_checks(
                    pyflakes::checks::unused_variables(
                        &self.scopes[index],
                        &self.settings.dummy_variable_rgx,
                    )
                    .into_iter(),
                );
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

        let mut checks: Vec<Check> = vec![];
        for scope in self.dead_scopes.iter().map(|index| &self.scopes[*index]) {
            let all_binding: Option<&Binding> = scope.values.get("__all__");
            let all_names: Option<Vec<&str>> =
                all_binding.and_then(|binding| match &binding.kind {
                    BindingKind::Export(names) => {
                        Some(names.iter().map(|name| name.as_str()).collect())
                    }
                    _ => None,
                });

            if self.settings.enabled.contains(&CheckCode::F822) {
                if !scope.import_starred && !self.path.ends_with("__init__.py") {
                    if let Some(all_binding) = all_binding {
                        if let Some(names) = &all_names {
                            for name in names {
                                if !scope.values.contains_key(name) {
                                    checks.push(Check::new(
                                        CheckKind::UndefinedExport(name.to_string()),
                                        all_binding.range,
                                    ));
                                }
                            }
                        }
                    }
                }
            }

            if self.settings.enabled.contains(&CheckCode::F405) {
                if scope.import_starred {
                    if let Some(all_binding) = all_binding {
                        if let Some(names) = &all_names {
                            let mut from_list = vec![];
                            for binding in scope.values.values() {
                                if let BindingKind::StarImportation(level, module) = &binding.kind {
                                    from_list.push(helpers::format_import_from(
                                        level.as_ref(),
                                        module.as_ref(),
                                    ));
                                }
                            }
                            from_list.sort();

                            for name in names {
                                if !scope.values.contains_key(name) {
                                    checks.push(Check::new(
                                        CheckKind::ImportStarUsage(
                                            name.to_string(),
                                            from_list.clone(),
                                        ),
                                        all_binding.range,
                                    ));
                                }
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

                for (name, binding) in scope.values.iter() {
                    if !matches!(
                        binding.kind,
                        BindingKind::Importation(..)
                            | BindingKind::SubmoduleImportation(..)
                            | BindingKind::FromImportation(..)
                    ) {
                        continue;
                    }

                    let used = binding.used.is_some()
                        || all_names
                            .as_ref()
                            .map(|names| names.contains(name))
                            .unwrap_or_default();

                    if !used {
                        match &binding.kind {
                            BindingKind::FromImportation(_, full_name, context) => {
                                unused
                                    .entry((
                                        ImportKind::ImportFrom,
                                        context.defined_by,
                                        context.defined_in,
                                    ))
                                    .or_default()
                                    .push(full_name);
                            }
                            BindingKind::Importation(_, full_name, context)
                            | BindingKind::SubmoduleImportation(_, full_name, context) => {
                                unused
                                    .entry((
                                        ImportKind::Import,
                                        context.defined_by,
                                        context.defined_in,
                                    ))
                                    .or_default()
                                    .push(full_name);
                            }
                            _ => unreachable!("Already filtered on BindingKind."),
                        }
                    }
                }

                for ((kind, defined_by, defined_in), full_names) in unused {
                    let child = self.parents[defined_by];
                    let parent = defined_in.map(|defined_in| self.parents[defined_in]);

                    let fix = if self.patch(&CheckCode::F401) {
                        let deleted: Vec<&Stmt> = self
                            .deletions
                            .iter()
                            .map(|index| self.parents[*index])
                            .collect();
                        match match kind {
                            ImportKind::Import => pyflakes::fixes::remove_unused_imports,
                            ImportKind::ImportFrom => pyflakes::fixes::remove_unused_import_froms,
                        }(
                            self.locator, &full_names, child, parent, &deleted
                        ) {
                            Ok(fix) => {
                                if fix.patch.content.is_empty() || fix.patch.content == "pass" {
                                    self.deletions.insert(defined_by);
                                }
                                Some(fix)
                            }
                            Err(e) => {
                                error!("Failed to remove unused imports: {}", e);
                                None
                            }
                        }
                    } else {
                        None
                    };

                    if self.path.ends_with("__init__.py") {
                        checks.push(Check::new(
                            CheckKind::UnusedImport(
                                full_names.into_iter().sorted().map(String::from).collect(),
                                true,
                            ),
                            Range::from_located(child),
                        ));
                    } else {
                        let mut check = Check::new(
                            CheckKind::UnusedImport(
                                full_names.into_iter().sorted().map(String::from).collect(),
                                false,
                            ),
                            Range::from_located(child),
                        );
                        if let Some(fix) = fix {
                            check.amend(fix);
                        }
                        checks.push(check);
                    }
                }
            }
        }
        self.add_checks(checks.into_iter());
    }

    fn check_definitions(&mut self) {
        while let Some((definition, visibility)) = self.definitions.pop() {
            // flake8-annotations
            if self.settings.enabled.contains(&CheckCode::ANN001)
                || self.settings.enabled.contains(&CheckCode::ANN002)
                || self.settings.enabled.contains(&CheckCode::ANN003)
                || self.settings.enabled.contains(&CheckCode::ANN101)
                || self.settings.enabled.contains(&CheckCode::ANN102)
                || self.settings.enabled.contains(&CheckCode::ANN201)
                || self.settings.enabled.contains(&CheckCode::ANN202)
                || self.settings.enabled.contains(&CheckCode::ANN204)
                || self.settings.enabled.contains(&CheckCode::ANN205)
                || self.settings.enabled.contains(&CheckCode::ANN206)
                || self.settings.enabled.contains(&CheckCode::ANN401)
            {
                flake8_annotations::plugins::definition(self, &definition, &visibility);
            }

            // pydocstyle
            if !pydocstyle::plugins::not_empty(self, &definition) {
                continue;
            }
            if !pydocstyle::plugins::not_missing(self, &definition, &visibility) {
                continue;
            }
            if self.settings.enabled.contains(&CheckCode::D200) {
                pydocstyle::plugins::one_liner(self, &definition);
            }
            if self.settings.enabled.contains(&CheckCode::D201)
                || self.settings.enabled.contains(&CheckCode::D202)
            {
                pydocstyle::plugins::blank_before_after_function(self, &definition);
            }
            if self.settings.enabled.contains(&CheckCode::D203)
                || self.settings.enabled.contains(&CheckCode::D204)
                || self.settings.enabled.contains(&CheckCode::D211)
            {
                pydocstyle::plugins::blank_before_after_class(self, &definition);
            }
            if self.settings.enabled.contains(&CheckCode::D205) {
                pydocstyle::plugins::blank_after_summary(self, &definition);
            }
            if self.settings.enabled.contains(&CheckCode::D206)
                || self.settings.enabled.contains(&CheckCode::D207)
                || self.settings.enabled.contains(&CheckCode::D208)
            {
                pydocstyle::plugins::indent(self, &definition);
            }
            if self.settings.enabled.contains(&CheckCode::D209) {
                pydocstyle::plugins::newline_after_last_paragraph(self, &definition);
            }
            if self.settings.enabled.contains(&CheckCode::D210) {
                pydocstyle::plugins::no_surrounding_whitespace(self, &definition);
            }
            if self.settings.enabled.contains(&CheckCode::D212)
                || self.settings.enabled.contains(&CheckCode::D213)
            {
                pydocstyle::plugins::multi_line_summary_start(self, &definition);
            }
            if self.settings.enabled.contains(&CheckCode::D300) {
                pydocstyle::plugins::triple_quotes(self, &definition);
            }
            if self.settings.enabled.contains(&CheckCode::D400) {
                pydocstyle::plugins::ends_with_period(self, &definition);
            }
            if self.settings.enabled.contains(&CheckCode::D402) {
                pydocstyle::plugins::no_signature(self, &definition);
            }
            if self.settings.enabled.contains(&CheckCode::D403) {
                pydocstyle::plugins::capitalized(self, &definition);
            }
            if self.settings.enabled.contains(&CheckCode::D404) {
                pydocstyle::plugins::starts_with_this(self, &definition);
            }
            if self.settings.enabled.contains(&CheckCode::D415) {
                pydocstyle::plugins::ends_with_punctuation(self, &definition);
            }
            if self.settings.enabled.contains(&CheckCode::D418) {
                pydocstyle::plugins::if_needed(self, &definition);
            }
            if self.settings.enabled.contains(&CheckCode::D212)
                || self.settings.enabled.contains(&CheckCode::D214)
                || self.settings.enabled.contains(&CheckCode::D215)
                || self.settings.enabled.contains(&CheckCode::D405)
                || self.settings.enabled.contains(&CheckCode::D406)
                || self.settings.enabled.contains(&CheckCode::D407)
                || self.settings.enabled.contains(&CheckCode::D408)
                || self.settings.enabled.contains(&CheckCode::D409)
                || self.settings.enabled.contains(&CheckCode::D410)
                || self.settings.enabled.contains(&CheckCode::D411)
                || self.settings.enabled.contains(&CheckCode::D412)
                || self.settings.enabled.contains(&CheckCode::D413)
                || self.settings.enabled.contains(&CheckCode::D414)
                || self.settings.enabled.contains(&CheckCode::D416)
                || self.settings.enabled.contains(&CheckCode::D417)
            {
                pydocstyle::plugins::sections(self, &definition);
            }
        }
    }

    fn check_builtin_shadowing(&mut self, name: &str, location: Range, is_attribute: bool) {
        if is_attribute && matches!(self.current_scope().kind, ScopeKind::Class(_)) {
            if self.settings.enabled.contains(&CheckCode::A003) {
                if let Some(check) = flake8_builtins::checks::builtin_shadowing(
                    name,
                    location,
                    flake8_builtins::types::ShadowingType::Attribute,
                ) {
                    self.add_check(check);
                }
            }
        } else {
            if self.settings.enabled.contains(&CheckCode::A001) {
                if let Some(check) = flake8_builtins::checks::builtin_shadowing(
                    name,
                    location,
                    flake8_builtins::types::ShadowingType::Variable,
                ) {
                    self.add_check(check);
                }
            }
        }
    }

    fn check_builtin_arg_shadowing(&mut self, name: &str, location: Range) {
        if self.settings.enabled.contains(&CheckCode::A002) {
            if let Some(check) = flake8_builtins::checks::builtin_shadowing(
                name,
                location,
                flake8_builtins::types::ShadowingType::Argument,
            ) {
                self.add_check(check);
            }
        }
    }
}

pub fn check_ast(
    python_ast: &Suite,
    locator: &SourceCodeLocator,
    settings: &Settings,
    autofix: &fixer::Mode,
    path: &Path,
) -> Vec<Check> {
    let mut checker = Checker::new(settings, autofix, path, locator);
    checker.push_scope(Scope::new(ScopeKind::Module));
    checker.bind_builtins();

    // Check for module docstring.
    let python_ast = if checker.visit_docstring(python_ast) {
        &python_ast[1..]
    } else {
        python_ast
    };

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

    // Check docstrings.
    checker.check_definitions();

    checker.checks
}
