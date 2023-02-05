//! Lint rules based on AST traversal.

use std::iter;
use std::path::Path;

use itertools::Itertools;
use log::error;
use nohash_hasher::IntMap;
use rustc_hash::{FxHashMap, FxHashSet};
use rustpython_ast::{Comprehension, Located, Location};
use rustpython_common::cformat::{CFormatError, CFormatErrorType};
use rustpython_parser::ast::{
    Arg, Arguments, Constant, Excepthandler, ExcepthandlerKind, Expr, ExprContext, ExprKind,
    KeywordData, Operator, Stmt, StmtKind, Suite,
};
use rustpython_parser::parser;
use smallvec::smallvec;

use ruff_python::builtins::{BUILTINS, MAGIC_GLOBALS};
use ruff_python::typing::TYPING_EXTENSIONS;

use crate::ast::helpers::{binding_range, collect_call_path, extract_handler_names};
use crate::ast::operations::{extract_all_names, AllNamesFlags};
use crate::ast::relocate::relocate_expr;
use crate::ast::types::{
    Binding, BindingKind, CallPath, ClassDef, ExecutionContext, FunctionDef, Lambda, Node, Range,
    RefEquality, Scope, ScopeKind,
};
use crate::ast::typing::{match_annotated_subscript, Callable, SubscriptKind};
use crate::ast::visitor::{walk_excepthandler, Visitor};
use crate::ast::{branch_detection, cast, helpers, operations, typing, visitor};
use crate::docstrings::definition::{Definition, DefinitionKind, Docstring, Documentable};
use crate::noqa::Directive;
use crate::registry::{Diagnostic, Rule};
use crate::rules::{
    flake8_2020, flake8_annotations, flake8_bandit, flake8_blind_except, flake8_boolean_trap,
    flake8_bugbear, flake8_builtins, flake8_comprehensions, flake8_datetimez, flake8_debugger,
    flake8_errmsg, flake8_implicit_str_concat, flake8_import_conventions, flake8_logging_format,
    flake8_pie, flake8_print, flake8_pytest_style, flake8_raise, flake8_return, flake8_self,
    flake8_simplify, flake8_tidy_imports, flake8_type_checking, flake8_unused_arguments,
    flake8_use_pathlib, mccabe, pandas_vet, pep8_naming, pycodestyle, pydocstyle, pyflakes,
    pygrep_hooks, pylint, pyupgrade, ruff, tryceratops,
};
use crate::settings::types::PythonVersion;
use crate::settings::{flags, Settings};
use crate::source_code::{Indexer, Locator, Stylist};
use crate::visibility::{module_visibility, transition_scope, Modifier, Visibility, VisibleScope};
use crate::{autofix, docstrings, noqa, visibility};

const GLOBAL_SCOPE_INDEX: usize = 0;

type DeferralContext<'a> = (Vec<usize>, Vec<RefEquality<'a, Stmt>>);

#[allow(clippy::struct_excessive_bools)]
pub struct Checker<'a> {
    // Input data.
    pub(crate) path: &'a Path,
    package: Option<&'a Path>,
    autofix: flags::Autofix,
    noqa: flags::Noqa,
    pub(crate) settings: &'a Settings,
    pub(crate) noqa_line_for: &'a IntMap<usize, usize>,
    pub(crate) locator: &'a Locator<'a>,
    pub(crate) stylist: &'a Stylist<'a>,
    pub(crate) indexer: &'a Indexer,
    // Computed diagnostics.
    pub(crate) diagnostics: Vec<Diagnostic>,
    // Function and class definition tracking (e.g., for docstring enforcement).
    definitions: Vec<(Definition<'a>, Visibility, DeferralContext<'a>)>,
    // Edit tracking.
    // TODO(charlie): Instead of exposing deletions, wrap in a public API.
    pub(crate) deletions: FxHashSet<RefEquality<'a, Stmt>>,
    // Retain all scopes and parent nodes, along with a stack of indexes to track which are active
    // at various points in time.
    pub(crate) parents: Vec<RefEquality<'a, Stmt>>,
    pub(crate) depths: FxHashMap<RefEquality<'a, Stmt>, usize>,
    pub(crate) child_to_parent: FxHashMap<RefEquality<'a, Stmt>, RefEquality<'a, Stmt>>,
    // A stack of all bindings created in any scope, at any point in execution.
    pub(crate) bindings: Vec<Binding<'a>>,
    // Map from binding index to indexes of bindings that redefine it in other scopes.
    pub(crate) redefinitions: IntMap<usize, Vec<usize>>,
    pub(crate) exprs: Vec<RefEquality<'a, Expr>>,
    pub(crate) scopes: Vec<Scope<'a>>,
    pub(crate) scope_stack: Vec<usize>,
    pub(crate) dead_scopes: Vec<(usize, Vec<usize>)>,
    deferred_string_type_definitions: Vec<(Range, &'a str, bool, DeferralContext<'a>)>,
    deferred_type_definitions: Vec<(&'a Expr, bool, DeferralContext<'a>)>,
    deferred_functions: Vec<(&'a Stmt, DeferralContext<'a>, VisibleScope)>,
    deferred_lambdas: Vec<(&'a Expr, DeferralContext<'a>)>,
    deferred_for_loops: Vec<(&'a Stmt, DeferralContext<'a>)>,
    deferred_assignments: Vec<DeferralContext<'a>>,
    // Body iteration; used to peek at siblings.
    body: &'a [Stmt],
    body_index: usize,
    // Internal, derivative state.
    visible_scope: VisibleScope,
    in_annotation: bool,
    in_type_definition: bool,
    in_deferred_string_type_definition: bool,
    in_deferred_type_definition: bool,
    in_exception_handler: bool,
    in_literal: bool,
    in_subscript: bool,
    in_type_checking_block: bool,
    pub(crate) seen_import_boundary: bool,
    futures_allowed: bool,
    annotations_future_enabled: bool,
    except_handlers: Vec<Vec<Vec<&'a str>>>,
    // Check-specific state.
    pub(crate) flake8_bugbear_seen: Vec<&'a Expr>,
}

