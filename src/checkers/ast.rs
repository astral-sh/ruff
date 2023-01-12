//! Lint rules based on AST traversal.

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

use crate::ast::helpers::{
    binding_range, collect_call_paths, dealias_call_path, extract_handler_names, match_call_path,
};
use crate::ast::operations::extract_all_names;
use crate::ast::relocate::relocate_expr;
use crate::ast::types::{
    Binding, BindingKind, ClassDef, FunctionDef, Lambda, Node, Range, RefEquality, Scope, ScopeKind,
};
use crate::ast::visitor::{walk_excepthandler, Visitor};
use crate::ast::{branch_detection, cast, helpers, operations, visitor};
use crate::docstrings::definition::{Definition, DefinitionKind, Docstring, Documentable};
use crate::noqa::Directive;
use crate::python::builtins::{BUILTINS, MAGIC_GLOBALS};
use crate::python::future::ALL_FEATURE_NAMES;
use crate::python::typing;
use crate::python::typing::SubscriptKind;
use crate::registry::{Diagnostic, RuleCode};
use crate::settings::types::PythonVersion;
use crate::settings::{flags, Settings};
use crate::source_code_locator::SourceCodeLocator;
use crate::source_code_style::SourceCodeStyleDetector;
use crate::violations::DeferralKeyword;
use crate::visibility::{module_visibility, transition_scope, Modifier, Visibility, VisibleScope};
use crate::{
    autofix, docstrings, flake8_2020, flake8_annotations, flake8_bandit, flake8_blind_except,
    flake8_boolean_trap, flake8_bugbear, flake8_builtins, flake8_comprehensions, flake8_datetimez,
    flake8_debugger, flake8_errmsg, flake8_implicit_str_concat, flake8_import_conventions,
    flake8_pie, flake8_print, flake8_pytest_style, flake8_return, flake8_simplify,
    flake8_tidy_imports, flake8_unused_arguments, mccabe, noqa, pandas_vet, pep8_naming,
    pycodestyle, pydocstyle, pyflakes, pygrep_hooks, pylint, pyupgrade, ruff, violations,
    visibility,
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
    pub(crate) style: &'a SourceCodeStyleDetector<'a>,
    // Computed diagnostics.
    pub(crate) diagnostics: Vec<Diagnostic>,
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
    pub(crate) exprs: Vec<RefEquality<'a, Expr>>,
    pub(crate) scopes: Vec<Scope<'a>>,
    pub(crate) scope_stack: Vec<usize>,
    pub(crate) dead_scopes: Vec<usize>,
    deferred_string_type_definitions: Vec<(Range, &'a str, bool, DeferralContext<'a>)>,
    deferred_type_definitions: Vec<(&'a Expr, bool, DeferralContext<'a>)>,
    deferred_functions: Vec<(&'a Stmt, DeferralContext<'a>, VisibleScope)>,
    deferred_lambdas: Vec<(&'a Expr, DeferralContext<'a>)>,
    deferred_assignments: Vec<DeferralContext<'a>>,
    // Internal, derivative state.
    visible_scope: VisibleScope,
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
        style: &'a SourceCodeStyleDetector,
    ) -> Checker<'a> {
        Checker {
            settings,
            noqa_line_for,
            autofix,
            noqa,
            path,
            locator,
            style,
            diagnostics: vec![],
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
            in_annotation: false,
            in_type_definition: false,
            in_deferred_string_type_definition: false,
            in_deferred_type_definition: false,
            in_literal: false,
            in_subscript: false,
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
    pub fn patch(&self, code: &RuleCode) -> bool {
        matches!(self.autofix, flags::Autofix::Enabled) && self.settings.fixable.contains(code)
    }

    /// Return `true` if the `Expr` is a reference to `typing.${target}`.
    pub fn match_typing_expr(&self, expr: &Expr, target: &str) -> bool {
        let call_path = dealias_call_path(collect_call_paths(expr), &self.import_aliases);
        self.match_typing_call_path(&call_path, target)
    }

    /// Return `true` if the call path is a reference to `typing.${target}`.
    pub fn match_typing_call_path(&self, call_path: &[&str], target: &str) -> bool {
        if match_call_path(call_path, "typing", target, &self.from_imports) {
            return true;
        }

        if typing::TYPING_EXTENSIONS.contains(target) {
            if match_call_path(call_path, "typing_extensions", target, &self.from_imports) {
                return true;
            }
        }

        if self
            .settings
            .typing_modules
            .iter()
            .any(|module| match_call_path(call_path, module, target, &self.from_imports))
        {
            return true;
        }

        false
    }

    /// Return the current `Binding` for a given `name`.
    pub fn find_binding(&self, member: &str) -> Option<&Binding> {
        self.current_scopes()
            .find_map(|scope| scope.values.get(member))
            .map(|index| &self.bindings[*index])
    }

    /// Return `true` if `member` is bound as a builtin.
    pub fn is_builtin(&self, member: &str) -> bool {
        self.find_binding(member).map_or(false, |binding| {
            matches!(binding.kind, BindingKind::Builtin)
        })
    }

    /// Return `true` if a `RuleCode` is disabled by a `noqa` directive.
    pub fn is_ignored(&self, code: &RuleCode, lineno: usize) -> bool {
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
                    let scope = &mut self.scopes[scope_index];
                    let usage = Some((scope.id, Range::from_located(stmt)));
                    for (name, range) in names.iter().zip(ranges.iter()) {
                        let index = self.bindings.len();
                        self.bindings.push(Binding {
                            kind: BindingKind::Global,
                            used: usage,
                            range: *range,
                            source: Some(RefEquality(stmt)),
                        });
                        scope.values.insert(name, index);
                    }
                }

                if self.settings.enabled.contains(&RuleCode::E741) {
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
                    let scope = &mut self.scopes[scope_index];
                    let usage = Some((scope.id, Range::from_located(stmt)));
                    for (name, range) in names.iter().zip(ranges.iter()) {
                        // Add a binding to the current scope.
                        let index = self.bindings.len();
                        self.bindings.push(Binding {
                            kind: BindingKind::Nonlocal,
                            used: usage,
                            range: *range,
                            source: Some(RefEquality(stmt)),
                        });
                        scope.values.insert(name, index);
                    }

                    // Mark the binding in the defining scopes as used too. (Skip the global scope
                    // and the current scope.)
                    for (name, range) in names.iter().zip(ranges.iter()) {
                        let mut exists = false;
                        for index in self.scope_stack.iter().skip(1).rev().skip(1) {
                            if let Some(index) = self.scopes[*index].values.get(&name.as_str()) {
                                exists = true;
                                self.bindings[*index].used = usage;
                            }
                        }

                        // Ensure that every nonlocal has an existing binding from a parent scope.
                        if !exists {
                            if self.settings.enabled.contains(&RuleCode::PLE0117) {
                                self.diagnostics.push(Diagnostic::new(
                                    violations::NonlocalWithoutBinding(name.to_string()),
                                    *range,
                                ));
                            }
                        }
                    }
                }

                if self.settings.enabled.contains(&RuleCode::E741) {
                    self.diagnostics
                        .extend(names.iter().zip(ranges.iter()).filter_map(|(name, range)| {
                            pycodestyle::rules::ambiguous_variable_name(name, *range)
                        }));
                }
            }
            StmtKind::Break => {
                if self.settings.enabled.contains(&RuleCode::F701) {
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
                if self.settings.enabled.contains(&RuleCode::F702) {
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
                if self.settings.enabled.contains(&RuleCode::E743) {
                    if let Some(diagnostic) =
                        pycodestyle::rules::ambiguous_function_name(name, || {
                            helpers::identifier_range(stmt, self.locator)
                        })
                    {
                        self.diagnostics.push(diagnostic);
                    }
                }

                if self.settings.enabled.contains(&RuleCode::N802) {
                    if let Some(diagnostic) = pep8_naming::rules::invalid_function_name(
                        stmt,
                        name,
                        &self.settings.pep8_naming.ignore_names,
                        self.locator,
                    ) {
                        self.diagnostics.push(diagnostic);
                    }
                }

                if self.settings.enabled.contains(&RuleCode::N804) {
                    if let Some(diagnostic) =
                        pep8_naming::rules::invalid_first_argument_name_for_class_method(
                            self.current_scope(),
                            name,
                            decorator_list,
                            args,
                            &self.from_imports,
                            &self.import_aliases,
                            &self.settings.pep8_naming,
                        )
                    {
                        self.diagnostics.push(diagnostic);
                    }
                }

                if self.settings.enabled.contains(&RuleCode::N805) {
                    if let Some(diagnostic) =
                        pep8_naming::rules::invalid_first_argument_name_for_method(
                            self.current_scope(),
                            name,
                            decorator_list,
                            args,
                            &self.from_imports,
                            &self.import_aliases,
                            &self.settings.pep8_naming,
                        )
                    {
                        self.diagnostics.push(diagnostic);
                    }
                }

                if self.settings.enabled.contains(&RuleCode::N807) {
                    if let Some(diagnostic) = pep8_naming::rules::dunder_function_name(
                        self.current_scope(),
                        stmt,
                        name,
                        self.locator,
                    ) {
                        self.diagnostics.push(diagnostic);
                    }
                }

                if self.settings.enabled.contains(&RuleCode::UP011)
                    && self.settings.target_version >= PythonVersion::Py38
                {
                    pyupgrade::rules::unnecessary_lru_cache_params(self, decorator_list);
                }

                if self.settings.enabled.contains(&RuleCode::B018) {
                    flake8_bugbear::rules::useless_expression(self, body);
                }

                if self.settings.enabled.contains(&RuleCode::B019) {
                    flake8_bugbear::rules::cached_instance_method(self, decorator_list);
                }

                if self.settings.enabled.contains(&RuleCode::RET501)
                    || self.settings.enabled.contains(&RuleCode::RET502)
                    || self.settings.enabled.contains(&RuleCode::RET503)
                    || self.settings.enabled.contains(&RuleCode::RET504)
                    || self.settings.enabled.contains(&RuleCode::RET505)
                    || self.settings.enabled.contains(&RuleCode::RET506)
                    || self.settings.enabled.contains(&RuleCode::RET507)
                    || self.settings.enabled.contains(&RuleCode::RET508)
                {
                    flake8_return::rules::function(self, body);
                }

                if self.settings.enabled.contains(&RuleCode::C901) {
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

                if self.settings.enabled.contains(&RuleCode::S107) {
                    self.diagnostics
                        .extend(flake8_bandit::rules::hardcoded_password_default(args));
                }

                if self.settings.enabled.contains(&RuleCode::PLR0206) {
                    pylint::rules::property_with_parameters(self, stmt, decorator_list, args);
                }

                if self.settings.enabled.contains(&RuleCode::PT001)
                    || self.settings.enabled.contains(&RuleCode::PT002)
                    || self.settings.enabled.contains(&RuleCode::PT003)
                    || self.settings.enabled.contains(&RuleCode::PT004)
                    || self.settings.enabled.contains(&RuleCode::PT005)
                    || self.settings.enabled.contains(&RuleCode::PT019)
                    || self.settings.enabled.contains(&RuleCode::PT020)
                    || self.settings.enabled.contains(&RuleCode::PT021)
                    || self.settings.enabled.contains(&RuleCode::PT022)
                    || self.settings.enabled.contains(&RuleCode::PT024)
                    || self.settings.enabled.contains(&RuleCode::PT025)
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

                if self.settings.enabled.contains(&RuleCode::PT006)
                    || self.settings.enabled.contains(&RuleCode::PT007)
                {
                    flake8_pytest_style::rules::parametrize(self, decorator_list);
                }

                if self.settings.enabled.contains(&RuleCode::PT023)
                    || self.settings.enabled.contains(&RuleCode::PT026)
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
                if self.settings.enabled.contains(&RuleCode::F706) {
                    if let Some(&index) = self.scope_stack.last() {
                        if matches!(
                            self.scopes[index].kind,
                            ScopeKind::Class(_) | ScopeKind::Module
                        ) {
                            self.diagnostics.push(Diagnostic::new(
                                violations::ReturnOutsideFunction,
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
                if self.settings.enabled.contains(&RuleCode::UP004) {
                    pyupgrade::rules::useless_object_inheritance(self, stmt, name, bases, keywords);
                }

                if self.settings.enabled.contains(&RuleCode::E742) {
                    if let Some(diagnostic) = pycodestyle::rules::ambiguous_class_name(name, || {
                        helpers::identifier_range(stmt, self.locator)
                    }) {
                        self.diagnostics.push(diagnostic);
                    }
                }

                if self.settings.enabled.contains(&RuleCode::N801) {
                    if let Some(diagnostic) =
                        pep8_naming::rules::invalid_class_name(stmt, name, self.locator)
                    {
                        self.diagnostics.push(diagnostic);
                    }
                }

                if self.settings.enabled.contains(&RuleCode::N818) {
                    if let Some(diagnostic) = pep8_naming::rules::error_suffix_on_exception_name(
                        stmt,
                        bases,
                        name,
                        self.locator,
                    ) {
                        self.diagnostics.push(diagnostic);
                    }
                }

                if self.settings.enabled.contains(&RuleCode::B018) {
                    flake8_bugbear::rules::useless_expression(self, body);
                }

                if self.settings.enabled.contains(&RuleCode::B024)
                    || self.settings.enabled.contains(&RuleCode::B027)
                {
                    flake8_bugbear::rules::abstract_base_class(
                        self, stmt, name, bases, keywords, body,
                    );
                }

                if self.settings.enabled.contains(&RuleCode::PT023) {
                    flake8_pytest_style::rules::marks(self, decorator_list);
                }

                if self.settings.enabled.contains(&RuleCode::PIE794) {
                    flake8_pie::rules::dupe_class_field_definitions(self, stmt, body);
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
                if self.settings.enabled.contains(&RuleCode::E401) {
                    if names.len() > 1 {
                        self.diagnostics.push(Diagnostic::new(
                            violations::MultipleImportsOnOneLine,
                            Range::from_located(stmt),
                        ));
                    }
                }

                if self.settings.enabled.contains(&RuleCode::E402) {
                    if self.seen_import_boundary && stmt.location.column() == 0 {
                        self.diagnostics.push(Diagnostic::new(
                            violations::ModuleImportNotAtTopOfFile,
                            Range::from_located(stmt),
                        ));
                    }
                }
                if self.settings.enabled.contains(&RuleCode::UP023) {
                    pyupgrade::rules::replace_c_element_tree(self, stmt);
                }
                if self.settings.enabled.contains(&RuleCode::UP026) {
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
                    if self.settings.enabled.contains(&RuleCode::T100) {
                        if let Some(diagnostic) =
                            flake8_debugger::rules::debugger_import(stmt, None, &alias.node.name)
                        {
                            self.diagnostics.push(diagnostic);
                        }
                    }

                    // flake8_tidy_imports
                    if self.settings.enabled.contains(&RuleCode::TID251) {
                        if let Some(diagnostic) =
                            flake8_tidy_imports::rules::name_or_parent_is_banned(
                                alias,
                                &alias.node.name,
                                &self.settings.flake8_tidy_imports.banned_api,
                            )
                        {
                            self.diagnostics.push(diagnostic);
                        }
                    }

                    // pylint
                    if self.settings.enabled.contains(&RuleCode::PLC0414) {
                        pylint::rules::useless_import_alias(self, alias);
                    }
                    if self.settings.enabled.contains(&RuleCode::PLR0402) {
                        pylint::rules::use_from_import(self, alias);
                    }

                    if let Some(asname) = &alias.node.asname {
                        for alias in names {
                            if let Some(asname) = &alias.node.asname {
                                self.import_aliases.insert(asname, &alias.node.name);
                            }
                        }

                        let name = alias.node.name.split('.').last().unwrap();
                        if self.settings.enabled.contains(&RuleCode::N811) {
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

                        if self.settings.enabled.contains(&RuleCode::N812) {
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

                        if self.settings.enabled.contains(&RuleCode::N813) {
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

                        if self.settings.enabled.contains(&RuleCode::N814) {
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

                        if self.settings.enabled.contains(&RuleCode::N817) {
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

                    if self.settings.enabled.contains(&RuleCode::ICN001) {
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

                    if self.settings.enabled.contains(&RuleCode::PT013) {
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
                // Track `import from` statements, to ensure that we can correctly attribute
                // references like `from typing import Union`.
                if self.settings.enabled.contains(&RuleCode::UP023) {
                    pyupgrade::rules::replace_c_element_tree(self, stmt);
                }
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

                if self.settings.enabled.contains(&RuleCode::E402) {
                    if self.seen_import_boundary && stmt.location.column() == 0 {
                        self.diagnostics.push(Diagnostic::new(
                            violations::ModuleImportNotAtTopOfFile,
                            Range::from_located(stmt),
                        ));
                    }
                }

                if self.settings.enabled.contains(&RuleCode::UP010) {
                    if let Some("__future__") = module.as_deref() {
                        pyupgrade::rules::unnecessary_future_import(self, stmt, names);
                    }
                }
                if self.settings.enabled.contains(&RuleCode::UP026) {
                    pyupgrade::rules::rewrite_mock_import(self, stmt);
                }
                if self.settings.enabled.contains(&RuleCode::UP029) {
                    if let Some(module) = module.as_deref() {
                        pyupgrade::rules::unnecessary_builtin_import(self, stmt, module, names);
                    }
                }

                if self.settings.enabled.contains(&RuleCode::TID251) {
                    if let Some(module) = module {
                        for name in names {
                            if let Some(diagnostic) = flake8_tidy_imports::rules::name_is_banned(
                                module,
                                name,
                                &self.settings.flake8_tidy_imports.banned_api,
                            ) {
                                self.diagnostics.push(diagnostic);
                            }
                        }
                        if let Some(diagnostic) =
                            flake8_tidy_imports::rules::name_or_parent_is_banned(
                                stmt,
                                module,
                                &self.settings.flake8_tidy_imports.banned_api,
                            )
                        {
                            self.diagnostics.push(diagnostic);
                        }
                    }
                }

                if self.settings.enabled.contains(&RuleCode::PT013) {
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

                        if self.settings.enabled.contains(&RuleCode::F407) {
                            if !ALL_FEATURE_NAMES.contains(&&*alias.node.name) {
                                self.diagnostics.push(Diagnostic::new(
                                    violations::FutureFeatureNotDefined(
                                        alias.node.name.to_string(),
                                    ),
                                    Range::from_located(alias),
                                ));
                            }
                        }

                        if self.settings.enabled.contains(&RuleCode::F404) && !self.futures_allowed
                        {
                            self.diagnostics.push(Diagnostic::new(
                                violations::LateFutureImport,
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

                        if self.settings.enabled.contains(&RuleCode::F406) {
                            let scope = &self.scopes
                                [*(self.scope_stack.last().expect("No current scope found"))];
                            if !matches!(scope.kind, ScopeKind::Module) {
                                self.diagnostics.push(Diagnostic::new(
                                    violations::ImportStarNotPermitted(
                                        helpers::format_import_from(
                                            level.as_ref(),
                                            module.as_deref(),
                                        ),
                                    ),
                                    Range::from_located(stmt),
                                ));
                            }
                        }

                        if self.settings.enabled.contains(&RuleCode::F403) {
                            self.diagnostics.push(Diagnostic::new(
                                violations::ImportStarUsed(helpers::format_import_from(
                                    level.as_ref(),
                                    module.as_deref(),
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

                    if self.settings.enabled.contains(&RuleCode::TID252) {
                        if let Some(diagnostic) = flake8_tidy_imports::rules::banned_relative_import(
                            stmt,
                            level.as_ref(),
                            &self.settings.flake8_tidy_imports.ban_relative_imports,
                        ) {
                            self.diagnostics.push(diagnostic);
                        }
                    }

                    // flake8-debugger
                    if self.settings.enabled.contains(&RuleCode::T100) {
                        if let Some(diagnostic) = flake8_debugger::rules::debugger_import(
                            stmt,
                            module.as_deref(),
                            &alias.node.name,
                        ) {
                            self.diagnostics.push(diagnostic);
                        }
                    }

                    if let Some(asname) = &alias.node.asname {
                        if self.settings.enabled.contains(&RuleCode::N811) {
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

                        if self.settings.enabled.contains(&RuleCode::N812) {
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

                        if self.settings.enabled.contains(&RuleCode::N813) {
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

                        if self.settings.enabled.contains(&RuleCode::N814) {
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

                        if self.settings.enabled.contains(&RuleCode::N817) {
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
                        if self.settings.enabled.contains(&RuleCode::PLC0414) {
                            pylint::rules::useless_import_alias(self, alias);
                        }
                    }
                }
            }
            StmtKind::Raise { exc, .. } => {
                if self.settings.enabled.contains(&RuleCode::F901) {
                    if let Some(expr) = exc {
                        pyflakes::rules::raise_not_implemented(self, expr);
                    }
                }
                if self.settings.enabled.contains(&RuleCode::B016) {
                    if let Some(exc) = exc {
                        flake8_bugbear::rules::cannot_raise_literal(self, exc);
                    }
                }
                if self.settings.enabled.contains(&RuleCode::EM101)
                    || self.settings.enabled.contains(&RuleCode::EM102)
                    || self.settings.enabled.contains(&RuleCode::EM103)
                {
                    if let Some(exc) = exc {
                        flake8_errmsg::rules::string_in_exception(self, exc);
                    }
                }
                if self.settings.enabled.contains(&RuleCode::UP024) {
                    if let Some(item) = exc {
                        pyupgrade::rules::os_error_alias(self, &item);
                    }
                }
            }
            StmtKind::AugAssign { target, .. } => {
                self.handle_node_load(target);
            }
            StmtKind::If { test, .. } => {
                if self.settings.enabled.contains(&RuleCode::F634) {
                    pyflakes::rules::if_tuple(self, stmt, test);
                }
                if self.settings.enabled.contains(&RuleCode::SIM102) {
                    flake8_simplify::rules::nested_if_statements(self, stmt);
                }
                if self.settings.enabled.contains(&RuleCode::SIM103) {
                    flake8_simplify::rules::return_bool_condition_directly(self, stmt);
                }
                if self.settings.enabled.contains(&RuleCode::SIM108) {
                    flake8_simplify::rules::use_ternary_operator(
                        self,
                        stmt,
                        self.current_stmt_parent().map(|parent| parent.0),
                    );
                }
            }
            StmtKind::Assert { test, msg } => {
                if self.settings.enabled.contains(&RuleCode::F631) {
                    pyflakes::rules::assert_tuple(self, stmt, test);
                }
                if self.settings.enabled.contains(&RuleCode::B011) {
                    flake8_bugbear::rules::assert_false(
                        self,
                        stmt,
                        test,
                        msg.as_ref().map(|expr| &**expr),
                    );
                }
                if self.settings.enabled.contains(&RuleCode::S101) {
                    self.diagnostics
                        .push(flake8_bandit::rules::assert_used(stmt));
                }
                if self.settings.enabled.contains(&RuleCode::PT015) {
                    if let Some(diagnostic) = flake8_pytest_style::rules::assert_falsy(stmt, test) {
                        self.diagnostics.push(diagnostic);
                    }
                }
                if self.settings.enabled.contains(&RuleCode::PT018) {
                    if let Some(diagnostic) =
                        flake8_pytest_style::rules::composite_condition(stmt, test)
                    {
                        self.diagnostics.push(diagnostic);
                    }
                }
            }
            StmtKind::With { items, body, .. } | StmtKind::AsyncWith { items, body, .. } => {
                if self.settings.enabled.contains(&RuleCode::B017) {
                    flake8_bugbear::rules::assert_raises_exception(self, stmt, items);
                }
                if self.settings.enabled.contains(&RuleCode::PT012) {
                    flake8_pytest_style::rules::complex_raises(self, stmt, items, body);
                }
                if self.settings.enabled.contains(&RuleCode::SIM117) {
                    flake8_simplify::rules::multiple_with_statements(self, stmt);
                }
            }
            StmtKind::While { body, orelse, .. } => {
                if self.settings.enabled.contains(&RuleCode::B023) {
                    flake8_bugbear::rules::function_uses_loop_variable(self, &Node::Stmt(stmt));
                }
                if self.settings.enabled.contains(&RuleCode::PLW0120) {
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
                if self.settings.enabled.contains(&RuleCode::B007) {
                    flake8_bugbear::rules::unused_loop_control_variable(self, target, body);
                }
                if self.settings.enabled.contains(&RuleCode::B020) {
                    flake8_bugbear::rules::loop_variable_overrides_iterator(self, target, iter);
                }
                if self.settings.enabled.contains(&RuleCode::B023) {
                    flake8_bugbear::rules::function_uses_loop_variable(self, &Node::Stmt(stmt));
                }
                if self.settings.enabled.contains(&RuleCode::PLW0120) {
                    pylint::rules::useless_else_on_loop(self, stmt, body, orelse);
                }
                if self.settings.enabled.contains(&RuleCode::SIM118) {
                    flake8_simplify::rules::key_in_dict_for(self, target, iter);
                }
            }
            StmtKind::Try {
                body,
                handlers,
                orelse,
                finalbody,
                ..
            } => {
                if self.settings.enabled.contains(&RuleCode::F707) {
                    if let Some(diagnostic) =
                        pyflakes::rules::default_except_not_last(handlers, self.locator)
                    {
                        self.diagnostics.push(diagnostic);
                    }
                }
                if self.settings.enabled.contains(&RuleCode::B014)
                    || self.settings.enabled.contains(&RuleCode::B025)
                {
                    flake8_bugbear::rules::duplicate_exceptions(self, handlers);
                }
                if self.settings.enabled.contains(&RuleCode::B013) {
                    flake8_bugbear::rules::redundant_tuple_in_exception_handler(self, handlers);
                }
                if self.settings.enabled.contains(&RuleCode::UP024) {
                    pyupgrade::rules::os_error_alias(self, &handlers);
                }
                if self.settings.enabled.contains(&RuleCode::PT017) {
                    self.diagnostics.extend(
                        flake8_pytest_style::rules::assert_in_exception_handler(handlers),
                    );
                }
                if self.settings.enabled.contains(&RuleCode::SIM105) {
                    flake8_simplify::rules::use_contextlib_suppress(
                        self, stmt, handlers, orelse, finalbody,
                    );
                }
                if self.settings.enabled.contains(&RuleCode::SIM107) {
                    flake8_simplify::rules::return_in_try_except_finally(
                        self, body, handlers, finalbody,
                    );
                }
            }
            StmtKind::Assign { targets, value, .. } => {
                if self.settings.enabled.contains(&RuleCode::E731) {
                    if let [target] = &targets[..] {
                        pycodestyle::rules::do_not_assign_lambda(self, target, value, stmt);
                    }
                }

                if self.settings.enabled.contains(&RuleCode::B003) {
                    flake8_bugbear::rules::assignment_to_os_environ(self, targets);
                }

                if self.settings.enabled.contains(&RuleCode::S105) {
                    if let Some(diagnostic) =
                        flake8_bandit::rules::assign_hardcoded_password_string(value, targets)
                    {
                        self.diagnostics.push(diagnostic);
                    }
                }

                if self.settings.enabled.contains(&RuleCode::UP001) {
                    pyupgrade::rules::useless_metaclass_type(self, stmt, value, targets);
                }
                if self.settings.enabled.contains(&RuleCode::UP013) {
                    pyupgrade::rules::convert_typed_dict_functional_to_class(
                        self, stmt, targets, value,
                    );
                }
                if self.settings.enabled.contains(&RuleCode::UP014) {
                    pyupgrade::rules::convert_named_tuple_functional_to_class(
                        self, stmt, targets, value,
                    );
                }
                if self.settings.enabled.contains(&RuleCode::UP027) {
                    pyupgrade::rules::unpack_list_comprehension(self, targets, value);
                }

                if self.settings.enabled.contains(&RuleCode::PD901) {
                    if let Some(diagnostic) = pandas_vet::rules::assignment_to_df(targets) {
                        self.diagnostics.push(diagnostic);
                    }
                }
            }
            StmtKind::AnnAssign { target, value, .. } => {
                if self.settings.enabled.contains(&RuleCode::E731) {
                    if let Some(value) = value {
                        pycodestyle::rules::do_not_assign_lambda(self, target, value, stmt);
                    }
                }
            }
            StmtKind::Delete { .. } => {}
            StmtKind::Expr { value, .. } => {
                if self.settings.enabled.contains(&RuleCode::B015) {
                    flake8_bugbear::rules::useless_comparison(self, value);
                }
                if self.settings.enabled.contains(&RuleCode::SIM112) {
                    flake8_simplify::rules::use_capital_environment_variables(self, value);
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
                if self.settings.enabled.contains(&RuleCode::B021) {
                    flake8_bugbear::rules::f_string_docstring(self, body);
                }
                let definition = docstrings::extraction::extract(
                    &self.visible_scope,
                    stmt,
                    body,
                    &Documentable::Function,
                );
                if self.settings.enabled.contains(&RuleCode::UP028) {
                    pyupgrade::rules::rewrite_yield_from(self, stmt);
                }
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
                if self.settings.enabled.contains(&RuleCode::B021) {
                    flake8_bugbear::rules::f_string_docstring(self, body);
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

                self.visit_body(body);
            }
            StmtKind::Try {
                body,
                handlers,
                orelse,
                finalbody,
            } => {
                self.except_handlers.push(extract_handler_names(handlers));
                if self.settings.enabled.contains(&RuleCode::B012) {
                    flake8_bugbear::rules::jump_statement_in_finally(self, finalbody);
                }
                self.visit_body(body);
                self.except_handlers.pop();
                for excepthandler in handlers {
                    self.visit_excepthandler(excepthandler);
                }
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

        let prev_in_literal = self.in_literal;
        let prev_in_type_definition = self.in_type_definition;

        // Pre-visit.
        match &expr.node {
            ExprKind::Subscript { value, slice, .. } => {
                // Ex) Optional[...]
                if !self.in_deferred_string_type_definition
                    && self.in_annotation
                    && self.settings.enabled.contains(&RuleCode::UP007)
                    && (self.settings.target_version >= PythonVersion::Py310
                        || (self.settings.target_version >= PythonVersion::Py37
                            && !self.settings.pyupgrade.keep_runtime_typing
                            && self.annotations_future_enabled))
                {
                    pyupgrade::rules::use_pep604_annotation(self, expr, value, slice);
                }

                if self.match_typing_expr(value, "Literal") {
                    self.in_literal = true;
                }

                if self.settings.enabled.contains(&RuleCode::YTT101)
                    || self.settings.enabled.contains(&RuleCode::YTT102)
                    || self.settings.enabled.contains(&RuleCode::YTT301)
                    || self.settings.enabled.contains(&RuleCode::YTT303)
                {
                    flake8_2020::rules::subscript(self, value, slice);
                }
            }
            ExprKind::Tuple { elts, ctx } | ExprKind::List { elts, ctx } => {
                if matches!(ctx, ExprContext::Store) {
                    let check_too_many_expressions =
                        self.settings.enabled.contains(&RuleCode::F621);
                    let check_two_starred_expressions =
                        self.settings.enabled.contains(&RuleCode::F622);
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
                        if self.settings.enabled.contains(&RuleCode::UP019) {
                            pyupgrade::rules::typing_text_str_alias(self, expr);
                        }

                        // Ex) List[...]
                        if !self.in_deferred_string_type_definition
                            && self.settings.enabled.contains(&RuleCode::UP006)
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
                            pyupgrade::rules::use_pep585_annotation(self, expr, id);
                        }

                        self.handle_node_load(expr);
                    }
                    ExprContext::Store => {
                        if self.settings.enabled.contains(&RuleCode::E741) {
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

                if self.settings.enabled.contains(&RuleCode::YTT202) {
                    flake8_2020::rules::name_or_attribute(self, expr);
                }

                if self.settings.enabled.contains(&RuleCode::PLE0118) {
                    pylint::rules::used_prior_global_declaration(self, id, expr);
                }
            }
            ExprKind::Attribute { attr, value, .. } => {
                // Ex) typing.List[...]
                if !self.in_deferred_string_type_definition
                    && self.settings.enabled.contains(&RuleCode::UP006)
                    && (self.settings.target_version >= PythonVersion::Py39
                        || (self.settings.target_version >= PythonVersion::Py37
                            && self.annotations_future_enabled
                            && self.in_annotation))
                    && typing::is_pep585_builtin(expr, &self.from_imports, &self.import_aliases)
                {
                    pyupgrade::rules::use_pep585_annotation(self, expr, attr);
                }

                if self.settings.enabled.contains(&RuleCode::UP016) {
                    pyupgrade::rules::remove_six_compat(self, expr);
                }

                if self.settings.enabled.contains(&RuleCode::UP017)
                    && self.settings.target_version >= PythonVersion::Py311
                {
                    pyupgrade::rules::datetime_utc_alias(self, expr);
                }
                if self.settings.enabled.contains(&RuleCode::UP019) {
                    pyupgrade::rules::typing_text_str_alias(self, expr);
                }
                if self.settings.enabled.contains(&RuleCode::UP026) {
                    pyupgrade::rules::rewrite_mock_attribute(self, expr);
                }

                if self.settings.enabled.contains(&RuleCode::YTT202) {
                    flake8_2020::rules::name_or_attribute(self, expr);
                }

                for (code, name) in vec![
                    (RuleCode::PD007, "ix"),
                    (RuleCode::PD008, "at"),
                    (RuleCode::PD009, "iat"),
                    (RuleCode::PD011, "values"),
                ] {
                    if self.settings.enabled.contains(&code) {
                        if attr == name {
                            // Avoid flagging on function calls (e.g., `df.values()`).
                            if let Some(parent) = self.current_expr_parent() {
                                if matches!(parent.node, ExprKind::Call { .. }) {
                                    continue;
                                }
                            }
                            // Avoid flagging on non-DataFrames (e.g., `{"a": 1}.values`).
                            if pandas_vet::helpers::is_dataframe_candidate(value) {
                                // If the target is a named variable, avoid triggering on
                                // irrelevant bindings (like imports).
                                if let ExprKind::Name { id, .. } = &value.node {
                                    if self.find_binding(id).map_or(true, |binding| {
                                        matches!(
                                            binding.kind,
                                            BindingKind::Builtin
                                                | BindingKind::ClassDefinition
                                                | BindingKind::FunctionDefinition
                                                | BindingKind::Export(..)
                                                | BindingKind::FutureImportation
                                                | BindingKind::StarImportation(..)
                                                | BindingKind::Importation(..)
                                                | BindingKind::FromImportation(..)
                                                | BindingKind::SubmoduleImportation(..)
                                        )
                                    }) {
                                        continue;
                                    }
                                }

                                self.diagnostics
                                    .push(Diagnostic::new(code.kind(), Range::from_located(expr)));
                            }
                        };
                    }
                }

                if self.settings.enabled.contains(&RuleCode::TID251) {
                    flake8_tidy_imports::rules::banned_attribute_access(
                        self,
                        &dealias_call_path(collect_call_paths(expr), &self.import_aliases),
                        expr,
                        &self.settings.flake8_tidy_imports.banned_api,
                    );
                }
            }
            ExprKind::Call {
                func,
                args,
                keywords,
            } => {
                // pyflakes
                if self.settings.enabled.contains(&RuleCode::F521)
                    || self.settings.enabled.contains(&RuleCode::F522)
                    || self.settings.enabled.contains(&RuleCode::F523)
                    || self.settings.enabled.contains(&RuleCode::F524)
                    || self.settings.enabled.contains(&RuleCode::F525)
                    // pyupgrade
                    || self.settings.enabled.contains(&RuleCode::UP030)
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
                                        if self.settings.enabled.contains(&RuleCode::F521) {
                                            self.diagnostics.push(Diagnostic::new(
                                                violations::StringDotFormatInvalidFormat(
                                                    pyflakes::format::error_to_string(&e),
                                                ),
                                                location,
                                            ));
                                        }
                                    }
                                    Ok(summary) => {
                                        if self.settings.enabled.contains(&RuleCode::F522) {
                                            pyflakes::rules::string_dot_format_extra_named_arguments(self,
                                                                                                     &summary, keywords, location,
                                            );
                                        }

                                        if self.settings.enabled.contains(&RuleCode::F523) {
                                            pyflakes::rules::string_dot_format_extra_positional_arguments(
                                                self,
                                                &summary, args, location,
                                            );
                                        }

                                        if self.settings.enabled.contains(&RuleCode::F524) {
                                            pyflakes::rules::string_dot_format_missing_argument(
                                                self, &summary, args, keywords, location,
                                            );
                                        }

                                        if self.settings.enabled.contains(&RuleCode::F525) {
                                            pyflakes::rules::string_dot_format_mixing_automatic(
                                                self, &summary, location,
                                            );
                                        }

                                        if self.settings.enabled.contains(&RuleCode::UP030) {
                                            pyupgrade::rules::format_literals(self, &summary, expr);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                // pyupgrade
                if self.settings.enabled.contains(&RuleCode::UP003) {
                    pyupgrade::rules::type_of_primitive(self, expr, func, args);
                }
                if self.settings.enabled.contains(&RuleCode::UP005) {
                    pyupgrade::rules::deprecated_unittest_alias(self, func);
                }
                if self.settings.enabled.contains(&RuleCode::UP008) {
                    pyupgrade::rules::super_call_with_parameters(self, expr, func, args);
                }
                if self.settings.enabled.contains(&RuleCode::UP012) {
                    pyupgrade::rules::unnecessary_encode_utf8(self, expr, func, args, keywords);
                }
                if self.settings.enabled.contains(&RuleCode::UP015) {
                    pyupgrade::rules::redundant_open_modes(self, expr);
                }
                if self.settings.enabled.contains(&RuleCode::UP016) {
                    pyupgrade::rules::remove_six_compat(self, expr);
                }
                if self.settings.enabled.contains(&RuleCode::UP018) {
                    pyupgrade::rules::native_literals(self, expr, func, args, keywords);
                }
                if self.settings.enabled.contains(&RuleCode::UP020) {
                    pyupgrade::rules::open_alias(self, expr, func);
                }
                if self.settings.enabled.contains(&RuleCode::UP021) {
                    pyupgrade::rules::replace_universal_newlines(self, expr, keywords);
                }
                if self.settings.enabled.contains(&RuleCode::UP022) {
                    pyupgrade::rules::replace_stdout_stderr(self, expr, keywords);
                }
                if self.settings.enabled.contains(&RuleCode::UP024) {
                    pyupgrade::rules::os_error_alias(self, &expr);
                }

                // flake8-print
                if self.settings.enabled.contains(&RuleCode::T201)
                    || self.settings.enabled.contains(&RuleCode::T203)
                {
                    flake8_print::rules::print_call(self, func, keywords);
                }

                // flake8-bugbear
                if self.settings.enabled.contains(&RuleCode::B004) {
                    flake8_bugbear::rules::unreliable_callable_check(self, expr, func, args);
                }
                if self.settings.enabled.contains(&RuleCode::B005) {
                    flake8_bugbear::rules::strip_with_multi_characters(self, expr, func, args);
                }
                if self.settings.enabled.contains(&RuleCode::B009) {
                    flake8_bugbear::rules::getattr_with_constant(self, expr, func, args);
                }
                if self.settings.enabled.contains(&RuleCode::B010) {
                    flake8_bugbear::rules::setattr_with_constant(self, expr, func, args);
                }
                if self.settings.enabled.contains(&RuleCode::B022) {
                    flake8_bugbear::rules::useless_contextlib_suppress(self, expr, args);
                }
                if self.settings.enabled.contains(&RuleCode::B026) {
                    flake8_bugbear::rules::star_arg_unpacking_after_keyword_arg(
                        self, args, keywords,
                    );
                }
                if self.settings.enabled.contains(&RuleCode::B905)
                    && self.settings.target_version >= PythonVersion::Py310
                {
                    flake8_bugbear::rules::zip_without_explicit_strict(self, expr, func, keywords);
                }

                // flake8-bandit
                if self.settings.enabled.contains(&RuleCode::S102) {
                    if let Some(diagnostic) = flake8_bandit::rules::exec_used(expr, func) {
                        self.diagnostics.push(diagnostic);
                    }
                }
                if self.settings.enabled.contains(&RuleCode::S103) {
                    if let Some(diagnostic) = flake8_bandit::rules::bad_file_permissions(
                        func,
                        args,
                        keywords,
                        &self.from_imports,
                        &self.import_aliases,
                    ) {
                        self.diagnostics.push(diagnostic);
                    }
                }
                if self.settings.enabled.contains(&RuleCode::S501) {
                    if let Some(diagnostic) = flake8_bandit::rules::request_with_no_cert_validation(
                        func,
                        args,
                        keywords,
                        &self.from_imports,
                        &self.import_aliases,
                    ) {
                        self.diagnostics.push(diagnostic);
                    }
                }
                if self.settings.enabled.contains(&RuleCode::S506) {
                    if let Some(diagnostic) = flake8_bandit::rules::unsafe_yaml_load(
                        func,
                        args,
                        keywords,
                        &self.from_imports,
                        &self.import_aliases,
                    ) {
                        self.diagnostics.push(diagnostic);
                    }
                }
                if self.settings.enabled.contains(&RuleCode::S508) {
                    if let Some(diagnostic) = flake8_bandit::rules::snmp_insecure_version(
                        func,
                        args,
                        keywords,
                        &self.from_imports,
                        &self.import_aliases,
                    ) {
                        self.diagnostics.push(diagnostic);
                    }
                }
                if self.settings.enabled.contains(&RuleCode::S509) {
                    if let Some(diagnostic) = flake8_bandit::rules::snmp_weak_cryptography(
                        func,
                        args,
                        keywords,
                        &self.from_imports,
                        &self.import_aliases,
                    ) {
                        self.diagnostics.push(diagnostic);
                    }
                }
                if self.settings.enabled.contains(&RuleCode::S106) {
                    self.diagnostics
                        .extend(flake8_bandit::rules::hardcoded_password_func_arg(keywords));
                }
                if self.settings.enabled.contains(&RuleCode::S324) {
                    if let Some(diagnostic) = flake8_bandit::rules::hashlib_insecure_hash_functions(
                        func,
                        args,
                        keywords,
                        &self.from_imports,
                        &self.import_aliases,
                    ) {
                        self.diagnostics.push(diagnostic);
                    }
                }
                if self.settings.enabled.contains(&RuleCode::S113) {
                    if let Some(diagnostic) = flake8_bandit::rules::request_without_timeout(
                        func,
                        args,
                        keywords,
                        &self.from_imports,
                        &self.import_aliases,
                    ) {
                        self.diagnostics.push(diagnostic);
                    }
                }

                // flake8-comprehensions
                if self.settings.enabled.contains(&RuleCode::C400) {
                    flake8_comprehensions::rules::unnecessary_generator_list(
                        self, expr, func, args, keywords,
                    );
                }
                if self.settings.enabled.contains(&RuleCode::C401) {
                    flake8_comprehensions::rules::unnecessary_generator_set(
                        self, expr, func, args, keywords,
                    );
                }
                if self.settings.enabled.contains(&RuleCode::C402) {
                    flake8_comprehensions::rules::unnecessary_generator_dict(
                        self, expr, func, args, keywords,
                    );
                }
                if self.settings.enabled.contains(&RuleCode::C403) {
                    flake8_comprehensions::rules::unnecessary_list_comprehension_set(
                        self, expr, func, args, keywords,
                    );
                }
                if self.settings.enabled.contains(&RuleCode::C404) {
                    flake8_comprehensions::rules::unnecessary_list_comprehension_dict(
                        self, expr, func, args, keywords,
                    );
                }
                if self.settings.enabled.contains(&RuleCode::C405) {
                    flake8_comprehensions::rules::unnecessary_literal_set(
                        self, expr, func, args, keywords,
                    );
                }
                if self.settings.enabled.contains(&RuleCode::C406) {
                    flake8_comprehensions::rules::unnecessary_literal_dict(
                        self, expr, func, args, keywords,
                    );
                }
                if self.settings.enabled.contains(&RuleCode::C408) {
                    flake8_comprehensions::rules::unnecessary_collection_call(
                        self, expr, func, args, keywords,
                    );
                }
                if self.settings.enabled.contains(&RuleCode::C409) {
                    flake8_comprehensions::rules::unnecessary_literal_within_tuple_call(
                        self, expr, func, args,
                    );
                }
                if self.settings.enabled.contains(&RuleCode::C410) {
                    flake8_comprehensions::rules::unnecessary_literal_within_list_call(
                        self, expr, func, args,
                    );
                }
                if self.settings.enabled.contains(&RuleCode::C411) {
                    flake8_comprehensions::rules::unnecessary_list_call(self, expr, func, args);
                }
                if self.settings.enabled.contains(&RuleCode::C413) {
                    flake8_comprehensions::rules::unnecessary_call_around_sorted(
                        self, expr, func, args,
                    );
                }
                if self.settings.enabled.contains(&RuleCode::C414) {
                    flake8_comprehensions::rules::unnecessary_double_cast_or_process(
                        self, expr, func, args,
                    );
                }
                if self.settings.enabled.contains(&RuleCode::C415) {
                    flake8_comprehensions::rules::unnecessary_subscript_reversal(
                        self, expr, func, args,
                    );
                }
                if self.settings.enabled.contains(&RuleCode::C417) {
                    flake8_comprehensions::rules::unnecessary_map(self, expr, func, args);
                }

                // flake8-boolean-trap
                if self.settings.enabled.contains(&RuleCode::FBT003) {
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
                if self.settings.enabled.contains(&RuleCode::T100) {
                    if let Some(diagnostic) = flake8_debugger::rules::debugger_call(
                        expr,
                        func,
                        &self.from_imports,
                        &self.import_aliases,
                    ) {
                        self.diagnostics.push(diagnostic);
                    }
                }

                // pandas-vet
                if self.settings.enabled.contains(&RuleCode::PD002) {
                    self.diagnostics
                        .extend(pandas_vet::rules::inplace_argument(keywords).into_iter());
                }
                for (code, name) in vec![
                    (RuleCode::PD003, "isnull"),
                    (RuleCode::PD004, "notnull"),
                    (RuleCode::PD010, "pivot"),
                    (RuleCode::PD010, "unstack"),
                    (RuleCode::PD012, "read_table"),
                    (RuleCode::PD013, "stack"),
                ] {
                    if self.settings.enabled.contains(&code) {
                        if let ExprKind::Attribute { value, attr, .. } = &func.node {
                            if attr == name {
                                if pandas_vet::helpers::is_dataframe_candidate(value) {
                                    // If the target is a named variable, avoid triggering on
                                    // irrelevant bindings (like non-Pandas imports).
                                    if let ExprKind::Name { id, .. } = &value.node {
                                        if self.find_binding(id).map_or(true, |binding| {
                                            if let BindingKind::Importation(.., module) =
                                                &binding.kind
                                            {
                                                module != "pandas"
                                            } else {
                                                matches!(
                                                    binding.kind,
                                                    BindingKind::Builtin
                                                        | BindingKind::ClassDefinition
                                                        | BindingKind::FunctionDefinition
                                                        | BindingKind::Export(..)
                                                        | BindingKind::FutureImportation
                                                        | BindingKind::StarImportation(..)
                                                        | BindingKind::Importation(..)
                                                        | BindingKind::FromImportation(..)
                                                        | BindingKind::SubmoduleImportation(..)
                                                )
                                            }
                                        }) {
                                            continue;
                                        }
                                    }

                                    self.diagnostics.push(Diagnostic::new(
                                        code.kind(),
                                        Range::from_located(func),
                                    ));
                                }
                            };
                        }
                    }
                }
                if self.settings.enabled.contains(&RuleCode::PD015) {
                    if let Some(diagnostic) = pandas_vet::rules::use_of_pd_merge(func) {
                        self.diagnostics.push(diagnostic);
                    };
                }

                // flake8-datetimez
                if self.settings.enabled.contains(&RuleCode::DTZ001) {
                    flake8_datetimez::rules::call_datetime_without_tzinfo(
                        self,
                        func,
                        args,
                        keywords,
                        Range::from_located(expr),
                    );
                }
                if self.settings.enabled.contains(&RuleCode::DTZ002) {
                    flake8_datetimez::rules::call_datetime_today(
                        self,
                        func,
                        Range::from_located(expr),
                    );
                }
                if self.settings.enabled.contains(&RuleCode::DTZ003) {
                    flake8_datetimez::rules::call_datetime_utcnow(
                        self,
                        func,
                        Range::from_located(expr),
                    );
                }
                if self.settings.enabled.contains(&RuleCode::DTZ004) {
                    flake8_datetimez::rules::call_datetime_utcfromtimestamp(
                        self,
                        func,
                        Range::from_located(expr),
                    );
                }
                if self.settings.enabled.contains(&RuleCode::DTZ005) {
                    flake8_datetimez::rules::call_datetime_now_without_tzinfo(
                        self,
                        func,
                        args,
                        keywords,
                        Range::from_located(expr),
                    );
                }
                if self.settings.enabled.contains(&RuleCode::DTZ006) {
                    flake8_datetimez::rules::call_datetime_fromtimestamp(
                        self,
                        func,
                        args,
                        keywords,
                        Range::from_located(expr),
                    );
                }
                if self.settings.enabled.contains(&RuleCode::DTZ007) {
                    flake8_datetimez::rules::call_datetime_strptime_without_zone(
                        self,
                        func,
                        args,
                        Range::from_located(expr),
                    );
                }
                if self.settings.enabled.contains(&RuleCode::DTZ011) {
                    flake8_datetimez::rules::call_date_today(self, func, Range::from_located(expr));
                }
                if self.settings.enabled.contains(&RuleCode::DTZ012) {
                    flake8_datetimez::rules::call_date_fromtimestamp(
                        self,
                        func,
                        Range::from_located(expr),
                    );
                }

                // pygrep-hooks
                if self.settings.enabled.contains(&RuleCode::PGH001) {
                    pygrep_hooks::rules::no_eval(self, func);
                }
                if self.settings.enabled.contains(&RuleCode::PGH002) {
                    pygrep_hooks::rules::deprecated_log_warn(self, func);
                }

                // pylint
                if self.settings.enabled.contains(&RuleCode::PLC3002) {
                    pylint::rules::unnecessary_direct_lambda_call(self, expr, func);
                }
                if self.settings.enabled.contains(&RuleCode::PLR1722) {
                    pylint::rules::use_sys_exit(self, func);
                }

                // flake8-pytest-style
                if self.settings.enabled.contains(&RuleCode::PT008) {
                    if let Some(diagnostic) =
                        flake8_pytest_style::rules::patch_with_lambda(func, args, keywords)
                    {
                        self.diagnostics.push(diagnostic);
                    }
                }
                if self.settings.enabled.contains(&RuleCode::PT009) {
                    if let Some(diagnostic) = flake8_pytest_style::rules::unittest_assertion(
                        self, expr, func, args, keywords,
                    ) {
                        self.diagnostics.push(diagnostic);
                    }
                }

                if self.settings.enabled.contains(&RuleCode::PT010)
                    || self.settings.enabled.contains(&RuleCode::PT011)
                {
                    flake8_pytest_style::rules::raises_call(self, func, args, keywords);
                }

                if self.settings.enabled.contains(&RuleCode::PT016) {
                    flake8_pytest_style::rules::fail_call(self, func, args, keywords);
                }

                // ruff
                if self.settings.enabled.contains(&RuleCode::RUF004) {
                    self.diagnostics
                        .extend(ruff::rules::keyword_argument_before_star_argument(
                            args, keywords,
                        ));
                }

                // flake8-simplify
                if self.settings.enabled.contains(&RuleCode::SIM115) {
                    flake8_simplify::rules::open_file_with_context_handler(self, func);
                }
            }
            ExprKind::Dict { keys, values } => {
                if self.settings.enabled.contains(&RuleCode::F601)
                    || self.settings.enabled.contains(&RuleCode::F602)
                {
                    pyflakes::rules::repeated_keys(self, keys, values);
                }
            }
            ExprKind::Yield { .. } => {
                if self.settings.enabled.contains(&RuleCode::F704) {
                    let scope = self.current_scope();
                    if matches!(scope.kind, ScopeKind::Class(_) | ScopeKind::Module) {
                        self.diagnostics.push(Diagnostic::new(
                            violations::YieldOutsideFunction(DeferralKeyword::Yield),
                            Range::from_located(expr),
                        ));
                    }
                }
            }
            ExprKind::YieldFrom { .. } => {
                if self.settings.enabled.contains(&RuleCode::F704) {
                    let scope = self.current_scope();
                    if matches!(scope.kind, ScopeKind::Class(_) | ScopeKind::Module) {
                        self.diagnostics.push(Diagnostic::new(
                            violations::YieldOutsideFunction(DeferralKeyword::YieldFrom),
                            Range::from_located(expr),
                        ));
                    }
                }
            }
            ExprKind::Await { .. } => {
                if self.settings.enabled.contains(&RuleCode::F704) {
                    let scope = self.current_scope();
                    if matches!(scope.kind, ScopeKind::Class(_) | ScopeKind::Module) {
                        self.diagnostics.push(Diagnostic::new(
                            violations::YieldOutsideFunction(DeferralKeyword::Await),
                            Range::from_located(expr),
                        ));
                    }
                }
                if self.settings.enabled.contains(&RuleCode::PLE1142) {
                    pylint::rules::await_outside_async(self, expr);
                }
            }
            ExprKind::JoinedStr { values } => {
                if self.settings.enabled.contains(&RuleCode::F541) {
                    pyflakes::rules::f_string_missing_placeholders(expr, values, self);
                }
            }
            ExprKind::BinOp {
                left,
                op: Operator::RShift,
                ..
            } => {
                if self.settings.enabled.contains(&RuleCode::F633) {
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
                    if self.settings.enabled.contains(&RuleCode::F501)
                        || self.settings.enabled.contains(&RuleCode::F502)
                        || self.settings.enabled.contains(&RuleCode::F503)
                        || self.settings.enabled.contains(&RuleCode::F504)
                        || self.settings.enabled.contains(&RuleCode::F505)
                        || self.settings.enabled.contains(&RuleCode::F506)
                        || self.settings.enabled.contains(&RuleCode::F507)
                        || self.settings.enabled.contains(&RuleCode::F508)
                        || self.settings.enabled.contains(&RuleCode::F509)
                    {
                        let location = Range::from_located(expr);
                        match pyflakes::cformat::CFormatSummary::try_from(value.as_ref()) {
                            Err(CFormatError {
                                typ: CFormatErrorType::UnsupportedFormatChar(c),
                                ..
                            }) => {
                                if self.settings.enabled.contains(&RuleCode::F509) {
                                    self.diagnostics.push(Diagnostic::new(
                                        violations::PercentFormatUnsupportedFormatCharacter(c),
                                        location,
                                    ));
                                }
                            }
                            Err(e) => {
                                if self.settings.enabled.contains(&RuleCode::F501) {
                                    self.diagnostics.push(Diagnostic::new(
                                        violations::PercentFormatInvalidFormat(e.to_string()),
                                        location,
                                    ));
                                }
                            }
                            Ok(summary) => {
                                if self.settings.enabled.contains(&RuleCode::F502) {
                                    pyflakes::rules::percent_format_expected_mapping(
                                        self, &summary, right, location,
                                    );
                                }
                                if self.settings.enabled.contains(&RuleCode::F503) {
                                    pyflakes::rules::percent_format_expected_sequence(
                                        self, &summary, right, location,
                                    );
                                }
                                if self.settings.enabled.contains(&RuleCode::F504) {
                                    pyflakes::rules::percent_format_extra_named_arguments(
                                        self, &summary, right, location,
                                    );
                                }
                                if self.settings.enabled.contains(&RuleCode::F505) {
                                    pyflakes::rules::percent_format_missing_arguments(
                                        self, &summary, right, location,
                                    );
                                }
                                if self.settings.enabled.contains(&RuleCode::F506) {
                                    pyflakes::rules::percent_format_mixed_positional_and_named(
                                        self, &summary, location,
                                    );
                                }
                                if self.settings.enabled.contains(&RuleCode::F507) {
                                    pyflakes::rules::percent_format_positional_count_mismatch(
                                        self, &summary, right, location,
                                    );
                                }
                                if self.settings.enabled.contains(&RuleCode::F508) {
                                    pyflakes::rules::percent_format_star_requires_sequence(
                                        self, &summary, right, location,
                                    );
                                }
                            }
                        }
                    }
                }
            }
            ExprKind::BinOp {
                op: Operator::Add, ..
            } => {
                if self.settings.enabled.contains(&RuleCode::ISC003) {
                    if let Some(diagnostic) = flake8_implicit_str_concat::rules::explicit(expr) {
                        self.diagnostics.push(diagnostic);
                    }
                }
            }
            ExprKind::UnaryOp { op, operand } => {
                let check_not_in = self.settings.enabled.contains(&RuleCode::E713);
                let check_not_is = self.settings.enabled.contains(&RuleCode::E714);
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

                if self.settings.enabled.contains(&RuleCode::B002) {
                    flake8_bugbear::rules::unary_prefix_increment(self, expr, op, operand);
                }

                if self.settings.enabled.contains(&RuleCode::SIM201) {
                    flake8_simplify::rules::negation_with_equal_op(self, expr, op, operand);
                }
                if self.settings.enabled.contains(&RuleCode::SIM202) {
                    flake8_simplify::rules::negation_with_not_equal_op(self, expr, op, operand);
                }
                if self.settings.enabled.contains(&RuleCode::SIM208) {
                    flake8_simplify::rules::double_negation(self, expr, op, operand);
                }
            }
            ExprKind::Compare {
                left,
                ops,
                comparators,
            } => {
                let check_none_comparisons = self.settings.enabled.contains(&RuleCode::E711);
                let check_true_false_comparisons = self.settings.enabled.contains(&RuleCode::E712);
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

                if self.settings.enabled.contains(&RuleCode::F632) {
                    pyflakes::rules::invalid_literal_comparison(
                        self,
                        left,
                        ops,
                        comparators,
                        Range::from_located(expr),
                    );
                }

                if self.settings.enabled.contains(&RuleCode::E721) {
                    self.diagnostics.extend(pycodestyle::rules::type_comparison(
                        ops,
                        comparators,
                        Range::from_located(expr),
                    ));
                }

                if self.settings.enabled.contains(&RuleCode::YTT103)
                    || self.settings.enabled.contains(&RuleCode::YTT201)
                    || self.settings.enabled.contains(&RuleCode::YTT203)
                    || self.settings.enabled.contains(&RuleCode::YTT204)
                    || self.settings.enabled.contains(&RuleCode::YTT302)
                {
                    flake8_2020::rules::compare(self, left, ops, comparators);
                }

                if self.settings.enabled.contains(&RuleCode::S105) {
                    self.diagnostics.extend(
                        flake8_bandit::rules::compare_to_hardcoded_password_string(
                            left,
                            comparators,
                        ),
                    );
                }

                if self.settings.enabled.contains(&RuleCode::PLC2201) {
                    pylint::rules::misplaced_comparison_constant(
                        self,
                        expr,
                        left,
                        ops,
                        comparators,
                    );
                }

                if self.settings.enabled.contains(&RuleCode::SIM118) {
                    flake8_simplify::rules::key_in_dict_compare(self, expr, left, ops, comparators);
                }

                if self.settings.enabled.contains(&RuleCode::SIM300) {
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
                if self.settings.enabled.contains(&RuleCode::S104) {
                    if let Some(diagnostic) = flake8_bandit::rules::hardcoded_bind_all_interfaces(
                        value,
                        &Range::from_located(expr),
                    ) {
                        self.diagnostics.push(diagnostic);
                    }
                }
                if self.settings.enabled.contains(&RuleCode::S108) {
                    if let Some(diagnostic) = flake8_bandit::rules::hardcoded_tmp_directory(
                        expr,
                        value,
                        &self.settings.flake8_bandit.hardcoded_tmp_directory,
                    ) {
                        self.diagnostics.push(diagnostic);
                    }
                }
                if self.settings.enabled.contains(&RuleCode::UP025) {
                    pyupgrade::rules::rewrite_unicode_literal(self, expr, kind.as_deref());
                }
            }
            ExprKind::Lambda { args, body, .. } => {
                if self.settings.enabled.contains(&RuleCode::PIE807) {
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
                if self.settings.enabled.contains(&RuleCode::SIM210) {
                    flake8_simplify::rules::explicit_true_false_in_ifexpr(
                        self, expr, test, body, orelse,
                    );
                }
                if self.settings.enabled.contains(&RuleCode::SIM211) {
                    flake8_simplify::rules::explicit_false_true_in_ifexpr(
                        self, expr, test, body, orelse,
                    );
                }
                if self.settings.enabled.contains(&RuleCode::SIM212) {
                    flake8_simplify::rules::twisted_arms_in_ifexpr(self, expr, test, body, orelse);
                }
            }
            ExprKind::ListComp { elt, generators } | ExprKind::SetComp { elt, generators } => {
                if self.settings.enabled.contains(&RuleCode::C416) {
                    flake8_comprehensions::rules::unnecessary_comprehension(
                        self, expr, elt, generators,
                    );
                }
                if self.settings.enabled.contains(&RuleCode::B023) {
                    flake8_bugbear::rules::function_uses_loop_variable(self, &Node::Expr(expr));
                }
                self.push_scope(Scope::new(ScopeKind::Generator));
            }
            ExprKind::GeneratorExp { .. } | ExprKind::DictComp { .. } => {
                if self.settings.enabled.contains(&RuleCode::B023) {
                    flake8_bugbear::rules::function_uses_loop_variable(self, &Node::Expr(expr));
                }
                self.push_scope(Scope::new(ScopeKind::Generator));
            }
            ExprKind::BoolOp { op, values } => {
                if self.settings.enabled.contains(&RuleCode::PLR1701) {
                    pylint::rules::merge_isinstance(self, expr, op, values);
                }
                if self.settings.enabled.contains(&RuleCode::SIM101) {
                    flake8_simplify::rules::duplicate_isinstance_call(self, expr);
                }
                if self.settings.enabled.contains(&RuleCode::SIM109) {
                    flake8_simplify::rules::compare_with_tuple(self, expr);
                }
                if self.settings.enabled.contains(&RuleCode::SIM220) {
                    flake8_simplify::rules::a_and_not_a(self, expr);
                }
                if self.settings.enabled.contains(&RuleCode::SIM221) {
                    flake8_simplify::rules::a_or_not_a(self, expr);
                }
                if self.settings.enabled.contains(&RuleCode::SIM222) {
                    flake8_simplify::rules::or_true(self, expr);
                }
                if self.settings.enabled.contains(&RuleCode::SIM223) {
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
                        self.settings.typing_modules.iter().map(String::as_str),
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
                if self.settings.enabled.contains(&RuleCode::E722) {
                    if let Some(diagnostic) = pycodestyle::rules::do_not_use_bare_except(
                        type_.as_deref(),
                        body,
                        excepthandler,
                        self.locator,
                    ) {
                        self.diagnostics.push(diagnostic);
                    }
                }
                if self.settings.enabled.contains(&RuleCode::B904) {
                    flake8_bugbear::rules::raise_without_from_inside_except(self, body);
                }
                if self.settings.enabled.contains(&RuleCode::BLE001) {
                    flake8_blind_except::rules::blind_except(
                        self,
                        type_.as_deref(),
                        name.as_deref(),
                        body,
                    );
                }
                match name {
                    Some(name) => {
                        if self.settings.enabled.contains(&RuleCode::E741) {
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

                        if self.current_scope().values.contains_key(&name.as_str()) {
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

                        let definition = self.current_scope().values.get(&name.as_str()).copied();
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
                            &scope.values.remove(&name.as_str())
                        } {
                            if self.bindings[*index].used.is_none() {
                                if self.settings.enabled.contains(&RuleCode::F841) {
                                    let mut diagnostic = Diagnostic::new(
                                        violations::UnusedVariable(name.to_string()),
                                        name_range,
                                    );
                                    if self.patch(&RuleCode::F841) {
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
                            scope.values.insert(name, index);
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
        if self.settings.enabled.contains(&RuleCode::SIM118) {
            flake8_simplify::rules::key_in_dict_for(
                self,
                &comprehension.target,
                &comprehension.iter,
            );
        }
        visitor::walk_comprehension(self, comprehension);
    }

    fn visit_arguments(&mut self, arguments: &'b Arguments) {
        if self.settings.enabled.contains(&RuleCode::B006) {
            flake8_bugbear::rules::mutable_argument_default(self, arguments);
        }
        if self.settings.enabled.contains(&RuleCode::B008) {
            flake8_bugbear::rules::function_call_argument_default(self, arguments);
        }

        // flake8-boolean-trap
        if self.settings.enabled.contains(&RuleCode::FBT001) {
            flake8_boolean_trap::rules::check_positional_boolean_in_def(self, arguments);
        }
        if self.settings.enabled.contains(&RuleCode::FBT002) {
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
                used: None,
                range: Range::from_located(arg),
                source: Some(self.current_stmt().clone()),
            },
        );

        if self.settings.enabled.contains(&RuleCode::E741) {
            if let Some(diagnostic) =
                pycodestyle::rules::ambiguous_variable_name(&arg.node.arg, Range::from_located(arg))
            {
                self.diagnostics.push(diagnostic);
            }
        }

        if self.settings.enabled.contains(&RuleCode::N803) {
            if let Some(diagnostic) = pep8_naming::rules::invalid_argument_name(&arg.node.arg, arg)
            {
                self.diagnostics.push(diagnostic);
            }
        }

        self.check_builtin_arg_shadowing(&arg.node.arg, arg);
    }

    fn visit_body(&mut self, body: &'b [Stmt]) {
        if self.settings.enabled.contains(&RuleCode::PIE790) {
            flake8_pie::rules::no_unnecessary_pass(self, body);
        }

        if self.settings.enabled.contains(&RuleCode::SIM110)
            || self.settings.enabled.contains(&RuleCode::SIM111)
        {
            for (stmt, sibling) in body.iter().tuple_windows() {
                if matches!(stmt.node, StmtKind::For { .. })
                    && matches!(sibling.node, StmtKind::Return { .. })
                {
                    flake8_simplify::rules::convert_loop_to_any_all(self, stmt, sibling);
                }
            }
        }

        visitor::walk_body(self, body);
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
                    if self.settings.enabled.contains(&RuleCode::F402) {
                        self.diagnostics.push(Diagnostic::new(
                            violations::ImportShadowedByLoopVar(
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
                                cast::decorator_list(existing.source.as_ref().unwrap()),
                            ))
                    {
                        overridden = Some((*scope_index, *existing_binding_index));
                        if self.settings.enabled.contains(&RuleCode::F811) {
                            self.diagnostics.push(Diagnostic::new(
                                violations::RedefinedWhileUnused(
                                    name.to_string(),
                                    existing.range.location.row(),
                                ),
                                binding_range(&binding, self.locator),
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

        // If we're about to lose the binding, store it as overridden.
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
                if self.settings.enabled.contains(&RuleCode::F405) {
                    let mut from_list = vec![];
                    for scope_index in self.scope_stack.iter().rev() {
                        let scope = &self.scopes[*scope_index];
                        for binding in scope.values.values().map(|index| &self.bindings[*index]) {
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
                        violations::ImportStarUsage(id.to_string(), from_list),
                        Range::from_located(expr),
                    ));
                }
                return;
            }

            if self.settings.enabled.contains(&RuleCode::F821) {
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

                self.diagnostics.push(Diagnostic::new(
                    violations::UndefinedName(id.clone()),
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

        if self.settings.enabled.contains(&RuleCode::F823) {
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

        if self.settings.enabled.contains(&RuleCode::N806) {
            if matches!(self.current_scope().kind, ScopeKind::Function(..)) {
                // Ignore globals.
                if !self.current_scope().values.get(id).map_or(false, |index| {
                    matches!(self.bindings[*index].kind, BindingKind::Global)
                }) {
                    pep8_naming::rules::non_lowercase_variable_in_function(self, expr, parent, id);
                }
            }
        }

        if self.settings.enabled.contains(&RuleCode::N815) {
            if matches!(self.current_scope().kind, ScopeKind::Class(..)) {
                pep8_naming::rules::mixed_case_variable_in_class_scope(self, expr, parent, id);
            }
        }

        if self.settings.enabled.contains(&RuleCode::N816) {
            if matches!(self.current_scope().kind, ScopeKind::Module) {
                pep8_naming::rules::mixed_case_variable_in_global_scope(self, expr, parent, id);
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
            if operations::on_conditional_branch(
                &mut self.parents.iter().rev().map(std::convert::Into::into),
            ) {
                return;
            }

            let scope =
                &mut self.scopes[*(self.scope_stack.last().expect("No current scope found"))];
            if scope.values.remove(&id.as_str()).is_none()
                && self.settings.enabled.contains(&RuleCode::F821)
            {
                self.diagnostics.push(Diagnostic::new(
                    violations::UndefinedName(id.to_string()),
                    Range::from_located(expr),
                ));
            }
        }
    }

    fn visit_docstring<'b>(&mut self, python_ast: &'b Suite) -> bool
    where
        'b: 'a,
    {
        if self.settings.enabled.contains(&RuleCode::B021) {
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
                if self.settings.enabled.contains(&RuleCode::F722) {
                    self.diagnostics.push(Diagnostic::new(
                        violations::ForwardAnnotationSyntaxError(expression.to_string()),
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
        while let Some((scopes, _parents)) = self.deferred_assignments.pop() {
            let scope_index = scopes[scopes.len() - 1];
            let parent_scope_index = scopes[scopes.len() - 2];
            if self.settings.enabled.contains(&RuleCode::F841) {
                pyflakes::rules::unused_variable(self, scope_index);
            }
            if self.settings.enabled.contains(&RuleCode::F842) {
                pyflakes::rules::unused_annotation(self, scope_index);
            }
            if self.settings.enabled.contains(&RuleCode::ARG001)
                || self.settings.enabled.contains(&RuleCode::ARG002)
                || self.settings.enabled.contains(&RuleCode::ARG003)
                || self.settings.enabled.contains(&RuleCode::ARG004)
                || self.settings.enabled.contains(&RuleCode::ARG005)
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

    fn check_dead_scopes(&mut self) {
        if !self.settings.enabled.contains(&RuleCode::F401)
            && !self.settings.enabled.contains(&RuleCode::F405)
            && !self.settings.enabled.contains(&RuleCode::F811)
            && !self.settings.enabled.contains(&RuleCode::F822)
            && !self.settings.enabled.contains(&RuleCode::PLW0602)
        {
            return;
        }

        let mut diagnostics: Vec<Diagnostic> = vec![];
        for scope in self
            .dead_scopes
            .iter()
            .rev()
            .map(|index| &self.scopes[*index])
        {
            // PLW0602
            if self.settings.enabled.contains(&RuleCode::PLW0602) {
                for (name, index) in &scope.values {
                    let binding = &self.bindings[*index];
                    if matches!(binding.kind, BindingKind::Global) {
                        diagnostics.push(Diagnostic::new(
                            violations::GlobalVariableNotAssigned((*name).to_string()),
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

            if self.settings.enabled.contains(&RuleCode::F822) {
                if !scope.import_starred && !self.path.ends_with("__init__.py") {
                    if let Some(all_binding) = all_binding {
                        if let Some(names) = &all_names {
                            for &name in names {
                                if !scope.values.contains_key(name) {
                                    diagnostics.push(Diagnostic::new(
                                        violations::UndefinedExport(name.to_string()),
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
            if self.settings.enabled.contains(&RuleCode::F811) {
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
                                diagnostics.push(Diagnostic::new(
                                    violations::RedefinedWhileUnused(
                                        (*name).to_string(),
                                        binding.range.location.row(),
                                    ),
                                    binding_range(&self.bindings[*index], self.locator),
                                ));
                            }
                        }
                    }
                }
            }

            if self.settings.enabled.contains(&RuleCode::F405) {
                if scope.import_starred {
                    if let Some(all_binding) = all_binding {
                        if let Some(names) = &all_names {
                            let mut from_list = vec![];
                            for binding in scope.values.values().map(|index| &self.bindings[*index])
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
                                if !scope.values.contains_key(name) {
                                    diagnostics.push(Diagnostic::new(
                                        violations::ImportStarUsage(
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

            if self.settings.enabled.contains(&RuleCode::F401) {
                // Collect all unused imports by location. (Multiple unused imports at the same
                // location indicates an `import from`.)
                type UnusedImport<'a> = (&'a str, &'a Range);
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
                    let child: &Stmt = defined_by.into();

                    let diagnostic_lineno = binding.range.location.row();
                    let parent_lineno = if matches!(child.node, StmtKind::ImportFrom { .. })
                        && child.location.row() != diagnostic_lineno
                    {
                        Some(child.location.row())
                    } else {
                        None
                    };

                    if self.is_ignored(&RuleCode::F401, diagnostic_lineno)
                        || parent_lineno.map_or(false, |parent_lineno| {
                            self.is_ignored(&RuleCode::F401, parent_lineno)
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

                    let fix = if !ignore_init && self.patch(&RuleCode::F401) {
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
                            violations::UnusedImport(full_name.to_string(), ignore_init, multiple),
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
                            violations::UnusedImport(full_name.to_string(), ignore_init, multiple),
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
        let enforce_annotations = self.settings.enabled.contains(&RuleCode::ANN001)
            || self.settings.enabled.contains(&RuleCode::ANN002)
            || self.settings.enabled.contains(&RuleCode::ANN003)
            || self.settings.enabled.contains(&RuleCode::ANN101)
            || self.settings.enabled.contains(&RuleCode::ANN102)
            || self.settings.enabled.contains(&RuleCode::ANN201)
            || self.settings.enabled.contains(&RuleCode::ANN202)
            || self.settings.enabled.contains(&RuleCode::ANN204)
            || self.settings.enabled.contains(&RuleCode::ANN205)
            || self.settings.enabled.contains(&RuleCode::ANN206)
            || self.settings.enabled.contains(&RuleCode::ANN401);
        let enforce_docstrings = self.settings.enabled.contains(&RuleCode::D100)
            || self.settings.enabled.contains(&RuleCode::D101)
            || self.settings.enabled.contains(&RuleCode::D102)
            || self.settings.enabled.contains(&RuleCode::D103)
            || self.settings.enabled.contains(&RuleCode::D104)
            || self.settings.enabled.contains(&RuleCode::D105)
            || self.settings.enabled.contains(&RuleCode::D106)
            || self.settings.enabled.contains(&RuleCode::D107)
            || self.settings.enabled.contains(&RuleCode::D200)
            || self.settings.enabled.contains(&RuleCode::D201)
            || self.settings.enabled.contains(&RuleCode::D202)
            || self.settings.enabled.contains(&RuleCode::D203)
            || self.settings.enabled.contains(&RuleCode::D204)
            || self.settings.enabled.contains(&RuleCode::D205)
            || self.settings.enabled.contains(&RuleCode::D206)
            || self.settings.enabled.contains(&RuleCode::D207)
            || self.settings.enabled.contains(&RuleCode::D208)
            || self.settings.enabled.contains(&RuleCode::D209)
            || self.settings.enabled.contains(&RuleCode::D210)
            || self.settings.enabled.contains(&RuleCode::D211)
            || self.settings.enabled.contains(&RuleCode::D212)
            || self.settings.enabled.contains(&RuleCode::D213)
            || self.settings.enabled.contains(&RuleCode::D214)
            || self.settings.enabled.contains(&RuleCode::D215)
            || self.settings.enabled.contains(&RuleCode::D300)
            || self.settings.enabled.contains(&RuleCode::D301)
            || self.settings.enabled.contains(&RuleCode::D400)
            || self.settings.enabled.contains(&RuleCode::D402)
            || self.settings.enabled.contains(&RuleCode::D403)
            || self.settings.enabled.contains(&RuleCode::D404)
            || self.settings.enabled.contains(&RuleCode::D405)
            || self.settings.enabled.contains(&RuleCode::D406)
            || self.settings.enabled.contains(&RuleCode::D407)
            || self.settings.enabled.contains(&RuleCode::D408)
            || self.settings.enabled.contains(&RuleCode::D409)
            || self.settings.enabled.contains(&RuleCode::D410)
            || self.settings.enabled.contains(&RuleCode::D411)
            || self.settings.enabled.contains(&RuleCode::D412)
            || self.settings.enabled.contains(&RuleCode::D413)
            || self.settings.enabled.contains(&RuleCode::D414)
            || self.settings.enabled.contains(&RuleCode::D415)
            || self.settings.enabled.contains(&RuleCode::D416)
            || self.settings.enabled.contains(&RuleCode::D417)
            || self.settings.enabled.contains(&RuleCode::D418)
            || self.settings.enabled.contains(&RuleCode::D419);

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
                let content = self
                    .locator
                    .slice_source_code_range(&Range::from_located(expr));
                let indentation = self.locator.slice_source_code_range(&Range::new(
                    Location::new(expr.location.row(), 0),
                    Location::new(expr.location.row(), expr.location.column()),
                ));
                let body = pydocstyle::helpers::raw_contents(&content);
                let docstring = Docstring {
                    kind: definition.kind,
                    expr,
                    contents: &content,
                    indentation: &indentation,
                    body,
                };

                if !pydocstyle::rules::not_empty(self, &docstring) {
                    continue;
                }

                if self.settings.enabled.contains(&RuleCode::D200) {
                    pydocstyle::rules::one_liner(self, &docstring);
                }
                if self.settings.enabled.contains(&RuleCode::D201)
                    || self.settings.enabled.contains(&RuleCode::D202)
                {
                    pydocstyle::rules::blank_before_after_function(self, &docstring);
                }
                if self.settings.enabled.contains(&RuleCode::D203)
                    || self.settings.enabled.contains(&RuleCode::D204)
                    || self.settings.enabled.contains(&RuleCode::D211)
                {
                    pydocstyle::rules::blank_before_after_class(self, &docstring);
                }
                if self.settings.enabled.contains(&RuleCode::D205) {
                    pydocstyle::rules::blank_after_summary(self, &docstring);
                }
                if self.settings.enabled.contains(&RuleCode::D206)
                    || self.settings.enabled.contains(&RuleCode::D207)
                    || self.settings.enabled.contains(&RuleCode::D208)
                {
                    pydocstyle::rules::indent(self, &docstring);
                }
                if self.settings.enabled.contains(&RuleCode::D209) {
                    pydocstyle::rules::newline_after_last_paragraph(self, &docstring);
                }
                if self.settings.enabled.contains(&RuleCode::D210) {
                    pydocstyle::rules::no_surrounding_whitespace(self, &docstring);
                }
                if self.settings.enabled.contains(&RuleCode::D212)
                    || self.settings.enabled.contains(&RuleCode::D213)
                {
                    pydocstyle::rules::multi_line_summary_start(self, &docstring);
                }
                if self.settings.enabled.contains(&RuleCode::D300) {
                    pydocstyle::rules::triple_quotes(self, &docstring);
                }
                if self.settings.enabled.contains(&RuleCode::D301) {
                    pydocstyle::rules::backslashes(self, &docstring);
                }
                if self.settings.enabled.contains(&RuleCode::D400) {
                    pydocstyle::rules::ends_with_period(self, &docstring);
                }
                if self.settings.enabled.contains(&RuleCode::D402) {
                    pydocstyle::rules::no_signature(self, &docstring);
                }
                if self.settings.enabled.contains(&RuleCode::D403) {
                    pydocstyle::rules::capitalized(self, &docstring);
                }
                if self.settings.enabled.contains(&RuleCode::D404) {
                    pydocstyle::rules::starts_with_this(self, &docstring);
                }
                if self.settings.enabled.contains(&RuleCode::D415) {
                    pydocstyle::rules::ends_with_punctuation(self, &docstring);
                }
                if self.settings.enabled.contains(&RuleCode::D418) {
                    pydocstyle::rules::if_needed(self, &docstring);
                }
                if self.settings.enabled.contains(&RuleCode::D212)
                    || self.settings.enabled.contains(&RuleCode::D214)
                    || self.settings.enabled.contains(&RuleCode::D215)
                    || self.settings.enabled.contains(&RuleCode::D405)
                    || self.settings.enabled.contains(&RuleCode::D406)
                    || self.settings.enabled.contains(&RuleCode::D407)
                    || self.settings.enabled.contains(&RuleCode::D408)
                    || self.settings.enabled.contains(&RuleCode::D409)
                    || self.settings.enabled.contains(&RuleCode::D410)
                    || self.settings.enabled.contains(&RuleCode::D411)
                    || self.settings.enabled.contains(&RuleCode::D412)
                    || self.settings.enabled.contains(&RuleCode::D413)
                    || self.settings.enabled.contains(&RuleCode::D414)
                    || self.settings.enabled.contains(&RuleCode::D416)
                    || self.settings.enabled.contains(&RuleCode::D417)
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
            if self.settings.enabled.contains(&RuleCode::A003) {
                if let Some(diagnostic) = flake8_builtins::rules::builtin_shadowing(
                    name,
                    located,
                    flake8_builtins::types::ShadowingType::Attribute,
                ) {
                    self.diagnostics.push(diagnostic);
                }
            }
        } else {
            if self.settings.enabled.contains(&RuleCode::A001) {
                if let Some(diagnostic) = flake8_builtins::rules::builtin_shadowing(
                    name,
                    located,
                    flake8_builtins::types::ShadowingType::Variable,
                ) {
                    self.diagnostics.push(diagnostic);
                }
            }
        }
    }

    fn check_builtin_arg_shadowing(&mut self, name: &str, arg: &Arg) {
        if self.settings.enabled.contains(&RuleCode::A002) {
            if let Some(diagnostic) = flake8_builtins::rules::builtin_shadowing(
                name,
                arg,
                flake8_builtins::types::ShadowingType::Argument,
            ) {
                self.diagnostics.push(diagnostic);
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub fn check_ast(
    python_ast: &Suite,
    locator: &SourceCodeLocator,
    stylist: &SourceCodeStyleDetector,
    noqa_line_for: &IntMap<usize, usize>,
    settings: &Settings,
    autofix: flags::Autofix,
    noqa: flags::Noqa,
    path: &Path,
) -> Vec<Diagnostic> {
    let mut checker = Checker::new(
        settings,
        noqa_line_for,
        autofix,
        noqa,
        path,
        locator,
        stylist,
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

    checker.diagnostics
}
