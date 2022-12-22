//! Lint rules based on AST traversal.

use std::path::Path;

use itertools::Itertools;
use log::error;
use nohash_hasher::IntMap;
use rustc_hash::{FxHashMap, FxHashSet};
use rustpython_ast::{Located, Location};
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
    Binding, BindingKind, ClassDef, FunctionDef, Lambda, Node, Range, RefEquality, Scope, ScopeKind,
};
use crate::ast::visitor::{walk_excepthandler, Visitor};
use crate::ast::{branch_detection, cast, helpers, operations, visitor};
use crate::checks::{Check, CheckCode, CheckKind, DeferralKeyword};
use crate::docstrings::definition::{Definition, DefinitionKind, Docstring, Documentable};
use crate::noqa::Directive;
use crate::python::builtins::{BUILTINS, MAGIC_GLOBALS};
use crate::python::future::ALL_FEATURE_NAMES;
use crate::python::typing;
use crate::python::typing::SubscriptKind;
use crate::settings::types::PythonVersion;
use crate::settings::{flags, Settings};
use crate::source_code_locator::SourceCodeLocator;
use crate::vendored::cformat::{CFormatError, CFormatErrorType};
use crate::visibility::{module_visibility, transition_scope, Modifier, Visibility, VisibleScope};
use crate::{
    docstrings, flake8_2020, flake8_annotations, flake8_bandit, flake8_blind_except,
    flake8_boolean_trap, flake8_bugbear, flake8_builtins, flake8_comprehensions, flake8_datetimez,
    flake8_debugger, flake8_errmsg, flake8_import_conventions, flake8_print, flake8_return,
    flake8_simplify, flake8_tidy_imports, flake8_unused_arguments, mccabe, noqa, pandas_vet,
    pep8_naming, pycodestyle, pydocstyle, pyflakes, pygrep_hooks, pylint, pyupgrade, visibility,
};

const GLOBAL_SCOPE_INDEX: usize = 0;

type DeferralContext<'a> = (Vec<usize>, Vec<RefEquality<'a, Stmt>>);

#[allow(clippy::struct_excessive_bools)]
pub struct Checker<'a> {
    // Input data.
    path: &'a Path,
    autofix: flags::Autofix,
    noqa: flags::Noqa,
    pub(crate) settings: &'a Settings,
    pub(crate) noqa_line_for: &'a IntMap<usize, usize>,
    pub(crate) locator: &'a SourceCodeLocator<'a>,
    // Computed checks.
    checks: Vec<Check>,
    // Function and class definition tracking (e.g., for docstring enforcement).
    definitions: Vec<(Definition<'a>, Visibility)>,
    // Edit tracking.
    // TODO(charlie): Instead of exposing deletions, wrap in a public API.
    pub(crate) deletions: FxHashSet<RefEquality<'a, Stmt>>,
    // Import tracking.
    pub(crate) from_imports: FxHashMap<&'a str, FxHashSet<&'a str>>,
    pub(crate) import_aliases: FxHashMap<&'a str, &'a str>,
    // Retain all scopes and parent nodes, along with a stack of indexes to track which are active
    // at various points in time.
    pub(crate) parents: Vec<RefEquality<'a, Stmt>>,
    pub(crate) depths: FxHashMap<RefEquality<'a, Stmt>, usize>,
    pub(crate) child_to_parent: FxHashMap<RefEquality<'a, Stmt>, RefEquality<'a, Stmt>>,
    pub(crate) bindings: Vec<Binding<'a>>,
    pub(crate) redefinitions: IntMap<usize, Vec<usize>>,
    exprs: Vec<RefEquality<'a, Expr>>,
    scopes: Vec<Scope<'a>>,
    scope_stack: Vec<usize>,
    dead_scopes: Vec<usize>,
    deferred_string_type_definitions: Vec<(Range, &'a str, bool, DeferralContext<'a>)>,
    deferred_type_definitions: Vec<(&'a Expr, bool, DeferralContext<'a>)>,
    deferred_functions: Vec<(&'a Stmt, DeferralContext<'a>, VisibleScope)>,
    deferred_lambdas: Vec<(&'a Expr, DeferralContext<'a>)>,
    deferred_assignments: Vec<DeferralContext<'a>>,
    // Internal, derivative state.
    visible_scope: VisibleScope,
    in_f_string: Option<Range>,
    in_annotation: bool,
    in_type_definition: bool,
    in_deferred_string_type_definition: bool,
    in_deferred_type_definition: bool,
    in_literal: bool,
    in_subscript: bool,
    seen_import_boundary: bool,
    futures_allowed: bool,
    annotations_future_enabled: bool,
    except_handlers: Vec<Vec<Vec<&'a str>>>,
    // Check-specific state.
    pub(crate) flake8_bugbear_seen: Vec<&'a Expr>,
}