impl<'a> Checker<'a> {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        settings: &'a Settings,
        noqa_line_for: &'a IntMap<usize, usize>,
        autofix: flags::Autofix,
        noqa: flags::Noqa,
        path: &'a Path,
        package: Option<&'a Path>,
        locator: &'a Locator,
        style: &'a Stylist,
        indexer: &'a Indexer,
    ) -> Checker<'a> {
        Checker {
            settings,
            noqa_line_for,
            autofix,
            noqa,
            path,
            package,
            locator,
            stylist: style,
            indexer,
            diagnostics: vec![],
            definitions: vec![],
            deletions: FxHashSet::default(),
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
            deferred_for_loops: vec![],
            deferred_assignments: vec![],
            // Body iteration.
            body: &[],
            body_index: 0,
            // Internal, derivative state.
            visible_scope: VisibleScope {
                modifier: Modifier::Module,
                visibility: module_visibility(path),
            },
            in_annotation: false,
            in_type_definition: false,
            in_deferred_string_type_definition: false,
            in_deferred_type_definition: false,
            in_exception_handler: false,
            in_literal: false,
            in_subscript: false,
            in_type_checking_block: false,
            seen_import_boundary: false,
            futures_allowed: true,
            annotations_future_enabled: path.extension().map_or(false, |ext| ext == "pyi"),
            except_handlers: vec![],
            // Check-specific state.
            flake8_bugbear_seen: vec![],
        }
    }

    /// Return `true` if a patch should be generated under the given autofix
    /// `Mode`.
    pub fn patch(&self, code: &Rule) -> bool {
        matches!(self.autofix, flags::Autofix::Enabled) && self.settings.rules.should_fix(code)
    }

    /// Return `true` if the `Expr` is a reference to `typing.${target}`.
    pub fn match_typing_expr(&self, expr: &Expr, target: &str) -> bool {
        self.resolve_call_path(expr).map_or(false, |call_path| {
            self.match_typing_call_path(&call_path, target)
        })
    }

    /// Return `true` if the call path is a reference to `typing.${target}`.
    pub fn match_typing_call_path(&self, call_path: &CallPath, target: &str) -> bool {
        if call_path.as_slice() == ["typing", target] {
            return true;
        }

        if TYPING_EXTENSIONS.contains(target) {
            if call_path.as_slice() == ["typing_extensions", target] {
                return true;
            }
        }

        if self.settings.typing_modules.iter().any(|module| {
            let mut module: CallPath = module.split('.').collect();
            module.push(target);
            *call_path == module
        }) {
            return true;
        }

        false
    }

    /// Return the current `Binding` for a given `name`.
    pub fn find_binding(&self, member: &str) -> Option<&Binding> {
        self.current_scopes()
            .find_map(|scope| scope.bindings.get(member))
            .map(|index| &self.bindings[*index])
    }

    /// Return `true` if `member` is bound as a builtin.
    pub fn is_builtin(&self, member: &str) -> bool {
        self.find_binding(member).map_or(false, |binding| {
            matches!(binding.kind, BindingKind::Builtin)
        })
    }

    pub fn resolve_call_path<'b>(&'a self, value: &'b Expr) -> Option<CallPath<'a>>
    where
        'b: 'a,
    {
        let call_path = collect_call_path(value);
        if let Some(head) = call_path.first() {
            if let Some(binding) = self.find_binding(head) {
                match &binding.kind {
                    BindingKind::Importation(.., name) => {
                        // Ignore relative imports.
                        if name.starts_with('.') {
                            return None;
                        }
                        let mut source_path: CallPath = name.split('.').collect();
                        source_path.extend(call_path.into_iter().skip(1));
                        return Some(source_path);
                    }
                    BindingKind::SubmoduleImportation(name, ..) => {
                        // Ignore relative imports.
                        if name.starts_with('.') {
                            return None;
                        }
                        let mut source_path: CallPath = name.split('.').collect();
                        source_path.extend(call_path.into_iter().skip(1));
                        return Some(source_path);
                    }
                    BindingKind::FromImportation(.., name) => {
                        // Ignore relative imports.
                        if name.starts_with('.') {
                            return None;
                        }
                        let mut source_path: CallPath = name.split('.').collect();
                        source_path.extend(call_path.into_iter().skip(1));
                        return Some(source_path);
                    }
                    BindingKind::Builtin => {
                        let mut source_path: CallPath = smallvec![];
                        source_path.push("");
                        source_path.extend(call_path);
                        return Some(source_path);
                    }
                    _ => {}
                }
            }
        }
        None
    }

    /// Return `true` if a `Rule` is disabled by a `noqa` directive.
    pub fn is_ignored(&self, code: &Rule, lineno: usize) -> bool {
        // TODO(charlie): `noqa` directives are mostly enforced in `check_lines.rs`.
        // However, in rare cases, we need to check them here. For example, when
        // removing unused imports, we create a single fix that's applied to all
        // unused members on a single import. We need to pre-emptively omit any
        // members from the fix that will eventually be excluded by a `noqa`.
        // Unfortunately, we _do_ want to register a `Diagnostic` for each
        // eventually-ignored import, so that our `noqa` counts are accurate.
        if matches!(self.noqa, flags::Noqa::Disabled) {
            return false;
        }
        let noqa_lineno = self.noqa_line_for.get(&lineno).unwrap_or(&lineno);
        let line = self.locator.slice_source_code_range(&Range::new(
            Location::new(*noqa_lineno, 0),
            Location::new(noqa_lineno + 1, 0),
        ));
        match noqa::extract_noqa_directive(line) {
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
                        self.parents.iter().rev().map(std::convert::Into::into),
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
                let ranges = helpers::find_names(stmt, self.locator);
                if scope_index != GLOBAL_SCOPE_INDEX {
                    // Add the binding to the current scope.
                    let context = self.execution_context();
                    let scope = &mut self.scopes[scope_index];
                    let usage = Some((scope.id, Range::from_located(stmt)));
                    for (name, range) in names.iter().zip(ranges.iter()) {
                        let index = self.bindings.len();
                        self.bindings.push(Binding {
                            kind: BindingKind::Global,
                            runtime_usage: None,
                            synthetic_usage: usage,
                            typing_usage: None,
                            range: *range,
                            source: Some(RefEquality(stmt)),
                            context,
                        });
                        scope.bindings.insert(name, index);
                    }
                }

                if self.settings.rules.enabled(&Rule::AmbiguousVariableName) {
                    self.diagnostics
                        .extend(names.iter().zip(ranges.iter()).filter_map(|(name, range)| {
                            pycodestyle::rules::ambiguous_variable_name(name, *range)
                        }));
                }
            }
            StmtKind::Nonlocal { names } => {
                let scope_index = *self.scope_stack.last().expect("No current scope found");
                let ranges = helpers::find_names(stmt, self.locator);
                if scope_index != GLOBAL_SCOPE_INDEX {
                    let context = self.execution_context();
                    let scope = &mut self.scopes[scope_index];
                    let usage = Some((scope.id, Range::from_located(stmt)));
                    for (name, range) in names.iter().zip(ranges.iter()) {
                        // Add a binding to the current scope.
                        let index = self.bindings.len();
                        self.bindings.push(Binding {
                            kind: BindingKind::Nonlocal,
                            runtime_usage: None,
                            synthetic_usage: usage,
                            typing_usage: None,
                            range: *range,
                            source: Some(RefEquality(stmt)),
                            context,
                        });
                        scope.bindings.insert(name, index);
                    }

                    // Mark the binding in the defining scopes as used too. (Skip the global scope
                    // and the current scope.)
                    for (name, range) in names.iter().zip(ranges.iter()) {
                        let mut exists = false;
                        for index in self.scope_stack.iter().skip(1).rev().skip(1) {
                            if let Some(index) = self.scopes[*index].bindings.get(&name.as_str()) {
                                exists = true;
                                self.bindings[*index].runtime_usage = usage;
                            }
                        }

                        // Ensure that every nonlocal has an existing binding from a parent scope.
                        if !exists {
                            if self.settings.rules.enabled(&Rule::NonlocalWithoutBinding) {
                                self.diagnostics.push(Diagnostic::new(
                                    pylint::rules::NonlocalWithoutBinding {
                                        name: name.to_string(),
                                    },
                                    *range,
                                ));
                            }
                        }
                    }
                }

                if self.settings.rules.enabled(&Rule::AmbiguousVariableName) {
                    self.diagnostics
                        .extend(names.iter().zip(ranges.iter()).filter_map(|(name, range)| {
                            pycodestyle::rules::ambiguous_variable_name(name, *range)
                        }));
                }
            }
            StmtKind::Break => {
                if self.settings.rules.enabled(&Rule::BreakOutsideLoop) {
                    if let Some(diagnostic) = pyflakes::rules::break_outside_loop(
                        stmt,
                        &mut self
                            .parents
                            .iter()
                            .rev()
                            .map(std::convert::Into::into)
                            .skip(1),
                    ) {
                        self.diagnostics.push(diagnostic);
                    }
                }
            }
            StmtKind::Continue => {
                if self.settings.rules.enabled(&Rule::ContinueOutsideLoop) {
                    if let Some(diagnostic) = pyflakes::rules::continue_outside_loop(
                        stmt,
                        &mut self
                            .parents
                            .iter()
                            .rev()
                            .map(std::convert::Into::into)
                            .skip(1),
                    ) {
                        self.diagnostics.push(diagnostic);
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
                if self.settings.rules.enabled(&Rule::AmbiguousFunctionName) {
                    if let Some(diagnostic) =
                        pycodestyle::rules::ambiguous_function_name(name, || {
                            helpers::identifier_range(stmt, self.locator)
                        })
                    {
                        self.diagnostics.push(diagnostic);
                    }
                }

                if self.settings.rules.enabled(&Rule::InvalidFunctionName) {
                    if let Some(diagnostic) = pep8_naming::rules::invalid_function_name(
                        stmt,
                        name,
                        &self.settings.pep8_naming.ignore_names,
                        self.locator,
                    ) {
                        self.diagnostics.push(diagnostic);
                    }
                }

                if self
                    .settings
                    .rules
                    .enabled(&Rule::InvalidFirstArgumentNameForClassMethod)
                {
                    if let Some(diagnostic) =
                        pep8_naming::rules::invalid_first_argument_name_for_class_method(
                            self,
                            self.current_scope(),
                            name,
                            decorator_list,
                            args,
                        )
                    {
                        self.diagnostics.push(diagnostic);
                    }
                }

                if self
                    .settings
                    .rules
                    .enabled(&Rule::InvalidFirstArgumentNameForMethod)
                {
                    if let Some(diagnostic) =
                        pep8_naming::rules::invalid_first_argument_name_for_method(
                            self,
                            self.current_scope(),
                            name,
                            decorator_list,
                            args,
                        )
                    {
                        self.diagnostics.push(diagnostic);
                    }
                }

                if self.settings.rules.enabled(&Rule::DunderFunctionName) {
                    if let Some(diagnostic) = pep8_naming::rules::dunder_function_name(
                        self.current_scope(),
                        stmt,
                        name,
                        self.locator,
                    ) {
                        self.diagnostics.push(diagnostic);
                    }
                }

                if self
                    .settings
                    .rules
                    .enabled(&Rule::LRUCacheWithoutParameters)
                    && self.settings.target_version >= PythonVersion::Py38
                {
                    pyupgrade::rules::lru_cache_without_parameters(self, decorator_list);
                }
                if self.settings.rules.enabled(&Rule::FunctoolsCache)
                    && self.settings.target_version >= PythonVersion::Py39
                {
                    pyupgrade::rules::functools_cache(self, decorator_list);
                }

                if self.settings.rules.enabled(&Rule::UselessExpression) {
                    flake8_bugbear::rules::useless_expression(self, body);
                }

                if self.settings.rules.enabled(&Rule::CachedInstanceMethod) {
                    flake8_bugbear::rules::cached_instance_method(self, decorator_list);
                }

                if self.settings.rules.enabled(&Rule::UnnecessaryReturnNone)
                    || self.settings.rules.enabled(&Rule::ImplicitReturnValue)
                    || self.settings.rules.enabled(&Rule::ImplicitReturn)
                    || self.settings.rules.enabled(&Rule::UnnecessaryAssign)
                    || self.settings.rules.enabled(&Rule::SuperfluousElseReturn)
                    || self.settings.rules.enabled(&Rule::SuperfluousElseRaise)
                    || self.settings.rules.enabled(&Rule::SuperfluousElseContinue)
                    || self.settings.rules.enabled(&Rule::SuperfluousElseBreak)
                {
                    flake8_return::rules::function(self, body);
                }

                if self.settings.rules.enabled(&Rule::FunctionIsTooComplex) {
                    if let Some(diagnostic) = mccabe::rules::function_is_too_complex(
                        stmt,
                        name,
                        body,
                        self.settings.mccabe.max_complexity,
                        self.locator,
                    ) {
                        self.diagnostics.push(diagnostic);
                    }
                }

                if self.settings.rules.enabled(&Rule::HardcodedPasswordDefault) {
                    self.diagnostics
                        .extend(flake8_bandit::rules::hardcoded_password_default(args));
                }

                if self.settings.rules.enabled(&Rule::PropertyWithParameters) {
                    pylint::rules::property_with_parameters(self, stmt, decorator_list, args);
                }

                if self.settings.rules.enabled(&Rule::TooManyArguments) {
                    pylint::rules::too_many_arguments(self, args, stmt);
                }

                if self.settings.rules.enabled(&Rule::TooManyReturnStatements) {
                    if let Some(diagnostic) = pylint::rules::too_many_return_statements(
                        stmt,
                        body,
                        self.settings.pylint.max_returns,
                        self.locator,
                    ) {
                        self.diagnostics.push(diagnostic);
                    }
                }

                if self.settings.rules.enabled(&Rule::TooManyBranches) {
                    if let Some(diagnostic) = pylint::rules::too_many_branches(
                        stmt,
                        body,
                        self.settings.pylint.max_branches,
                        self.locator,
                    ) {
                        self.diagnostics.push(diagnostic);
                    }
                }

                if self.settings.rules.enabled(&Rule::TooManyStatements) {
                    if let Some(diagnostic) = pylint::rules::too_many_statements(
                        stmt,
                        body,
                        self.settings.pylint.max_statements,
                        self.locator,
                    ) {
                        self.diagnostics.push(diagnostic);
                    }
                }

                if self
                    .settings
                    .rules
                    .enabled(&Rule::IncorrectFixtureParenthesesStyle)
                    || self.settings.rules.enabled(&Rule::FixturePositionalArgs)
                    || self.settings.rules.enabled(&Rule::ExtraneousScopeFunction)
                    || self
                        .settings
                        .rules
                        .enabled(&Rule::MissingFixtureNameUnderscore)
                    || self
                        .settings
                        .rules
                        .enabled(&Rule::IncorrectFixtureNameUnderscore)
                    || self.settings.rules.enabled(&Rule::FixtureParamWithoutValue)
                    || self.settings.rules.enabled(&Rule::DeprecatedYieldFixture)
                    || self.settings.rules.enabled(&Rule::FixtureFinalizerCallback)
                    || self.settings.rules.enabled(&Rule::UselessYieldFixture)
                    || self
                        .settings
                        .rules
                        .enabled(&Rule::UnnecessaryAsyncioMarkOnFixture)
                    || self
                        .settings
                        .rules
                        .enabled(&Rule::ErroneousUseFixturesOnFixture)
                {
                    flake8_pytest_style::rules::fixture(
                        self,
                        stmt,
                        name,
                        args,
                        decorator_list,
                        body,
                    );
                }

                if self
                    .settings
                    .rules
                    .enabled(&Rule::ParametrizeNamesWrongType)
                    || self
                        .settings
                        .rules
                        .enabled(&Rule::ParametrizeValuesWrongType)
                {
                    flake8_pytest_style::rules::parametrize(self, decorator_list);
                }

                if self
                    .settings
                    .rules
                    .enabled(&Rule::IncorrectMarkParenthesesStyle)
                    || self
                        .settings
                        .rules
                        .enabled(&Rule::UseFixturesWithoutParameters)
                {
                    flake8_pytest_style::rules::marks(self, decorator_list);
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

                let context = self.execution_context();
                self.add_binding(
                    name,
                    Binding {
                        kind: BindingKind::FunctionDefinition,
                        runtime_usage: None,
                        synthetic_usage: None,
                        typing_usage: None,
                        range: Range::from_located(stmt),
                        source: Some(self.current_stmt().clone()),
                        context,
                    },
                );
            }
            StmtKind::Return { .. } => {
                if self.settings.rules.enabled(&Rule::ReturnOutsideFunction) {
                    pyflakes::rules::return_outside_function(self, stmt);
                }
            }
            StmtKind::ClassDef {
                name,
                bases,
                keywords,
                decorator_list,
                body,
            } => {
                if self.settings.rules.enabled(&Rule::UselessObjectInheritance) {
                    pyupgrade::rules::useless_object_inheritance(self, stmt, name, bases, keywords);
                }

                if self.settings.rules.enabled(&Rule::AmbiguousClassName) {
                    if let Some(diagnostic) = pycodestyle::rules::ambiguous_class_name(name, || {
                        helpers::identifier_range(stmt, self.locator)
                    }) {
                        self.diagnostics.push(diagnostic);
                    }
                }

                if self.settings.rules.enabled(&Rule::InvalidClassName) {
                    if let Some(diagnostic) =
                        pep8_naming::rules::invalid_class_name(stmt, name, self.locator)
                    {
                        self.diagnostics.push(diagnostic);
                    }
                }

                if self
                    .settings
                    .rules
                    .enabled(&Rule::ErrorSuffixOnExceptionName)
                {
                    if let Some(diagnostic) = pep8_naming::rules::error_suffix_on_exception_name(
                        stmt,
                        bases,
                        name,
                        self.locator,
                    ) {
                        self.diagnostics.push(diagnostic);
                    }
                }

                if self.settings.rules.enabled(&Rule::UselessExpression) {
                    flake8_bugbear::rules::useless_expression(self, body);
                }

                if self
                    .settings
                    .rules
                    .enabled(&Rule::AbstractBaseClassWithoutAbstractMethod)
                    || self
                        .settings
                        .rules
                        .enabled(&Rule::EmptyMethodWithoutAbstractDecorator)
                {
                    flake8_bugbear::rules::abstract_base_class(
                        self, stmt, name, bases, keywords, body,
                    );
                }

                if self
                    .settings
                    .rules
                    .enabled(&Rule::IncorrectMarkParenthesesStyle)
                {
                    flake8_pytest_style::rules::marks(self, decorator_list);
                }

                if self
                    .settings
                    .rules
                    .enabled(&Rule::DupeClassFieldDefinitions)
                {
                    flake8_pie::rules::dupe_class_field_definitions(self, stmt, body);
                }

                if self.settings.rules.enabled(&Rule::PreferUniqueEnums) {
                    flake8_pie::rules::prefer_unique_enums(self, stmt, body);
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
                if self.settings.rules.enabled(&Rule::MultipleImportsOnOneLine) {
                    pycodestyle::rules::multiple_imports_on_one_line(self, stmt, names);
                }
                if self
                    .settings
                    .rules
                    .enabled(&Rule::ModuleImportNotAtTopOfFile)
                {
                    pycodestyle::rules::module_import_not_at_top_of_file(self, stmt);
                }
                if self.settings.rules.enabled(&Rule::RewriteCElementTree) {
                    pyupgrade::rules::replace_c_element_tree(self, stmt);
                }
                if self.settings.rules.enabled(&Rule::RewriteMockImport) {
                    pyupgrade::rules::rewrite_mock_import(self, stmt);
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
                                kind: BindingKind::SubmoduleImportation(name, full_name),
                                runtime_usage: None,
                                synthetic_usage: None,
                                typing_usage: None,
                                range: Range::from_located(alias),
                                source: Some(self.current_stmt().clone()),
                                context: self.execution_context(),
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
                                kind: BindingKind::Importation(name, full_name),
                                runtime_usage: None,
                                // Treat explicit re-export as usage (e.g., `import applications
                                // as applications`).
                                synthetic_usage: if alias
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
                                typing_usage: None,
                                range: Range::from_located(alias),
                                source: Some(self.current_stmt().clone()),
                                context: self.execution_context(),
                            },
                        );
                    }

                    // flake8-debugger
                    if self.settings.rules.enabled(&Rule::Debugger) {
                        if let Some(diagnostic) =
                            flake8_debugger::rules::debugger_import(stmt, None, &alias.node.name)
                        {
                            self.diagnostics.push(diagnostic);
                        }
                    }

                    // flake8_tidy_imports
                    if self.settings.rules.enabled(&Rule::BannedApi) {
                        if let Some(diagnostic) =
                            flake8_tidy_imports::banned_api::name_or_parent_is_banned(
                                alias,
                                &alias.node.name,
                                &self.settings.flake8_tidy_imports.banned_api,
                            )
                        {
                            self.diagnostics.push(diagnostic);
                        }
                    }

                    // pylint
                    if self.settings.rules.enabled(&Rule::UselessImportAlias) {
                        pylint::rules::useless_import_alias(self, alias);
                    }
                    if self.settings.rules.enabled(&Rule::ConsiderUsingFromImport) {
                        pylint::rules::use_from_import(self, stmt, alias, names);
                    }

                    if let Some(asname) = &alias.node.asname {
                        let name = alias.node.name.split('.').last().unwrap();
                        if self
                            .settings
                            .rules
                            .enabled(&Rule::ConstantImportedAsNonConstant)
                        {
                            if let Some(diagnostic) =
                                pep8_naming::rules::constant_imported_as_non_constant(
                                    stmt,
                                    name,
                                    asname,
                                    self.locator,
                                )
                            {
                                self.diagnostics.push(diagnostic);
                            }
                        }

                        if self
                            .settings
                            .rules
                            .enabled(&Rule::LowercaseImportedAsNonLowercase)
                        {
                            if let Some(diagnostic) =
                                pep8_naming::rules::lowercase_imported_as_non_lowercase(
                                    stmt,
                                    name,
                                    asname,
                                    self.locator,
                                )
                            {
                                self.diagnostics.push(diagnostic);
                            }
                        }

                        if self
                            .settings
                            .rules
                            .enabled(&Rule::CamelcaseImportedAsLowercase)
                        {
                            if let Some(diagnostic) =
                                pep8_naming::rules::camelcase_imported_as_lowercase(
                                    stmt,
                                    name,
                                    asname,
                                    self.locator,
                                )
                            {
                                self.diagnostics.push(diagnostic);
                            }
                        }

                        if self
                            .settings
                            .rules
                            .enabled(&Rule::CamelcaseImportedAsConstant)
                        {
                            if let Some(diagnostic) =
                                pep8_naming::rules::camelcase_imported_as_constant(
                                    stmt,
                                    name,
                                    asname,
                                    self.locator,
                                )
                            {
                                self.diagnostics.push(diagnostic);
                            }
                        }

                        if self
                            .settings
                            .rules
                            .enabled(&Rule::CamelcaseImportedAsAcronym)
                        {
                            if let Some(diagnostic) =
                                pep8_naming::rules::camelcase_imported_as_acronym(
                                    stmt,
                                    name,
                                    asname,
                                    self.locator,
                                )
                            {
                                self.diagnostics.push(diagnostic);
                            }
                        }
                    }

                    if self
                        .settings
                        .rules
                        .enabled(&Rule::ImportAliasIsNotConventional)
                    {
                        if let Some(diagnostic) =
                            flake8_import_conventions::rules::check_conventional_import(
                                stmt,
                                &alias.node.name,
                                alias.node.asname.as_deref(),
                                &self.settings.flake8_import_conventions.aliases,
                            )
                        {
                            self.diagnostics.push(diagnostic);
                        }
                    }

                    if self.settings.rules.enabled(&Rule::IncorrectPytestImport) {
                        if let Some(diagnostic) = flake8_pytest_style::rules::import(
                            stmt,
                            &alias.node.name,
                            alias.node.asname.as_deref(),
                        ) {
                            self.diagnostics.push(diagnostic);
                        }
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
                    .rules
                    .enabled(&Rule::ModuleImportNotAtTopOfFile)
                {
                    pycodestyle::rules::module_import_not_at_top_of_file(self, stmt);
                }

                if self.settings.rules.enabled(&Rule::UnnecessaryFutureImport)
                    && self.settings.target_version >= PythonVersion::Py37
                {
                    if let Some("__future__") = module.as_deref() {
                        pyupgrade::rules::unnecessary_future_import(self, stmt, names);
                    }
                }
                if self.settings.rules.enabled(&Rule::RewriteMockImport) {
                    pyupgrade::rules::rewrite_mock_import(self, stmt);
                }
                if self.settings.rules.enabled(&Rule::RewriteCElementTree) {
                    pyupgrade::rules::replace_c_element_tree(self, stmt);
                }
                if self.settings.rules.enabled(&Rule::ImportReplacements) {
                    pyupgrade::rules::import_replacements(
                        self,
                        stmt,
                        names,
                        module.as_ref().map(String::as_str),
                        level.as_ref(),
                    );
                }
                if self.settings.rules.enabled(&Rule::UnnecessaryBuiltinImport) {
                    if let Some(module) = module.as_deref() {
                        pyupgrade::rules::unnecessary_builtin_import(self, stmt, module, names);
                    }
                }

                if self.settings.rules.enabled(&Rule::BannedApi) {
                    if let Some(module) = module {
                        for name in names {
                            if let Some(diagnostic) =
                                flake8_tidy_imports::banned_api::name_is_banned(
                                    module,
                                    name,
                                    &self.settings.flake8_tidy_imports.banned_api,
                                )
                            {
                                self.diagnostics.push(diagnostic);
                            }
                        }
                        if let Some(diagnostic) =
                            flake8_tidy_imports::banned_api::name_or_parent_is_banned(
                                stmt,
                                module,
                                &self.settings.flake8_tidy_imports.banned_api,
                            )
                        {
                            self.diagnostics.push(diagnostic);
                        }
                    }
                }

                if self.settings.rules.enabled(&Rule::IncorrectPytestImport) {
                    if let Some(diagnostic) = flake8_pytest_style::rules::import_from(
                        stmt,
                        module.as_deref(),
                        level.as_ref(),
                    ) {
                        self.diagnostics.push(diagnostic);
                    }
                }

                for alias in names {
                    if let Some("__future__") = module.as_deref() {
                        let name = alias.node.asname.as_ref().unwrap_or(&alias.node.name);
                        self.add_binding(
                            name,
                            Binding {
                                kind: BindingKind::FutureImportation,
                                runtime_usage: None,
                                // Always mark `__future__` imports as used.
                                synthetic_usage: Some((
                                    self.scopes[*(self
                                        .scope_stack
                                        .last()
                                        .expect("No current scope found"))]
                                    .id,
                                    Range::from_located(alias),
                                )),
                                typing_usage: None,
                                range: Range::from_located(alias),
                                source: Some(self.current_stmt().clone()),
                                context: self.execution_context(),
                            },
                        );

                        if alias.node.name == "annotations" {
                            self.annotations_future_enabled = true;
                        }

                        if self.settings.rules.enabled(&Rule::FutureFeatureNotDefined) {
                            pyflakes::rules::future_feature_not_defined(self, alias);
                        }

                        if self.settings.rules.enabled(&Rule::LateFutureImport)
                            && !self.futures_allowed
                        {
                            self.diagnostics.push(Diagnostic::new(
                                pyflakes::rules::LateFutureImport,
                                Range::from_located(stmt),
                            ));
                        }
                    } else if alias.node.name == "*" {
                        self.add_binding(
                            "*",
                            Binding {
                                kind: BindingKind::StarImportation(*level, module.clone()),
                                runtime_usage: None,
                                synthetic_usage: None,
                                typing_usage: None,
                                range: Range::from_located(stmt),
                                source: Some(self.current_stmt().clone()),
                                context: self.execution_context(),
                            },
                        );

                        if self.settings.rules.enabled(&Rule::ImportStarNotPermitted) {
                            let scope = &self.scopes
                                [*(self.scope_stack.last().expect("No current scope found"))];
                            if !matches!(scope.kind, ScopeKind::Module) {
                                self.diagnostics.push(Diagnostic::new(
                                    pyflakes::rules::ImportStarNotPermitted {
                                        name: helpers::format_import_from(
                                            level.as_ref(),
                                            module.as_deref(),
                                        ),
                                    },
                                    Range::from_located(stmt),
                                ));
                            }
                        }

                        if self.settings.rules.enabled(&Rule::ImportStarUsed) {
                            self.diagnostics.push(Diagnostic::new(
                                pyflakes::rules::ImportStarUsed {
                                    name: helpers::format_import_from(
                                        level.as_ref(),
                                        module.as_deref(),
                                    ),
                                },
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
                        let full_name = helpers::format_import_from_member(
                            level.as_ref(),
                            module.as_deref(),
                            &alias.node.name,
                        );
                        let range = Range::from_located(alias);
                        self.add_binding(
                            name,
                            Binding {
                                kind: BindingKind::FromImportation(name, full_name),
                                runtime_usage: None,
                                // Treat explicit re-export as usage (e.g., `from .applications
                                // import FastAPI as FastAPI`).
                                synthetic_usage: if alias
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
                                typing_usage: None,
                                range,
                                source: Some(self.current_stmt().clone()),
                                context: self.execution_context(),
                            },
                        );
                    }

                    if self.settings.rules.enabled(&Rule::RelativeImports) {
                        if let Some(diagnostic) =
                            flake8_tidy_imports::relative_imports::banned_relative_import(
                                stmt,
                                level.as_ref(),
                                &self.settings.flake8_tidy_imports.ban_relative_imports,
                            )
                        {
                            self.diagnostics.push(diagnostic);
                        }
                    }

                    // flake8-debugger
                    if self.settings.rules.enabled(&Rule::Debugger) {
                        if let Some(diagnostic) = flake8_debugger::rules::debugger_import(
                            stmt,
                            module.as_deref(),
                            &alias.node.name,
                        ) {
                            self.diagnostics.push(diagnostic);
                        }
                    }

                    if self
                        .settings
                        .rules
                        .enabled(&Rule::ImportAliasIsNotConventional)
                    {
                        let full_name = helpers::format_import_from_member(
                            level.as_ref(),
                            module.as_deref(),
                            &alias.node.name,
                        );
                        if let Some(diagnostic) =
                            flake8_import_conventions::rules::check_conventional_import(
                                stmt,
                                &full_name,
                                alias.node.asname.as_deref(),
                                &self.settings.flake8_import_conventions.aliases,
                            )
                        {
                            self.diagnostics.push(diagnostic);
                        }
                    }

                    if let Some(asname) = &alias.node.asname {
                        if self
                            .settings
                            .rules
                            .enabled(&Rule::ConstantImportedAsNonConstant)
                        {
                            if let Some(diagnostic) =
                                pep8_naming::rules::constant_imported_as_non_constant(
                                    stmt,
                                    &alias.node.name,
                                    asname,
                                    self.locator,
                                )
                            {
                                self.diagnostics.push(diagnostic);
                            }
                        }

                        if self
                            .settings
                            .rules
                            .enabled(&Rule::LowercaseImportedAsNonLowercase)
                        {
                            if let Some(diagnostic) =
                                pep8_naming::rules::lowercase_imported_as_non_lowercase(
                                    stmt,
                                    &alias.node.name,
                                    asname,
                                    self.locator,
                                )
                            {
                                self.diagnostics.push(diagnostic);
                            }
                        }

                        if self
                            .settings
                            .rules
                            .enabled(&Rule::CamelcaseImportedAsLowercase)
                        {
                            if let Some(diagnostic) =
                                pep8_naming::rules::camelcase_imported_as_lowercase(
                                    stmt,
                                    &alias.node.name,
                                    asname,
                                    self.locator,
                                )
                            {
                                self.diagnostics.push(diagnostic);
                            }
                        }

                        if self
                            .settings
                            .rules
                            .enabled(&Rule::CamelcaseImportedAsConstant)
                        {
                            if let Some(diagnostic) =
                                pep8_naming::rules::camelcase_imported_as_constant(
                                    stmt,
                                    &alias.node.name,
                                    asname,
                                    self.locator,
                                )
                            {
                                self.diagnostics.push(diagnostic);
                            }
                        }

                        if self
                            .settings
                            .rules
                            .enabled(&Rule::CamelcaseImportedAsAcronym)
                        {
                            if let Some(diagnostic) =
                                pep8_naming::rules::camelcase_imported_as_acronym(
                                    stmt,
                                    &alias.node.name,
                                    asname,
                                    self.locator,
                                )
                            {
                                self.diagnostics.push(diagnostic);
                            }
                        }

                        // pylint
                        if self.settings.rules.enabled(&Rule::UselessImportAlias) {
                            pylint::rules::useless_import_alias(self, alias);
                        }
                    }
                }
            }
            StmtKind::Raise { exc, .. } => {
                if self.settings.rules.enabled(&Rule::RaiseNotImplemented) {
                    if let Some(expr) = exc {
                        pyflakes::rules::raise_not_implemented(self, expr);
                    }
                }
                if self.settings.rules.enabled(&Rule::CannotRaiseLiteral) {
                    if let Some(exc) = exc {
                        flake8_bugbear::rules::cannot_raise_literal(self, exc);
                    }
                }
                if self.settings.rules.enabled(&Rule::RawStringInException)
                    || self.settings.rules.enabled(&Rule::FStringInException)
                    || self.settings.rules.enabled(&Rule::DotFormatInException)
                {
                    if let Some(exc) = exc {
                        flake8_errmsg::rules::string_in_exception(self, exc);
                    }
                }
                if self.settings.rules.enabled(&Rule::OSErrorAlias) {
                    if let Some(item) = exc {
                        pyupgrade::rules::os_error_alias(self, &item);
                    }
                }
                if self.settings.rules.enabled(&Rule::RaiseVanillaClass) {
                    if let Some(expr) = exc {
                        tryceratops::rules::raise_vanilla_class(self, expr);
                    }
                }
                if self.settings.rules.enabled(&Rule::RaiseVanillaArgs) {
                    if let Some(expr) = exc {
                        tryceratops::rules::raise_vanilla_args(self, expr);
                    }
                }
                if self
                    .settings
                    .rules
                    .enabled(&Rule::UnnecessaryParenOnRaiseException)
                {
                    if let Some(expr) = exc {
                        flake8_raise::rules::unnecessary_paren_on_raise_exception(self, expr);
                    }
                }
            }
            StmtKind::AugAssign { target, .. } => {
                self.handle_node_load(target);
            }
            StmtKind::If { test, body, orelse } => {
                if self.settings.rules.enabled(&Rule::IfTuple) {
                    pyflakes::rules::if_tuple(self, stmt, test);
                }
                if self.settings.rules.enabled(&Rule::NestedIfStatements) {
                    flake8_simplify::rules::nested_if_statements(
                        self,
                        stmt,
                        test,
                        body,
                        orelse,
                        self.current_stmt_parent().map(Into::into),
                    );
                }
                if self
                    .settings
                    .rules
                    .enabled(&Rule::ReturnBoolConditionDirectly)
                {
                    flake8_simplify::rules::return_bool_condition_directly(self, stmt);
                }
                if self.settings.rules.enabled(&Rule::UseTernaryOperator) {
                    flake8_simplify::rules::use_ternary_operator(
                        self,
                        stmt,
                        self.current_stmt_parent().map(std::convert::Into::into),
                    );
                }
                if self.settings.rules.enabled(&Rule::DictGetWithDefault) {
                    flake8_simplify::rules::use_dict_get_with_default(
                        self,
                        stmt,
                        test,
                        body,
                        orelse,
                        self.current_stmt_parent().map(std::convert::Into::into),
                    );
                }
                if self.settings.rules.enabled(&Rule::PreferTypeError) {
                    tryceratops::rules::prefer_type_error(
                        self,
                        body,
                        test,
                        orelse,
                        self.current_stmt_parent().map(Into::into),
                    );
                }
                if self.settings.rules.enabled(&Rule::OutdatedVersionBlock) {
                    pyupgrade::rules::outdated_version_block(self, stmt, test, body, orelse);
                }
            }
            StmtKind::Assert { test, msg } => {
                if self.settings.rules.enabled(&Rule::AssertTuple) {
                    pyflakes::rules::assert_tuple(self, stmt, test);
                }
                if self.settings.rules.enabled(&Rule::DoNotAssertFalse) {
                    flake8_bugbear::rules::assert_false(
                        self,
                        stmt,
                        test,
                        msg.as_ref().map(|expr| &**expr),
                    );
                }
                if self.settings.rules.enabled(&Rule::AssertUsed) {
                    self.diagnostics
                        .push(flake8_bandit::rules::assert_used(stmt));
                }
                if self.settings.rules.enabled(&Rule::AssertAlwaysFalse) {
                    if let Some(diagnostic) = flake8_pytest_style::rules::assert_falsy(stmt, test) {
                        self.diagnostics.push(diagnostic);
                    }
                }
                if self.settings.rules.enabled(&Rule::CompositeAssertion) {
                    if let Some(diagnostic) =
                        flake8_pytest_style::rules::composite_condition(stmt, test)
                    {
                        self.diagnostics.push(diagnostic);
                    }
                }
            }
            StmtKind::With { items, body, .. } => {
                if self.settings.rules.enabled(&Rule::NoAssertRaisesException) {
                    flake8_bugbear::rules::assert_raises_exception(self, stmt, items);
                }
                if self
                    .settings
                    .rules
                    .enabled(&Rule::RaisesWithMultipleStatements)
                {
                    flake8_pytest_style::rules::complex_raises(self, stmt, items, body);
                }
                if self.settings.rules.enabled(&Rule::MultipleWithStatements) {
                    flake8_simplify::rules::multiple_with_statements(
                        self,
                        stmt,
                        body,
                        self.current_stmt_parent().map(Into::into),
                    );
                }
            }
            StmtKind::While { body, orelse, .. } => {
                if self.settings.rules.enabled(&Rule::FunctionUsesLoopVariable) {
                    flake8_bugbear::rules::function_uses_loop_variable(self, &Node::Stmt(stmt));
                }
                if self.settings.rules.enabled(&Rule::UselessElseOnLoop) {
                    pylint::rules::useless_else_on_loop(self, stmt, body, orelse);
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
                if self
                    .settings
                    .rules
                    .enabled(&Rule::UnusedLoopControlVariable)
                {
                    self.deferred_for_loops
                        .push((stmt, (self.scope_stack.clone(), self.parents.clone())));
                }
                if self
                    .settings
                    .rules
                    .enabled(&Rule::LoopVariableOverridesIterator)
                {
                    flake8_bugbear::rules::loop_variable_overrides_iterator(self, target, iter);
                }
                if self.settings.rules.enabled(&Rule::FunctionUsesLoopVariable) {
                    flake8_bugbear::rules::function_uses_loop_variable(self, &Node::Stmt(stmt));
                }
                if self.settings.rules.enabled(&Rule::UselessElseOnLoop) {
                    pylint::rules::useless_else_on_loop(self, stmt, body, orelse);
                }
                if matches!(stmt.node, StmtKind::For { .. }) {
                    if self.settings.rules.enabled(&Rule::ConvertLoopToAny)
                        || self.settings.rules.enabled(&Rule::ConvertLoopToAll)
                    {
                        flake8_simplify::rules::convert_for_loop_to_any_all(
                            self,
                            stmt,
                            self.current_sibling_stmt(),
                        );
                    }
                    if self.settings.rules.enabled(&Rule::KeyInDict) {
                        flake8_simplify::rules::key_in_dict_for(self, target, iter);
                    }
                }
            }
            StmtKind::Try {
                body,
                handlers,
                orelse,
                finalbody,
                ..
            } => {
                if self.settings.rules.enabled(&Rule::DefaultExceptNotLast) {
                    if let Some(diagnostic) =
                        pyflakes::rules::default_except_not_last(handlers, self.locator)
                    {
                        self.diagnostics.push(diagnostic);
                    }
                }
                if self
                    .settings
                    .rules
                    .enabled(&Rule::DuplicateHandlerException)
                    || self
                        .settings
                        .rules
                        .enabled(&Rule::DuplicateTryBlockException)
                {
                    flake8_bugbear::rules::duplicate_exceptions(self, handlers);
                }
                if self
                    .settings
                    .rules
                    .enabled(&Rule::RedundantTupleInExceptionHandler)
                {
                    flake8_bugbear::rules::redundant_tuple_in_exception_handler(self, handlers);
                }
                if self.settings.rules.enabled(&Rule::OSErrorAlias) {
                    pyupgrade::rules::os_error_alias(self, &handlers);
                }
                if self.settings.rules.enabled(&Rule::AssertInExcept) {
                    self.diagnostics.extend(
                        flake8_pytest_style::rules::assert_in_exception_handler(handlers),
                    );
                }
                if self.settings.rules.enabled(&Rule::UseContextlibSuppress) {
                    flake8_simplify::rules::use_contextlib_suppress(
                        self, stmt, body, handlers, orelse, finalbody,
                    );
                }
                if self.settings.rules.enabled(&Rule::ReturnInTryExceptFinally) {
                    flake8_simplify::rules::return_in_try_except_finally(
                        self, body, handlers, finalbody,
                    );
                }
                if self.settings.rules.enabled(&Rule::TryConsiderElse) {
                    tryceratops::rules::try_consider_else(self, body, orelse);
                }
                if self.settings.rules.enabled(&Rule::VerboseRaise) {
                    tryceratops::rules::verbose_raise(self, handlers);
                }
                if self.settings.rules.enabled(&Rule::RaiseWithinTry) {
                    tryceratops::rules::raise_within_try(self, body);
                }
                if self.settings.rules.enabled(&Rule::ErrorInsteadOfException) {
                    tryceratops::rules::error_instead_of_exception(self, handlers);
                }
            }
            StmtKind::Assign { targets, value, .. } => {
                if self.settings.rules.enabled(&Rule::DoNotAssignLambda) {
                    if let [target] = &targets[..] {
                        pycodestyle::rules::do_not_assign_lambda(self, target, value, stmt);
                    }
                }

                if self.settings.rules.enabled(&Rule::AssignmentToOsEnviron) {
                    flake8_bugbear::rules::assignment_to_os_environ(self, targets);
                }

                if self.settings.rules.enabled(&Rule::HardcodedPasswordString) {
                    if let Some(diagnostic) =
                        flake8_bandit::rules::assign_hardcoded_password_string(value, targets)
                    {
                        self.diagnostics.push(diagnostic);
                    }
                }

                if self.settings.rules.enabled(&Rule::UselessMetaclassType) {
                    pyupgrade::rules::useless_metaclass_type(self, stmt, value, targets);
                }
                if self
                    .settings
                    .rules
                    .enabled(&Rule::ConvertTypedDictFunctionalToClass)
                {
                    pyupgrade::rules::convert_typed_dict_functional_to_class(
                        self, stmt, targets, value,
                    );
                }
                if self
                    .settings
                    .rules
                    .enabled(&Rule::ConvertNamedTupleFunctionalToClass)
                {
                    pyupgrade::rules::convert_named_tuple_functional_to_class(
                        self, stmt, targets, value,
                    );
                }
                if self.settings.rules.enabled(&Rule::RewriteListComprehension) {
                    pyupgrade::rules::unpack_list_comprehension(self, targets, value);
                }

                if self.settings.rules.enabled(&Rule::DfIsABadVariableName) {
                    if let Some(diagnostic) = pandas_vet::rules::assignment_to_df(targets) {
                        self.diagnostics.push(diagnostic);
                    }
                }
            }
            StmtKind::AnnAssign { target, value, .. } => {
                if self.settings.rules.enabled(&Rule::DoNotAssignLambda) {
                    if let Some(value) = value {
                        pycodestyle::rules::do_not_assign_lambda(self, target, value, stmt);
                    }
                }
            }
            StmtKind::Delete { .. } => {}
            StmtKind::Expr { value, .. } => {
                if self.settings.rules.enabled(&Rule::UselessComparison) {
                    flake8_bugbear::rules::useless_comparison(self, value);
                }
                if self
                    .settings
                    .rules
                    .enabled(&Rule::UseCapitalEnvironmentVariables)
                {
                    flake8_simplify::rules::use_capital_environment_variables(self, value);
                }
            }
            _ => {}
        }

        // Recurse.
        let prev_in_exception_handler = self.in_exception_handler;
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
                if self.settings.rules.enabled(&Rule::FStringDocstring) {
                    flake8_bugbear::rules::f_string_docstring(self, body);
                }
                let definition = docstrings::extraction::extract(
                    &self.visible_scope,
                    stmt,
                    body,
                    &Documentable::Function,
                );
                if self.settings.rules.enabled(&Rule::RewriteYieldFrom) {
                    pyupgrade::rules::rewrite_yield_from(self, stmt);
                }
                let scope = transition_scope(&self.visible_scope, stmt, &Documentable::Function);
                self.definitions.push((
                    definition,
                    scope.visibility.clone(),
                    (self.scope_stack.clone(), self.parents.clone()),
                ));
                self.visible_scope = scope;

                // If any global bindings don't already exist in the global scope, add it.
                let globals = operations::extract_globals(body);
                for (name, stmt) in operations::extract_globals(body) {
                    if self.scopes[GLOBAL_SCOPE_INDEX]
                        .bindings
                        .get(name)
                        .map_or(true, |index| {
                            matches!(self.bindings[*index].kind, BindingKind::Annotation)
                        })
                    {
                        let index = self.bindings.len();
                        self.bindings.push(Binding {
                            kind: BindingKind::Assignment,
                            runtime_usage: None,
                            synthetic_usage: None,
                            typing_usage: None,
                            range: Range::from_located(stmt),
                            source: Some(RefEquality(stmt)),
                            context: self.execution_context(),
                        });
                        self.scopes[GLOBAL_SCOPE_INDEX].bindings.insert(name, index);
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
                if self.settings.rules.enabled(&Rule::FStringDocstring) {
                    flake8_bugbear::rules::f_string_docstring(self, body);
                }
                let definition = docstrings::extraction::extract(
                    &self.visible_scope,
                    stmt,
                    body,
                    &Documentable::Class,
                );
                let scope = transition_scope(&self.visible_scope, stmt, &Documentable::Class);
                self.definitions.push((
                    definition,
                    scope.visibility.clone(),
                    (self.scope_stack.clone(), self.parents.clone()),
                ));
                self.visible_scope = scope;

                // If any global bindings don't already exist in the global scope, add it.
                let globals = operations::extract_globals(body);
                for (name, stmt) in &globals {
                    if self.scopes[GLOBAL_SCOPE_INDEX]
                        .bindings
                        .get(name)
                        .map_or(true, |index| {
                            matches!(self.bindings[*index].kind, BindingKind::Annotation)
                        })
                    {
                        let index = self.bindings.len();
                        self.bindings.push(Binding {
                            kind: BindingKind::Assignment,
                            runtime_usage: None,
                            synthetic_usage: None,
                            typing_usage: None,
                            range: Range::from_located(stmt),
                            source: Some(RefEquality(stmt)),
                            context: self.execution_context(),
                        });
                        self.scopes[GLOBAL_SCOPE_INDEX].bindings.insert(name, index);
                    }
                }

                self.push_scope(Scope::new(ScopeKind::Class(ClassDef {
                    name,
                    bases,
                    keywords,
                    decorator_list,
                    globals,
                })));

                self.visit_body(body);
            }
            StmtKind::Try {
                body,
                handlers,
                orelse,
                finalbody,
            } => {
                // TODO(charlie): The use of `smallvec` here leads to a lifetime issue.
                let handler_names = extract_handler_names(handlers)
                    .into_iter()
                    .map(|call_path| call_path.to_vec())
                    .collect();
                self.except_handlers.push(handler_names);
                if self.settings.rules.enabled(&Rule::JumpStatementInFinally) {
                    flake8_bugbear::rules::jump_statement_in_finally(self, finalbody);
                }
                self.visit_body(body);
                self.except_handlers.pop();

                self.in_exception_handler = true;
                for excepthandler in handlers {
                    self.visit_excepthandler(excepthandler);
                }
                self.in_exception_handler = prev_in_exception_handler;

                self.visit_body(orelse);
                self.visit_body(finalbody);
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
            StmtKind::If { test, body, orelse } => {
                self.visit_expr(test);

                if flake8_type_checking::helpers::is_type_checking_block(self, test) {
                    if self.settings.rules.enabled(&Rule::EmptyTypeCheckingBlock) {
                        flake8_type_checking::rules::empty_type_checking_block(self, stmt, body);
                    }

                    let prev_in_type_checking_block = self.in_type_checking_block;
                    self.in_type_checking_block = true;
                    self.visit_body(body);
                    self.in_type_checking_block = prev_in_type_checking_block;
                } else {
                    self.visit_body(body);
                }

                self.visit_body(orelse);
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
                        runtime_usage: None,
                        synthetic_usage: None,
                        typing_usage: None,
                        range: Range::from_located(stmt),
                        source: Some(self.current_stmt().clone()),
                        context: self.execution_context(),
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

        let prev_in_literal = self.in_literal;
        let prev_in_type_definition = self.in_type_definition;

        // Pre-visit.
        match &expr.node {
            ExprKind::Subscript { value, slice, .. } => {
                // Ex) Optional[...]
                if !self.in_deferred_string_type_definition
                    && !self.settings.pyupgrade.keep_runtime_typing
                    && self.settings.rules.enabled(&Rule::UsePEP604Annotation)
                    && (self.settings.target_version >= PythonVersion::Py310
                        || (self.settings.target_version >= PythonVersion::Py37
                            && self.annotations_future_enabled
                            && self.in_annotation))
                {
                    pyupgrade::rules::use_pep604_annotation(self, expr, value, slice);
                }

                if self.match_typing_expr(value, "Literal") {
                    self.in_literal = true;
                }

                if self
                    .settings
                    .rules
                    .enabled(&Rule::SysVersionSlice3Referenced)
                    || self.settings.rules.enabled(&Rule::SysVersion2Referenced)
                    || self.settings.rules.enabled(&Rule::SysVersion0Referenced)
                    || self
                        .settings
                        .rules
                        .enabled(&Rule::SysVersionSlice1Referenced)
                {
                    flake8_2020::rules::subscript(self, value, slice);
                }
            }
            ExprKind::Tuple { elts, ctx } | ExprKind::List { elts, ctx } => {
                if matches!(ctx, ExprContext::Store) {
                    let check_too_many_expressions = self
                        .settings
                        .rules
                        .enabled(&Rule::ExpressionsInStarAssignment);
                    let check_two_starred_expressions =
                        self.settings.rules.enabled(&Rule::TwoStarredExpressions);
                    if let Some(diagnostic) = pyflakes::rules::starred_expressions(
                        elts,
                        check_too_many_expressions,
                        check_two_starred_expressions,
                        Range::from_located(expr),
                    ) {
                        self.diagnostics.push(diagnostic);
                    }
                }
            }
            ExprKind::Name { id, ctx } => {
                match ctx {
                    ExprContext::Load => {
                        if self.settings.rules.enabled(&Rule::TypingTextStrAlias) {
                            pyupgrade::rules::typing_text_str_alias(self, expr);
                        }

                        // Ex) List[...]
                        if !self.in_deferred_string_type_definition
                            && !self.settings.pyupgrade.keep_runtime_typing
                            && self.settings.rules.enabled(&Rule::UsePEP585Annotation)
                            && (self.settings.target_version >= PythonVersion::Py39
                                || (self.settings.target_version >= PythonVersion::Py37
                                    && self.annotations_future_enabled
                                    && self.in_annotation))
                            && typing::is_pep585_builtin(expr, |expr| self.resolve_call_path(expr))
                        {
                            pyupgrade::rules::use_pep585_annotation(self, expr);
                        }

                        self.handle_node_load(expr);
                    }
                    ExprContext::Store => {
                        if self.settings.rules.enabled(&Rule::AmbiguousVariableName) {
                            if let Some(diagnostic) = pycodestyle::rules::ambiguous_variable_name(
                                id,
                                Range::from_located(expr),
                            ) {
                                self.diagnostics.push(diagnostic);
                            }
                        }

                        self.check_builtin_shadowing(id, expr, true);

                        self.handle_node_store(id, expr);
                    }
                    ExprContext::Del => self.handle_node_delete(expr),
                }

                if self.settings.rules.enabled(&Rule::SixPY3Referenced) {
                    flake8_2020::rules::name_or_attribute(self, expr);
                }

                if self
                    .settings
                    .rules
                    .enabled(&Rule::UsedPriorGlobalDeclaration)
                {
                    pylint::rules::used_prior_global_declaration(self, id, expr);
                }
            }
            ExprKind::Attribute { attr, value, .. } => {
                // Ex) typing.List[...]
                if !self.in_deferred_string_type_definition
                    && !self.settings.pyupgrade.keep_runtime_typing
                    && self.settings.rules.enabled(&Rule::UsePEP585Annotation)
                    && (self.settings.target_version >= PythonVersion::Py39
                        || (self.settings.target_version >= PythonVersion::Py37
                            && self.annotations_future_enabled
                            && self.in_annotation))
                    && typing::is_pep585_builtin(expr, |expr| self.resolve_call_path(expr))
                {
                    pyupgrade::rules::use_pep585_annotation(self, expr);
                }
                if self.settings.rules.enabled(&Rule::DatetimeTimezoneUTC)
                    && self.settings.target_version >= PythonVersion::Py311
                {
                    pyupgrade::rules::datetime_utc_alias(self, expr);
                }
                if self.settings.rules.enabled(&Rule::TypingTextStrAlias) {
                    pyupgrade::rules::typing_text_str_alias(self, expr);
                }
                if self.settings.rules.enabled(&Rule::RewriteMockImport) {
                    pyupgrade::rules::rewrite_mock_attribute(self, expr);
                }
                if self.settings.rules.enabled(&Rule::SixPY3Referenced) {
                    flake8_2020::rules::name_or_attribute(self, expr);
                }
                if self.settings.rules.enabled(&Rule::BannedApi) {
                    flake8_tidy_imports::banned_api::banned_attribute_access(self, expr);
                }
                if self.settings.rules.enabled(&Rule::PrivateMemberAccess) {
                    flake8_self::rules::private_member_access(self, expr);
                }
                pandas_vet::rules::check_attr(self, attr, value, expr);
            }
            ExprKind::Call {
                func,
                args,
                keywords,
            } => {
                // pyflakes
                if self.settings.rules.enabled(&Rule::StringDotFormatInvalidFormat)
                    || self.settings.rules.enabled(&Rule::StringDotFormatExtraNamedArguments)
                    || self.settings.rules.enabled(&Rule::StringDotFormatExtraPositionalArguments)
                    || self.settings.rules.enabled(&Rule::StringDotFormatMissingArguments)
                    || self.settings.rules.enabled(&Rule::StringDotFormatMixingAutomatic)
                    // pyupgrade
                    || self.settings.rules.enabled(&Rule::FormatLiterals)
                    || self.settings.rules.enabled(&Rule::FString)
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
                                        if self
                                            .settings
                                            .rules
                                            .enabled(&Rule::StringDotFormatInvalidFormat)
                                        {
                                            self.diagnostics.push(Diagnostic::new(
                                                pyflakes::rules::StringDotFormatInvalidFormat {
                                                    message: pyflakes::format::error_to_string(&e),
                                                },
                                                location,
                                            ));
                                        }
                                    }
                                    Ok(summary) => {
                                        if self
                                            .settings
                                            .rules
                                            .enabled(&Rule::StringDotFormatExtraNamedArguments)
                                        {
                                            pyflakes::rules::string_dot_format_extra_named_arguments(
                                                self, &summary, keywords, location,
                                            );
                                        }

                                        if self
                                            .settings
                                            .rules
                                            .enabled(&Rule::StringDotFormatExtraPositionalArguments)
                                        {
                                            pyflakes::rules::string_dot_format_extra_positional_arguments(
                                                self,
                                                &summary, args, location,
                                            );
                                        }

                                        if self
                                            .settings
                                            .rules
                                            .enabled(&Rule::StringDotFormatMissingArguments)
                                        {
                                            pyflakes::rules::string_dot_format_missing_argument(
                                                self, &summary, args, keywords, location,
                                            );
                                        }

                                        if self
                                            .settings
                                            .rules
                                            .enabled(&Rule::StringDotFormatMixingAutomatic)
                                        {
                                            pyflakes::rules::string_dot_format_mixing_automatic(
                                                self, &summary, location,
                                            );
                                        }

                                        if self.settings.rules.enabled(&Rule::FormatLiterals) {
                                            pyupgrade::rules::format_literals(self, &summary, expr);
                                        }

                                        if self.settings.rules.enabled(&Rule::FString) {
                                            pyupgrade::rules::f_strings(self, &summary, expr);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                // pyupgrade
                if self.settings.rules.enabled(&Rule::TypeOfPrimitive) {
                    pyupgrade::rules::type_of_primitive(self, expr, func, args);
                }
                if self.settings.rules.enabled(&Rule::DeprecatedUnittestAlias) {
                    pyupgrade::rules::deprecated_unittest_alias(self, func);
                }
                if self.settings.rules.enabled(&Rule::SuperCallWithParameters) {
                    pyupgrade::rules::super_call_with_parameters(self, expr, func, args);
                }
                if self.settings.rules.enabled(&Rule::UnnecessaryEncodeUTF8) {
                    pyupgrade::rules::unnecessary_encode_utf8(self, expr, func, args, keywords);
                }
                if self.settings.rules.enabled(&Rule::RedundantOpenModes) {
                    pyupgrade::rules::redundant_open_modes(self, expr);
                }
                if self.settings.rules.enabled(&Rule::NativeLiterals) {
                    pyupgrade::rules::native_literals(self, expr, func, args, keywords);
                }
                if self.settings.rules.enabled(&Rule::OpenAlias) {
                    pyupgrade::rules::open_alias(self, expr, func);
                }
                if self.settings.rules.enabled(&Rule::ReplaceUniversalNewlines) {
                    pyupgrade::rules::replace_universal_newlines(self, expr, keywords);
                }
                if self.settings.rules.enabled(&Rule::ReplaceStdoutStderr) {
                    pyupgrade::rules::replace_stdout_stderr(self, expr, keywords);
                }
                if self.settings.rules.enabled(&Rule::OSErrorAlias) {
                    pyupgrade::rules::os_error_alias(self, &expr);
                }

                // flake8-print
                if self.settings.rules.enabled(&Rule::PrintFound)
                    || self.settings.rules.enabled(&Rule::PPrintFound)
                {
                    flake8_print::rules::print_call(self, func, keywords);
                }

                // flake8-bugbear
                if self.settings.rules.enabled(&Rule::UnreliableCallableCheck) {
                    flake8_bugbear::rules::unreliable_callable_check(self, expr, func, args);
                }
                if self.settings.rules.enabled(&Rule::StripWithMultiCharacters) {
                    flake8_bugbear::rules::strip_with_multi_characters(self, expr, func, args);
                }
                if self.settings.rules.enabled(&Rule::GetAttrWithConstant) {
                    flake8_bugbear::rules::getattr_with_constant(self, expr, func, args);
                }
                if self.settings.rules.enabled(&Rule::SetAttrWithConstant) {
                    flake8_bugbear::rules::setattr_with_constant(self, expr, func, args);
                }
                if self
                    .settings
                    .rules
                    .enabled(&Rule::UselessContextlibSuppress)
                {
                    flake8_bugbear::rules::useless_contextlib_suppress(self, expr, args);
                }
                if self
                    .settings
                    .rules
                    .enabled(&Rule::StarArgUnpackingAfterKeywordArg)
                {
                    flake8_bugbear::rules::star_arg_unpacking_after_keyword_arg(
                        self, args, keywords,
                    );
                }
                if self.settings.rules.enabled(&Rule::ZipWithoutExplicitStrict)
                    && self.settings.target_version >= PythonVersion::Py310
                {
                    flake8_bugbear::rules::zip_without_explicit_strict(self, expr, func, keywords);
                }

                // flake8-pie
                if self.settings.rules.enabled(&Rule::NoUnnecessaryDictKwargs) {
                    flake8_pie::rules::no_unnecessary_dict_kwargs(self, expr, keywords);
                }

                // flake8-bandit
                if self.settings.rules.enabled(&Rule::ExecUsed) {
                    if let Some(diagnostic) = flake8_bandit::rules::exec_used(expr, func) {
                        self.diagnostics.push(diagnostic);
                    }
                }
                if self.settings.rules.enabled(&Rule::BadFilePermissions) {
                    flake8_bandit::rules::bad_file_permissions(self, func, args, keywords);
                }
                if self
                    .settings
                    .rules
                    .enabled(&Rule::RequestWithNoCertValidation)
                {
                    flake8_bandit::rules::request_with_no_cert_validation(
                        self, func, args, keywords,
                    );
                }
                if self.settings.rules.enabled(&Rule::UnsafeYAMLLoad) {
                    flake8_bandit::rules::unsafe_yaml_load(self, func, args, keywords);
                }
                if self.settings.rules.enabled(&Rule::SnmpInsecureVersion) {
                    flake8_bandit::rules::snmp_insecure_version(self, func, args, keywords);
                }
                if self.settings.rules.enabled(&Rule::SnmpWeakCryptography) {
                    flake8_bandit::rules::snmp_weak_cryptography(self, func, args, keywords);
                }
                if self.settings.rules.enabled(&Rule::Jinja2AutoescapeFalse) {
                    flake8_bandit::rules::jinja2_autoescape_false(self, func, args, keywords);
                }
                if self.settings.rules.enabled(&Rule::HardcodedPasswordFuncArg) {
                    self.diagnostics
                        .extend(flake8_bandit::rules::hardcoded_password_func_arg(keywords));
                }
                if self
                    .settings
                    .rules
                    .enabled(&Rule::HashlibInsecureHashFunction)
                {
                    flake8_bandit::rules::hashlib_insecure_hash_functions(
                        self, func, args, keywords,
                    );
                }
                if self.settings.rules.enabled(&Rule::RequestWithoutTimeout) {
                    flake8_bandit::rules::request_without_timeout(self, func, args, keywords);
                }
                if self
                    .settings
                    .rules
                    .enabled(&Rule::LoggingConfigInsecureListen)
                {
                    flake8_bandit::rules::logging_config_insecure_listen(
                        self, func, args, keywords,
                    );
                }

                // flake8-comprehensions
                if self.settings.rules.enabled(&Rule::UnnecessaryGeneratorList) {
                    flake8_comprehensions::rules::unnecessary_generator_list(
                        self, expr, func, args, keywords,
                    );
                }
                if self.settings.rules.enabled(&Rule::UnnecessaryGeneratorSet) {
                    flake8_comprehensions::rules::unnecessary_generator_set(
                        self, expr, func, args, keywords,
                    );
                }
                if self.settings.rules.enabled(&Rule::UnnecessaryGeneratorDict) {
                    flake8_comprehensions::rules::unnecessary_generator_dict(
                        self, expr, func, args, keywords,
                    );
                }
                if self
                    .settings
                    .rules
                    .enabled(&Rule::UnnecessaryListComprehensionSet)
                {
                    flake8_comprehensions::rules::unnecessary_list_comprehension_set(
                        self, expr, func, args, keywords,
                    );
                }
                if self
                    .settings
                    .rules
                    .enabled(&Rule::UnnecessaryListComprehensionDict)
                {
                    flake8_comprehensions::rules::unnecessary_list_comprehension_dict(
                        self, expr, func, args, keywords,
                    );
                }
                if self.settings.rules.enabled(&Rule::UnnecessaryLiteralSet) {
                    flake8_comprehensions::rules::unnecessary_literal_set(
                        self, expr, func, args, keywords,
                    );
                }
                if self.settings.rules.enabled(&Rule::UnnecessaryLiteralDict) {
                    flake8_comprehensions::rules::unnecessary_literal_dict(
                        self, expr, func, args, keywords,
                    );
                }
                if self
                    .settings
                    .rules
                    .enabled(&Rule::UnnecessaryCollectionCall)
                {
                    flake8_comprehensions::rules::unnecessary_collection_call(
                        self, expr, func, args, keywords,
                    );
                }
                if self
                    .settings
                    .rules
                    .enabled(&Rule::UnnecessaryLiteralWithinTupleCall)
                {
                    flake8_comprehensions::rules::unnecessary_literal_within_tuple_call(
                        self, expr, func, args,
                    );
                }
                if self
                    .settings
                    .rules
                    .enabled(&Rule::UnnecessaryLiteralWithinListCall)
                {
                    flake8_comprehensions::rules::unnecessary_literal_within_list_call(
                        self, expr, func, args,
                    );
                }
                if self.settings.rules.enabled(&Rule::UnnecessaryListCall) {
                    flake8_comprehensions::rules::unnecessary_list_call(self, expr, func, args);
                }
                if self
                    .settings
                    .rules
                    .enabled(&Rule::UnnecessaryCallAroundSorted)
                {
                    flake8_comprehensions::rules::unnecessary_call_around_sorted(
                        self, expr, func, args,
                    );
                }
                if self
                    .settings
                    .rules
                    .enabled(&Rule::UnnecessaryDoubleCastOrProcess)
                {
                    flake8_comprehensions::rules::unnecessary_double_cast_or_process(
                        self, expr, func, args,
                    );
                }
                if self
                    .settings
                    .rules
                    .enabled(&Rule::UnnecessarySubscriptReversal)
                {
                    flake8_comprehensions::rules::unnecessary_subscript_reversal(
                        self, expr, func, args,
                    );
                }
                if self.settings.rules.enabled(&Rule::UnnecessaryMap) {
                    flake8_comprehensions::rules::unnecessary_map(self, expr, func, args);
                }

                // flake8-boolean-trap
                if self
                    .settings
                    .rules
                    .enabled(&Rule::BooleanPositionalValueInFunctionCall)
                {
                    flake8_boolean_trap::rules::check_boolean_positional_value_in_function_call(
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
                if self.settings.rules.enabled(&Rule::Debugger) {
                    flake8_debugger::rules::debugger_call(self, expr, func);
                }

                // pandas-vet
                if self.settings.rules.enabled(&Rule::UseOfInplaceArgument) {
                    self.diagnostics.extend(
                        pandas_vet::rules::inplace_argument(self, expr, args, keywords).into_iter(),
                    );
                }
                pandas_vet::rules::check_call(self, func);

                if self.settings.rules.enabled(&Rule::UseOfPdMerge) {
                    if let Some(diagnostic) = pandas_vet::rules::use_of_pd_merge(func) {
                        self.diagnostics.push(diagnostic);
                    };
                }

                // flake8-datetimez
                if self
                    .settings
                    .rules
                    .enabled(&Rule::CallDatetimeWithoutTzinfo)
                {
                    flake8_datetimez::rules::call_datetime_without_tzinfo(
                        self,
                        func,
                        args,
                        keywords,
                        Range::from_located(expr),
                    );
                }
                if self.settings.rules.enabled(&Rule::CallDatetimeToday) {
                    flake8_datetimez::rules::call_datetime_today(
                        self,
                        func,
                        Range::from_located(expr),
                    );
                }
                if self.settings.rules.enabled(&Rule::CallDatetimeUtcnow) {
                    flake8_datetimez::rules::call_datetime_utcnow(
                        self,
                        func,
                        Range::from_located(expr),
                    );
                }
                if self
                    .settings
                    .rules
                    .enabled(&Rule::CallDatetimeUtcfromtimestamp)
                {
                    flake8_datetimez::rules::call_datetime_utcfromtimestamp(
                        self,
                        func,
                        Range::from_located(expr),
                    );
                }
                if self
                    .settings
                    .rules
                    .enabled(&Rule::CallDatetimeNowWithoutTzinfo)
                {
                    flake8_datetimez::rules::call_datetime_now_without_tzinfo(
                        self,
                        func,
                        args,
                        keywords,
                        Range::from_located(expr),
                    );
                }
                if self
                    .settings
                    .rules
                    .enabled(&Rule::CallDatetimeFromtimestamp)
                {
                    flake8_datetimez::rules::call_datetime_fromtimestamp(
                        self,
                        func,
                        args,
                        keywords,
                        Range::from_located(expr),
                    );
                }
                if self
                    .settings
                    .rules
                    .enabled(&Rule::CallDatetimeStrptimeWithoutZone)
                {
                    flake8_datetimez::rules::call_datetime_strptime_without_zone(
                        self,
                        func,
                        args,
                        Range::from_located(expr),
                    );
                }
                if self.settings.rules.enabled(&Rule::CallDateToday) {
                    flake8_datetimez::rules::call_date_today(self, func, Range::from_located(expr));
                }
                if self.settings.rules.enabled(&Rule::CallDateFromtimestamp) {
                    flake8_datetimez::rules::call_date_fromtimestamp(
                        self,
                        func,
                        Range::from_located(expr),
                    );
                }

                // pygrep-hooks
                if self.settings.rules.enabled(&Rule::NoEval) {
                    pygrep_hooks::rules::no_eval(self, func);
                }
                if self.settings.rules.enabled(&Rule::DeprecatedLogWarn) {
                    pygrep_hooks::rules::deprecated_log_warn(self, func);
                }

                // pylint
                if self
                    .settings
                    .rules
                    .enabled(&Rule::UnnecessaryDirectLambdaCall)
                {
                    pylint::rules::unnecessary_direct_lambda_call(self, expr, func);
                }
                if self.settings.rules.enabled(&Rule::ConsiderUsingSysExit) {
                    pylint::rules::consider_using_sys_exit(self, func);
                }

                // flake8-pytest-style
                if self.settings.rules.enabled(&Rule::PatchWithLambda) {
                    if let Some(diagnostic) =
                        flake8_pytest_style::rules::patch_with_lambda(func, args, keywords)
                    {
                        self.diagnostics.push(diagnostic);
                    }
                }
                if self.settings.rules.enabled(&Rule::UnittestAssertion) {
                    if let Some(diagnostic) = flake8_pytest_style::rules::unittest_assertion(
                        self, expr, func, args, keywords,
                    ) {
                        self.diagnostics.push(diagnostic);
                    }
                }

                if self.settings.rules.enabled(&Rule::RaisesWithoutException)
                    || self.settings.rules.enabled(&Rule::RaisesTooBroad)
                {
                    flake8_pytest_style::rules::raises_call(self, func, args, keywords);
                }

                if self.settings.rules.enabled(&Rule::FailWithoutMessage) {
                    flake8_pytest_style::rules::fail_call(self, func, args, keywords);
                }

                // ruff
                if self
                    .settings
                    .rules
                    .enabled(&Rule::KeywordArgumentBeforeStarArgument)
                {
                    self.diagnostics
                        .extend(ruff::rules::keyword_argument_before_star_argument(
                            args, keywords,
                        ));
                }

                // flake8-simplify
                if self
                    .settings
                    .rules
                    .enabled(&Rule::OpenFileWithContextHandler)
                {
                    flake8_simplify::rules::open_file_with_context_handler(self, func);
                }

                // flake8-use-pathlib
                if self.settings.rules.enabled(&Rule::PathlibAbspath)
                    || self.settings.rules.enabled(&Rule::PathlibChmod)
                    || self.settings.rules.enabled(&Rule::PathlibMkdir)
                    || self.settings.rules.enabled(&Rule::PathlibMakedirs)
                    || self.settings.rules.enabled(&Rule::PathlibRename)
                    || self.settings.rules.enabled(&Rule::PathlibReplace)
                    || self.settings.rules.enabled(&Rule::PathlibRmdir)
                    || self.settings.rules.enabled(&Rule::PathlibRemove)
                    || self.settings.rules.enabled(&Rule::PathlibUnlink)
                    || self.settings.rules.enabled(&Rule::PathlibGetcwd)
                    || self.settings.rules.enabled(&Rule::PathlibExists)
                    || self.settings.rules.enabled(&Rule::PathlibExpanduser)
                    || self.settings.rules.enabled(&Rule::PathlibIsDir)
                    || self.settings.rules.enabled(&Rule::PathlibIsFile)
                    || self.settings.rules.enabled(&Rule::PathlibIsLink)
                    || self.settings.rules.enabled(&Rule::PathlibReadlink)
                    || self.settings.rules.enabled(&Rule::PathlibStat)
                    || self.settings.rules.enabled(&Rule::PathlibIsAbs)
                    || self.settings.rules.enabled(&Rule::PathlibJoin)
                    || self.settings.rules.enabled(&Rule::PathlibBasename)
                    || self.settings.rules.enabled(&Rule::PathlibSamefile)
                    || self.settings.rules.enabled(&Rule::PathlibSplitext)
                    || self.settings.rules.enabled(&Rule::PathlibOpen)
                    || self.settings.rules.enabled(&Rule::PathlibPyPath)
                {
                    flake8_use_pathlib::helpers::replaceable_by_pathlib(self, func);
                }

                // flake8-logging-format
                if self.settings.rules.enabled(&Rule::LoggingStringFormat)
                    || self.settings.rules.enabled(&Rule::LoggingPercentFormat)
                    || self.settings.rules.enabled(&Rule::LoggingStringConcat)
                    || self.settings.rules.enabled(&Rule::LoggingFString)
                    || self.settings.rules.enabled(&Rule::LoggingWarn)
                    || self.settings.rules.enabled(&Rule::LoggingExtraAttrClash)
                    || self.settings.rules.enabled(&Rule::LoggingExcInfo)
                    || self.settings.rules.enabled(&Rule::LoggingRedundantExcInfo)
                {
                    flake8_logging_format::rules::logging_call(self, func, args, keywords);
                }
            }
            ExprKind::Dict { keys, values } => {
                if self
                    .settings
                    .rules
                    .enabled(&Rule::MultiValueRepeatedKeyLiteral)
                    || self
                        .settings
                        .rules
                        .enabled(&Rule::MultiValueRepeatedKeyVariable)
                {
                    pyflakes::rules::repeated_keys(self, keys, values);
                }

                if self.settings.rules.enabled(&Rule::NoUnnecessarySpread) {
                    flake8_pie::rules::no_unnecessary_spread(self, keys, values);
                }
            }
            ExprKind::Yield { .. } => {
                if self.settings.rules.enabled(&Rule::YieldOutsideFunction) {
                    pyflakes::rules::yield_outside_function(self, expr);
                }
            }
            ExprKind::YieldFrom { .. } => {
                if self.settings.rules.enabled(&Rule::YieldOutsideFunction) {
                    pyflakes::rules::yield_outside_function(self, expr);
                }
            }
            ExprKind::Await { .. } => {
                if self.settings.rules.enabled(&Rule::YieldOutsideFunction) {
                    pyflakes::rules::yield_outside_function(self, expr);
                }
                if self.settings.rules.enabled(&Rule::AwaitOutsideAsync) {
                    pylint::rules::await_outside_async(self, expr);
                }
            }
            ExprKind::JoinedStr { values } => {
                if self
                    .settings
                    .rules
                    .enabled(&Rule::FStringMissingPlaceholders)
                {
                    pyflakes::rules::f_string_missing_placeholders(expr, values, self);
                }
            }
            ExprKind::BinOp {
                left,
                op: Operator::RShift,
                ..
            } => {
                if self.settings.rules.enabled(&Rule::InvalidPrintSyntax) {
                    pyflakes::rules::invalid_print_syntax(self, left);
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
                    if self
                        .settings
                        .rules
                        .enabled(&Rule::PercentFormatInvalidFormat)
                        || self
                            .settings
                            .rules
                            .enabled(&Rule::PercentFormatExpectedMapping)
                        || self
                            .settings
                            .rules
                            .enabled(&Rule::PercentFormatExpectedSequence)
                        || self
                            .settings
                            .rules
                            .enabled(&Rule::PercentFormatExtraNamedArguments)
                        || self
                            .settings
                            .rules
                            .enabled(&Rule::PercentFormatMissingArgument)
                        || self
                            .settings
                            .rules
                            .enabled(&Rule::PercentFormatMixedPositionalAndNamed)
                        || self
                            .settings
                            .rules
                            .enabled(&Rule::PercentFormatPositionalCountMismatch)
                        || self
                            .settings
                            .rules
                            .enabled(&Rule::PercentFormatStarRequiresSequence)
                        || self
                            .settings
                            .rules
                            .enabled(&Rule::PercentFormatUnsupportedFormatCharacter)
                    {
                        let location = Range::from_located(expr);
                        match pyflakes::cformat::CFormatSummary::try_from(value.as_str()) {
                            Err(CFormatError {
                                typ: CFormatErrorType::UnsupportedFormatChar(c),
                                ..
                            }) => {
                                if self
                                    .settings
                                    .rules
                                    .enabled(&Rule::PercentFormatUnsupportedFormatCharacter)
                                {
                                    self.diagnostics.push(Diagnostic::new(
                                        pyflakes::rules::PercentFormatUnsupportedFormatCharacter {
                                            char: c,
                                        },
                                        location,
                                    ));
                                }
                            }
                            Err(e) => {
                                if self
                                    .settings
                                    .rules
                                    .enabled(&Rule::PercentFormatInvalidFormat)
                                {
                                    self.diagnostics.push(Diagnostic::new(
                                        pyflakes::rules::PercentFormatInvalidFormat {
                                            message: e.to_string(),
                                        },
                                        location,
                                    ));
                                }
                            }
                            Ok(summary) => {
                                if self
                                    .settings
                                    .rules
                                    .enabled(&Rule::PercentFormatExpectedMapping)
                                {
                                    pyflakes::rules::percent_format_expected_mapping(
                                        self, &summary, right, location,
                                    );
                                }
                                if self
                                    .settings
                                    .rules
                                    .enabled(&Rule::PercentFormatExpectedSequence)
                                {
                                    pyflakes::rules::percent_format_expected_sequence(
                                        self, &summary, right, location,
                                    );
                                }
                                if self
                                    .settings
                                    .rules
                                    .enabled(&Rule::PercentFormatExtraNamedArguments)
                                {
                                    pyflakes::rules::percent_format_extra_named_arguments(
                                        self, &summary, right, location,
                                    );
                                }
                                if self
                                    .settings
                                    .rules
                                    .enabled(&Rule::PercentFormatMissingArgument)
                                {
                                    pyflakes::rules::percent_format_missing_arguments(
                                        self, &summary, right, location,
                                    );
                                }
                                if self
                                    .settings
                                    .rules
                                    .enabled(&Rule::PercentFormatMixedPositionalAndNamed)
                                {
                                    pyflakes::rules::percent_format_mixed_positional_and_named(
                                        self, &summary, location,
                                    );
                                }
                                if self
                                    .settings
                                    .rules
                                    .enabled(&Rule::PercentFormatPositionalCountMismatch)
                                {
                                    pyflakes::rules::percent_format_positional_count_mismatch(
                                        self, &summary, right, location,
                                    );
                                }
                                if self
                                    .settings
                                    .rules
                                    .enabled(&Rule::PercentFormatStarRequiresSequence)
                                {
                                    pyflakes::rules::percent_format_star_requires_sequence(
                                        self, &summary, right, location,
                                    );
                                }
                            }
                        }
                    }

                    if self.settings.rules.enabled(&Rule::PrintfStringFormatting) {
                        pyupgrade::rules::printf_string_formatting(self, expr, left, right);
                    }
                }
            }
            ExprKind::BinOp {
                op: Operator::Add, ..
            } => {
                if self
                    .settings
                    .rules
                    .enabled(&Rule::ExplicitStringConcatenation)
                {
                    if let Some(diagnostic) = flake8_implicit_str_concat::rules::explicit(expr) {
                        self.diagnostics.push(diagnostic);
                    }
                }
                if self
                    .settings
                    .rules
                    .enabled(&Rule::UnpackInsteadOfConcatenatingToCollectionLiteral)
                {
                    ruff::rules::unpack_instead_of_concatenating_to_collection_literal(self, expr);
                }
            }
            ExprKind::UnaryOp { op, operand } => {
                let check_not_in = self.settings.rules.enabled(&Rule::NotInTest);
                let check_not_is = self.settings.rules.enabled(&Rule::NotIsTest);
                if check_not_in || check_not_is {
                    pycodestyle::rules::not_tests(
                        self,
                        expr,
                        op,
                        operand,
                        check_not_in,
                        check_not_is,
                    );
                }

                if self.settings.rules.enabled(&Rule::UnaryPrefixIncrement) {
                    flake8_bugbear::rules::unary_prefix_increment(self, expr, op, operand);
                }

                if self.settings.rules.enabled(&Rule::NegateEqualOp) {
                    flake8_simplify::rules::negation_with_equal_op(self, expr, op, operand);
                }
                if self.settings.rules.enabled(&Rule::NegateNotEqualOp) {
                    flake8_simplify::rules::negation_with_not_equal_op(self, expr, op, operand);
                }
                if self.settings.rules.enabled(&Rule::DoubleNegation) {
                    flake8_simplify::rules::double_negation(self, expr, op, operand);
                }
            }
            ExprKind::Compare {
                left,
                ops,
                comparators,
            } => {
                let check_none_comparisons = self.settings.rules.enabled(&Rule::NoneComparison);
                let check_true_false_comparisons =
                    self.settings.rules.enabled(&Rule::TrueFalseComparison);
                if check_none_comparisons || check_true_false_comparisons {
                    pycodestyle::rules::literal_comparisons(
                        self,
                        expr,
                        left,
                        ops,
                        comparators,
                        check_none_comparisons,
                        check_true_false_comparisons,
                    );
                }

                if self.settings.rules.enabled(&Rule::IsLiteral) {
                    pyflakes::rules::invalid_literal_comparison(
                        self,
                        left,
                        ops,
                        comparators,
                        Range::from_located(expr),
                    );
                }

                if self.settings.rules.enabled(&Rule::TypeComparison) {
                    self.diagnostics.extend(pycodestyle::rules::type_comparison(
                        ops,
                        comparators,
                        Range::from_located(expr),
                    ));
                }

                if self.settings.rules.enabled(&Rule::SysVersionCmpStr3)
                    || self
                        .settings
                        .rules
                        .enabled(&Rule::SysVersionInfo0Eq3Referenced)
                    || self.settings.rules.enabled(&Rule::SysVersionInfo1CmpInt)
                    || self
                        .settings
                        .rules
                        .enabled(&Rule::SysVersionInfoMinorCmpInt)
                    || self.settings.rules.enabled(&Rule::SysVersionCmpStr10)
                {
                    flake8_2020::rules::compare(self, left, ops, comparators);
                }

                if self.settings.rules.enabled(&Rule::HardcodedPasswordString) {
                    self.diagnostics.extend(
                        flake8_bandit::rules::compare_to_hardcoded_password_string(
                            left,
                            comparators,
                        ),
                    );
                }

                if self.settings.rules.enabled(&Rule::ComparisonOfConstant) {
                    pylint::rules::comparison_of_constant(self, left, ops, comparators);
                }

                if self.settings.rules.enabled(&Rule::MagicValueComparison) {
                    pylint::rules::magic_value_comparison(self, left, comparators);
                }

                if self.settings.rules.enabled(&Rule::KeyInDict) {
                    flake8_simplify::rules::key_in_dict_compare(self, expr, left, ops, comparators);
                }

                if self.settings.rules.enabled(&Rule::YodaConditions) {
                    flake8_simplify::rules::yoda_conditions(self, expr, left, ops, comparators);
                }
            }
            ExprKind::Constant {
                value: Constant::Str(value),
                kind,
            } => {
                if self.in_type_definition && !self.in_literal {
                    self.deferred_string_type_definitions.push((
                        Range::from_located(expr),
                        value,
                        self.in_annotation,
                        (self.scope_stack.clone(), self.parents.clone()),
                    ));
                }
                if self
                    .settings
                    .rules
                    .enabled(&Rule::HardcodedBindAllInterfaces)
                {
                    if let Some(diagnostic) = flake8_bandit::rules::hardcoded_bind_all_interfaces(
                        value,
                        &Range::from_located(expr),
                    ) {
                        self.diagnostics.push(diagnostic);
                    }
                }
                if self.settings.rules.enabled(&Rule::HardcodedTempFile) {
                    if let Some(diagnostic) = flake8_bandit::rules::hardcoded_tmp_directory(
                        expr,
                        value,
                        &self.settings.flake8_bandit.hardcoded_tmp_directory,
                    ) {
                        self.diagnostics.push(diagnostic);
                    }
                }
                if self.settings.rules.enabled(&Rule::RewriteUnicodeLiteral) {
                    pyupgrade::rules::rewrite_unicode_literal(self, expr, kind.as_deref());
                }
            }
            ExprKind::Lambda { args, body, .. } => {
                if self.settings.rules.enabled(&Rule::PreferListBuiltin) {
                    flake8_pie::rules::prefer_list_builtin(self, expr);
                }

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
            ExprKind::IfExp { test, body, orelse } => {
                if self.settings.rules.enabled(&Rule::IfExprWithTrueFalse) {
                    flake8_simplify::rules::explicit_true_false_in_ifexpr(
                        self, expr, test, body, orelse,
                    );
                }
                if self.settings.rules.enabled(&Rule::IfExprWithFalseTrue) {
                    flake8_simplify::rules::explicit_false_true_in_ifexpr(
                        self, expr, test, body, orelse,
                    );
                }
                if self.settings.rules.enabled(&Rule::IfExprWithTwistedArms) {
                    flake8_simplify::rules::twisted_arms_in_ifexpr(self, expr, test, body, orelse);
                }
            }
            ExprKind::ListComp { elt, generators } | ExprKind::SetComp { elt, generators } => {
                if self.settings.rules.enabled(&Rule::UnnecessaryComprehension) {
                    flake8_comprehensions::rules::unnecessary_comprehension(
                        self, expr, elt, generators,
                    );
                }
                if self.settings.rules.enabled(&Rule::FunctionUsesLoopVariable) {
                    flake8_bugbear::rules::function_uses_loop_variable(self, &Node::Expr(expr));
                }
                self.push_scope(Scope::new(ScopeKind::Generator));
            }
            ExprKind::GeneratorExp { .. } | ExprKind::DictComp { .. } => {
                if self.settings.rules.enabled(&Rule::FunctionUsesLoopVariable) {
                    flake8_bugbear::rules::function_uses_loop_variable(self, &Node::Expr(expr));
                }
                self.push_scope(Scope::new(ScopeKind::Generator));
            }
            ExprKind::BoolOp { op, values } => {
                if self
                    .settings
                    .rules
                    .enabled(&Rule::ConsiderMergingIsinstance)
                {
                    pylint::rules::merge_isinstance(self, expr, op, values);
                }
                if self.settings.rules.enabled(&Rule::DuplicateIsinstanceCall) {
                    flake8_simplify::rules::duplicate_isinstance_call(self, expr);
                }
                if self.settings.rules.enabled(&Rule::CompareWithTuple) {
                    flake8_simplify::rules::compare_with_tuple(self, expr);
                }
                if self.settings.rules.enabled(&Rule::AAndNotA) {
                    flake8_simplify::rules::a_and_not_a(self, expr);
                }
                if self.settings.rules.enabled(&Rule::AOrNotA) {
                    flake8_simplify::rules::a_or_not_a(self, expr);
                }
                if self.settings.rules.enabled(&Rule::OrTrue) {
                    flake8_simplify::rules::or_true(self, expr);
                }
                if self.settings.rules.enabled(&Rule::AndFalse) {
                    flake8_simplify::rules::and_false(self, expr);
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
                let callable = self.resolve_call_path(func).and_then(|call_path| {
                    if self.match_typing_call_path(&call_path, "ForwardRef") {
                        Some(Callable::ForwardRef)
                    } else if self.match_typing_call_path(&call_path, "cast") {
                        Some(Callable::Cast)
                    } else if self.match_typing_call_path(&call_path, "NewType") {
                        Some(Callable::NewType)
                    } else if self.match_typing_call_path(&call_path, "TypeVar") {
                        Some(Callable::TypeVar)
                    } else if self.match_typing_call_path(&call_path, "NamedTuple") {
                        Some(Callable::NamedTuple)
                    } else if self.match_typing_call_path(&call_path, "TypedDict") {
                        Some(Callable::TypedDict)
                    } else if ["Arg", "DefaultArg", "NamedArg", "DefaultNamedArg"]
                        .iter()
                        .any(|target| call_path.as_slice() == ["mypy_extensions", target])
                    {
                        Some(Callable::MypyExtension)
                    } else {
                        None
                    }
                });
                match callable {
                    Some(Callable::ForwardRef) => {
                        self.visit_expr(func);
                        for expr in args {
                            self.in_type_definition = true;
                            self.visit_expr(expr);
                            self.in_type_definition = prev_in_type_definition;
                        }
                    }
                    Some(Callable::Cast) => {
                        self.visit_expr(func);
                        if !args.is_empty() {
                            self.in_type_definition = true;
                            self.visit_expr(&args[0]);
                            self.in_type_definition = prev_in_type_definition;
                        }
                        for expr in args.iter().skip(1) {
                            self.visit_expr(expr);
                        }
                    }
                    Some(Callable::NewType) => {
                        self.visit_expr(func);
                        for expr in args.iter().skip(1) {
                            self.in_type_definition = true;
                            self.visit_expr(expr);
                            self.in_type_definition = prev_in_type_definition;
                        }
                    }
                    Some(Callable::TypeVar) => {
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
                    }
                    Some(Callable::NamedTuple) => {
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
                                                    self.in_type_definition =
                                                        prev_in_type_definition;

                                                    self.in_type_definition = true;
                                                    self.visit_expr(&elts[1]);
                                                    self.in_type_definition =
                                                        prev_in_type_definition;
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
                    }
                    Some(Callable::TypedDict) => {
                        self.visit_expr(func);

                        // Ex) TypedDict("a", {"a": int})
                        if args.len() > 1 {
                            if let ExprKind::Dict { keys, values } = &args[1].node {
                                for key in keys.iter().flatten() {
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
                    }
                    Some(Callable::MypyExtension) => {
                        self.visit_expr(func);

                        if let Some(arg) = args.first() {
                            // Ex) DefaultNamedArg(bool | None, name="some_prop_name")
                            self.in_type_definition = true;
                            self.visit_expr(arg);
                            self.in_type_definition = prev_in_type_definition;

                            for arg in args.iter().skip(1) {
                                self.in_type_definition = false;
                                self.visit_expr(arg);
                                self.in_type_definition = prev_in_type_definition;
                            }
                            for keyword in keywords {
                                let KeywordData { value, .. } = &keyword.node;
                                self.in_type_definition = false;
                                self.visit_expr(value);
                                self.in_type_definition = prev_in_type_definition;
                            }
                        } else {
                            // Ex) DefaultNamedArg(type="bool", name="some_prop_name")
                            for keyword in keywords {
                                let KeywordData { value, arg, .. } = &keyword.node;
                                if arg.as_ref().map_or(false, |arg| arg == "type") {
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
                    }
                    None => {
                        visitor::walk_expr(self, expr);
                    }
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
                    match match_annotated_subscript(
                        value,
                        |expr| self.resolve_call_path(expr),
                        self.settings.typing_modules.iter().map(String::as_str),
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
            ExprKind::JoinedStr { .. } => {
                visitor::walk_expr(self, expr);
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

        self.pop_expr();
    }

    fn visit_excepthandler(&mut self, excepthandler: &'b Excepthandler) {
        match &excepthandler.node {
            ExcepthandlerKind::ExceptHandler {
                type_, name, body, ..
            } => {
                if self.settings.rules.enabled(&Rule::DoNotUseBareExcept) {
                    if let Some(diagnostic) = pycodestyle::rules::do_not_use_bare_except(
                        type_.as_deref(),
                        body,
                        excepthandler,
                        self.locator,
                    ) {
                        self.diagnostics.push(diagnostic);
                    }
                }
                if self
                    .settings
                    .rules
                    .enabled(&Rule::RaiseWithoutFromInsideExcept)
                {
                    flake8_bugbear::rules::raise_without_from_inside_except(self, body);
                }
                if self.settings.rules.enabled(&Rule::BlindExcept) {
                    flake8_blind_except::rules::blind_except(
                        self,
                        type_.as_deref(),
                        name.as_deref(),
                        body,
                    );
                }
                if self.settings.rules.enabled(&Rule::TryExceptPass) {
                    flake8_bandit::rules::try_except_pass(
                        self,
                        type_.as_deref(),
                        name.as_deref(),
                        body,
                        self.settings.flake8_bandit.check_typed_exception,
                    );
                }
                if self.settings.rules.enabled(&Rule::ReraiseNoCause) {
                    tryceratops::rules::reraise_no_cause(self, body);
                }
                match name {
                    Some(name) => {
                        if self.settings.rules.enabled(&Rule::AmbiguousVariableName) {
                            if let Some(diagnostic) = pycodestyle::rules::ambiguous_variable_name(
                                name,
                                helpers::excepthandler_name_range(excepthandler, self.locator)
                                    .expect("Failed to find `name` range"),
                            ) {
                                self.diagnostics.push(diagnostic);
                            }
                        }

                        self.check_builtin_shadowing(name, excepthandler, false);

                        let name_range =
                            helpers::excepthandler_name_range(excepthandler, self.locator).unwrap();

                        if self.current_scope().bindings.contains_key(&name.as_str()) {
                            self.handle_node_store(
                                name,
                                &Expr::new(
                                    name_range.location,
                                    name_range.end_location,
                                    ExprKind::Name {
                                        id: name.to_string(),
                                        ctx: ExprContext::Store,
                                    },
                                ),
                            );
                        }

                        let definition = self.current_scope().bindings.get(&name.as_str()).copied();
                        self.handle_node_store(
                            name,
                            &Expr::new(
                                name_range.location,
                                name_range.end_location,
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
                            &scope.bindings.remove(&name.as_str())
                        } {
                            if !self.bindings[*index].used() {
                                if self.settings.rules.enabled(&Rule::UnusedVariable) {
                                    let mut diagnostic = Diagnostic::new(
                                        pyflakes::rules::UnusedVariable {
                                            name: name.to_string(),
                                        },
                                        name_range,
                                    );
                                    if self.patch(&Rule::UnusedVariable) {
                                        match pyflakes::fixes::remove_exception_handler_assignment(
                                            excepthandler,
                                            self.locator,
                                        ) {
                                            Ok(fix) => {
                                                diagnostic.amend(fix);
                                            }
                                            Err(e) => {
                                                error!(
                                                    "Failed to remove exception handler \
                                                     assignment: {}",
                                                    e
                                                );
                                            }
                                        }
                                    }
                                    self.diagnostics.push(diagnostic);
                                }
                            }
                        }

                        if let Some(index) = definition {
                            let scope = &mut self.scopes
                                [*(self.scope_stack.last().expect("No current scope found"))];
                            scope.bindings.insert(name, index);
                        }
                    }
                    None => walk_excepthandler(self, excepthandler),
                }
            }
        }
    }

    fn visit_format_spec(&mut self, format_spec: &'b Expr) {
        match &format_spec.node {
            ExprKind::JoinedStr { values } => {
                for value in values {
                    self.visit_expr(value);
                }
            }
            _ => unreachable!("Unexpected expression for format_spec"),
        }
    }

    fn visit_comprehension(&mut self, comprehension: &'b Comprehension) {
        if self.settings.rules.enabled(&Rule::KeyInDict) {
            flake8_simplify::rules::key_in_dict_for(
                self,
                &comprehension.target,
                &comprehension.iter,
            );
        }
        visitor::walk_comprehension(self, comprehension);
    }

    fn visit_arguments(&mut self, arguments: &'b Arguments) {
        if self.settings.rules.enabled(&Rule::MutableArgumentDefault) {
            flake8_bugbear::rules::mutable_argument_default(self, arguments);
        }
        if self
            .settings
            .rules
            .enabled(&Rule::FunctionCallArgumentDefault)
        {
            flake8_bugbear::rules::function_call_argument_default(self, arguments);
        }

        // flake8-boolean-trap
        if self
            .settings
            .rules
            .enabled(&Rule::BooleanPositionalArgInFunctionDefinition)
        {
            flake8_boolean_trap::rules::check_positional_boolean_in_def(self, arguments);
        }
        if self
            .settings
            .rules
            .enabled(&Rule::BooleanDefaultValueInFunctionDefinition)
        {
            flake8_boolean_trap::rules::check_boolean_default_value_in_function_definition(
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
                runtime_usage: None,
                synthetic_usage: None,
                typing_usage: None,
                range: Range::from_located(arg),
                source: Some(self.current_stmt().clone()),
                context: self.execution_context(),
            },
        );

        if self.settings.rules.enabled(&Rule::AmbiguousVariableName) {
            if let Some(diagnostic) =
                pycodestyle::rules::ambiguous_variable_name(&arg.node.arg, Range::from_located(arg))
            {
                self.diagnostics.push(diagnostic);
            }
        }

        if self.settings.rules.enabled(&Rule::InvalidArgumentName) {
            if let Some(diagnostic) = pep8_naming::rules::invalid_argument_name(&arg.node.arg, arg)
            {
                self.diagnostics.push(diagnostic);
            }
        }

        self.check_builtin_arg_shadowing(&arg.node.arg, arg);
    }

    fn visit_body(&mut self, body: &'b [Stmt]) {
        if self.settings.rules.enabled(&Rule::NoUnnecessaryPass) {
            flake8_pie::rules::no_unnecessary_pass(self, body);
        }

        let prev_body = self.body;
        let prev_body_index = self.body_index;
        self.body = body;
        self.body_index = 0;

        for stmt in body {
            self.visit_stmt(stmt);
            self.body_index += 1;
        }

        self.body = prev_body;
        self.body_index = prev_body_index;
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
        self.dead_scopes.push((
            self.scope_stack
                .pop()
                .expect("Attempted to pop without scope"),
            self.scope_stack.clone(),
        ));
    }

    fn bind_builtins(&mut self) {
        let scope = &mut self.scopes[*(self.scope_stack.last().expect("No current scope found"))];

        for builtin in BUILTINS
            .iter()
            .chain(MAGIC_GLOBALS.iter())
            .copied()
            .chain(self.settings.builtins.iter().map(String::as_str))
        {
            let index = self.bindings.len();
            self.bindings.push(Binding {
                kind: BindingKind::Builtin,
                range: Range::default(),
                runtime_usage: None,
                synthetic_usage: Some((0, Range::default())),
                typing_usage: None,
                source: None,
                context: ExecutionContext::Runtime,
            });
            scope.bindings.insert(builtin, index);
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

    /// Return the parent `Expr` of the current `Expr`.
    pub fn current_expr_parent(&self) -> Option<&RefEquality<'a, Expr>> {
        self.exprs.iter().rev().nth(1)
    }

    /// Return the grandparent `Expr` of the current `Expr`.
    pub fn current_expr_grandparent(&self) -> Option<&RefEquality<'a, Expr>> {
        self.exprs.iter().rev().nth(2)
    }

    /// Return the `Stmt` that immediately follows the current `Stmt`, if any.
    pub fn current_sibling_stmt(&self) -> Option<&'a Stmt> {
        self.body.get(self.body_index + 1)
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

    pub const fn in_exception_handler(&self) -> bool {
        self.in_exception_handler
    }

    pub const fn execution_context(&self) -> ExecutionContext {
        if self.in_type_checking_block
            || self.in_annotation
            || self.in_deferred_string_type_definition
            || self.in_deferred_type_definition
        {
            ExecutionContext::Typing
        } else {
            ExecutionContext::Runtime
        }
    }

    fn add_binding<'b>(&mut self, name: &'b str, binding: Binding<'a>)
    where
        'b: 'a,
    {
        let binding_index = self.bindings.len();

        if let Some((stack_index, scope_index)) = self
            .scope_stack
            .iter()
            .rev()
            .enumerate()
            .find(|(_, scope_index)| self.scopes[**scope_index].bindings.contains_key(&name))
        {
            let existing_binding_index = self.scopes[*scope_index].bindings.get(&name).unwrap();
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
                    if self.settings.rules.enabled(&Rule::ImportShadowedByLoopVar) {
                        self.diagnostics.push(Diagnostic::new(
                            pyflakes::rules::ImportShadowedByLoopVar {
                                name: name.to_string(),
                                line: existing.range.location.row(),
                            },
                            binding.range,
                        ));
                    }
                } else if in_current_scope {
                    if !existing.used()
                        && binding.redefines(existing)
                        && (!self.settings.dummy_variable_rgx.is_match(name) || existing_is_import)
                        && !(matches!(existing.kind, BindingKind::FunctionDefinition)
                            && visibility::is_overload(
                                self,
                                cast::decorator_list(existing.source.as_ref().unwrap()),
                            ))
                    {
                        if self.settings.rules.enabled(&Rule::RedefinedWhileUnused) {
                            let mut diagnostic = Diagnostic::new(
                                pyflakes::rules::RedefinedWhileUnused {
                                    name: name.to_string(),
                                    line: existing.range.location.row(),
                                },
                                binding_range(&binding, self.locator),
                            );
                            if let Some(parent) = binding.source.as_ref() {
                                if matches!(parent.node, StmtKind::ImportFrom { .. })
                                    && parent.location.row() != binding.range.location.row()
                                {
                                    diagnostic.parent(parent.location);
                                }
                            }
                            self.diagnostics.push(diagnostic);
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

        let scope = self.current_scope();
        let binding = if let Some(index) = scope.bindings.get(&name) {
            if matches!(self.bindings[*index].kind, BindingKind::Builtin) {
                // Avoid overriding builtins.
                binding
            } else if matches!(self.bindings[*index].kind, BindingKind::Global) {
                // If the original binding was a global, and the new binding conflicts within the
                // current scope, then the new binding is also a global.
                Binding {
                    runtime_usage: self.bindings[*index].runtime_usage,
                    synthetic_usage: self.bindings[*index].synthetic_usage,
                    typing_usage: self.bindings[*index].typing_usage,
                    kind: BindingKind::Global,
                    ..binding
                }
            } else if matches!(self.bindings[*index].kind, BindingKind::Nonlocal) {
                // If the original binding was a nonlocal, and the new binding conflicts within the
                // current scope, then the new binding is also a nonlocal.
                Binding {
                    runtime_usage: self.bindings[*index].runtime_usage,
                    synthetic_usage: self.bindings[*index].synthetic_usage,
                    typing_usage: self.bindings[*index].typing_usage,
                    kind: BindingKind::Nonlocal,
                    ..binding
                }
            } else {
                Binding {
                    runtime_usage: self.bindings[*index].runtime_usage,
                    synthetic_usage: self.bindings[*index].synthetic_usage,
                    typing_usage: self.bindings[*index].typing_usage,
                    ..binding
                }
            }
        } else {
            binding
        };

        // Don't treat annotations as assignments if there is an existing value
        // in scope.
        let scope = &mut self.scopes[*(self.scope_stack.last().expect("No current scope found"))];
        if !(matches!(binding.kind, BindingKind::Annotation) && scope.bindings.contains_key(name)) {
            if let Some(rebound_index) = scope.bindings.insert(name, binding_index) {
                scope
                    .rebounds
                    .entry(name)
                    .or_insert_with(Vec::new)
                    .push(rebound_index);
            }
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

                if let Some(index) = scope.bindings.get(&id.as_str()) {
                    // Mark the binding as used.
                    let context = self.execution_context();
                    self.bindings[*index].mark_used(scope_id, Range::from_located(expr), context);

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
                    match &self.bindings[*index].kind {
                        BindingKind::Importation(name, full_name)
                        | BindingKind::SubmoduleImportation(name, full_name) => {
                            let has_alias = full_name
                                .split('.')
                                .last()
                                .map(|segment| &segment != name)
                                .unwrap_or_default();
                            if has_alias {
                                // Mark the sub-importation as used.
                                if let Some(index) = scope.bindings.get(full_name) {
                                    self.bindings[*index].mark_used(
                                        scope_id,
                                        Range::from_located(expr),
                                        context,
                                    );
                                }
                            }
                        }
                        BindingKind::FromImportation(name, full_name) => {
                            let has_alias = full_name
                                .split('.')
                                .last()
                                .map(|segment| &segment != name)
                                .unwrap_or_default();
                            if has_alias {
                                // Mark the sub-importation as used.
                                if let Some(index) = scope.bindings.get(full_name.as_str()) {
                                    self.bindings[*index].mark_used(
                                        scope_id,
                                        Range::from_located(expr),
                                        context,
                                    );
                                }
                            }
                        }
                        _ => {}
                    }

                    return;
                }

                first_iter = false;
                in_generator = matches!(scope.kind, ScopeKind::Generator);
                import_starred = import_starred || scope.import_starred;
            }

            if import_starred {
                if self.settings.rules.enabled(&Rule::ImportStarUsage) {
                    let mut from_list = vec![];
                    for scope_index in self.scope_stack.iter().rev() {
                        let scope = &self.scopes[*scope_index];
                        for binding in scope.bindings.values().map(|index| &self.bindings[*index]) {
                            if let BindingKind::StarImportation(level, module) = &binding.kind {
                                from_list.push(helpers::format_import_from(
                                    level.as_ref(),
                                    module.as_deref(),
                                ));
                            }
                        }
                    }
                    from_list.sort();

                    self.diagnostics.push(Diagnostic::new(
                        pyflakes::rules::ImportStarUsage {
                            name: id.to_string(),
                            sources: from_list,
                        },
                        Range::from_located(expr),
                    ));
                }
                return;
            }

            if self.settings.rules.enabled(&Rule::UndefinedName) {
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
                        .any(|call_path| call_path.as_slice() == ["NameError"])
                    {
                        return;
                    }
                }

                self.diagnostics.push(Diagnostic::new(
                    pyflakes::rules::UndefinedName { name: id.clone() },
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

        if self.settings.rules.enabled(&Rule::UndefinedLocal) {
            let scopes: Vec<&Scope> = self
                .scope_stack
                .iter()
                .map(|index| &self.scopes[*index])
                .collect();
            if let Some(diagnostic) = pyflakes::rules::undefined_local(id, &scopes, &self.bindings)
            {
                self.diagnostics.push(diagnostic);
            }
        }

        if self
            .settings
            .rules
            .enabled(&Rule::NonLowercaseVariableInFunction)
        {
            if matches!(self.current_scope().kind, ScopeKind::Function(..)) {
                // Ignore globals.
                if !self
                    .current_scope()
                    .bindings
                    .get(id)
                    .map_or(false, |index| {
                        matches!(self.bindings[*index].kind, BindingKind::Global)
                    })
                {
                    pep8_naming::rules::non_lowercase_variable_in_function(self, expr, parent, id);
                }
            }
        }

        if self
            .settings
            .rules
            .enabled(&Rule::MixedCaseVariableInClassScope)
        {
            if matches!(self.current_scope().kind, ScopeKind::Class(..)) {
                pep8_naming::rules::mixed_case_variable_in_class_scope(self, expr, parent, id);
            }
        }

        if self
            .settings
            .rules
            .enabled(&Rule::MixedCaseVariableInGlobalScope)
        {
            if matches!(self.current_scope().kind, ScopeKind::Module) {
                pep8_naming::rules::mixed_case_variable_in_global_scope(self, expr, parent, id);
            }
        }

        if matches!(parent.node, StmtKind::AnnAssign { value: None, .. }) {
            self.add_binding(
                id,
                Binding {
                    kind: BindingKind::Annotation,
                    runtime_usage: None,
                    synthetic_usage: None,
                    typing_usage: None,
                    range: Range::from_located(expr),
                    source: Some(self.current_stmt().clone()),
                    context: self.execution_context(),
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
                    runtime_usage: None,
                    synthetic_usage: None,
                    typing_usage: None,
                    range: Range::from_located(expr),
                    source: Some(self.current_stmt().clone()),
                    context: self.execution_context(),
                },
            );
            return;
        }

        if operations::is_unpacking_assignment(parent, expr) {
            self.add_binding(
                id,
                Binding {
                    kind: BindingKind::Binding,
                    runtime_usage: None,
                    synthetic_usage: None,
                    typing_usage: None,
                    range: Range::from_located(expr),
                    source: Some(self.current_stmt().clone()),
                    context: self.execution_context(),
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
                let (all_names, all_names_flags) = extract_all_names(self, parent, current);

                if self.settings.rules.enabled(&Rule::InvalidAllFormat)
                    && matches!(all_names_flags, AllNamesFlags::INVALID_FORMAT)
                {
                    pylint::rules::invalid_all_format(self, expr);
                }

                if self.settings.rules.enabled(&Rule::InvalidAllObject)
                    && matches!(all_names_flags, AllNamesFlags::INVALID_OBJECT)
                {
                    pylint::rules::invalid_all_object(self, expr);
                }

                self.add_binding(
                    id,
                    Binding {
                        kind: BindingKind::Export(all_names),
                        runtime_usage: None,
                        synthetic_usage: None,
                        typing_usage: None,
                        range: Range::from_located(expr),
                        source: Some(self.current_stmt().clone()),
                        context: self.execution_context(),
                    },
                );
                return;
            }
        }

        self.add_binding(
            id,
            Binding {
                kind: BindingKind::Assignment,
                runtime_usage: None,
                synthetic_usage: None,
                typing_usage: None,
                range: Range::from_located(expr),
                source: Some(self.current_stmt().clone()),
                context: self.execution_context(),
            },
        );
    }

    fn handle_node_delete<'b>(&mut self, expr: &'b Expr)
    where
        'b: 'a,
    {
        if let ExprKind::Name { id, .. } = &expr.node {
            if operations::on_conditional_branch(
                &mut self.parents.iter().rev().map(std::convert::Into::into),
            ) {
                return;
            }

            let scope =
                &mut self.scopes[*(self.scope_stack.last().expect("No current scope found"))];
            if scope.bindings.remove(&id.as_str()).is_none()
                && self.settings.rules.enabled(&Rule::UndefinedName)
            {
                self.diagnostics.push(Diagnostic::new(
                    pyflakes::rules::UndefinedName {
                        name: id.to_string(),
                    },
                    Range::from_located(expr),
                ));
            }
        }
    }

    fn visit_docstring<'b>(&mut self, python_ast: &'b Suite) -> bool
    where
        'b: 'a,
    {
        if self.settings.rules.enabled(&Rule::FStringDocstring) {
            flake8_bugbear::rules::f_string_docstring(self, python_ast);
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
            (self.scope_stack.clone(), self.parents.clone()),
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
                if self.annotations_future_enabled {
                    if self.settings.rules.enabled(&Rule::QuotedAnnotation) {
                        pyupgrade::rules::quoted_annotation(self, expression, range);
                    }
                }
                relocate_expr(&mut expr, range);
                allocator.push(expr);
                stacks.push((in_annotation, context));
            } else {
                if self
                    .settings
                    .rules
                    .enabled(&Rule::ForwardAnnotationSyntaxError)
                {
                    self.diagnostics.push(Diagnostic::new(
                        pyflakes::rules::ForwardAnnotationSyntaxError {
                            body: expression.to_string(),
                        },
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
                    self.visit_body(body);
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
        while let Some((scopes, ..)) = self.deferred_assignments.pop() {
            let scope_index = scopes[scopes.len() - 1];
            let parent_scope_index = scopes[scopes.len() - 2];
            if self.settings.rules.enabled(&Rule::UnusedVariable) {
                pyflakes::rules::unused_variable(self, scope_index);
            }
            if self.settings.rules.enabled(&Rule::UnusedAnnotation) {
                pyflakes::rules::unused_annotation(self, scope_index);
            }
            if self.settings.rules.enabled(&Rule::UnusedFunctionArgument)
                || self.settings.rules.enabled(&Rule::UnusedMethodArgument)
                || self
                    .settings
                    .rules
                    .enabled(&Rule::UnusedClassMethodArgument)
                || self
                    .settings
                    .rules
                    .enabled(&Rule::UnusedStaticMethodArgument)
                || self.settings.rules.enabled(&Rule::UnusedLambdaArgument)
            {
                self.diagnostics
                    .extend(flake8_unused_arguments::rules::unused_arguments(
                        self,
                        &self.scopes[parent_scope_index],
                        &self.scopes[scope_index],
                        &self.bindings,
                    ));
            }
        }
    }

    fn check_deferred_for_loops(&mut self) {
        self.deferred_for_loops.reverse();
        while let Some((stmt, (scopes, parents))) = self.deferred_for_loops.pop() {
            self.scope_stack = scopes.clone();
            self.parents = parents.clone();

            if let StmtKind::For { target, body, .. } | StmtKind::AsyncFor { target, body, .. } =
                &stmt.node
            {
                if self
                    .settings
                    .rules
                    .enabled(&Rule::UnusedLoopControlVariable)
                {
                    flake8_bugbear::rules::unused_loop_control_variable(self, stmt, target, body);
                }
            } else {
                unreachable!("Expected ExprKind::Lambda");
            }
        }
    }

    fn check_dead_scopes(&mut self) {
        if !(self.settings.rules.enabled(&Rule::UnusedImport)
            || self.settings.rules.enabled(&Rule::ImportStarUsage)
            || self.settings.rules.enabled(&Rule::RedefinedWhileUnused)
            || self.settings.rules.enabled(&Rule::UndefinedExport)
            || self
                .settings
                .rules
                .enabled(&Rule::GlobalVariableNotAssigned)
            || self
                .settings
                .rules
                .enabled(&Rule::RuntimeImportInTypeCheckingBlock)
            || self
                .settings
                .rules
                .enabled(&Rule::TypingOnlyFirstPartyImport)
            || self
                .settings
                .rules
                .enabled(&Rule::TypingOnlyThirdPartyImport)
            || self
                .settings
                .rules
                .enabled(&Rule::TypingOnlyStandardLibraryImport))
        {
            return;
        }

        // Identify any valid runtime imports. If a module is imported at runtime, and
        // used at runtime, then by default, we avoid flagging any other
        // imports from that model as typing-only.
        let runtime_imports: Vec<Vec<&Binding>> = if !self.settings.flake8_type_checking.strict
            && (self
                .settings
                .rules
                .enabled(&Rule::RuntimeImportInTypeCheckingBlock)
                || self
                    .settings
                    .rules
                    .enabled(&Rule::TypingOnlyFirstPartyImport)
                || self
                    .settings
                    .rules
                    .enabled(&Rule::TypingOnlyThirdPartyImport)
                || self
                    .settings
                    .rules
                    .enabled(&Rule::TypingOnlyStandardLibraryImport))
        {
            self.scopes
                .iter()
                .map(|scope| {
                    scope
                        .bindings
                        .values()
                        .map(|index| &self.bindings[*index])
                        .filter(|binding| {
                            flake8_type_checking::helpers::is_valid_runtime_import(binding)
                        })
                        .collect::<Vec<_>>()
                })
                .collect::<Vec<_>>()
        } else {
            vec![]
        };

        let mut diagnostics: Vec<Diagnostic> = vec![];
        for (index, stack) in self.dead_scopes.iter().rev() {
            let scope = &self.scopes[*index];

            // PLW0602
            if self
                .settings
                .rules
                .enabled(&Rule::GlobalVariableNotAssigned)
            {
                for (name, index) in &scope.bindings {
                    let binding = &self.bindings[*index];
                    if matches!(binding.kind, BindingKind::Global) {
                        if let Some(stmt) = &binding.source {
                            if matches!(stmt.node, StmtKind::Global { .. }) {
                                diagnostics.push(Diagnostic::new(
                                    pylint::rules::GlobalVariableNotAssigned {
                                        name: (*name).to_string(),
                                    },
                                    binding.range,
                                ));
                            }
                        }
                    }
                }
            }

            // Imports in classes are public members.
            if matches!(scope.kind, ScopeKind::Class(..)) {
                continue;
            }

            let all_binding: Option<&Binding> = scope
                .bindings
                .get("__all__")
                .map(|index| &self.bindings[*index]);
            let all_names: Option<Vec<&str>> =
                all_binding.and_then(|binding| match &binding.kind {
                    BindingKind::Export(names) => Some(names.iter().map(String::as_str).collect()),
                    _ => None,
                });

            if self.settings.rules.enabled(&Rule::UndefinedExport) {
                if !scope.import_starred && !self.path.ends_with("__init__.py") {
                    if let Some(all_binding) = all_binding {
                        if let Some(names) = &all_names {
                            for &name in names {
                                if !scope.bindings.contains_key(name) {
                                    diagnostics.push(Diagnostic::new(
                                        pyflakes::rules::UndefinedExport {
                                            name: name.to_string(),
                                        },
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
            if self.settings.rules.enabled(&Rule::RedefinedWhileUnused) {
                for (name, index) in &scope.bindings {
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
                        if binding.used()
                            || all_names
                                .as_ref()
                                .map(|names| names.contains(name))
                                .unwrap_or_default()
                        {
                            continue;
                        }

                        if let Some(indices) = self.redefinitions.get(index) {
                            for index in indices {
                                let rebound = &self.bindings[*index];
                                let mut diagnostic = Diagnostic::new(
                                    pyflakes::rules::RedefinedWhileUnused {
                                        name: (*name).to_string(),
                                        line: binding.range.location.row(),
                                    },
                                    binding_range(rebound, self.locator),
                                );
                                if let Some(parent) = &rebound.source {
                                    if matches!(parent.node, StmtKind::ImportFrom { .. })
                                        && parent.location.row() != rebound.range.location.row()
                                    {
                                        diagnostic.parent(parent.location);
                                    }
                                };
                                diagnostics.push(diagnostic);
                            }
                        }
                    }
                }
            }

            if self.settings.rules.enabled(&Rule::ImportStarUsage) {
                if scope.import_starred {
                    if let Some(all_binding) = all_binding {
                        if let Some(names) = &all_names {
                            let mut from_list = vec![];
                            for binding in
                                scope.bindings.values().map(|index| &self.bindings[*index])
                            {
                                if let BindingKind::StarImportation(level, module) = &binding.kind {
                                    from_list.push(helpers::format_import_from(
                                        level.as_ref(),
                                        module.as_deref(),
                                    ));
                                }
                            }
                            from_list.sort();

                            for &name in names {
                                if !scope.bindings.contains_key(name) {
                                    diagnostics.push(Diagnostic::new(
                                        pyflakes::rules::ImportStarUsage {
                                            name: name.to_string(),
                                            sources: from_list.clone(),
                                        },
                                        all_binding.range,
                                    ));
                                }
                            }
                        }
                    }
                }
            }

            if self
                .settings
                .rules
                .enabled(&Rule::RuntimeImportInTypeCheckingBlock)
                || self
                    .settings
                    .rules
                    .enabled(&Rule::TypingOnlyFirstPartyImport)
                || self
                    .settings
                    .rules
                    .enabled(&Rule::TypingOnlyThirdPartyImport)
                || self
                    .settings
                    .rules
                    .enabled(&Rule::TypingOnlyStandardLibraryImport)
            {
                let runtime_imports: Vec<&Binding> = if self.settings.flake8_type_checking.strict {
                    vec![]
                } else {
                    stack
                        .iter()
                        .chain(iter::once(index))
                        .flat_map(|index| runtime_imports[*index].iter())
                        .copied()
                        .collect()
                };
                for (.., index) in &scope.bindings {
                    let binding = &self.bindings[*index];

                    if let Some(diagnostic) =
                        flake8_type_checking::rules::runtime_import_in_type_checking_block(binding)
                    {
                        diagnostics.push(diagnostic);
                    }
                    if let Some(diagnostic) =
                        flake8_type_checking::rules::typing_only_runtime_import(
                            binding,
                            &runtime_imports,
                            self.package,
                            self.settings,
                        )
                    {
                        if self.settings.rules.enabled(diagnostic.kind.rule()) {
                            diagnostics.push(diagnostic);
                        }
                    }
                }
            }

            if self.settings.rules.enabled(&Rule::UnusedImport) {
                // Collect all unused imports by location. (Multiple unused imports at the same
                // location indicates an `import from`.)
                type UnusedImport<'a> = (&'a str, &'a Range);
                type BindingContext<'a, 'b> =
                    (&'a RefEquality<'b, Stmt>, Option<&'a RefEquality<'b, Stmt>>);

                let mut unused: FxHashMap<BindingContext, Vec<UnusedImport>> = FxHashMap::default();
                let mut ignored: FxHashMap<BindingContext, Vec<UnusedImport>> =
                    FxHashMap::default();

                for (name, index) in &scope.bindings {
                    let binding = &self.bindings[*index];

                    let full_name = match &binding.kind {
                        BindingKind::Importation(.., full_name) => full_name,
                        BindingKind::FromImportation(.., full_name) => full_name.as_str(),
                        BindingKind::SubmoduleImportation(.., full_name) => full_name,
                        _ => continue,
                    };

                    // Skip used exports from `__all__`
                    if binding.used()
                        || all_names
                            .as_ref()
                            .map(|names| names.contains(name))
                            .unwrap_or_default()
                    {
                        continue;
                    }

                    let defined_by = binding.source.as_ref().unwrap();
                    let defined_in = self.child_to_parent.get(defined_by);
                    let child: &Stmt = defined_by.into();

                    let diagnostic_lineno = binding.range.location.row();
                    let parent_lineno = if matches!(child.node, StmtKind::ImportFrom { .. })
                        && child.location.row() != diagnostic_lineno
                    {
                        Some(child.location.row())
                    } else {
                        None
                    };

                    if self.is_ignored(&Rule::UnusedImport, diagnostic_lineno)
                        || parent_lineno.map_or(false, |parent_lineno| {
                            self.is_ignored(&Rule::UnusedImport, parent_lineno)
                        })
                    {
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
                    .sorted_by_key(|((defined_by, _), _)| defined_by.location)
                {
                    let child: &Stmt = defined_by.into();
                    let parent: Option<&Stmt> = defined_in.map(std::convert::Into::into);

                    let fix = if !ignore_init && self.patch(&Rule::UnusedImport) {
                        let deleted: Vec<&Stmt> = self
                            .deletions
                            .iter()
                            .map(std::convert::Into::into)
                            .collect();
                        match autofix::helpers::remove_unused_imports(
                            unused_imports.iter().map(|(full_name, _)| *full_name),
                            child,
                            parent,
                            &deleted,
                            self.locator,
                            self.indexer,
                            self.stylist,
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

                    let multiple = unused_imports.len() > 1;
                    for (full_name, range) in unused_imports {
                        let mut diagnostic = Diagnostic::new(
                            pyflakes::rules::UnusedImport {
                                name: full_name.to_string(),
                                ignore_init,
                                multiple,
                            },
                            *range,
                        );
                        if matches!(child.node, StmtKind::ImportFrom { .. })
                            && child.location.row() != range.location.row()
                        {
                            diagnostic.parent(child.location);
                        }
                        if let Some(fix) = fix.as_ref() {
                            diagnostic.amend(fix.clone());
                        }
                        diagnostics.push(diagnostic);
                    }
                }
                for ((defined_by, ..), unused_imports) in ignored
                    .into_iter()
                    .sorted_by_key(|((defined_by, _), _)| defined_by.location)
                {
                    let child: &Stmt = defined_by.into();
                    let multiple = unused_imports.len() > 1;
                    for (full_name, range) in unused_imports {
                        let mut diagnostic = Diagnostic::new(
                            pyflakes::rules::UnusedImport {
                                name: full_name.to_string(),
                                ignore_init,
                                multiple,
                            },
                            *range,
                        );
                        if matches!(child.node, StmtKind::ImportFrom { .. })
                            && child.location.row() != range.location.row()
                        {
                            diagnostic.parent(child.location);
                        }
                        diagnostics.push(diagnostic);
                    }
                }
            }
        }
        self.diagnostics.extend(diagnostics);
    }

    fn check_definitions(&mut self) {
        let enforce_annotations = self
            .settings
            .rules
            .enabled(&Rule::MissingTypeFunctionArgument)
            || self.settings.rules.enabled(&Rule::MissingTypeArgs)
            || self.settings.rules.enabled(&Rule::MissingTypeKwargs)
            || self.settings.rules.enabled(&Rule::MissingTypeSelf)
            || self.settings.rules.enabled(&Rule::MissingTypeCls)
            || self
                .settings
                .rules
                .enabled(&Rule::MissingReturnTypePublicFunction)
            || self
                .settings
                .rules
                .enabled(&Rule::MissingReturnTypePrivateFunction)
            || self
                .settings
                .rules
                .enabled(&Rule::MissingReturnTypeSpecialMethod)
            || self
                .settings
                .rules
                .enabled(&Rule::MissingReturnTypeStaticMethod)
            || self
                .settings
                .rules
                .enabled(&Rule::MissingReturnTypeClassMethod)
            || self
                .settings
                .rules
                .enabled(&Rule::DynamicallyTypedExpression);
        let enforce_docstrings = self.settings.rules.enabled(&Rule::PublicModule)
            || self.settings.rules.enabled(&Rule::PublicClass)
            || self.settings.rules.enabled(&Rule::PublicMethod)
            || self.settings.rules.enabled(&Rule::PublicFunction)
            || self.settings.rules.enabled(&Rule::PublicPackage)
            || self.settings.rules.enabled(&Rule::MagicMethod)
            || self.settings.rules.enabled(&Rule::PublicNestedClass)
            || self.settings.rules.enabled(&Rule::PublicInit)
            || self.settings.rules.enabled(&Rule::FitsOnOneLine)
            || self
                .settings
                .rules
                .enabled(&Rule::NoBlankLineBeforeFunction)
            || self.settings.rules.enabled(&Rule::NoBlankLineAfterFunction)
            || self.settings.rules.enabled(&Rule::OneBlankLineBeforeClass)
            || self.settings.rules.enabled(&Rule::OneBlankLineAfterClass)
            || self.settings.rules.enabled(&Rule::BlankLineAfterSummary)
            || self.settings.rules.enabled(&Rule::IndentWithSpaces)
            || self.settings.rules.enabled(&Rule::NoUnderIndentation)
            || self.settings.rules.enabled(&Rule::NoOverIndentation)
            || self
                .settings
                .rules
                .enabled(&Rule::NewLineAfterLastParagraph)
            || self.settings.rules.enabled(&Rule::NoSurroundingWhitespace)
            || self.settings.rules.enabled(&Rule::NoBlankLineBeforeClass)
            || self
                .settings
                .rules
                .enabled(&Rule::MultiLineSummaryFirstLine)
            || self
                .settings
                .rules
                .enabled(&Rule::MultiLineSummarySecondLine)
            || self.settings.rules.enabled(&Rule::SectionNotOverIndented)
            || self
                .settings
                .rules
                .enabled(&Rule::SectionUnderlineNotOverIndented)
            || self.settings.rules.enabled(&Rule::UsesTripleQuotes)
            || self
                .settings
                .rules
                .enabled(&Rule::UsesRPrefixForBackslashedContent)
            || self.settings.rules.enabled(&Rule::EndsInPeriod)
            || self.settings.rules.enabled(&Rule::NonImperativeMood)
            || self.settings.rules.enabled(&Rule::NoSignature)
            || self.settings.rules.enabled(&Rule::FirstLineCapitalized)
            || self.settings.rules.enabled(&Rule::NoThisPrefix)
            || self.settings.rules.enabled(&Rule::CapitalizeSectionName)
            || self.settings.rules.enabled(&Rule::NewLineAfterSectionName)
            || self
                .settings
                .rules
                .enabled(&Rule::DashedUnderlineAfterSection)
            || self
                .settings
                .rules
                .enabled(&Rule::SectionUnderlineAfterName)
            || self
                .settings
                .rules
                .enabled(&Rule::SectionUnderlineMatchesSectionLength)
            || self.settings.rules.enabled(&Rule::BlankLineAfterSection)
            || self.settings.rules.enabled(&Rule::BlankLineBeforeSection)
            || self
                .settings
                .rules
                .enabled(&Rule::NoBlankLinesBetweenHeaderAndContent)
            || self
                .settings
                .rules
                .enabled(&Rule::BlankLineAfterLastSection)
            || self.settings.rules.enabled(&Rule::NonEmptySection)
            || self.settings.rules.enabled(&Rule::EndsInPunctuation)
            || self.settings.rules.enabled(&Rule::SectionNameEndsInColon)
            || self.settings.rules.enabled(&Rule::DocumentAllArguments)
            || self.settings.rules.enabled(&Rule::SkipDocstring)
            || self.settings.rules.enabled(&Rule::NonEmpty);

        let mut overloaded_name: Option<String> = None;
        self.definitions.reverse();
        while let Some((definition, visibility, (scopes, parents))) = self.definitions.pop() {
            self.scope_stack = scopes.clone();
            self.parents = parents.clone();

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
                    flake8_annotations::rules::definition(self, &definition, &visibility);
                }
                overloaded_name = flake8_annotations::helpers::overloaded_name(self, &definition);
            }

            // pydocstyle
            if enforce_docstrings {
                if definition.docstring.is_none() {
                    pydocstyle::rules::not_missing(self, &definition, &visibility);
                    continue;
                }

                // Extract a `Docstring` from a `Definition`.
                let expr = definition.docstring.unwrap();
                let contents = self
                    .locator
                    .slice_source_code_range(&Range::from_located(expr));
                let indentation = self.locator.slice_source_code_range(&Range::new(
                    Location::new(expr.location.row(), 0),
                    Location::new(expr.location.row(), expr.location.column()),
                ));

                let body = pydocstyle::helpers::raw_contents(contents);
                let docstring = Docstring {
                    kind: definition.kind,
                    expr,
                    contents,
                    indentation,
                    body,
                };

                if !pydocstyle::rules::not_empty(self, &docstring) {
                    continue;
                }

                if self.settings.rules.enabled(&Rule::FitsOnOneLine) {
                    pydocstyle::rules::one_liner(self, &docstring);
                }
                if self
                    .settings
                    .rules
                    .enabled(&Rule::NoBlankLineBeforeFunction)
                    || self.settings.rules.enabled(&Rule::NoBlankLineAfterFunction)
                {
                    pydocstyle::rules::blank_before_after_function(self, &docstring);
                }
                if self.settings.rules.enabled(&Rule::OneBlankLineBeforeClass)
                    || self.settings.rules.enabled(&Rule::OneBlankLineAfterClass)
                    || self.settings.rules.enabled(&Rule::NoBlankLineBeforeClass)
                {
                    pydocstyle::rules::blank_before_after_class(self, &docstring);
                }
                if self.settings.rules.enabled(&Rule::BlankLineAfterSummary) {
                    pydocstyle::rules::blank_after_summary(self, &docstring);
                }
                if self.settings.rules.enabled(&Rule::IndentWithSpaces)
                    || self.settings.rules.enabled(&Rule::NoUnderIndentation)
                    || self.settings.rules.enabled(&Rule::NoOverIndentation)
                {
                    pydocstyle::rules::indent(self, &docstring);
                }
                if self
                    .settings
                    .rules
                    .enabled(&Rule::NewLineAfterLastParagraph)
                {
                    pydocstyle::rules::newline_after_last_paragraph(self, &docstring);
                }
                if self.settings.rules.enabled(&Rule::NoSurroundingWhitespace) {
                    pydocstyle::rules::no_surrounding_whitespace(self, &docstring);
                }
                if self
                    .settings
                    .rules
                    .enabled(&Rule::MultiLineSummaryFirstLine)
                    || self
                        .settings
                        .rules
                        .enabled(&Rule::MultiLineSummarySecondLine)
                {
                    pydocstyle::rules::multi_line_summary_start(self, &docstring);
                }
                if self.settings.rules.enabled(&Rule::UsesTripleQuotes) {
                    pydocstyle::rules::triple_quotes(self, &docstring);
                }
                if self
                    .settings
                    .rules
                    .enabled(&Rule::UsesRPrefixForBackslashedContent)
                {
                    pydocstyle::rules::backslashes(self, &docstring);
                }
                if self.settings.rules.enabled(&Rule::EndsInPeriod) {
                    pydocstyle::rules::ends_with_period(self, &docstring);
                }
                if self.settings.rules.enabled(&Rule::NonImperativeMood) {
                    pydocstyle::rules::non_imperative_mood(self, &docstring);
                }
                if self.settings.rules.enabled(&Rule::NoSignature) {
                    pydocstyle::rules::no_signature(self, &docstring);
                }
                if self.settings.rules.enabled(&Rule::FirstLineCapitalized) {
                    pydocstyle::rules::capitalized(self, &docstring);
                }
                if self.settings.rules.enabled(&Rule::NoThisPrefix) {
                    pydocstyle::rules::starts_with_this(self, &docstring);
                }
                if self.settings.rules.enabled(&Rule::EndsInPunctuation) {
                    pydocstyle::rules::ends_with_punctuation(self, &docstring);
                }
                if self.settings.rules.enabled(&Rule::SkipDocstring) {
                    pydocstyle::rules::if_needed(self, &docstring);
                }
                if self
                    .settings
                    .rules
                    .enabled(&Rule::MultiLineSummaryFirstLine)
                    || self.settings.rules.enabled(&Rule::SectionNotOverIndented)
                    || self
                        .settings
                        .rules
                        .enabled(&Rule::SectionUnderlineNotOverIndented)
                    || self.settings.rules.enabled(&Rule::CapitalizeSectionName)
                    || self.settings.rules.enabled(&Rule::NewLineAfterSectionName)
                    || self
                        .settings
                        .rules
                        .enabled(&Rule::DashedUnderlineAfterSection)
                    || self
                        .settings
                        .rules
                        .enabled(&Rule::SectionUnderlineAfterName)
                    || self
                        .settings
                        .rules
                        .enabled(&Rule::SectionUnderlineMatchesSectionLength)
                    || self.settings.rules.enabled(&Rule::BlankLineAfterSection)
                    || self.settings.rules.enabled(&Rule::BlankLineBeforeSection)
                    || self
                        .settings
                        .rules
                        .enabled(&Rule::NoBlankLinesBetweenHeaderAndContent)
                    || self
                        .settings
                        .rules
                        .enabled(&Rule::BlankLineAfterLastSection)
                    || self.settings.rules.enabled(&Rule::NonEmptySection)
                    || self.settings.rules.enabled(&Rule::SectionNameEndsInColon)
                    || self.settings.rules.enabled(&Rule::DocumentAllArguments)
                {
                    pydocstyle::rules::sections(
                        self,
                        &docstring,
                        self.settings.pydocstyle.convention.as_ref(),
                    );
                }
            }
        }
    }

    fn check_builtin_shadowing<T>(&mut self, name: &str, located: &Located<T>, is_attribute: bool) {
        if is_attribute && matches!(self.current_scope().kind, ScopeKind::Class(_)) {
            if self
                .settings
                .rules
                .enabled(&Rule::BuiltinAttributeShadowing)
            {
                if let Some(diagnostic) = flake8_builtins::rules::builtin_shadowing(
                    name,
                    located,
                    flake8_builtins::types::ShadowingType::Attribute,
                    &self.settings.flake8_builtins.builtins_ignorelist,
                ) {
                    self.diagnostics.push(diagnostic);
                }
            }
        } else {
            if self.settings.rules.enabled(&Rule::BuiltinVariableShadowing) {
                if let Some(diagnostic) = flake8_builtins::rules::builtin_shadowing(
                    name,
                    located,
                    flake8_builtins::types::ShadowingType::Variable,
                    &self.settings.flake8_builtins.builtins_ignorelist,
                ) {
                    self.diagnostics.push(diagnostic);
                }
            }
        }
    }

    fn check_builtin_arg_shadowing(&mut self, name: &str, arg: &Arg) {
        if self.settings.rules.enabled(&Rule::BuiltinArgumentShadowing) {
            if let Some(diagnostic) = flake8_builtins::rules::builtin_shadowing(
                name,
                arg,
                flake8_builtins::types::ShadowingType::Argument,
                &self.settings.flake8_builtins.builtins_ignorelist,
            ) {
                self.diagnostics.push(diagnostic);
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub fn check_ast(
    python_ast: &Suite,
    locator: &Locator,
    stylist: &Stylist,
    indexer: &Indexer,
    noqa_line_for: &IntMap<usize, usize>,
    settings: &Settings,
    autofix: flags::Autofix,
    noqa: flags::Noqa,
    path: &Path,
    package: Option<&Path>,
) -> Vec<Diagnostic> {
    let mut checker = Checker::new(
        settings,
        noqa_line_for,
        autofix,
        noqa,
        path,
        package,
        locator,
        stylist,
        indexer,
    );
    checker.push_scope(Scope::new(ScopeKind::Module));
    checker.bind_builtins();

    // Check for module docstring.
    let python_ast = if checker.visit_docstring(python_ast) {
        &python_ast[1..]
    } else {
        python_ast
    };

    // Iterate over the AST.
    checker.visit_body(python_ast);

    // Check any deferred statements.
    checker.check_deferred_functions();
    checker.check_deferred_lambdas();
    checker.check_deferred_assignments();
    checker.check_deferred_type_definitions();
    let mut allocator = vec![];
    checker.check_deferred_string_type_definitions(&mut allocator);
    checker.check_deferred_for_loops();

    // Check docstrings.
    checker.check_definitions();

    // Reset the scope to module-level, and check all consumed scopes.
    checker.scope_stack = vec![GLOBAL_SCOPE_INDEX];
    checker.pop_scope();
    checker.check_dead_scopes();

    checker.diagnostics
}