impl<'a> Checker<'a> {
    pub fn new(
        settings: &'a Settings,
        noqa_line_for: &'a IntMap<usize, usize>,
        autofix: flags::Autofix,
        noqa: flags::Noqa,
        path: &'a Path,
        locator: &'a SourceCodeLocator,
    ) -> Checker<'a> {
        Checker {
            settings,
            noqa_line_for,
            autofix,
            noqa,
            path,
            locator,
            checks: vec![],
            definitions: vec![],
            deletions: FxHashSet::default(),
            from_imports: FxHashMap::default(),
            import_aliases: FxHashMap::default(),
            parents: vec![],
            depths: FxHashMap::default(),
            child_to_parent: FxHashMap::default(),
            bindings: vec![],
            redefinitions: IntMap::default(),
            exprs: vec![],
            scopes: vec![],
            scope_stack: vec![],
            dead_scopes: vec![],
            deferred_string_type_definitions: vec![],
            deferred_type_definitions: vec![],
            deferred_functions: vec![],
            deferred_lambdas: vec![],
            deferred_assignments: vec![],
            // Internal, derivative state.
            visible_scope: VisibleScope {
                modifier: Modifier::Module,
                visibility: module_visibility(path),
            },
            in_f_string: None,
            in_annotation: false,
            in_type_definition: false,
            in_deferred_string_type_definition: false,
            in_deferred_type_definition: false,
            in_literal: false,
            in_subscript: false,
            seen_import_boundary: false,
            futures_allowed: true,
            annotations_future_enabled: false,
            except_handlers: vec![],
            // Check-specific state.
            flake8_bugbear_seen: vec![],
        }
    }

    /// Add a `Check` to the `Checker`.
    pub(crate) fn add_check(&mut self, mut check: Check) {
        // If we're in an f-string, override the location. RustPython doesn't produce
        // reliable locations for expressions within f-strings, so we use the
        // span of the f-string itself as a best-effort default.
        if let Some(range) = self.in_f_string {
            check.location = range.location;
            check.end_location = range.end_location;
        }
        self.checks.push(check);
    }

    /// Add multiple `Check` items to the `Checker`.
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
        matches!(self.autofix, flags::Autofix::Enabled)
            && self.in_f_string.is_none()
            && self.settings.fixable.contains(code)
    }

    /// Return the amended `Range` from a `Located`.
    pub fn range_for<T>(&self, located: &Located<T>) -> Range {
        // If we're in an f-string, override the location.
        self.in_f_string
            .unwrap_or_else(|| Range::from_located(located))
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

    /// Return `true` if `member` is bound as a builtin.
    pub fn is_builtin(&self, member: &str) -> bool {
        self.current_scopes()
            .find_map(|scope| scope.values.get(member))
            .map_or(false, |index| {
                matches!(self.bindings[*index].kind, BindingKind::Builtin)
            })
    }

    /// Return `true` if a `CheckCode` is disabled by a `noqa` directive.
    pub fn is_ignored(&self, code: &CheckCode, lineno: usize) -> bool {
        // TODO(charlie): `noqa` directives are mostly enforced in `check_lines.rs`.
        // However, in rare cases, we need to check them here. For example, when
        // removing unused imports, we create a single fix that's applied to all
        // unused members on a single import. We need to pre-emptively omit any
        // members from the fix that will eventually be excluded by a `noqa`.
        // Unfortunately, we _do_ want to register a `Check` for each eventually-ignored
        // import, so that our `noqa` counts are accurate.
        if matches!(self.noqa, flags::Noqa::Disabled) {
            return false;
        }
        let noqa_lineno = self.noqa_line_for.get(&lineno).unwrap_or(&lineno);
        let line = self.locator.slice_source_code_range(&Range {
            location: Location::new(*noqa_lineno, 0),
            end_location: Location::new(noqa_lineno + 1, 0),
        });
        match noqa::extract_noqa_directive(&line) {
            Directive::None => false,
            Directive::All(..) => true,
            Directive::Codes(.., codes) => noqa::includes(code, &codes),
        }
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
            _ => {
                self.futures_allowed = false;
                if !self.seen_import_boundary
                    && !helpers::is_assignment_to_a_dunder(stmt)
                    && !operations::in_nested_block(
                        &mut self.parents.iter().rev().map(|node| node.0),
                    )
                {
                    self.seen_import_boundary = true;
                }
            }
        }

        // Pre-visit.
        match &stmt.node {
            StmtKind::Global { names } => {
                let scope_index = *self.scope_stack.last().expect("No current scope found");
                if scope_index != GLOBAL_SCOPE_INDEX {
                    // Add the binding to the current scope.
                    let scope = &mut self.scopes[scope_index];
                    let usage = Some((scope.id, Range::from_located(stmt)));
                    for name in names {
                        let index = self.bindings.len();
                        self.bindings.push(Binding {
                            kind: BindingKind::Global,
                            used: usage,
                            range: Range::from_located(stmt),
                            source: Some(RefEquality(stmt)),
                        });
                        scope.values.insert(name, index);
                    }
                }

                if self.settings.enabled.contains(&CheckCode::E741) {
                    self.add_checks(
                        names
                            .iter()
                            .filter_map(|name| {
                                pycodestyle::checks::ambiguous_variable_name(name, stmt)
                            })
                            .into_iter(),
                    );
                }
            }
            StmtKind::Nonlocal { names } => {
                let scope_index = *self.scope_stack.last().expect("No current scope found");
                if scope_index != GLOBAL_SCOPE_INDEX {
                    let scope = &mut self.scopes[scope_index];
                    let usage = Some((scope.id, Range::from_located(stmt)));
                    for name in names {
                        // Add a binding to the current scope.
                        let index = self.bindings.len();
                        self.bindings.push(Binding {
                            kind: BindingKind::Nonlocal,
                            used: usage,
                            range: Range::from_located(stmt),
                            source: Some(RefEquality(stmt)),
                        });
                        scope.values.insert(name, index);
                    }

                    // Mark the binding in the defining scopes as used too. (Skip the global scope
                    // and the current scope.)
                    for name in names {
                        let mut exists = false;
                        for index in self.scope_stack.iter().skip(1).rev().skip(1) {
                            if let Some(index) = self.scopes[*index].values.get(&name.as_str()) {
                                exists = true;
                                self.bindings[*index].used = usage;
                            }
                        }

                        // Ensure that every nonlocal has an existing binding from a parent scope.
                        if !exists {
                            if self.settings.enabled.contains(&CheckCode::PLE0117) {
                                self.add_check(Check::new(
                                    CheckKind::NonlocalWithoutBinding(name.to_string()),
                                    Range::from_located(stmt),
                                ));
                            }
                        }
                    }
                }

                if self.settings.enabled.contains(&CheckCode::E741) {
                    self.add_checks(
                        names
                            .iter()
                            .filter_map(|name| {
                                pycodestyle::checks::ambiguous_variable_name(name, stmt)
                            })
                            .into_iter(),
                    );
                }
            }
            StmtKind::Break => {
                if self.settings.enabled.contains(&CheckCode::F701) {
                    if let Some(check) = pyflakes::checks::break_outside_loop(
                        stmt,
                        &mut self.parents.iter().rev().map(|node| node.0).skip(1),
                    ) {
                        self.add_check(check);
                    }
                }
            }
            StmtKind::Continue => {
                if self.settings.enabled.contains(&CheckCode::F702) {
                    if let Some(check) = pyflakes::checks::continue_outside_loop(
                        stmt,
                        &mut self.parents.iter().rev().map(|node| node.0).skip(1),
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
                        &self.settings.pep8_naming.ignore_names,
                        self.locator,
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
                    if let Some(check) = pep8_naming::checks::dunder_function_name(
                        self.current_scope(),
                        stmt,
                        name,
                        self.locator,
                    ) {
                        self.add_check(check);
                    }
                }

                if self.settings.enabled.contains(&CheckCode::UP011)
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

                if self.settings.enabled.contains(&CheckCode::RET501)
                    || self.settings.enabled.contains(&CheckCode::RET502)
                    || self.settings.enabled.contains(&CheckCode::RET503)
                    || self.settings.enabled.contains(&CheckCode::RET504)
                    || self.settings.enabled.contains(&CheckCode::RET505)
                    || self.settings.enabled.contains(&CheckCode::RET506)
                    || self.settings.enabled.contains(&CheckCode::RET507)
                    || self.settings.enabled.contains(&CheckCode::RET508)
                {
                    flake8_return::plugins::function(self, body);
                }

                if self.settings.enabled.contains(&CheckCode::C901) {
                    if let Some(check) = mccabe::checks::function_is_too_complex(
                        stmt,
                        name,
                        body,
                        self.settings.mccabe.max_complexity,
                        self.locator,
                    ) {
                        self.add_check(check);
                    }
                }

                if self.settings.enabled.contains(&CheckCode::S107) {
                    self.add_checks(
                        flake8_bandit::plugins::hardcoded_password_default(args).into_iter(),
                    );
                }

                if self.settings.enabled.contains(&CheckCode::PLR0206) {
                    pylint::plugins::property_with_parameters(self, stmt, decorator_list, args);
                }

                self.check_builtin_shadowing(name, stmt, true);

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
                        kind: BindingKind::FunctionDefinition,
                        used: None,
                        range: Range::from_located(stmt),
                        source: Some(self.current_stmt().clone()),
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
                if self.settings.enabled.contains(&CheckCode::UP004) {
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
                    if let Some(check) =
                        pep8_naming::checks::invalid_class_name(stmt, name, self.locator)
                    {
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

                self.check_builtin_shadowing(name, stmt, false);

                for expr in bases {
                    self.visit_expr(expr);
                }
                for keyword in keywords {
                    self.visit_keyword(keyword);
                }
                for expr in decorator_list {
                    self.visit_expr(expr);
                }
            }
            StmtKind::Import { names } => {
                if self.settings.enabled.contains(&CheckCode::E401) {
                    if names.len() > 1 {
                        self.add_check(Check::new(
                            CheckKind::MultipleImportsOnOneLine,
                            Range::from_located(stmt),
                        ));
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
                                ),
                                used: None,
                                range: Range::from_located(alias),
                                source: Some(self.current_stmt().clone()),
                            },
                        );
                    } else {
                        if let Some(asname) = &alias.node.asname {
                            self.check_builtin_shadowing(asname, stmt, false);
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
                                ),
                                // Treat explicit re-export as usage (e.g., `import applications
                                // as applications`).
                                used: if alias
                                    .node
                                    .asname
                                    .as_ref()
                                    .map_or(false, |asname| asname == &alias.node.name)
                                {
                                    Some((
                                        self.scopes[*(self
                                            .scope_stack
                                            .last()
                                            .expect("No current scope found"))]
                                        .id,
                                        Range::from_located(alias),
                                    ))
                                } else {
                                    None
                                },
                                range: Range::from_located(alias),
                                source: Some(self.current_stmt().clone()),
                            },
                        );
                    }

                    // flake8-debugger
                    if self.settings.enabled.contains(&CheckCode::T100) {
                        if let Some(check) =
                            flake8_debugger::checks::debugger_import(stmt, None, &alias.node.name)
                        {
                            self.add_check(check);
                        }
                    }

                    // pylint
                    if self.settings.enabled.contains(&CheckCode::PLC0414) {
                        pylint::plugins::useless_import_alias(self, alias);
                    }
                    if self.settings.enabled.contains(&CheckCode::PLR0402) {
                        pylint::plugins::use_from_import(self, alias);
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
                                    stmt,
                                    name,
                                    asname,
                                    self.locator,
                                )
                            {
                                self.add_check(check);
                            }
                        }

                        if self.settings.enabled.contains(&CheckCode::N812) {
                            if let Some(check) =
                                pep8_naming::checks::lowercase_imported_as_non_lowercase(
                                    stmt,
                                    name,
                                    asname,
                                    self.locator,
                                )
                            {
                                self.add_check(check);
                            }
                        }

                        if self.settings.enabled.contains(&CheckCode::N813) {
                            if let Some(check) =
                                pep8_naming::checks::camelcase_imported_as_lowercase(
                                    stmt,
                                    name,
                                    asname,
                                    self.locator,
                                )
                            {
                                self.add_check(check);
                            }
                        }

                        if self.settings.enabled.contains(&CheckCode::N814) {
                            if let Some(check) = pep8_naming::checks::camelcase_imported_as_constant(
                                stmt,
                                name,
                                asname,
                                self.locator,
                            ) {
                                self.add_check(check);
                            }
                        }

                        if self.settings.enabled.contains(&CheckCode::N817) {
                            if let Some(check) = pep8_naming::checks::camelcase_imported_as_acronym(
                                stmt,
                                name,
                                asname,
                                self.locator,
                            ) {
                                self.add_check(check);
                            }
                        }
                    }

                    if self.settings.enabled.contains(&CheckCode::ICN001) {
                        if let Some(check) =
                            flake8_import_conventions::checks::check_conventional_import(
                                stmt,
                                &alias.node.name,
                                alias.node.asname.as_deref(),
                                &self.settings.flake8_import_conventions.aliases,
                            )
                        {
                            self.add_check(check);
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
                            .extend(names.iter().map(|alias| alias.node.name.as_str()));
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
                    if self.settings.enabled.contains(&CheckCode::UP010) {
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
                                        .expect("No current scope found"))]
                                    .id,
                                    Range::from_located(alias),
                                )),
                                range: Range::from_located(alias),
                                source: Some(self.current_stmt().clone()),
                            },
                        );

                        if alias.node.name == "annotations" {
                            self.annotations_future_enabled = true;
                        }

                        if self.settings.enabled.contains(&CheckCode::F407) {
                            if !ALL_FEATURE_NAMES.contains(&&*alias.node.name) {
                                self.add_check(Check::new(
                                    CheckKind::FutureFeatureNotDefined(alias.node.name.to_string()),
                                    Range::from_located(alias),
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
                                source: Some(self.current_stmt().clone()),
                            },
                        );

                        if self.settings.enabled.contains(&CheckCode::F406) {
                            let scope = &self.scopes
                                [*(self.scope_stack.last().expect("No current scope found"))];
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
                            [*(self.scope_stack.last().expect("No current scope found"))];
                        scope.import_starred = true;
                    } else {
                        if let Some(asname) = &alias.node.asname {
                            self.check_builtin_shadowing(asname, stmt, false);
                        }

                        // Given `from foo import bar`, `name` would be "bar" and `full_name` would
                        // be "foo.bar". Given `from foo import bar as baz`, `name` would be "baz"
                        // and `full_name` would be "foo.bar".
                        let name = alias.node.asname.as_ref().unwrap_or(&alias.node.name);
                        let full_name = match module {
                            None => alias.node.name.to_string(),
                            Some(parent) => format!("{parent}.{}", alias.node.name),
                        };
                        let range = Range::from_located(alias);
                        self.add_binding(
                            name,
                            Binding {
                                kind: BindingKind::FromImportation(name.to_string(), full_name),
                                // Treat explicit re-export as usage (e.g., `from .applications
                                // import FastAPI as FastAPI`).
                                used: if alias
                                    .node
                                    .asname
                                    .as_ref()
                                    .map_or(false, |asname| asname == &alias.node.name)
                                {
                                    Some((
                                        self.scopes[*(self
                                            .scope_stack
                                            .last()
                                            .expect("No current scope found"))]
                                        .id,
                                        range,
                                    ))
                                } else {
                                    None
                                },
                                range,
                                source: Some(self.current_stmt().clone()),
                            },
                        );
                    }

                    if self.settings.enabled.contains(&CheckCode::TID252) {
                        if let Some(check) = flake8_tidy_imports::checks::banned_relative_import(
                            stmt,
                            level.as_ref(),
                            &self.settings.flake8_tidy_imports.ban_relative_imports,
                        ) {
                            self.add_check(check);
                        }
                    }

                    // flake8-debugger
                    if self.settings.enabled.contains(&CheckCode::T100) {
                        if let Some(check) = flake8_debugger::checks::debugger_import(
                            stmt,
                            module.as_ref().map(String::as_str),
                            &alias.node.name,
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
                                    self.locator,
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
                                    self.locator,
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
                                    self.locator,
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
                                self.locator,
                            ) {
                                self.add_check(check);
                            }
                        }

                        if self.settings.enabled.contains(&CheckCode::N817) {
                            if let Some(check) = pep8_naming::checks::camelcase_imported_as_acronym(
                                stmt,
                                &alias.node.name,
                                asname,
                                self.locator,
                            ) {
                                self.add_check(check);
                            }
                        }

                        // pylint
                        if self.settings.enabled.contains(&CheckCode::PLC0414) {
                            pylint::plugins::useless_import_alias(self, alias);
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
                if self.settings.enabled.contains(&CheckCode::EM101)
                    | self.settings.enabled.contains(&CheckCode::EM102)
                    | self.settings.enabled.contains(&CheckCode::EM103)
                {
                    if let Some(exc) = exc {
                        self.add_checks(
                            flake8_errmsg::checks::check_string_in_exception(
                                exc,
                                self.settings.flake8_errmsg.max_string_length,
                            )
                            .into_iter(),
                        );
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
                        msg.as_ref().map(|expr| &**expr),
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
            StmtKind::While { body, orelse, .. } => {
                if self.settings.enabled.contains(&CheckCode::B023) {
                    flake8_bugbear::plugins::function_uses_loop_variable(self, &Node::Stmt(stmt));
                }
                if self.settings.enabled.contains(&CheckCode::PLW0120) {
                    pylint::plugins::useless_else_on_loop(self, stmt, body, orelse);
                }
            }
            StmtKind::For {
                target,
                body,
                iter,
                orelse,
                ..
            }
            | StmtKind::AsyncFor {
                target,
                body,
                iter,
                orelse,
                ..
            } => {
                if self.settings.enabled.contains(&CheckCode::B007) {
                    flake8_bugbear::plugins::unused_loop_control_variable(self, target, body);
                }
                if self.settings.enabled.contains(&CheckCode::B020) {
                    flake8_bugbear::plugins::loop_variable_overrides_iterator(self, target, iter);
                }
                if self.settings.enabled.contains(&CheckCode::B023) {
                    flake8_bugbear::plugins::function_uses_loop_variable(self, &Node::Stmt(stmt));
                }
                if self.settings.enabled.contains(&CheckCode::PLW0120) {
                    pylint::plugins::useless_else_on_loop(self, stmt, body, orelse);
                }
                if self.settings.enabled.contains(&CheckCode::SIM118) {
                    flake8_simplify::plugins::key_in_dict_for(self, target, iter);
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
            }
            StmtKind::Assign { targets, value, .. } => {
                if self.settings.enabled.contains(&CheckCode::E731) {
                    if let [target] = &targets[..] {
                        pycodestyle::plugins::do_not_assign_lambda(self, target, value, stmt);
                    }
                }
                if self.settings.enabled.contains(&CheckCode::UP001) {
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
                if self.settings.enabled.contains(&CheckCode::UP013) {
                    pyupgrade::plugins::convert_typed_dict_functional_to_class(
                        self, stmt, targets, value,
                    );
                }
                if self.settings.enabled.contains(&CheckCode::UP014) {
                    pyupgrade::plugins::convert_named_tuple_functional_to_class(
                        self, stmt, targets, value,
                    );
                }
                if self.settings.enabled.contains(&CheckCode::PD901) {
                    if let Some(check) = pandas_vet::checks::assignment_to_df(targets) {
                        self.add_check(check);
                    }
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
                    flake8_bugbear::plugins::useless_comparison(self, value);
                }
            }
            _ => {}
        }

        // Recurse.
        let prev_visible_scope = self.visible_scope.clone();
        match &stmt.node {
            StmtKind::FunctionDef {
                body,
                name,
                args,
                decorator_list,
                ..
            }
            | StmtKind::AsyncFunctionDef {
                body,
                name,
                args,
                decorator_list,
                ..
            } => {
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

                // If any global bindings don't already exist in the global scope, add it.
                let globals = operations::extract_globals(body);
                for (name, stmt) in operations::extract_globals(body) {
                    if self.scopes[GLOBAL_SCOPE_INDEX]
                        .values
                        .get(name)
                        .map_or(true, |index| {
                            matches!(self.bindings[*index].kind, BindingKind::Annotation)
                        })
                    {
                        let index = self.bindings.len();
                        self.bindings.push(Binding {
                            kind: BindingKind::Assignment,
                            used: None,
                            range: Range::from_located(stmt),
                            source: Some(RefEquality(stmt)),
                        });
                        self.scopes[GLOBAL_SCOPE_INDEX].values.insert(name, index);
                    }
                }

                self.push_scope(Scope::new(ScopeKind::Function(FunctionDef {
                    name,
                    body,
                    args,
                    decorator_list,
                    async_: matches!(stmt.node, StmtKind::AsyncFunctionDef { .. }),
                    globals,
                })));

                self.deferred_functions.push((
                    stmt,
                    (self.scope_stack.clone(), self.parents.clone()),
                    self.visible_scope.clone(),
                ));
            }
            StmtKind::ClassDef {
                body,
                name,
                bases,
                keywords,
                decorator_list,
                ..
            } => {
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

                // If any global bindings don't already exist in the global scope, add it.
                let globals = operations::extract_globals(body);
                for (name, stmt) in &globals {
                    if self.scopes[GLOBAL_SCOPE_INDEX]
                        .values
                        .get(name)
                        .map_or(true, |index| {
                            matches!(self.bindings[*index].kind, BindingKind::Annotation)
                        })
                    {
                        let index = self.bindings.len();
                        self.bindings.push(Binding {
                            kind: BindingKind::Assignment,
                            used: None,
                            range: Range::from_located(stmt),
                            source: Some(RefEquality(stmt)),
                        });
                        self.scopes[GLOBAL_SCOPE_INDEX].values.insert(name, index);
                    }
                }

                self.push_scope(Scope::new(ScopeKind::Class(ClassDef {
                    name,
                    bases,
                    keywords,
                    decorator_list,
                    globals,
                })));

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
                    self.visit_excepthandler(excepthandler);
                }
                for stmt in orelse {
                    self.visit_stmt(stmt);
                }
                for stmt in finalbody {
                    self.visit_stmt(stmt);
                }
            }
            StmtKind::AnnAssign {
                target,
                annotation,
                value,
                ..
            } => {
                self.visit_annotation(annotation);
                if let Some(expr) = value {
                    if self.match_typing_expr(annotation, "TypeAlias") {
                        self.in_type_definition = true;
                        self.visit_expr(expr);
                        self.in_type_definition = false;
                    } else {
                        self.visit_expr(expr);
                    }
                }
                self.visit_expr(target);
            }
            _ => visitor::walk_stmt(self, stmt),
        };
        self.visible_scope = prev_visible_scope;

        // Post-visit.
        match &stmt.node {
            StmtKind::FunctionDef { .. } | StmtKind::AsyncFunctionDef { .. } => {
                self.pop_scope();
            }
            StmtKind::ClassDef { name, .. } => {
                self.pop_scope();
                self.add_binding(
                    name,
                    Binding {
                        kind: BindingKind::ClassDefinition,
                        used: None,
                        range: Range::from_located(stmt),
                        source: Some(self.current_stmt().clone()),
                    },
                );
            }
            _ => {}
        }

        self.pop_parent();
    }

    fn visit_annotation(&mut self, expr: &'b Expr) {
        let prev_in_annotation = self.in_annotation;
        let prev_in_type_definition = self.in_type_definition;
        self.in_annotation = true;
        self.in_type_definition = true;
        self.visit_expr(expr);
        self.in_annotation = prev_in_annotation;
        self.in_type_definition = prev_in_type_definition;
    }

    fn visit_expr(&mut self, expr: &'b Expr) {
        if !(self.in_deferred_type_definition || self.in_deferred_string_type_definition)
            && self.in_type_definition
            && self.annotations_future_enabled
        {
            if let ExprKind::Constant {
                value: Constant::Str(value),
                ..
            } = &expr.node
            {
                self.deferred_string_type_definitions.push((
                    Range::from_located(expr),
                    value,
                    self.in_annotation,
                    (self.scope_stack.clone(), self.parents.clone()),
                ));
            } else {
                self.deferred_type_definitions.push((
                    expr,
                    self.in_annotation,
                    (self.scope_stack.clone(), self.parents.clone()),
                ));
            }
            return;
        }

        self.push_expr(expr);

        let prev_in_f_string = self.in_f_string;
        let prev_in_literal = self.in_literal;
        let prev_in_type_definition = self.in_type_definition;

        // Pre-visit.
        match &expr.node {
            ExprKind::Subscript { value, slice, .. } => {
                // Ex) Optional[...]
                if !self.in_deferred_string_type_definition
                    && self.settings.enabled.contains(&CheckCode::UP007)
                    && (self.settings.target_version >= PythonVersion::Py310
                        || (self.settings.target_version >= PythonVersion::Py37
                            && !self.settings.pyupgrade.keep_runtime_typing
                            && self.annotations_future_enabled
                            && self.in_annotation))
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
                        if !self.in_deferred_string_type_definition
                            && self.settings.enabled.contains(&CheckCode::UP006)
                            && (self.settings.target_version >= PythonVersion::Py39
                                || (self.settings.target_version >= PythonVersion::Py37
                                    && !self.settings.pyupgrade.keep_runtime_typing
                                    && self.annotations_future_enabled
                                    && self.in_annotation))
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
                            if let Some(check) =
                                pycodestyle::checks::ambiguous_variable_name(id, expr)
                            {
                                self.add_check(check);
                            }
                        }

                        self.check_builtin_shadowing(id, expr, true);

                        self.handle_node_store(id, expr);
                    }
                    ExprContext::Del => self.handle_node_delete(expr),
                }

                if self.settings.enabled.contains(&CheckCode::YTT202) {
                    flake8_2020::plugins::name_or_attribute(self, expr);
                }

                if self.settings.enabled.contains(&CheckCode::PLE0118) {
                    pylint::plugins::used_prior_global_declaration(self, id, expr);
                }
            }
            ExprKind::Attribute { attr, .. } => {
                // Ex) typing.List[...]
                if !self.in_deferred_string_type_definition
                    && self.settings.enabled.contains(&CheckCode::UP006)
                    && (self.settings.target_version >= PythonVersion::Py39
                        || (self.settings.target_version >= PythonVersion::Py37
                            && self.annotations_future_enabled
                            && self.in_annotation))
                    && typing::is_pep585_builtin(expr, &self.from_imports, &self.import_aliases)
                {
                    pyupgrade::plugins::use_pep585_annotation(self, expr, attr);
                }

                if self.settings.enabled.contains(&CheckCode::UP016) {
                    pyupgrade::plugins::remove_six_compat(self, expr);
                }

                if self.settings.enabled.contains(&CheckCode::YTT202) {
                    flake8_2020::plugins::name_or_attribute(self, expr);
                }

                for (code, name) in vec![
                    (CheckCode::PD007, "ix"),
                    (CheckCode::PD008, "at"),
                    (CheckCode::PD009, "iat"),
                    (CheckCode::PD011, "values"),
                ] {
                    if self.settings.enabled.contains(&code) {
                        if attr == name {
                            self.add_check(Check::new(code.kind(), Range::from_located(expr)));
                        };
                    }
                }
            }
            ExprKind::Call {
                func,
                args,
                keywords,
            } => {
                // pyflakes
                if self.settings.enabled.contains(&CheckCode::F521)
                    || self.settings.enabled.contains(&CheckCode::F522)
                    || self.settings.enabled.contains(&CheckCode::F523)
                    || self.settings.enabled.contains(&CheckCode::F524)
                    || self.settings.enabled.contains(&CheckCode::F525)
                {
                    if let ExprKind::Attribute { value, attr, .. } = &func.node {
                        if let ExprKind::Constant {
                            value: Constant::Str(value),
                            ..
                        } = &value.node
                        {
                            if attr == "format" {
                                // "...".format(...) call
                                let location = Range::from_located(expr);
                                match pyflakes::format::FormatSummary::try_from(value.as_ref()) {
                                    Err(e) => {
                                        if self.settings.enabled.contains(&CheckCode::F521) {
                                            self.add_check(Check::new(
                                                CheckKind::StringDotFormatInvalidFormat(
                                                    e.to_string(),
                                                ),
                                                location,
                                            ));
                                        }
                                    }
                                    Ok(summary) => {
                                        if self.settings.enabled.contains(&CheckCode::F522) {
                                            pyflakes::plugins::string_dot_format_extra_named_arguments(self,
                                                &summary, keywords, location,
                                            );
                                        }

                                        if self.settings.enabled.contains(&CheckCode::F523) {
                                            pyflakes::plugins::string_dot_format_extra_positional_arguments(
                                                self,
                                                &summary, args, location,
                                            );
                                        }

                                        if self.settings.enabled.contains(&CheckCode::F524) {
                                            pyflakes::plugins::string_dot_format_missing_argument(
                                                self, &summary, args, keywords, location,
                                            );
                                        }

                                        if self.settings.enabled.contains(&CheckCode::F525) {
                                            pyflakes::plugins::string_dot_format_mixing_automatic(
                                                self, &summary, location,
                                            );
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                // pyupgrade
                if self.settings.enabled.contains(&CheckCode::UP005) {
                    pyupgrade::plugins::deprecated_unittest_alias(self, func);
                }
                if self.settings.enabled.contains(&CheckCode::UP012) {
                    pyupgrade::plugins::unnecessary_encode_utf8(self, expr, func, args, keywords);
                }
                if self.settings.enabled.contains(&CheckCode::UP016) {
                    pyupgrade::plugins::remove_six_compat(self, expr);
                }

                // flake8-super
                if self.settings.enabled.contains(&CheckCode::UP008) {
                    pyupgrade::plugins::super_call_with_parameters(self, expr, func, args);
                }

                // flake8-print
                if self.settings.enabled.contains(&CheckCode::T201)
                    || self.settings.enabled.contains(&CheckCode::T203)
                {
                    flake8_print::plugins::print_call(self, expr, func, keywords);
                }

                // flake8-bugbear
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
                    flake8_bugbear::plugins::setattr_with_constant(self, expr, func, args);
                }
                if self.settings.enabled.contains(&CheckCode::B022) {
                    flake8_bugbear::plugins::useless_contextlib_suppress(self, expr, args);
                }
                if self.settings.enabled.contains(&CheckCode::B026) {
                    flake8_bugbear::plugins::star_arg_unpacking_after_keyword_arg(
                        self, args, keywords,
                    );
                }
                if self.settings.enabled.contains(&CheckCode::B905)
                    && self.settings.target_version >= PythonVersion::Py310
                {
                    flake8_bugbear::plugins::zip_without_explicit_strict(
                        self, expr, func, keywords,
                    );
                }

                // flake8-bandit
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
                if self.settings.enabled.contains(&CheckCode::UP003) {
                    pyupgrade::plugins::type_of_primitive(self, expr, func, args);
                }

                if self.settings.enabled.contains(&CheckCode::UP015) {
                    pyupgrade::plugins::redundant_open_modes(self, expr);
                }

                // flake8-boolean-trap
                if self.settings.enabled.contains(&CheckCode::FBT003) {
                    flake8_boolean_trap::plugins::check_boolean_positional_value_in_function_call(
                        self, args, func,
                    );
                }
                if let ExprKind::Name { id, ctx } = &func.node {
                    if id == "locals" && matches!(ctx, ExprContext::Load) {
                        let scope = &mut self.scopes
                            [*(self.scope_stack.last().expect("No current scope found"))];
                        scope.uses_locals = true;
                    }
                }

                // flake8-debugger
                if self.settings.enabled.contains(&CheckCode::T100) {
                    if let Some(check) = flake8_debugger::checks::debugger_call(
                        expr,
                        func,
                        &self.from_imports,
                        &self.import_aliases,
                    ) {
                        self.add_check(check);
                    }
                }

                // pandas-vet
                if self.settings.enabled.contains(&CheckCode::PD002) {
                    self.add_checks(pandas_vet::checks::inplace_argument(keywords).into_iter());
                }

                for (code, name) in vec![
                    (CheckCode::PD003, "isnull"),
                    (CheckCode::PD004, "notnull"),
                    (CheckCode::PD010, "pivot"),
                    (CheckCode::PD010, "unstack"),
                    (CheckCode::PD012, "read_table"),
                    (CheckCode::PD013, "stack"),
                ] {
                    if self.settings.enabled.contains(&code) {
                        if let ExprKind::Attribute { attr, .. } = &func.node {
                            if attr == name {
                                self.add_check(Check::new(code.kind(), Range::from_located(func)));
                            };
                        }
                    }
                }

                if self.settings.enabled.contains(&CheckCode::PD015) {
                    if let Some(check) = pandas_vet::checks::use_of_pd_merge(func) {
                        self.add_check(check);
                    };
                }

                // flake8-datetimez
                if self.settings.enabled.contains(&CheckCode::DTZ001) {
                    flake8_datetimez::plugins::call_datetime_without_tzinfo(
                        self,
                        func,
                        args,
                        keywords,
                        Range::from_located(expr),
                    );
                }
                if self.settings.enabled.contains(&CheckCode::DTZ002) {
                    flake8_datetimez::plugins::call_datetime_today(
                        self,
                        func,
                        Range::from_located(expr),
                    );
                }
                if self.settings.enabled.contains(&CheckCode::DTZ003) {
                    flake8_datetimez::plugins::call_datetime_utcnow(
                        self,
                        func,
                        Range::from_located(expr),
                    );
                }
                if self.settings.enabled.contains(&CheckCode::DTZ004) {
                    flake8_datetimez::plugins::call_datetime_utcfromtimestamp(
                        self,
                        func,
                        Range::from_located(expr),
                    );
                }
                if self.settings.enabled.contains(&CheckCode::DTZ005) {
                    flake8_datetimez::plugins::call_datetime_now_without_tzinfo(
                        self,
                        func,
                        args,
                        keywords,
                        Range::from_located(expr),
                    );
                }
                if self.settings.enabled.contains(&CheckCode::DTZ006) {
                    flake8_datetimez::plugins::call_datetime_fromtimestamp(
                        self,
                        func,
                        args,
                        keywords,
                        Range::from_located(expr),
                    );
                }
                if self.settings.enabled.contains(&CheckCode::DTZ007) {
                    flake8_datetimez::plugins::call_datetime_strptime_without_zone(
                        self,
                        func,
                        args,
                        Range::from_located(expr),
                    );
                }
                if self.settings.enabled.contains(&CheckCode::DTZ011) {
                    flake8_datetimez::plugins::call_date_today(
                        self,
                        func,
                        Range::from_located(expr),
                    );
                }
                if self.settings.enabled.contains(&CheckCode::DTZ012) {
                    flake8_datetimez::plugins::call_date_fromtimestamp(
                        self,
                        func,
                        Range::from_located(expr),
                    );
                }

                // pygrep-hooks
                if self.settings.enabled.contains(&CheckCode::PGH001) {
                    pygrep_hooks::plugins::no_eval(self, func);
                }
                if self.settings.enabled.contains(&CheckCode::PGH002) {
                    pygrep_hooks::plugins::deprecated_log_warn(self, func);
                }

                // pylint
                if self.settings.enabled.contains(&CheckCode::PLC3002) {
                    pylint::plugins::unnecessary_direct_lambda_call(self, expr, func);
                }
                if self.settings.enabled.contains(&CheckCode::PLR1722) {
                    pylint::plugins::use_sys_exit(self, func);
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
            ExprKind::Yield { .. } => {
                if self.settings.enabled.contains(&CheckCode::F704) {
                    let scope = self.current_scope();
                    if matches!(scope.kind, ScopeKind::Class(_) | ScopeKind::Module) {
                        self.add_check(Check::new(
                            CheckKind::YieldOutsideFunction(DeferralKeyword::Yield),
                            Range::from_located(expr),
                        ));
                    }
                }
            }
            ExprKind::YieldFrom { .. } => {
                if self.settings.enabled.contains(&CheckCode::F704) {
                    let scope = self.current_scope();
                    if matches!(scope.kind, ScopeKind::Class(_) | ScopeKind::Module) {
                        self.add_check(Check::new(
                            CheckKind::YieldOutsideFunction(DeferralKeyword::YieldFrom),
                            Range::from_located(expr),
                        ));
                    }
                }
            }
            ExprKind::Await { .. } => {
                if self.settings.enabled.contains(&CheckCode::F704) {
                    let scope = self.current_scope();
                    if matches!(scope.kind, ScopeKind::Class(_) | ScopeKind::Module) {
                        self.add_check(Check::new(
                            CheckKind::YieldOutsideFunction(DeferralKeyword::Await),
                            Range::from_located(expr),
                        ));
                    }
                }
                if self.settings.enabled.contains(&CheckCode::PLE1142) {
                    pylint::plugins::await_outside_async(self, expr);
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
            ExprKind::BinOp {
                left,
                op: Operator::Mod,
                right,
            } => {
                if let ExprKind::Constant {
                    value: Constant::Str(value),
                    ..
                } = &left.node
                {
                    if self.settings.enabled.contains(&CheckCode::F501)
                        || self.settings.enabled.contains(&CheckCode::F502)
                        || self.settings.enabled.contains(&CheckCode::F503)
                        || self.settings.enabled.contains(&CheckCode::F504)
                        || self.settings.enabled.contains(&CheckCode::F505)
                        || self.settings.enabled.contains(&CheckCode::F506)
                        || self.settings.enabled.contains(&CheckCode::F507)
                        || self.settings.enabled.contains(&CheckCode::F508)
                        || self.settings.enabled.contains(&CheckCode::F509)
                    {
                        let location = Range::from_located(expr);
                        match pyflakes::cformat::CFormatSummary::try_from(value.as_ref()) {
                            Err(CFormatError {
                                typ: CFormatErrorType::UnsupportedFormatChar(c),
                                ..
                            }) => {
                                if self.settings.enabled.contains(&CheckCode::F509) {
                                    self.add_check(Check::new(
                                        CheckKind::PercentFormatUnsupportedFormatCharacter(c),
                                        location,
                                    ));
                                }
                            }
                            Err(e) => {
                                if self.settings.enabled.contains(&CheckCode::F501) {
                                    self.add_check(Check::new(
                                        CheckKind::PercentFormatInvalidFormat(e.to_string()),
                                        location,
                                    ));
                                }
                            }
                            Ok(summary) => {
                                if self.settings.enabled.contains(&CheckCode::F502) {
                                    pyflakes::plugins::percent_format_expected_mapping(
                                        self, &summary, right, location,
                                    );
                                }
                                if self.settings.enabled.contains(&CheckCode::F503) {
                                    pyflakes::plugins::percent_format_expected_sequence(
                                        self, &summary, right, location,
                                    );
                                }
                                if self.settings.enabled.contains(&CheckCode::F504) {
                                    pyflakes::plugins::percent_format_extra_named_arguments(
                                        self, &summary, right, location,
                                    );
                                }
                                if self.settings.enabled.contains(&CheckCode::F505) {
                                    pyflakes::plugins::percent_format_missing_arguments(
                                        self, &summary, right, location,
                                    );
                                }
                                if self.settings.enabled.contains(&CheckCode::F506) {
                                    pyflakes::plugins::percent_format_mixed_positional_and_named(
                                        self, &summary, location,
                                    );
                                }
                                if self.settings.enabled.contains(&CheckCode::F507) {
                                    pyflakes::plugins::percent_format_positional_count_mismatch(
                                        self, &summary, right, location,
                                    );
                                }
                                if self.settings.enabled.contains(&CheckCode::F508) {
                                    pyflakes::plugins::percent_format_star_requires_sequence(
                                        self, &summary, right, location,
                                    );
                                }
                            }
                        }
                    }
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
                    );
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

                if self.settings.enabled.contains(&CheckCode::PLC2201) {
                    pylint::plugins::misplaced_comparison_constant(
                        self,
                        expr,
                        left,
                        ops,
                        comparators,
                    );
                }

                if self.settings.enabled.contains(&CheckCode::SIM118) {
                    flake8_simplify::plugins::key_in_dict_compare(
                        self,
                        expr,
                        left,
                        ops,
                        comparators,
                    );
                }
            }
            ExprKind::Constant {
                value: Constant::Str(value),
                ..
            } => {
                if self.in_type_definition && !self.in_literal {
                    self.deferred_string_type_definitions.push((
                        Range::from_located(expr),
                        value,
                        self.in_annotation,
                        (self.scope_stack.clone(), self.parents.clone()),
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
            ExprKind::Lambda { args, body, .. } => {
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
                self.push_scope(Scope::new(ScopeKind::Lambda(Lambda { args, body })));
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
                if self.settings.enabled.contains(&CheckCode::B023) {
                    flake8_bugbear::plugins::function_uses_loop_variable(self, &Node::Expr(expr));
                }
                self.push_scope(Scope::new(ScopeKind::Generator));
            }
            ExprKind::GeneratorExp { .. } | ExprKind::DictComp { .. } => {
                if self.settings.enabled.contains(&CheckCode::B023) {
                    flake8_bugbear::plugins::function_uses_loop_variable(self, &Node::Expr(expr));
                }
                self.push_scope(Scope::new(ScopeKind::Generator));
            }
            ExprKind::BoolOp { op, values } => {
                if self.settings.enabled.contains(&CheckCode::PLR1701) {
                    pylint::plugins::merge_isinstance(self, expr, op, values);
                }
            }
            _ => {}
        };

        // Recurse.
        match &expr.node {
            ExprKind::Lambda { .. } => {
                self.deferred_lambdas
                    .push((expr, (self.scope_stack.clone(), self.parents.clone())));
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
                        self.in_type_definition = true;
                        self.visit_expr(expr);
                        self.in_type_definition = prev_in_type_definition;
                    }
                } else if self.match_typing_call_path(&call_path, "cast") {
                    self.visit_expr(func);
                    if !args.is_empty() {
                        self.in_type_definition = true;
                        self.visit_expr(&args[0]);
                        self.in_type_definition = prev_in_type_definition;
                    }
                    for expr in args.iter().skip(1) {
                        self.visit_expr(expr);
                    }
                } else if self.match_typing_call_path(&call_path, "NewType") {
                    self.visit_expr(func);
                    for expr in args.iter().skip(1) {
                        self.in_type_definition = true;
                        self.visit_expr(expr);
                        self.in_type_definition = prev_in_type_definition;
                    }
                } else if self.match_typing_call_path(&call_path, "TypeVar") {
                    self.visit_expr(func);
                    for expr in args.iter().skip(1) {
                        self.in_type_definition = true;
                        self.visit_expr(expr);
                        self.in_type_definition = prev_in_type_definition;
                    }
                    for keyword in keywords {
                        let KeywordData { arg, value } = &keyword.node;
                        if let Some(id) = arg {
                            if id == "bound" {
                                self.in_type_definition = true;
                                self.visit_expr(value);
                                self.in_type_definition = prev_in_type_definition;
                            } else {
                                self.in_type_definition = false;
                                self.visit_expr(value);
                                self.in_type_definition = prev_in_type_definition;
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
                                                self.in_type_definition = false;
                                                self.visit_expr(&elts[0]);
                                                self.in_type_definition = prev_in_type_definition;

                                                self.in_type_definition = true;
                                                self.visit_expr(&elts[1]);
                                                self.in_type_definition = prev_in_type_definition;
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
                        self.in_type_definition = true;
                        self.visit_expr(value);
                        self.in_type_definition = prev_in_type_definition;
                    }
                } else if self.match_typing_call_path(&call_path, "TypedDict") {
                    self.visit_expr(func);

                    // Ex) TypedDict("a", {"a": int})
                    if args.len() > 1 {
                        if let ExprKind::Dict { keys, values } = &args[1].node {
                            for key in keys {
                                self.in_type_definition = false;
                                self.visit_expr(key);
                                self.in_type_definition = prev_in_type_definition;
                            }
                            for value in values {
                                self.in_type_definition = true;
                                self.visit_expr(value);
                                self.in_type_definition = prev_in_type_definition;
                            }
                        }
                    }

                    // Ex) TypedDict("a", a=int)
                    for keyword in keywords {
                        let KeywordData { value, .. } = &keyword.node;
                        self.in_type_definition = true;
                        self.visit_expr(value);
                        self.in_type_definition = prev_in_type_definition;
                    }
                } else if ["Arg", "DefaultArg", "NamedArg", "DefaultNamedArg"]
                    .iter()
                    .any(|target| {
                        match_call_path(&call_path, "mypy_extensions", target, &self.from_imports)
                    })
                {
                    self.visit_expr(func);

                    // Ex) DefaultNamedArg(bool | None, name="some_prop_name")
                    let mut arguments = args.iter().chain(keywords.iter().map(|keyword| {
                        let KeywordData { value, .. } = &keyword.node;
                        value
                    }));
                    if let Some(expr) = arguments.next() {
                        self.in_type_definition = true;
                        self.visit_expr(expr);
                        self.in_type_definition = prev_in_type_definition;
                    }
                    for expr in arguments {
                        self.in_type_definition = false;
                        self.visit_expr(expr);
                        self.in_type_definition = prev_in_type_definition;
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
                    match typing::match_annotated_subscript(
                        value,
                        &self.from_imports,
                        &self.import_aliases,
                        |member| self.is_builtin(member),
                    ) {
                        Some(subscript) => {
                            match subscript {
                                // Ex) Optional[int]
                                SubscriptKind::AnnotatedSubscript => {
                                    self.visit_expr(value);
                                    self.in_type_definition = true;
                                    self.visit_expr(slice);
                                    self.in_type_definition = prev_in_type_definition;
                                    self.visit_expr_context(ctx);
                                }
                                // Ex) Annotated[int, "Hello, world!"]
                                SubscriptKind::PEP593AnnotatedSubscript => {
                                    // First argument is a type (including forward references); the
                                    // rest are arbitrary Python objects.
                                    self.visit_expr(value);
                                    if let ExprKind::Tuple { elts, ctx } = &slice.node {
                                        if let Some(expr) = elts.first() {
                                            self.visit_expr(expr);
                                            self.in_type_definition = false;
                                            for expr in elts.iter().skip(1) {
                                                self.visit_expr(expr);
                                            }
                                            self.in_type_definition = prev_in_type_definition;
                                            self.visit_expr_context(ctx);
                                        }
                                    } else {
                                        error!(
                                            "Found non-ExprKind::Tuple argument to PEP 593 \
                                             Annotation."
                                        );
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

        self.in_type_definition = prev_in_type_definition;
        self.in_literal = prev_in_literal;
        self.in_f_string = prev_in_f_string;

        self.pop_expr();
    }

    fn visit_excepthandler(&mut self, excepthandler: &'b Excepthandler) {
        match &excepthandler.node {
            ExcepthandlerKind::ExceptHandler {
                type_, name, body, ..
            } => {
                if self.settings.enabled.contains(&CheckCode::E722) {
                    if let Some(check) = pycodestyle::checks::do_not_use_bare_except(
                        type_.as_deref(),
                        body,
                        Range::from_located(excepthandler),
                    ) {
                        self.add_check(check);
                    }
                }
                if self.settings.enabled.contains(&CheckCode::B904) {
                    flake8_bugbear::plugins::raise_without_from_inside_except(self, body);
                }
                if self.settings.enabled.contains(&CheckCode::BLE001) {
                    flake8_blind_except::plugins::blind_except(
                        self,
                        type_.as_deref(),
                        name.as_ref().map(String::as_str),
                        body,
                    );
                }
                match name {
                    Some(name) => {
                        if self.settings.enabled.contains(&CheckCode::E741) {
                            if let Some(check) =
                                pycodestyle::checks::ambiguous_variable_name(name, excepthandler)
                            {
                                self.add_check(check);
                            }
                        }

                        self.check_builtin_shadowing(name, excepthandler, false);

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
                            );
                        }

                        let definition = self.current_scope().values.get(&name.as_str()).copied();
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
                        );

                        walk_excepthandler(self, excepthandler);

                        if let Some(index) = {
                            let scope = &mut self.scopes
                                [*(self.scope_stack.last().expect("No current scope found"))];
                            &scope.values.remove(&name.as_str())
                        } {
                            if self.bindings[*index].used.is_none() {
                                if self.settings.enabled.contains(&CheckCode::F841) {
                                    self.add_check(Check::new(
                                        CheckKind::UnusedVariable(name.to_string()),
                                        Range::from_located(excepthandler),
                                    ));
                                }
                            }
                        }

                        if let Some(index) = definition {
                            let scope = &mut self.scopes
                                [*(self.scope_stack.last().expect("No current scope found"))];
                            scope.values.insert(name, index);
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
            flake8_bugbear::plugins::mutable_argument_default(self, arguments);
        }
        if self.settings.enabled.contains(&CheckCode::B008) {
            flake8_bugbear::plugins::function_call_argument_default(self, arguments);
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
                source: Some(self.current_stmt().clone()),
            },
        );

        if self.settings.enabled.contains(&CheckCode::E741) {
            if let Some(check) = pycodestyle::checks::ambiguous_variable_name(&arg.node.arg, arg) {
                self.add_check(check);
            }
        }

        if self.settings.enabled.contains(&CheckCode::N803) {
            if let Some(check) = pep8_naming::checks::invalid_argument_name(&arg.node.arg, arg) {
                self.add_check(check);
            }
        }

        self.check_builtin_arg_shadowing(&arg.node.arg, arg);
    }
}

impl<'a> Checker<'a> {
    fn push_parent(&mut self, parent: &'a Stmt) {
        let num_existing = self.parents.len();
        self.parents.push(RefEquality(parent));
        self.depths
            .insert(self.parents[num_existing].clone(), num_existing);
        if num_existing > 0 {
            self.child_to_parent.insert(
                self.parents[num_existing].clone(),
                self.parents[num_existing - 1].clone(),
            );
        }
    }

    fn pop_parent(&mut self) {
        self.parents.pop().expect("Attempted to pop without parent");
    }

    fn push_expr(&mut self, expr: &'a Expr) {
        self.exprs.push(RefEquality(expr));
    }

    fn pop_expr(&mut self) {
        self.exprs
            .pop()
            .expect("Attempted to pop without expression");
    }

    fn push_scope(&mut self, scope: Scope<'a>) {
        self.scope_stack.push(self.scopes.len());
        self.scopes.push(scope);
    }

    fn pop_scope(&mut self) {
        self.dead_scopes.push(
            self.scope_stack
                .pop()
                .expect("Attempted to pop without scope"),
        );
    }

    fn bind_builtins(&mut self) {
        let scope = &mut self.scopes[*(self.scope_stack.last().expect("No current scope found"))];

        for builtin in BUILTINS.iter().chain(MAGIC_GLOBALS.iter()) {
            let index = self.bindings.len();
            self.bindings.push(Binding {
                kind: BindingKind::Builtin,
                range: Range::default(),
                used: None,
                source: None,
            });
            scope.values.insert(builtin, index);
        }
    }

    /// Return the current `Stmt`.
    pub fn current_stmt(&self) -> &RefEquality<'a, Stmt> {
        self.parents.iter().rev().next().expect("No parent found")
    }

    /// Return the parent `Stmt` of the current `Stmt`, if any.
    pub fn current_stmt_parent(&self) -> Option<&RefEquality<'a, Stmt>> {
        self.parents.iter().rev().nth(1)
    }

    /// Return the grandparent `Stmt` of the current `Stmt`, if any.
    pub fn current_stmt_grandparent(&self) -> Option<&RefEquality<'a, Stmt>> {
        self.parents.iter().rev().nth(2)
    }

    /// Return the current `Expr`.
    pub fn current_expr(&self) -> Option<&RefEquality<'a, Expr>> {
        self.exprs.iter().rev().next()
    }

    /// Return the parent `Expr` of the current `Expr`.
    pub fn current_expr_parent(&self) -> Option<&RefEquality<'a, Expr>> {
        self.exprs.iter().rev().nth(1)
    }

    /// Return the grandparent `Expr` of the current `Expr`.
    pub fn current_expr_grandparent(&self) -> Option<&RefEquality<'a, Expr>> {
        self.exprs.iter().rev().nth(2)
    }

    pub fn current_scope(&self) -> &Scope {
        &self.scopes[*(self.scope_stack.last().expect("No current scope found"))]
    }

    pub fn current_scopes(&self) -> impl Iterator<Item = &Scope> {
        self.scope_stack
            .iter()
            .rev()
            .map(|index| &self.scopes[*index])
    }

    fn add_binding<'b>(&mut self, name: &'b str, binding: Binding<'a>)
    where
        'b: 'a,
    {
        let binding_index = self.bindings.len();

        let mut overridden = None;
        if let Some((stack_index, scope_index)) = self
            .scope_stack
            .iter()
            .rev()
            .enumerate()
            .find(|(_, scope_index)| self.scopes[**scope_index].values.contains_key(&name))
        {
            let existing_binding_index = self.scopes[*scope_index].values.get(&name).unwrap();
            let existing = &self.bindings[*existing_binding_index];
            let in_current_scope = stack_index == 0;
            if !matches!(existing.kind, BindingKind::Builtin)
                && existing.source.as_ref().map_or(true, |left| {
                    binding.source.as_ref().map_or(true, |right| {
                        !branch_detection::different_forks(
                            left,
                            right,
                            &self.depths,
                            &self.child_to_parent,
                        )
                    })
                })
            {
                let existing_is_import = matches!(
                    existing.kind,
                    BindingKind::Importation(..)
                        | BindingKind::FromImportation(..)
                        | BindingKind::SubmoduleImportation(..)
                        | BindingKind::StarImportation(..)
                        | BindingKind::FutureImportation
                );
                if matches!(binding.kind, BindingKind::LoopVar) && existing_is_import {
                    overridden = Some((*scope_index, *existing_binding_index));
                    if self.settings.enabled.contains(&CheckCode::F402) {
                        self.add_check(Check::new(
                            CheckKind::ImportShadowedByLoopVar(
                                name.to_string(),
                                existing.range.location.row(),
                            ),
                            binding.range,
                        ));
                    }
                } else if in_current_scope {
                    if existing.used.is_none()
                        && binding.redefines(existing)
                        && (!self.settings.dummy_variable_rgx.is_match(name) || existing_is_import)
                        && !(matches!(existing.kind, BindingKind::FunctionDefinition)
                            && visibility::is_overload(
                                self,
                                cast::decorator_list(existing.source.as_ref().unwrap().0),
                            ))
                    {
                        overridden = Some((*scope_index, *existing_binding_index));
                        if self.settings.enabled.contains(&CheckCode::F811) {
                            self.add_check(Check::new(
                                CheckKind::RedefinedWhileUnused(
                                    name.to_string(),
                                    existing.range.location.row(),
                                ),
                                binding.range,
                            ));
                        }
                    }
                } else if existing_is_import && binding.redefines(existing) {
                    self.redefinitions
                        .entry(*existing_binding_index)
                        .or_insert_with(Vec::new)
                        .push(binding_index);
                }
            }
        }

        // If we're about to lose the binding, store it as overriden.
        if let Some((scope_index, binding_index)) = overridden {
            self.scopes[scope_index]
                .overridden
                .push((name, binding_index));
        }

        // Assume the rebound name is used as a global or within a loop.
        let scope = self.current_scope();
        let binding = match scope.values.get(&name) {
            None => binding,
            Some(index) => Binding {
                used: self.bindings[*index].used,
                ..binding
            },
        };

        // Don't treat annotations as assignments if there is an existing value
        // in scope.
        let scope = &mut self.scopes[*(self.scope_stack.last().expect("No current scope found"))];
        if !(matches!(binding.kind, BindingKind::Annotation) && scope.values.contains_key(name)) {
            scope.values.insert(name, binding_index);
        }

        self.bindings.push(binding);
    }

    fn handle_node_load(&mut self, expr: &Expr) {
        if let ExprKind::Name { id, .. } = &expr.node {
            let scope_id = self.current_scope().id;

            let mut first_iter = true;
            let mut in_generator = false;
            let mut import_starred = false;

            for scope_index in self.scope_stack.iter().rev() {
                let scope = &self.scopes[*scope_index];

                if matches!(scope.kind, ScopeKind::Class(_)) {
                    if id == "__class__" {
                        return;
                    } else if !first_iter && !in_generator {
                        continue;
                    }
                }

                if let Some(index) = scope.values.get(&id.as_str()) {
                    // Mark the binding as used.
                    self.bindings[*index].used = Some((scope_id, Range::from_located(expr)));

                    if matches!(self.bindings[*index].kind, BindingKind::Annotation)
                        && !self.in_deferred_string_type_definition
                        && !self.in_deferred_type_definition
                    {
                        continue;
                    }

                    // If the name of the sub-importation is the same as an alias of another
                    // importation and the alias is used, that sub-importation should be
                    // marked as used too.
                    //
                    // This handles code like:
                    //   import pyarrow as pa
                    //   import pyarrow.csv
                    //   print(pa.csv.read_csv("test.csv"))
                    if let BindingKind::Importation(name, full_name)
                    | BindingKind::FromImportation(name, full_name)
                    | BindingKind::SubmoduleImportation(name, full_name) =
                        &self.bindings[*index].kind
                    {
                        let has_alias = full_name
                            .split('.')
                            .last()
                            .map(|segment| segment != name)
                            .unwrap_or_default();
                        if has_alias {
                            // Mark the sub-importation as used.
                            if let Some(index) = scope.values.get(full_name.as_str()) {
                                self.bindings[*index].used =
                                    Some((scope_id, Range::from_located(expr)));
                            }
                        }
                    }

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
                        for binding in scope.values.values().map(|index| &self.bindings[*index]) {
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

                // Allow "__module__" and "__qualname__" in class scopes.
                if (id == "__module__" || id == "__qualname__")
                    && matches!(self.current_scope().kind, ScopeKind::Class(..))
                {
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
                ));
            }
        }
    }

    fn handle_node_store<'b>(&mut self, id: &'b str, expr: &Expr)
    where
        'b: 'a,
    {
        let parent = self.current_stmt().0;

        if self.settings.enabled.contains(&CheckCode::F823) {
            let scopes: Vec<&Scope> = self
                .scope_stack
                .iter()
                .map(|index| &self.scopes[*index])
                .collect();
            if let Some(check) = pyflakes::checks::undefined_local(id, &scopes, &self.bindings) {
                self.add_check(check);
            }
        }

        if self.settings.enabled.contains(&CheckCode::N806) {
            if matches!(self.current_scope().kind, ScopeKind::Function(..)) {
                // Ignore globals.
                if !self.current_scope().values.get(id).map_or(false, |index| {
                    matches!(self.bindings[*index].kind, BindingKind::Global)
                }) {
                    pep8_naming::plugins::non_lowercase_variable_in_function(
                        self, expr, parent, id,
                    );
                }
            }
        }

        if self.settings.enabled.contains(&CheckCode::N815) {
            if matches!(self.current_scope().kind, ScopeKind::Class(..)) {
                pep8_naming::plugins::mixed_case_variable_in_class_scope(self, expr, parent, id);
            }
        }

        if self.settings.enabled.contains(&CheckCode::N816) {
            if matches!(self.current_scope().kind, ScopeKind::Module) {
                pep8_naming::plugins::mixed_case_variable_in_global_scope(self, expr, parent, id);
            }
        }

        if matches!(parent.node, StmtKind::AnnAssign { value: None, .. }) {
            self.add_binding(
                id,
                Binding {
                    kind: BindingKind::Annotation,
                    used: None,
                    range: Range::from_located(expr),
                    source: Some(self.current_stmt().clone()),
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
                    source: Some(self.current_stmt().clone()),
                },
            );
            return;
        }

        if operations::is_unpacking_assignment(parent, expr) {
            self.add_binding(
                id,
                Binding {
                    kind: BindingKind::Binding,
                    used: None,
                    range: Range::from_located(expr),
                    source: Some(self.current_stmt().clone()),
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
            if match &parent.node {
                StmtKind::Assign { targets, .. } => {
                    if let Some(ExprKind::Name { id, .. }) =
                        targets.first().map(|target| &target.node)
                    {
                        id == "__all__"
                    } else {
                        false
                    }
                }
                StmtKind::AugAssign { target, .. } => {
                    if let ExprKind::Name { id, .. } = &target.node {
                        id == "__all__"
                    } else {
                        false
                    }
                }
                StmtKind::AnnAssign { target, .. } => {
                    if let ExprKind::Name { id, .. } = &target.node {
                        id == "__all__"
                    } else {
                        false
                    }
                }
                _ => false,
            } {
                self.add_binding(
                    id,
                    Binding {
                        kind: BindingKind::Export(extract_all_names(
                            parent,
                            current,
                            &self.bindings,
                        )),
                        used: None,
                        range: Range::from_located(expr),
                        source: Some(self.current_stmt().clone()),
                    },
                );
                return;
            }
        }

        self.add_binding(
            id,
            Binding {
                kind: BindingKind::Assignment,
                used: None,
                range: Range::from_located(expr),
                source: Some(self.current_stmt().clone()),
            },
        );
    }

    fn handle_node_delete<'b>(&mut self, expr: &'b Expr)
    where
        'b: 'a,
    {
        if let ExprKind::Name { id, .. } = &expr.node {
            if operations::on_conditional_branch(&mut self.parents.iter().rev().map(|node| node.0))
            {
                return;
            }

            let scope =
                &mut self.scopes[*(self.scope_stack.last().expect("No current scope found"))];
            if scope.values.remove(&id.as_str()).is_none()
                && self.settings.enabled.contains(&CheckCode::F821)
            {
                self.add_check(Check::new(
                    CheckKind::UndefinedName(id.to_string()),
                    Range::from_located(expr),
                ));
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

    fn check_deferred_type_definitions(&mut self) {
        self.deferred_type_definitions.reverse();
        while let Some((expr, in_annotation, (scopes, parents))) =
            self.deferred_type_definitions.pop()
        {
            self.scope_stack = scopes;
            self.parents = parents;
            self.in_annotation = in_annotation;
            self.in_type_definition = true;
            self.in_deferred_type_definition = true;
            self.visit_expr(expr);
            self.in_deferred_type_definition = false;
            self.in_type_definition = false;
        }
    }

    fn check_deferred_string_type_definitions<'b>(&mut self, allocator: &'b mut Vec<Expr>)
    where
        'b: 'a,
    {
        let mut stacks = vec![];
        self.deferred_string_type_definitions.reverse();
        while let Some((range, expression, in_annotation, context)) =
            self.deferred_string_type_definitions.pop()
        {
            if let Ok(mut expr) = parser::parse_expression(expression, "<filename>") {
                relocate_expr(&mut expr, range);
                allocator.push(expr);
                stacks.push((in_annotation, context));
            } else {
                if self.settings.enabled.contains(&CheckCode::F722) {
                    self.add_check(Check::new(
                        CheckKind::ForwardAnnotationSyntaxError(expression.to_string()),
                        range,
                    ));
                }
            }
        }
        for (expr, (in_annotation, (scopes, parents))) in allocator.iter().zip(stacks) {
            self.scope_stack = scopes;
            self.parents = parents;
            self.in_annotation = in_annotation;
            self.in_type_definition = true;
            self.in_deferred_string_type_definition = true;
            self.visit_expr(expr);
            self.in_deferred_string_type_definition = false;
            self.in_type_definition = false;
        }
    }

    fn check_deferred_functions(&mut self) {
        self.deferred_functions.reverse();
        while let Some((stmt, (scopes, parents), visibility)) = self.deferred_functions.pop() {
            self.scope_stack = scopes.clone();
            self.parents = parents.clone();
            self.visible_scope = visibility;

            match &stmt.node {
                StmtKind::FunctionDef { body, args, .. }
                | StmtKind::AsyncFunctionDef { body, args, .. } => {
                    self.visit_arguments(args);
                    for stmt in body {
                        self.visit_stmt(stmt);
                    }
                }
                _ => unreachable!("Expected StmtKind::FunctionDef | StmtKind::AsyncFunctionDef"),
            }

            self.deferred_assignments.push((scopes, parents));
        }
    }

    fn check_deferred_lambdas(&mut self) {
        self.deferred_lambdas.reverse();
        while let Some((expr, (scopes, parents))) = self.deferred_lambdas.pop() {
            self.scope_stack = scopes.clone();
            self.parents = parents.clone();

            if let ExprKind::Lambda { args, body } = &expr.node {
                self.visit_arguments(args);
                self.visit_expr(body);
            } else {
                unreachable!("Expected ExprKind::Lambda");
            }

            self.deferred_assignments.push((scopes, parents));
        }
    }

    fn check_deferred_assignments(&mut self) {
        self.deferred_assignments.reverse();
        while let Some((scopes, _parents)) = self.deferred_assignments.pop() {
            let scope_index = scopes[scopes.len() - 1];
            let parent_scope_index = scopes[scopes.len() - 2];
            if self.settings.enabled.contains(&CheckCode::F841) {
                self.add_checks(
                    pyflakes::checks::unused_variable(
                        &self.scopes[scope_index],
                        &self.bindings,
                        &self.settings.dummy_variable_rgx,
                    )
                    .into_iter(),
                );
            }
            if self.settings.enabled.contains(&CheckCode::F842) {
                self.add_checks(
                    pyflakes::checks::unused_annotation(
                        &self.scopes[scope_index],
                        &self.bindings,
                        &self.settings.dummy_variable_rgx,
                    )
                    .into_iter(),
                );
            }
            if self.settings.enabled.contains(&CheckCode::ARG001)
                || self.settings.enabled.contains(&CheckCode::ARG002)
                || self.settings.enabled.contains(&CheckCode::ARG003)
                || self.settings.enabled.contains(&CheckCode::ARG004)
                || self.settings.enabled.contains(&CheckCode::ARG005)
            {
                self.add_checks(
                    flake8_unused_arguments::plugins::unused_arguments(
                        self,
                        &self.scopes[parent_scope_index],
                        &self.scopes[scope_index],
                        &self.bindings,
                    )
                    .into_iter(),
                );
            }
        }
    }

    fn check_dead_scopes(&mut self) {
        if !self.settings.enabled.contains(&CheckCode::F401)
            && !self.settings.enabled.contains(&CheckCode::F405)
            && !self.settings.enabled.contains(&CheckCode::F811)
            && !self.settings.enabled.contains(&CheckCode::F822)
            && !self.settings.enabled.contains(&CheckCode::PLW0602)
        {
            return;
        }

        let mut checks: Vec<Check> = vec![];
        for scope in self
            .dead_scopes
            .iter()
            .rev()
            .map(|index| &self.scopes[*index])
        {
            // PLW0602
            if self.settings.enabled.contains(&CheckCode::PLW0602) {
                for (name, index) in &scope.values {
                    let binding = &self.bindings[*index];
                    if matches!(binding.kind, BindingKind::Global) {
                        checks.push(Check::new(
                            CheckKind::GlobalVariableNotAssigned((*name).to_string()),
                            binding.range,
                        ));
                    }
                }
            }

            // Imports in classes are public members.
            if matches!(scope.kind, ScopeKind::Class(..)) {
                continue;
            }

            let all_binding: Option<&Binding> = scope
                .values
                .get("__all__")
                .map(|index| &self.bindings[*index]);
            let all_names: Option<Vec<&str>> =
                all_binding.and_then(|binding| match &binding.kind {
                    BindingKind::Export(names) => Some(names.iter().map(String::as_str).collect()),
                    _ => None,
                });

            if self.settings.enabled.contains(&CheckCode::F822) {
                if !scope.import_starred && !self.path.ends_with("__init__.py") {
                    if let Some(all_binding) = all_binding {
                        if let Some(names) = &all_names {
                            for &name in names {
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

            // Look for any bindings that were redefined in another scope, and remain
            // unused. Note that we only store references in `redefinitions` if
            // the bindings are in different scopes.
            if self.settings.enabled.contains(&CheckCode::F811) {
                for (name, index) in &scope.values {
                    let binding = &self.bindings[*index];

                    if matches!(
                        binding.kind,
                        BindingKind::Importation(..)
                            | BindingKind::FromImportation(..)
                            | BindingKind::SubmoduleImportation(..)
                            | BindingKind::StarImportation(..)
                            | BindingKind::FutureImportation
                    ) {
                        // Skip used exports from `__all__`
                        if binding.used.is_some()
                            || all_names
                                .as_ref()
                                .map(|names| names.contains(name))
                                .unwrap_or_default()
                        {
                            continue;
                        }

                        if let Some(indices) = self.redefinitions.get(index) {
                            for index in indices {
                                checks.push(Check::new(
                                    CheckKind::RedefinedWhileUnused(
                                        (*name).to_string(),
                                        binding.range.location.row(),
                                    ),
                                    self.bindings[*index].range,
                                ));
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
                            for binding in scope.values.values().map(|index| &self.bindings[*index])
                            {
                                if let BindingKind::StarImportation(level, module) = &binding.kind {
                                    from_list.push(helpers::format_import_from(
                                        level.as_ref(),
                                        module.as_ref(),
                                    ));
                                }
                            }
                            from_list.sort();

                            for &name in names {
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
                type UnusedImport<'a> = (&'a String, &'a Range);
                type BindingContext<'a, 'b> =
                    (&'a RefEquality<'b, Stmt>, Option<&'a RefEquality<'b, Stmt>>);

                let mut unused: FxHashMap<BindingContext, Vec<UnusedImport>> = FxHashMap::default();
                let mut ignored: FxHashMap<BindingContext, Vec<UnusedImport>> =
                    FxHashMap::default();

                for (name, index) in scope
                    .values
                    .iter()
                    .chain(scope.overridden.iter().map(|(a, b)| (a, b)))
                {
                    let binding = &self.bindings[*index];

                    let (BindingKind::Importation(_, full_name)
                    | BindingKind::SubmoduleImportation(_, full_name)
                    | BindingKind::FromImportation(_, full_name)) = &binding.kind else { continue; };

                    // Skip used exports from `__all__`
                    if binding.used.is_some()
                        || all_names
                            .as_ref()
                            .map(|names| names.contains(name))
                            .unwrap_or_default()
                    {
                        continue;
                    }

                    let defined_by = binding.source.as_ref().unwrap();
                    let defined_in = self.child_to_parent.get(defined_by);
                    if self.is_ignored(&CheckCode::F401, binding.range.location.row()) {
                        ignored
                            .entry((defined_by, defined_in))
                            .or_default()
                            .push((full_name, &binding.range));
                    } else {
                        unused
                            .entry((defined_by, defined_in))
                            .or_default()
                            .push((full_name, &binding.range));
                    }
                }

                let ignore_init =
                    self.settings.ignore_init_module_imports && self.path.ends_with("__init__.py");
                for ((defined_by, defined_in), unused_imports) in unused
                    .into_iter()
                    .sorted_by_key(|((defined_by, _), _)| defined_by.0.location)
                {
                    let child = defined_by.0;
                    let parent = defined_in.map(|defined_in| defined_in.0);

                    let fix = if !ignore_init && self.patch(&CheckCode::F401) {
                        let deleted: Vec<&Stmt> =
                            self.deletions.iter().map(|node| node.0).collect();
                        match pyflakes::fixes::remove_unused_imports(
                            &unused_imports,
                            child,
                            parent,
                            &deleted,
                            self.locator,
                        ) {
                            Ok(fix) => {
                                if fix.content.is_empty() || fix.content == "pass" {
                                    self.deletions.insert(defined_by.clone());
                                }
                                Some(fix)
                            }
                            Err(e) => {
                                error!("Failed to remove unused imports: {e}");
                                None
                            }
                        }
                    } else {
                        None
                    };

                    for (full_name, range) in unused_imports {
                        let mut check = Check::new(
                            CheckKind::UnusedImport(full_name.clone(), ignore_init),
                            *range,
                        );
                        if let Some(fix) = fix.as_ref() {
                            check.amend(fix.clone());
                        }
                        checks.push(check);
                    }
                }
                for (_, unused_imports) in ignored
                    .into_iter()
                    .sorted_by_key(|((defined_by, _), _)| defined_by.0.location)
                {
                    for (full_name, range) in unused_imports {
                        checks.push(Check::new(
                            CheckKind::UnusedImport(full_name.clone(), ignore_init),
                            *range,
                        ));
                    }
                }
            }
        }
        self.add_checks(checks.into_iter());
    }

    fn check_definitions(&mut self) {
        let enforce_annotations = self.settings.enabled.contains(&CheckCode::ANN001)
            || self.settings.enabled.contains(&CheckCode::ANN002)
            || self.settings.enabled.contains(&CheckCode::ANN003)
            || self.settings.enabled.contains(&CheckCode::ANN101)
            || self.settings.enabled.contains(&CheckCode::ANN102)
            || self.settings.enabled.contains(&CheckCode::ANN201)
            || self.settings.enabled.contains(&CheckCode::ANN202)
            || self.settings.enabled.contains(&CheckCode::ANN204)
            || self.settings.enabled.contains(&CheckCode::ANN205)
            || self.settings.enabled.contains(&CheckCode::ANN206)
            || self.settings.enabled.contains(&CheckCode::ANN401);
        let enforce_docstrings = self.settings.enabled.contains(&CheckCode::D100)
            || self.settings.enabled.contains(&CheckCode::D101)
            || self.settings.enabled.contains(&CheckCode::D102)
            || self.settings.enabled.contains(&CheckCode::D103)
            || self.settings.enabled.contains(&CheckCode::D104)
            || self.settings.enabled.contains(&CheckCode::D105)
            || self.settings.enabled.contains(&CheckCode::D106)
            || self.settings.enabled.contains(&CheckCode::D107)
            || self.settings.enabled.contains(&CheckCode::D200)
            || self.settings.enabled.contains(&CheckCode::D201)
            || self.settings.enabled.contains(&CheckCode::D202)
            || self.settings.enabled.contains(&CheckCode::D203)
            || self.settings.enabled.contains(&CheckCode::D204)
            || self.settings.enabled.contains(&CheckCode::D205)
            || self.settings.enabled.contains(&CheckCode::D206)
            || self.settings.enabled.contains(&CheckCode::D207)
            || self.settings.enabled.contains(&CheckCode::D208)
            || self.settings.enabled.contains(&CheckCode::D209)
            || self.settings.enabled.contains(&CheckCode::D210)
            || self.settings.enabled.contains(&CheckCode::D211)
            || self.settings.enabled.contains(&CheckCode::D212)
            || self.settings.enabled.contains(&CheckCode::D213)
            || self.settings.enabled.contains(&CheckCode::D214)
            || self.settings.enabled.contains(&CheckCode::D215)
            || self.settings.enabled.contains(&CheckCode::D300)
            || self.settings.enabled.contains(&CheckCode::D301)
            || self.settings.enabled.contains(&CheckCode::D400)
            || self.settings.enabled.contains(&CheckCode::D402)
            || self.settings.enabled.contains(&CheckCode::D403)
            || self.settings.enabled.contains(&CheckCode::D404)
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
            || self.settings.enabled.contains(&CheckCode::D415)
            || self.settings.enabled.contains(&CheckCode::D416)
            || self.settings.enabled.contains(&CheckCode::D417)
            || self.settings.enabled.contains(&CheckCode::D418)
            || self.settings.enabled.contains(&CheckCode::D419);

        let mut overloaded_name: Option<String> = None;
        self.definitions.reverse();
        while let Some((definition, visibility)) = self.definitions.pop() {
            // flake8-annotations
            if enforce_annotations {
                // TODO(charlie): This should be even stricter, in that an overload
                // implementation should come immediately after the overloaded
                // interfaces, without any AST nodes in between. Right now, we
                // only error when traversing definition boundaries (functions,
                // classes, etc.).
                if !overloaded_name.map_or(false, |overloaded_name| {
                    flake8_annotations::helpers::is_overload_impl(
                        self,
                        &definition,
                        &overloaded_name,
                    )
                }) {
                    flake8_annotations::plugins::definition(self, &definition, &visibility);
                }
                overloaded_name = flake8_annotations::helpers::overloaded_name(self, &definition);
            }

            // pydocstyle
            if enforce_docstrings {
                if definition.docstring.is_none() {
                    pydocstyle::plugins::not_missing(self, &definition, &visibility);
                    continue;
                }

                // Extract a `Docstring` from a `Definition`.
                let expr = definition.docstring.unwrap();
                let content = self
                    .locator
                    .slice_source_code_range(&Range::from_located(expr));
                let indentation = self.locator.slice_source_code_range(&Range {
                    location: Location::new(expr.location.row(), 0),
                    end_location: Location::new(expr.location.row(), expr.location.column()),
                });
                let body = pydocstyle::helpers::raw_contents(&content);
                let docstring = Docstring {
                    kind: definition.kind,
                    expr,
                    contents: &content,
                    indentation: &indentation,
                    body,
                };

                if !pydocstyle::plugins::not_empty(self, &docstring) {
                    continue;
                }

                if self.settings.enabled.contains(&CheckCode::D200) {
                    pydocstyle::plugins::one_liner(self, &docstring);
                }
                if self.settings.enabled.contains(&CheckCode::D201)
                    || self.settings.enabled.contains(&CheckCode::D202)
                {
                    pydocstyle::plugins::blank_before_after_function(self, &docstring);
                }
                if self.settings.enabled.contains(&CheckCode::D203)
                    || self.settings.enabled.contains(&CheckCode::D204)
                    || self.settings.enabled.contains(&CheckCode::D211)
                {
                    pydocstyle::plugins::blank_before_after_class(self, &docstring);
                }
                if self.settings.enabled.contains(&CheckCode::D205) {
                    pydocstyle::plugins::blank_after_summary(self, &docstring);
                }
                if self.settings.enabled.contains(&CheckCode::D206)
                    || self.settings.enabled.contains(&CheckCode::D207)
                    || self.settings.enabled.contains(&CheckCode::D208)
                {
                    pydocstyle::plugins::indent(self, &docstring);
                }
                if self.settings.enabled.contains(&CheckCode::D209) {
                    pydocstyle::plugins::newline_after_last_paragraph(self, &docstring);
                }
                if self.settings.enabled.contains(&CheckCode::D210) {
                    pydocstyle::plugins::no_surrounding_whitespace(self, &docstring);
                }
                if self.settings.enabled.contains(&CheckCode::D212)
                    || self.settings.enabled.contains(&CheckCode::D213)
                {
                    pydocstyle::plugins::multi_line_summary_start(self, &docstring);
                }
                if self.settings.enabled.contains(&CheckCode::D300) {
                    pydocstyle::plugins::triple_quotes(self, &docstring);
                }
                if self.settings.enabled.contains(&CheckCode::D301) {
                    pydocstyle::plugins::backslashes(self, &docstring);
                }
                if self.settings.enabled.contains(&CheckCode::D400) {
                    pydocstyle::plugins::ends_with_period(self, &docstring);
                }
                if self.settings.enabled.contains(&CheckCode::D402) {
                    pydocstyle::plugins::no_signature(self, &docstring);
                }
                if self.settings.enabled.contains(&CheckCode::D403) {
                    pydocstyle::plugins::capitalized(self, &docstring);
                }
                if self.settings.enabled.contains(&CheckCode::D404) {
                    pydocstyle::plugins::starts_with_this(self, &docstring);
                }
                if self.settings.enabled.contains(&CheckCode::D415) {
                    pydocstyle::plugins::ends_with_punctuation(self, &docstring);
                }
                if self.settings.enabled.contains(&CheckCode::D418) {
                    pydocstyle::plugins::if_needed(self, &docstring);
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
                    pydocstyle::plugins::sections(self, &docstring);
                }
            }
        }
    }

    fn check_builtin_shadowing<T>(&mut self, name: &str, located: &Located<T>, is_attribute: bool) {
        if is_attribute && matches!(self.current_scope().kind, ScopeKind::Class(_)) {
            if self.settings.enabled.contains(&CheckCode::A003) {
                if let Some(check) = flake8_builtins::checks::builtin_shadowing(
                    name,
                    located,
                    flake8_builtins::types::ShadowingType::Attribute,
                ) {
                    self.add_check(check);
                }
            }
        } else {
            if self.settings.enabled.contains(&CheckCode::A001) {
                if let Some(check) = flake8_builtins::checks::builtin_shadowing(
                    name,
                    located,
                    flake8_builtins::types::ShadowingType::Variable,
                ) {
                    self.add_check(check);
                }
            }
        }
    }

    fn check_builtin_arg_shadowing(&mut self, name: &str, arg: &Arg) {
        if self.settings.enabled.contains(&CheckCode::A002) {
            if let Some(check) = flake8_builtins::checks::builtin_shadowing(
                name,
                arg,
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
    noqa_line_for: &IntMap<usize, usize>,
    settings: &Settings,
    autofix: flags::Autofix,
    noqa: flags::Noqa,
    path: &Path,
) -> Vec<Check> {
    let mut checker = Checker::new(settings, noqa_line_for, autofix, noqa, path, locator);
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
    checker.check_deferred_type_definitions();
    let mut allocator = vec![];
    checker.check_deferred_string_type_definitions(&mut allocator);

    // Reset the scope to module-level, and check all consumed scopes.
    checker.scope_stack = vec![GLOBAL_SCOPE_INDEX];
    checker.pop_scope();
    checker.check_dead_scopes();

    // Check docstrings.
    checker.check_definitions();

    checker.checks
}
