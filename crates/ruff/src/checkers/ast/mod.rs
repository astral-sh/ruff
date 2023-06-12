use std::path::Path;

use itertools::Itertools;
use log::error;
use ruff_text_size::{TextRange, TextSize};
use rustpython_format::cformat::{CFormatError, CFormatErrorType};
use rustpython_parser::ast::{
    self, Arg, Arguments, Comprehension, Constant, Excepthandler, Expr, ExprContext, Keyword,
    Operator, Pattern, Ranged, Stmt, Suite, Unaryop,
};

use ruff_diagnostics::{Diagnostic, IsolationLevel};
use ruff_python_ast::all::{extract_all_names, AllNamesFlags};
use ruff_python_ast::helpers::{extract_handled_exceptions, to_module_path};
use ruff_python_ast::source_code::{Generator, Indexer, Locator, Quote, Stylist};
use ruff_python_ast::str::trailing_quote;
use ruff_python_ast::types::Node;
use ruff_python_ast::typing::{parse_type_annotation, AnnotationKind};
use ruff_python_ast::visitor::{walk_excepthandler, walk_pattern, Visitor};
use ruff_python_ast::{cast, helpers, str, visitor};
use ruff_python_semantic::analyze;
use ruff_python_semantic::analyze::branch_detection;
use ruff_python_semantic::analyze::typing::{Callable, SubscriptKind};
use ruff_python_semantic::analyze::visibility::ModuleSource;
use ruff_python_semantic::binding::{
    Binding, BindingFlags, BindingId, BindingKind, Exceptions, Export, FromImportation,
    Importation, StarImportation, SubmoduleImportation,
};
use ruff_python_semantic::context::ExecutionContext;
use ruff_python_semantic::definition::{ContextualizedDefinition, Module, ModuleKind};
use ruff_python_semantic::globals::Globals;
use ruff_python_semantic::model::{ResolvedRead, SemanticModel, SemanticModelFlags};
use ruff_python_semantic::scope::{Scope, ScopeId, ScopeKind};
use ruff_python_stdlib::builtins::{BUILTINS, MAGIC_GLOBALS};
use ruff_python_stdlib::path::is_python_stub_file;

use crate::checkers::ast::deferred::Deferred;
use crate::docstrings::extraction::ExtractionTarget;
use crate::docstrings::Docstring;
use crate::fs::relativize_path;
use crate::importer::Importer;
use crate::noqa::NoqaMapping;
use crate::registry::Rule;
use crate::rules::flake8_builtins::helpers::AnyShadowing;
use crate::rules::{
    airflow, flake8_2020, flake8_annotations, flake8_async, flake8_bandit, flake8_blind_except,
    flake8_boolean_trap, flake8_bugbear, flake8_builtins, flake8_comprehensions, flake8_datetimez,
    flake8_debugger, flake8_django, flake8_errmsg, flake8_future_annotations, flake8_gettext,
    flake8_implicit_str_concat, flake8_import_conventions, flake8_logging_format, flake8_pie,
    flake8_print, flake8_pyi, flake8_pytest_style, flake8_raise, flake8_return, flake8_self,
    flake8_simplify, flake8_slots, flake8_tidy_imports, flake8_type_checking,
    flake8_unused_arguments, flake8_use_pathlib, flynt, mccabe, numpy, pandas_vet, pep8_naming,
    pycodestyle, pydocstyle, pyflakes, pygrep_hooks, pylint, pyupgrade, ruff, tryceratops,
};
use crate::settings::types::PythonVersion;
use crate::settings::{flags, Settings};
use crate::{docstrings, noqa, warn_user};

mod deferred;

pub(crate) struct Checker<'a> {
    // Settings, static metadata, etc.
    path: &'a Path,
    module_path: Option<&'a [String]>,
    package: Option<&'a Path>,
    is_stub: bool,
    noqa: flags::Noqa,
    noqa_line_for: &'a NoqaMapping,
    pub(crate) settings: &'a Settings,
    pub(crate) locator: &'a Locator<'a>,
    pub(crate) stylist: &'a Stylist<'a>,
    pub(crate) indexer: &'a Indexer,
    pub(crate) importer: Importer<'a>,
    // Stateful fields.
    semantic_model: SemanticModel<'a>,
    deferred: Deferred<'a>,
    pub(crate) diagnostics: Vec<Diagnostic>,
    // Check-specific state.
    pub(crate) flake8_bugbear_seen: Vec<&'a Expr>,
}

impl<'a> Checker<'a> {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        settings: &'a Settings,
        noqa_line_for: &'a NoqaMapping,
        noqa: flags::Noqa,
        path: &'a Path,
        package: Option<&'a Path>,
        module: Module<'a>,
        locator: &'a Locator,
        stylist: &'a Stylist,
        indexer: &'a Indexer,
        importer: Importer<'a>,
    ) -> Checker<'a> {
        Checker {
            settings,
            noqa_line_for,
            noqa,
            path,
            package,
            module_path: module.path(),
            is_stub: is_python_stub_file(path),
            locator,
            stylist,
            indexer,
            importer,
            semantic_model: SemanticModel::new(&settings.typing_modules, path, module),
            deferred: Deferred::default(),
            diagnostics: Vec::default(),
            flake8_bugbear_seen: Vec::default(),
        }
    }
}

impl<'a> Checker<'a> {
    /// Return `true` if a patch should be generated for a given [`Rule`].
    pub(crate) fn patch(&self, code: Rule) -> bool {
        self.settings.rules.should_fix(code)
    }

    /// Return `true` if a [`Rule`] is disabled by a `noqa` directive.
    pub(crate) fn rule_is_ignored(&self, code: Rule, offset: TextSize) -> bool {
        // TODO(charlie): `noqa` directives are mostly enforced in `check_lines.rs`.
        // However, in rare cases, we need to check them here. For example, when
        // removing unused imports, we create a single fix that's applied to all
        // unused members on a single import. We need to pre-emptively omit any
        // members from the fix that will eventually be excluded by a `noqa`.
        // Unfortunately, we _do_ want to register a `Diagnostic` for each
        // eventually-ignored import, so that our `noqa` counts are accurate.
        if !self.noqa.to_bool() {
            return false;
        }
        noqa::rule_is_ignored(code, offset, self.noqa_line_for, self.locator)
    }

    /// Create a [`Generator`] to generate source code based on the current AST state.
    pub(crate) fn generator(&self) -> Generator {
        Generator::new(
            self.stylist.indentation(),
            self.f_string_quote_style().unwrap_or(self.stylist.quote()),
            self.stylist.line_ending(),
        )
    }

    /// Returns the appropriate quoting for f-string by reversing the one used outside of
    /// the f-string.
    ///
    /// If the current expression in the context is not an f-string, returns ``None``.
    pub(crate) fn f_string_quote_style(&self) -> Option<Quote> {
        let model = &self.semantic_model;
        if !model.in_f_string() {
            return None;
        }

        // Find the quote character used to start the containing f-string.
        let expr = model.expr()?;
        let string_range = self.indexer.f_string_range(expr.start())?;
        let trailing_quote = trailing_quote(self.locator.slice(string_range))?;

        // Invert the quote character, if it's a single quote.
        match *trailing_quote {
            "'" => Some(Quote::Double),
            "\"" => Some(Quote::Single),
            _ => None,
        }
    }

    /// Returns the [`IsolationLevel`] for fixes in the current context.
    ///
    /// The primary use-case for fix isolation is to ensure that we don't delete all statements
    /// in a given indented block, which would cause a syntax error. We therefore need to ensure
    /// that we delete at most one statement per indented block per fixer pass. Fix isolation should
    /// thus be applied whenever we delete a statement, but can otherwise be omitted.
    pub(crate) fn isolation(&self, parent: Option<&Stmt>) -> IsolationLevel {
        parent
            .and_then(|stmt| self.semantic_model.stmts.node_id(stmt))
            .map_or(IsolationLevel::default(), |node_id| {
                IsolationLevel::Group(node_id.into())
            })
    }

    pub(crate) const fn semantic_model(&self) -> &SemanticModel<'a> {
        &self.semantic_model
    }

    pub(crate) const fn package(&self) -> Option<&'a Path> {
        self.package
    }

    pub(crate) const fn path(&self) -> &'a Path {
        self.path
    }

    /// Returns whether the given rule should be checked.
    #[inline]
    pub(crate) const fn enabled(&self, rule: Rule) -> bool {
        self.settings.rules.enabled(rule)
    }

    /// Returns whether any of the given rules should be checked.
    #[inline]
    pub(crate) const fn any_enabled(&self, rules: &[Rule]) -> bool {
        self.settings.rules.any_enabled(rules)
    }
}

impl<'a, 'b> Visitor<'b> for Checker<'a>
where
    'b: 'a,
{
    fn visit_stmt(&mut self, stmt: &'b Stmt) {
        self.semantic_model.push_stmt(stmt);

        // Track whether we've seen docstrings, non-imports, etc.
        match stmt {
            Stmt::ImportFrom(ast::StmtImportFrom { module, names, .. }) => {
                // Allow __future__ imports until we see a non-__future__ import.
                if let Some("__future__") = module.as_deref() {
                    if names
                        .iter()
                        .any(|alias| alias.name.as_str() == "annotations")
                    {
                        self.semantic_model.flags |= SemanticModelFlags::FUTURE_ANNOTATIONS;
                    }
                } else {
                    self.semantic_model.flags |= SemanticModelFlags::FUTURES_BOUNDARY;
                }
            }
            Stmt::Import(_) => {
                self.semantic_model.flags |= SemanticModelFlags::FUTURES_BOUNDARY;
            }
            _ => {
                self.semantic_model.flags |= SemanticModelFlags::FUTURES_BOUNDARY;
                if !self.semantic_model.seen_import_boundary()
                    && !helpers::is_assignment_to_a_dunder(stmt)
                    && !helpers::in_nested_block(self.semantic_model.parents())
                {
                    self.semantic_model.flags |= SemanticModelFlags::IMPORT_BOUNDARY;
                }
            }
        }

        // Track each top-level import, to guide import insertions.
        if matches!(stmt, Stmt::Import(_) | Stmt::ImportFrom(_)) {
            if self.semantic_model.at_top_level() {
                self.importer.visit_import(stmt);
            }
        }

        // Store the flags prior to any further descent, so that we can restore them after visiting
        // the node.
        let flags_snapshot = self.semantic_model.flags;

        // Pre-visit.
        match stmt {
            Stmt::Global(ast::StmtGlobal { names, range: _ }) => {
                let ranges: Vec<TextRange> = helpers::find_names(stmt, self.locator).collect();
                if !self.semantic_model.scope_id.is_global() {
                    for (name, range) in names.iter().zip(ranges.iter()) {
                        // Add a binding to the current scope.
                        let binding_id = self.semantic_model.push_binding(
                            *range,
                            BindingKind::Global,
                            BindingFlags::empty(),
                        );
                        let scope = self.semantic_model.scope_mut();
                        scope.add(name, binding_id);
                    }
                }

                if self.enabled(Rule::AmbiguousVariableName) {
                    self.diagnostics
                        .extend(names.iter().zip(ranges.iter()).filter_map(|(name, range)| {
                            pycodestyle::rules::ambiguous_variable_name(name, *range)
                        }));
                }
            }
            Stmt::Nonlocal(ast::StmtNonlocal { names, range: _ }) => {
                let ranges: Vec<TextRange> = helpers::find_names(stmt, self.locator).collect();
                if !self.semantic_model.scope_id.is_global() {
                    for (name, range) in names.iter().zip(ranges.iter()) {
                        // Add a binding to the current scope.
                        let binding_id = self.semantic_model.push_binding(
                            *range,
                            BindingKind::Nonlocal,
                            BindingFlags::empty(),
                        );
                        let scope = self.semantic_model.scope_mut();
                        scope.add(name, binding_id);
                    }

                    // Mark the binding in the defining scopes as used too. (Skip the global scope
                    // and the current scope, and, per standard resolution rules, any class scopes.)
                    for (name, range) in names.iter().zip(ranges.iter()) {
                        let binding_id = self
                            .semantic_model
                            .scopes
                            .ancestors(self.semantic_model.scope_id)
                            .skip(1)
                            .filter(|scope| !(scope.kind.is_module() || scope.kind.is_class()))
                            .find_map(|scope| scope.get(name.as_str()));

                        if let Some(binding_id) = binding_id {
                            self.semantic_model.add_local_reference(
                                binding_id,
                                stmt.range(),
                                ExecutionContext::Runtime,
                            );
                        }

                        // Ensure that every nonlocal has an existing binding from a parent scope.
                        if self.enabled(Rule::NonlocalWithoutBinding) {
                            if self
                                .semantic_model
                                .scopes
                                .ancestors(self.semantic_model.scope_id)
                                .skip(1)
                                .take_while(|scope| !scope.kind.is_module())
                                .all(|scope| !scope.declares(name.as_str()))
                            {
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

                if self.enabled(Rule::AmbiguousVariableName) {
                    self.diagnostics
                        .extend(names.iter().zip(ranges.iter()).filter_map(|(name, range)| {
                            pycodestyle::rules::ambiguous_variable_name(name, *range)
                        }));
                }
            }
            Stmt::Break(_) => {
                if self.enabled(Rule::BreakOutsideLoop) {
                    if let Some(diagnostic) = pyflakes::rules::break_outside_loop(
                        stmt,
                        &mut self.semantic_model.parents().skip(1),
                    ) {
                        self.diagnostics.push(diagnostic);
                    }
                }
            }
            Stmt::Continue(_) => {
                if self.enabled(Rule::ContinueOutsideLoop) {
                    if let Some(diagnostic) = pyflakes::rules::continue_outside_loop(
                        stmt,
                        &mut self.semantic_model.parents().skip(1),
                    ) {
                        self.diagnostics.push(diagnostic);
                    }
                }
            }
            Stmt::FunctionDef(ast::StmtFunctionDef {
                name,
                decorator_list,
                returns,
                args,
                body,
                ..
            })
            | Stmt::AsyncFunctionDef(ast::StmtAsyncFunctionDef {
                name,
                decorator_list,
                returns,
                args,
                body,
                ..
            }) => {
                if self.enabled(Rule::DjangoNonLeadingReceiverDecorator) {
                    self.diagnostics
                        .extend(flake8_django::rules::non_leading_receiver_decorator(
                            decorator_list,
                            |expr| self.semantic_model.resolve_call_path(expr),
                        ));
                }

                if self.enabled(Rule::AmbiguousFunctionName) {
                    if let Some(diagnostic) =
                        pycodestyle::rules::ambiguous_function_name(name, || {
                            helpers::identifier_range(stmt, self.locator)
                        })
                    {
                        self.diagnostics.push(diagnostic);
                    }
                }

                if self.enabled(Rule::InvalidStrReturnType) {
                    pylint::rules::invalid_str_return(self, name, body);
                }

                if self.enabled(Rule::InvalidFunctionName) {
                    if let Some(diagnostic) = pep8_naming::rules::invalid_function_name(
                        stmt,
                        name,
                        decorator_list,
                        &self.settings.pep8_naming.ignore_names,
                        &self.semantic_model,
                        self.locator,
                    ) {
                        self.diagnostics.push(diagnostic);
                    }
                }

                if self.enabled(Rule::InvalidFirstArgumentNameForClassMethod) {
                    if let Some(diagnostic) =
                        pep8_naming::rules::invalid_first_argument_name_for_class_method(
                            self,
                            self.semantic_model.scope(),
                            name,
                            decorator_list,
                            args,
                        )
                    {
                        self.diagnostics.push(diagnostic);
                    }
                }

                if self.enabled(Rule::InvalidFirstArgumentNameForMethod) {
                    if let Some(diagnostic) =
                        pep8_naming::rules::invalid_first_argument_name_for_method(
                            self,
                            self.semantic_model.scope(),
                            name,
                            decorator_list,
                            args,
                        )
                    {
                        self.diagnostics.push(diagnostic);
                    }
                }

                if self.is_stub {
                    if self.enabled(Rule::PassStatementStubBody) {
                        flake8_pyi::rules::pass_statement_stub_body(self, body);
                    }
                    if self.enabled(Rule::NonEmptyStubBody) {
                        flake8_pyi::rules::non_empty_stub_body(self, body);
                    }
                    if self.enabled(Rule::StubBodyMultipleStatements) {
                        flake8_pyi::rules::stub_body_multiple_statements(self, stmt, body);
                    }
                    if self.enabled(Rule::AnyEqNeAnnotation) {
                        flake8_pyi::rules::any_eq_ne_annotation(self, name, args);
                    }
                    if self.enabled(Rule::NonSelfReturnType) {
                        flake8_pyi::rules::non_self_return_type(
                            self,
                            stmt,
                            name,
                            decorator_list,
                            returns.as_ref().map(|expr| &**expr),
                            args,
                            stmt.is_async_function_def_stmt(),
                        );
                    }
                    if self.enabled(Rule::StrOrReprDefinedInStub) {
                        flake8_pyi::rules::str_or_repr_defined_in_stub(self, stmt);
                    }
                    if self.enabled(Rule::NoReturnArgumentAnnotationInStub) {
                        flake8_pyi::rules::no_return_argument_annotation(self, args);
                    }
                }

                if self.enabled(Rule::DunderFunctionName) {
                    if let Some(diagnostic) = pep8_naming::rules::dunder_function_name(
                        self.semantic_model.scope(),
                        stmt,
                        name,
                        self.locator,
                    ) {
                        self.diagnostics.push(diagnostic);
                    }
                }

                if self.enabled(Rule::GlobalStatement) {
                    pylint::rules::global_statement(self, name);
                }

                if self.enabled(Rule::LRUCacheWithoutParameters)
                    && self.settings.target_version >= PythonVersion::Py38
                {
                    pyupgrade::rules::lru_cache_without_parameters(self, decorator_list);
                }
                if self.enabled(Rule::LRUCacheWithMaxsizeNone)
                    && self.settings.target_version >= PythonVersion::Py39
                {
                    pyupgrade::rules::lru_cache_with_maxsize_none(self, decorator_list);
                }

                if self.enabled(Rule::CachedInstanceMethod) {
                    flake8_bugbear::rules::cached_instance_method(self, decorator_list);
                }

                if self.any_enabled(&[
                    Rule::UnnecessaryReturnNone,
                    Rule::ImplicitReturnValue,
                    Rule::ImplicitReturn,
                    Rule::UnnecessaryAssign,
                    Rule::SuperfluousElseReturn,
                    Rule::SuperfluousElseRaise,
                    Rule::SuperfluousElseContinue,
                    Rule::SuperfluousElseBreak,
                ]) {
                    flake8_return::rules::function(
                        self,
                        body,
                        returns.as_ref().map(|expr| &**expr),
                    );
                }

                if self.enabled(Rule::UselessReturn) {
                    pylint::rules::useless_return(
                        self,
                        stmt,
                        body,
                        returns.as_ref().map(|expr| &**expr),
                    );
                }

                if self.enabled(Rule::ComplexStructure) {
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

                if self.enabled(Rule::HardcodedPasswordDefault) {
                    self.diagnostics
                        .extend(flake8_bandit::rules::hardcoded_password_default(args));
                }

                if self.enabled(Rule::PropertyWithParameters) {
                    pylint::rules::property_with_parameters(self, stmt, decorator_list, args);
                }

                if self.enabled(Rule::TooManyArguments) {
                    pylint::rules::too_many_arguments(self, args, stmt);
                }

                if self.enabled(Rule::TooManyReturnStatements) {
                    if let Some(diagnostic) = pylint::rules::too_many_return_statements(
                        stmt,
                        body,
                        self.settings.pylint.max_returns,
                        self.locator,
                    ) {
                        self.diagnostics.push(diagnostic);
                    }
                }

                if self.enabled(Rule::TooManyBranches) {
                    if let Some(diagnostic) = pylint::rules::too_many_branches(
                        stmt,
                        body,
                        self.settings.pylint.max_branches,
                        self.locator,
                    ) {
                        self.diagnostics.push(diagnostic);
                    }
                }

                if self.enabled(Rule::TooManyStatements) {
                    if let Some(diagnostic) = pylint::rules::too_many_statements(
                        stmt,
                        body,
                        self.settings.pylint.max_statements,
                        self.locator,
                    ) {
                        self.diagnostics.push(diagnostic);
                    }
                }

                if self.any_enabled(&[
                    Rule::PytestFixtureIncorrectParenthesesStyle,
                    Rule::PytestFixturePositionalArgs,
                    Rule::PytestExtraneousScopeFunction,
                    Rule::PytestMissingFixtureNameUnderscore,
                    Rule::PytestIncorrectFixtureNameUnderscore,
                    Rule::PytestFixtureParamWithoutValue,
                    Rule::PytestDeprecatedYieldFixture,
                    Rule::PytestFixtureFinalizerCallback,
                    Rule::PytestUselessYieldFixture,
                    Rule::PytestUnnecessaryAsyncioMarkOnFixture,
                    Rule::PytestErroneousUseFixturesOnFixture,
                ]) {
                    flake8_pytest_style::rules::fixture(
                        self,
                        stmt,
                        name,
                        args,
                        decorator_list,
                        body,
                    );
                }

                if self.any_enabled(&[
                    Rule::PytestParametrizeNamesWrongType,
                    Rule::PytestParametrizeValuesWrongType,
                ]) {
                    flake8_pytest_style::rules::parametrize(self, decorator_list);
                }

                if self.any_enabled(&[
                    Rule::PytestIncorrectMarkParenthesesStyle,
                    Rule::PytestUseFixturesWithoutParameters,
                ]) {
                    flake8_pytest_style::rules::marks(self, decorator_list);
                }

                if self.enabled(Rule::BooleanPositionalArgInFunctionDefinition) {
                    flake8_boolean_trap::rules::check_positional_boolean_in_def(
                        self,
                        name,
                        decorator_list,
                        args,
                    );
                }

                if self.enabled(Rule::BooleanDefaultValueInFunctionDefinition) {
                    flake8_boolean_trap::rules::check_boolean_default_value_in_function_definition(
                        self,
                        name,
                        decorator_list,
                        args,
                    );
                }

                if self.enabled(Rule::UnexpectedSpecialMethodSignature) {
                    pylint::rules::unexpected_special_method_signature(
                        self,
                        stmt,
                        name,
                        decorator_list,
                        args,
                        self.locator,
                    );
                }

                if self.enabled(Rule::FStringDocstring) {
                    flake8_bugbear::rules::f_string_docstring(self, body);
                }

                if self.enabled(Rule::YieldInForLoop) {
                    pyupgrade::rules::yield_in_for_loop(self, stmt);
                }

                if self.semantic_model.scope().kind.is_class() {
                    if self.enabled(Rule::BuiltinAttributeShadowing) {
                        flake8_builtins::rules::builtin_attribute_shadowing(
                            self,
                            name,
                            AnyShadowing::from(stmt),
                        );
                    }
                } else {
                    if self.enabled(Rule::BuiltinVariableShadowing) {
                        flake8_builtins::rules::builtin_variable_shadowing(
                            self,
                            name,
                            AnyShadowing::from(stmt),
                        );
                    }
                }
            }
            Stmt::Return(_) => {
                if self.enabled(Rule::ReturnOutsideFunction) {
                    pyflakes::rules::return_outside_function(self, stmt);
                }
                if self.enabled(Rule::ReturnInInit) {
                    pylint::rules::return_in_init(self, stmt);
                }
            }
            Stmt::ClassDef(
                class_def @ ast::StmtClassDef {
                    name,
                    bases,
                    keywords,
                    decorator_list,
                    body,
                    range: _,
                },
            ) => {
                if self.enabled(Rule::DjangoNullableModelStringField) {
                    self.diagnostics
                        .extend(flake8_django::rules::nullable_model_string_field(
                            self, body,
                        ));
                }

                if self.enabled(Rule::DjangoExcludeWithModelForm) {
                    if let Some(diagnostic) =
                        flake8_django::rules::exclude_with_model_form(self, bases, body)
                    {
                        self.diagnostics.push(diagnostic);
                    }
                }
                if self.enabled(Rule::DjangoAllWithModelForm) {
                    if let Some(diagnostic) =
                        flake8_django::rules::all_with_model_form(self, bases, body)
                    {
                        self.diagnostics.push(diagnostic);
                    }
                }
                if self.enabled(Rule::DjangoModelWithoutDunderStr) {
                    if let Some(diagnostic) =
                        flake8_django::rules::model_without_dunder_str(self, bases, body, stmt)
                    {
                        self.diagnostics.push(diagnostic);
                    }
                }
                if self.enabled(Rule::DjangoUnorderedBodyContentInModel) {
                    flake8_django::rules::unordered_body_content_in_model(self, bases, body);
                }
                if self.enabled(Rule::GlobalStatement) {
                    pylint::rules::global_statement(self, name);
                }
                if self.enabled(Rule::UselessObjectInheritance) {
                    pyupgrade::rules::useless_object_inheritance(self, stmt, name, bases, keywords);
                }

                if self.enabled(Rule::AmbiguousClassName) {
                    if let Some(diagnostic) = pycodestyle::rules::ambiguous_class_name(name, || {
                        helpers::identifier_range(stmt, self.locator)
                    }) {
                        self.diagnostics.push(diagnostic);
                    }
                }

                if self.enabled(Rule::InvalidClassName) {
                    if let Some(diagnostic) =
                        pep8_naming::rules::invalid_class_name(stmt, name, self.locator)
                    {
                        self.diagnostics.push(diagnostic);
                    }
                }

                if self.enabled(Rule::ErrorSuffixOnExceptionName) {
                    if let Some(diagnostic) = pep8_naming::rules::error_suffix_on_exception_name(
                        stmt,
                        bases,
                        name,
                        self.locator,
                    ) {
                        self.diagnostics.push(diagnostic);
                    }
                }

                if !self.is_stub {
                    if self.any_enabled(&[
                        Rule::AbstractBaseClassWithoutAbstractMethod,
                        Rule::EmptyMethodWithoutAbstractDecorator,
                    ]) {
                        flake8_bugbear::rules::abstract_base_class(
                            self, stmt, name, bases, keywords, body,
                        );
                    }
                }
                if self.is_stub {
                    if self.enabled(Rule::PassStatementStubBody) {
                        flake8_pyi::rules::pass_statement_stub_body(self, body);
                    }
                    if self.enabled(Rule::PassInClassBody) {
                        flake8_pyi::rules::pass_in_class_body(self, stmt, body);
                    }
                    if self.enabled(Rule::EllipsisInNonEmptyClassBody) {
                        flake8_pyi::rules::ellipsis_in_non_empty_class_body(self, stmt, body);
                    }
                }

                if self.enabled(Rule::PytestIncorrectMarkParenthesesStyle) {
                    flake8_pytest_style::rules::marks(self, decorator_list);
                }

                if self.enabled(Rule::DuplicateClassFieldDefinition) {
                    flake8_pie::rules::duplicate_class_field_definition(self, stmt, body);
                }

                if self.enabled(Rule::NonUniqueEnums) {
                    flake8_pie::rules::non_unique_enums(self, stmt, body);
                }

                if self.any_enabled(&[
                    Rule::MutableDataclassDefault,
                    Rule::FunctionCallInDataclassDefaultArgument,
                ]) && ruff::rules::is_dataclass(&self.semantic_model, decorator_list)
                {
                    if self.enabled(Rule::MutableDataclassDefault) {
                        ruff::rules::mutable_dataclass_default(self, body);
                    }

                    if self.enabled(Rule::FunctionCallInDataclassDefaultArgument) {
                        ruff::rules::function_call_in_dataclass_defaults(self, body);
                    }
                }

                if self.enabled(Rule::FStringDocstring) {
                    flake8_bugbear::rules::f_string_docstring(self, body);
                }

                if self.enabled(Rule::BuiltinVariableShadowing) {
                    flake8_builtins::rules::builtin_variable_shadowing(
                        self,
                        name,
                        AnyShadowing::from(stmt),
                    );
                }

                if self.enabled(Rule::DuplicateBases) {
                    pylint::rules::duplicate_bases(self, name, bases);
                }

                if self.enabled(Rule::NoSlotsInStrSubclass) {
                    flake8_slots::rules::no_slots_in_str_subclass(self, stmt, class_def);
                }

                if self.enabled(Rule::NoSlotsInTupleSubclass) {
                    flake8_slots::rules::no_slots_in_tuple_subclass(self, stmt, class_def);
                }

                if self.enabled(Rule::NoSlotsInNamedtupleSubclass) {
                    flake8_slots::rules::no_slots_in_namedtuple_subclass(self, stmt, class_def);
                }
            }
            Stmt::Import(ast::StmtImport { names, range: _ }) => {
                if self.enabled(Rule::MultipleImportsOnOneLine) {
                    pycodestyle::rules::multiple_imports_on_one_line(self, stmt, names);
                }
                if self.enabled(Rule::ModuleImportNotAtTopOfFile) {
                    pycodestyle::rules::module_import_not_at_top_of_file(self, stmt, self.locator);
                }

                if self.enabled(Rule::GlobalStatement) {
                    for name in names.iter() {
                        if let Some(asname) = name.asname.as_ref() {
                            pylint::rules::global_statement(self, asname);
                        } else {
                            pylint::rules::global_statement(self, &name.name);
                        }
                    }
                }

                if self.enabled(Rule::DeprecatedCElementTree) {
                    pyupgrade::rules::deprecated_c_element_tree(self, stmt);
                }
                if self.enabled(Rule::DeprecatedMockImport) {
                    pyupgrade::rules::deprecated_mock_import(self, stmt);
                }

                for alias in names {
                    if &alias.name == "__future__" {
                        let name = alias.asname.as_ref().unwrap_or(&alias.name);
                        self.add_binding(
                            name,
                            alias.range(),
                            BindingKind::FutureImportation,
                            BindingFlags::empty(),
                        );

                        if self.enabled(Rule::LateFutureImport) {
                            if self.semantic_model.seen_futures_boundary() {
                                self.diagnostics.push(Diagnostic::new(
                                    pyflakes::rules::LateFutureImport,
                                    stmt.range(),
                                ));
                            }
                        }
                    } else if alias.name.contains('.') && alias.asname.is_none() {
                        // Given `import foo.bar`, `name` would be "foo", and `qualified_name` would be
                        // "foo.bar".
                        let name = alias.name.split('.').next().unwrap();
                        let qualified_name = &alias.name;
                        self.add_binding(
                            name,
                            alias.range(),
                            BindingKind::SubmoduleImportation(SubmoduleImportation {
                                qualified_name,
                            }),
                            BindingFlags::empty(),
                        );
                    } else {
                        let name = alias.asname.as_ref().unwrap_or(&alias.name);
                        let qualified_name = &alias.name;
                        self.add_binding(
                            name,
                            alias.range(),
                            BindingKind::Importation(Importation { qualified_name }),
                            if alias
                                .asname
                                .as_ref()
                                .map_or(false, |asname| asname == &alias.name)
                            {
                                BindingFlags::EXPLICIT_EXPORT
                            } else {
                                BindingFlags::empty()
                            },
                        );

                        if let Some(asname) = &alias.asname {
                            if self.enabled(Rule::BuiltinVariableShadowing) {
                                flake8_builtins::rules::builtin_variable_shadowing(
                                    self,
                                    asname,
                                    AnyShadowing::from(stmt),
                                );
                            }
                        }
                    }

                    // flake8-debugger
                    if self.enabled(Rule::Debugger) {
                        if let Some(diagnostic) =
                            flake8_debugger::rules::debugger_import(stmt, None, &alias.name)
                        {
                            self.diagnostics.push(diagnostic);
                        }
                    }

                    // flake8_tidy_imports
                    if self.enabled(Rule::BannedApi) {
                        flake8_tidy_imports::rules::name_or_parent_is_banned(
                            self,
                            &alias.name,
                            alias,
                        );
                    }

                    // pylint
                    if !self.is_stub {
                        if self.enabled(Rule::UselessImportAlias) {
                            pylint::rules::useless_import_alias(self, alias);
                        }
                    }
                    if self.enabled(Rule::ManualFromImport) {
                        pylint::rules::manual_from_import(self, stmt, alias, names);
                    }
                    if self.enabled(Rule::ImportSelf) {
                        if let Some(diagnostic) =
                            pylint::rules::import_self(alias, self.module_path)
                        {
                            self.diagnostics.push(diagnostic);
                        }
                    }

                    if let Some(asname) = &alias.asname {
                        let name = alias.name.split('.').last().unwrap();
                        if self.enabled(Rule::ConstantImportedAsNonConstant) {
                            if let Some(diagnostic) =
                                pep8_naming::rules::constant_imported_as_non_constant(
                                    name, asname, alias, stmt,
                                )
                            {
                                self.diagnostics.push(diagnostic);
                            }
                        }

                        if self.enabled(Rule::LowercaseImportedAsNonLowercase) {
                            if let Some(diagnostic) =
                                pep8_naming::rules::lowercase_imported_as_non_lowercase(
                                    name, asname, alias, stmt,
                                )
                            {
                                self.diagnostics.push(diagnostic);
                            }
                        }

                        if self.enabled(Rule::CamelcaseImportedAsLowercase) {
                            if let Some(diagnostic) =
                                pep8_naming::rules::camelcase_imported_as_lowercase(
                                    name, asname, alias, stmt,
                                )
                            {
                                self.diagnostics.push(diagnostic);
                            }
                        }

                        if self.enabled(Rule::CamelcaseImportedAsConstant) {
                            if let Some(diagnostic) =
                                pep8_naming::rules::camelcase_imported_as_constant(
                                    name, asname, alias, stmt,
                                )
                            {
                                self.diagnostics.push(diagnostic);
                            }
                        }

                        if self.enabled(Rule::CamelcaseImportedAsAcronym) {
                            if let Some(diagnostic) =
                                pep8_naming::rules::camelcase_imported_as_acronym(
                                    name, asname, alias, stmt,
                                )
                            {
                                self.diagnostics.push(diagnostic);
                            }
                        }
                    }

                    if self.enabled(Rule::UnconventionalImportAlias) {
                        if let Some(diagnostic) =
                            flake8_import_conventions::rules::conventional_import_alias(
                                stmt,
                                &alias.name,
                                alias.asname.as_deref(),
                                &self.settings.flake8_import_conventions.aliases,
                            )
                        {
                            self.diagnostics.push(diagnostic);
                        }
                    }

                    if self.enabled(Rule::BannedImportAlias) {
                        if let Some(asname) = &alias.asname {
                            if let Some(diagnostic) =
                                flake8_import_conventions::rules::banned_import_alias(
                                    stmt,
                                    &alias.name,
                                    asname,
                                    &self.settings.flake8_import_conventions.banned_aliases,
                                )
                            {
                                self.diagnostics.push(diagnostic);
                            }
                        }
                    }

                    if self.enabled(Rule::PytestIncorrectPytestImport) {
                        if let Some(diagnostic) = flake8_pytest_style::rules::import(
                            stmt,
                            &alias.name,
                            alias.asname.as_deref(),
                        ) {
                            self.diagnostics.push(diagnostic);
                        }
                    }
                }
            }
            Stmt::ImportFrom(
                import_from @ ast::StmtImportFrom {
                    names,
                    module,
                    level,
                    range: _,
                },
            ) => {
                let module = module.as_deref();
                let level = level.map(|level| level.to_u32());
                if self.enabled(Rule::ModuleImportNotAtTopOfFile) {
                    pycodestyle::rules::module_import_not_at_top_of_file(self, stmt, self.locator);
                }

                if self.enabled(Rule::GlobalStatement) {
                    for name in names.iter() {
                        if let Some(asname) = name.asname.as_ref() {
                            pylint::rules::global_statement(self, asname);
                        } else {
                            pylint::rules::global_statement(self, &name.name);
                        }
                    }
                }

                if self.enabled(Rule::UnnecessaryFutureImport)
                    && self.settings.target_version >= PythonVersion::Py37
                {
                    if let Some("__future__") = module {
                        pyupgrade::rules::unnecessary_future_import(self, stmt, names);
                    }
                }
                if self.enabled(Rule::DeprecatedMockImport) {
                    pyupgrade::rules::deprecated_mock_import(self, stmt);
                }
                if self.enabled(Rule::DeprecatedCElementTree) {
                    pyupgrade::rules::deprecated_c_element_tree(self, stmt);
                }
                if self.enabled(Rule::DeprecatedImport) {
                    pyupgrade::rules::deprecated_import(self, stmt, names, module, level);
                }
                if self.enabled(Rule::UnnecessaryBuiltinImport) {
                    if let Some(module) = module {
                        pyupgrade::rules::unnecessary_builtin_import(self, stmt, module, names);
                    }
                }
                if self.enabled(Rule::BannedApi) {
                    if let Some(module) =
                        helpers::resolve_imported_module_path(level, module, self.module_path)
                    {
                        flake8_tidy_imports::rules::name_or_parent_is_banned(self, &module, stmt);

                        for alias in names {
                            if &alias.name == "*" {
                                continue;
                            }
                            flake8_tidy_imports::rules::name_is_banned(
                                self,
                                format!("{module}.{}", alias.name),
                                alias,
                            );
                        }
                    }
                }

                if self.enabled(Rule::PytestIncorrectPytestImport) {
                    if let Some(diagnostic) =
                        flake8_pytest_style::rules::import_from(stmt, module, level)
                    {
                        self.diagnostics.push(diagnostic);
                    }
                }

                if self.is_stub {
                    if self.enabled(Rule::UnaliasedCollectionsAbcSetImport) {
                        flake8_pyi::rules::unaliased_collections_abc_set_import(self, import_from);
                    }
                    if self.enabled(Rule::FutureAnnotationsInStub) {
                        flake8_pyi::rules::from_future_import(self, import_from);
                    }
                }
                for alias in names {
                    if let Some("__future__") = module {
                        let name = alias.asname.as_ref().unwrap_or(&alias.name);

                        self.add_binding(
                            name,
                            alias.range(),
                            BindingKind::FutureImportation,
                            BindingFlags::empty(),
                        );

                        if self.enabled(Rule::FutureFeatureNotDefined) {
                            pyflakes::rules::future_feature_not_defined(self, alias);
                        }

                        if self.enabled(Rule::LateFutureImport) {
                            if self.semantic_model.seen_futures_boundary() {
                                self.diagnostics.push(Diagnostic::new(
                                    pyflakes::rules::LateFutureImport,
                                    stmt.range(),
                                ));
                            }
                        }
                    } else if &alias.name == "*" {
                        self.semantic_model
                            .scope_mut()
                            .add_star_import(StarImportation { level, module });

                        if self.enabled(Rule::UndefinedLocalWithNestedImportStarUsage) {
                            let scope = self.semantic_model.scope();
                            if !matches!(scope.kind, ScopeKind::Module) {
                                self.diagnostics.push(Diagnostic::new(
                                    pyflakes::rules::UndefinedLocalWithNestedImportStarUsage {
                                        name: helpers::format_import_from(level, module),
                                    },
                                    stmt.range(),
                                ));
                            }
                        }

                        if self.enabled(Rule::UndefinedLocalWithImportStar) {
                            self.diagnostics.push(Diagnostic::new(
                                pyflakes::rules::UndefinedLocalWithImportStar {
                                    name: helpers::format_import_from(level, module),
                                },
                                stmt.range(),
                            ));
                        }
                    } else {
                        if let Some(asname) = &alias.asname {
                            if self.enabled(Rule::BuiltinVariableShadowing) {
                                flake8_builtins::rules::builtin_variable_shadowing(
                                    self,
                                    asname,
                                    AnyShadowing::from(stmt),
                                );
                            }
                        }

                        // Given `from foo import bar`, `name` would be "bar" and `qualified_name` would
                        // be "foo.bar". Given `from foo import bar as baz`, `name` would be "baz"
                        // and `qualified_name` would be "foo.bar".
                        let name = alias.asname.as_ref().unwrap_or(&alias.name);
                        let qualified_name =
                            helpers::format_import_from_member(level, module, &alias.name);
                        self.add_binding(
                            name,
                            alias.range(),
                            BindingKind::FromImportation(FromImportation { qualified_name }),
                            if alias
                                .asname
                                .as_ref()
                                .map_or(false, |asname| asname == &alias.name)
                            {
                                BindingFlags::EXPLICIT_EXPORT
                            } else {
                                BindingFlags::empty()
                            },
                        );
                    }

                    if self.enabled(Rule::RelativeImports) {
                        if let Some(diagnostic) = flake8_tidy_imports::rules::banned_relative_import(
                            self,
                            stmt,
                            level,
                            module,
                            self.module_path,
                            self.settings.flake8_tidy_imports.ban_relative_imports,
                        ) {
                            self.diagnostics.push(diagnostic);
                        }
                    }

                    // flake8-debugger
                    if self.enabled(Rule::Debugger) {
                        if let Some(diagnostic) =
                            flake8_debugger::rules::debugger_import(stmt, module, &alias.name)
                        {
                            self.diagnostics.push(diagnostic);
                        }
                    }

                    if self.enabled(Rule::UnconventionalImportAlias) {
                        let qualified_name =
                            helpers::format_import_from_member(level, module, &alias.name);
                        if let Some(diagnostic) =
                            flake8_import_conventions::rules::conventional_import_alias(
                                stmt,
                                &qualified_name,
                                alias.asname.as_deref(),
                                &self.settings.flake8_import_conventions.aliases,
                            )
                        {
                            self.diagnostics.push(diagnostic);
                        }
                    }

                    if self.enabled(Rule::BannedImportAlias) {
                        if let Some(asname) = &alias.asname {
                            let qualified_name =
                                helpers::format_import_from_member(level, module, &alias.name);
                            if let Some(diagnostic) =
                                flake8_import_conventions::rules::banned_import_alias(
                                    stmt,
                                    &qualified_name,
                                    asname,
                                    &self.settings.flake8_import_conventions.banned_aliases,
                                )
                            {
                                self.diagnostics.push(diagnostic);
                            }
                        }
                    }

                    if let Some(asname) = &alias.asname {
                        if self.enabled(Rule::ConstantImportedAsNonConstant) {
                            if let Some(diagnostic) =
                                pep8_naming::rules::constant_imported_as_non_constant(
                                    &alias.name,
                                    asname,
                                    alias,
                                    stmt,
                                )
                            {
                                self.diagnostics.push(diagnostic);
                            }
                        }

                        if self.enabled(Rule::LowercaseImportedAsNonLowercase) {
                            if let Some(diagnostic) =
                                pep8_naming::rules::lowercase_imported_as_non_lowercase(
                                    &alias.name,
                                    asname,
                                    alias,
                                    stmt,
                                )
                            {
                                self.diagnostics.push(diagnostic);
                            }
                        }

                        if self.enabled(Rule::CamelcaseImportedAsLowercase) {
                            if let Some(diagnostic) =
                                pep8_naming::rules::camelcase_imported_as_lowercase(
                                    &alias.name,
                                    asname,
                                    alias,
                                    stmt,
                                )
                            {
                                self.diagnostics.push(diagnostic);
                            }
                        }

                        if self.enabled(Rule::CamelcaseImportedAsConstant) {
                            if let Some(diagnostic) =
                                pep8_naming::rules::camelcase_imported_as_constant(
                                    &alias.name,
                                    asname,
                                    alias,
                                    stmt,
                                )
                            {
                                self.diagnostics.push(diagnostic);
                            }
                        }

                        if self.enabled(Rule::CamelcaseImportedAsAcronym) {
                            if let Some(diagnostic) =
                                pep8_naming::rules::camelcase_imported_as_acronym(
                                    &alias.name,
                                    asname,
                                    alias,
                                    stmt,
                                )
                            {
                                self.diagnostics.push(diagnostic);
                            }
                        }

                        // pylint
                        if !self.is_stub {
                            if self.enabled(Rule::UselessImportAlias) {
                                pylint::rules::useless_import_alias(self, alias);
                            }
                        }
                    }
                }

                if self.enabled(Rule::ImportSelf) {
                    if let Some(diagnostic) =
                        pylint::rules::import_from_self(level, module, names, self.module_path)
                    {
                        self.diagnostics.push(diagnostic);
                    }
                }

                if self.enabled(Rule::BannedImportFrom) {
                    if let Some(diagnostic) = flake8_import_conventions::rules::banned_import_from(
                        stmt,
                        &helpers::format_import_from(level, module),
                        &self.settings.flake8_import_conventions.banned_from,
                    ) {
                        self.diagnostics.push(diagnostic);
                    }
                }
            }
            Stmt::Raise(ast::StmtRaise { exc, .. }) => {
                if self.enabled(Rule::RaiseNotImplemented) {
                    if let Some(expr) = exc {
                        pyflakes::rules::raise_not_implemented(self, expr);
                    }
                }
                if self.enabled(Rule::RaiseLiteral) {
                    if let Some(exc) = exc {
                        flake8_bugbear::rules::raise_literal(self, exc);
                    }
                }
                if self.any_enabled(&[
                    Rule::RawStringInException,
                    Rule::FStringInException,
                    Rule::DotFormatInException,
                ]) {
                    if let Some(exc) = exc {
                        flake8_errmsg::rules::string_in_exception(self, stmt, exc);
                    }
                }
                if self.enabled(Rule::OSErrorAlias) {
                    if let Some(item) = exc {
                        pyupgrade::rules::os_error_alias_raise(self, item);
                    }
                }
                if self.enabled(Rule::RaiseVanillaClass) {
                    if let Some(expr) = exc {
                        tryceratops::rules::raise_vanilla_class(self, expr);
                    }
                }
                if self.enabled(Rule::RaiseVanillaArgs) {
                    if let Some(expr) = exc {
                        tryceratops::rules::raise_vanilla_args(self, expr);
                    }
                }
                if self.enabled(Rule::UnnecessaryParenOnRaiseException) {
                    if let Some(expr) = exc {
                        flake8_raise::rules::unnecessary_paren_on_raise_exception(self, expr);
                    }
                }
            }
            Stmt::AugAssign(ast::StmtAugAssign { target, .. }) => {
                self.handle_node_load(target);

                if self.enabled(Rule::GlobalStatement) {
                    if let Expr::Name(ast::ExprName { id, .. }) = target.as_ref() {
                        pylint::rules::global_statement(self, id);
                    }
                }
            }
            Stmt::If(ast::StmtIf {
                test,
                body,
                orelse,
                range: _,
            }) => {
                if self.enabled(Rule::IfTuple) {
                    pyflakes::rules::if_tuple(self, stmt, test);
                }
                if self.enabled(Rule::CollapsibleIf) {
                    flake8_simplify::rules::nested_if_statements(
                        self,
                        stmt,
                        test,
                        body,
                        orelse,
                        self.semantic_model.stmt_parent(),
                    );
                }
                if self.enabled(Rule::IfWithSameArms) {
                    flake8_simplify::rules::if_with_same_arms(
                        self,
                        stmt,
                        self.semantic_model.stmt_parent(),
                    );
                }
                if self.enabled(Rule::NeedlessBool) {
                    flake8_simplify::rules::needless_bool(self, stmt);
                }
                if self.enabled(Rule::IfElseBlockInsteadOfDictLookup) {
                    flake8_simplify::rules::manual_dict_lookup(
                        self,
                        stmt,
                        test,
                        body,
                        orelse,
                        self.semantic_model.stmt_parent(),
                    );
                }
                if self.enabled(Rule::IfElseBlockInsteadOfIfExp) {
                    flake8_simplify::rules::use_ternary_operator(
                        self,
                        stmt,
                        self.semantic_model.stmt_parent(),
                    );
                }
                if self.enabled(Rule::IfElseBlockInsteadOfDictGet) {
                    flake8_simplify::rules::use_dict_get_with_default(
                        self,
                        stmt,
                        test,
                        body,
                        orelse,
                        self.semantic_model.stmt_parent(),
                    );
                }
                if self.enabled(Rule::TypeCheckWithoutTypeError) {
                    tryceratops::rules::type_check_without_type_error(
                        self,
                        body,
                        test,
                        orelse,
                        self.semantic_model.stmt_parent(),
                    );
                }
                if self.enabled(Rule::OutdatedVersionBlock) {
                    pyupgrade::rules::outdated_version_block(self, stmt, test, body, orelse);
                }
                if self.enabled(Rule::CollapsibleElseIf) {
                    if let Some(diagnostic) =
                        pylint::rules::collapsible_else_if(orelse, self.locator)
                    {
                        self.diagnostics.push(diagnostic);
                    }
                }
            }
            Stmt::Assert(ast::StmtAssert {
                test,
                msg,
                range: _,
            }) => {
                if !self.semantic_model.in_type_checking_block() {
                    if self.enabled(Rule::Assert) {
                        self.diagnostics
                            .push(flake8_bandit::rules::assert_used(stmt));
                    }
                }
                if self.enabled(Rule::AssertTuple) {
                    pyflakes::rules::assert_tuple(self, stmt, test);
                }
                if self.enabled(Rule::AssertFalse) {
                    flake8_bugbear::rules::assert_false(self, stmt, test, msg.as_deref());
                }
                if self.enabled(Rule::PytestAssertAlwaysFalse) {
                    flake8_pytest_style::rules::assert_falsy(self, stmt, test);
                }
                if self.enabled(Rule::PytestCompositeAssertion) {
                    flake8_pytest_style::rules::composite_condition(
                        self,
                        stmt,
                        test,
                        msg.as_deref(),
                    );
                }
                if self.enabled(Rule::AssertOnStringLiteral) {
                    pylint::rules::assert_on_string_literal(self, test);
                }
                if self.enabled(Rule::InvalidMockAccess) {
                    pygrep_hooks::rules::non_existent_mock_method(self, test);
                }
            }
            Stmt::With(ast::StmtWith { items, body, .. })
            | Stmt::AsyncWith(ast::StmtAsyncWith { items, body, .. }) => {
                if self.enabled(Rule::AssertRaisesException) {
                    flake8_bugbear::rules::assert_raises_exception(self, stmt, items);
                }
                if self.enabled(Rule::PytestRaisesWithMultipleStatements) {
                    flake8_pytest_style::rules::complex_raises(self, stmt, items, body);
                }
                if self.enabled(Rule::MultipleWithStatements) {
                    flake8_simplify::rules::multiple_with_statements(
                        self,
                        stmt,
                        body,
                        self.semantic_model.stmt_parent(),
                    );
                }
                if self.enabled(Rule::RedefinedLoopName) {
                    pylint::rules::redefined_loop_name(self, &Node::Stmt(stmt));
                }
            }
            Stmt::While(ast::StmtWhile { body, orelse, .. }) => {
                if self.enabled(Rule::FunctionUsesLoopVariable) {
                    flake8_bugbear::rules::function_uses_loop_variable(self, &Node::Stmt(stmt));
                }
                if self.enabled(Rule::UselessElseOnLoop) {
                    pylint::rules::useless_else_on_loop(self, stmt, body, orelse);
                }
            }
            Stmt::For(ast::StmtFor {
                target,
                body,
                iter,
                orelse,
                ..
            })
            | Stmt::AsyncFor(ast::StmtAsyncFor {
                target,
                body,
                iter,
                orelse,
                ..
            }) => {
                if self.enabled(Rule::UnusedLoopControlVariable) {
                    self.deferred.for_loops.push(self.semantic_model.snapshot());
                }
                if self.enabled(Rule::LoopVariableOverridesIterator) {
                    flake8_bugbear::rules::loop_variable_overrides_iterator(self, target, iter);
                }
                if self.enabled(Rule::FunctionUsesLoopVariable) {
                    flake8_bugbear::rules::function_uses_loop_variable(self, &Node::Stmt(stmt));
                }
                if self.enabled(Rule::ReuseOfGroupbyGenerator) {
                    flake8_bugbear::rules::reuse_of_groupby_generator(self, target, body, iter);
                }
                if self.enabled(Rule::UselessElseOnLoop) {
                    pylint::rules::useless_else_on_loop(self, stmt, body, orelse);
                }
                if self.enabled(Rule::RedefinedLoopName) {
                    pylint::rules::redefined_loop_name(self, &Node::Stmt(stmt));
                }
                if self.enabled(Rule::IterationOverSet) {
                    pylint::rules::iteration_over_set(self, iter);
                }
                if stmt.is_for_stmt() {
                    if self.enabled(Rule::ReimplementedBuiltin) {
                        flake8_simplify::rules::convert_for_loop_to_any_all(
                            self,
                            stmt,
                            self.semantic_model.sibling_stmt(),
                        );
                    }
                    if self.enabled(Rule::InDictKeys) {
                        flake8_simplify::rules::key_in_dict_for(self, target, iter);
                    }
                }
            }
            Stmt::Try(ast::StmtTry {
                body,
                handlers,
                orelse,
                finalbody,
                range: _,
            })
            | Stmt::TryStar(ast::StmtTryStar {
                body,
                handlers,
                orelse,
                finalbody,
                range: _,
            }) => {
                if self.enabled(Rule::DefaultExceptNotLast) {
                    if let Some(diagnostic) =
                        pyflakes::rules::default_except_not_last(handlers, self.locator)
                    {
                        self.diagnostics.push(diagnostic);
                    }
                }
                if self.any_enabled(&[
                    Rule::DuplicateHandlerException,
                    Rule::DuplicateTryBlockException,
                ]) {
                    flake8_bugbear::rules::duplicate_exceptions(self, handlers);
                }
                if self.enabled(Rule::RedundantTupleInExceptionHandler) {
                    flake8_bugbear::rules::redundant_tuple_in_exception_handler(self, handlers);
                }
                if self.enabled(Rule::OSErrorAlias) {
                    pyupgrade::rules::os_error_alias_handlers(self, handlers);
                }
                if self.enabled(Rule::PytestAssertInExcept) {
                    self.diagnostics.extend(
                        flake8_pytest_style::rules::assert_in_exception_handler(handlers),
                    );
                }
                if self.enabled(Rule::SuppressibleException) {
                    flake8_simplify::rules::suppressible_exception(
                        self, stmt, body, handlers, orelse, finalbody,
                    );
                }
                if self.enabled(Rule::ReturnInTryExceptFinally) {
                    flake8_simplify::rules::return_in_try_except_finally(
                        self, body, handlers, finalbody,
                    );
                }
                if self.enabled(Rule::TryConsiderElse) {
                    tryceratops::rules::try_consider_else(self, body, orelse, handlers);
                }
                if self.enabled(Rule::VerboseRaise) {
                    tryceratops::rules::verbose_raise(self, handlers);
                }
                if self.enabled(Rule::VerboseLogMessage) {
                    tryceratops::rules::verbose_log_message(self, handlers);
                }
                if self.enabled(Rule::RaiseWithinTry) {
                    tryceratops::rules::raise_within_try(self, body, handlers);
                }
                if self.enabled(Rule::UselessTryExcept) {
                    tryceratops::rules::useless_try_except(self, handlers);
                }
                if self.enabled(Rule::ErrorInsteadOfException) {
                    tryceratops::rules::error_instead_of_exception(self, handlers);
                }
            }
            Stmt::Assign(ast::StmtAssign { targets, value, .. }) => {
                if self.enabled(Rule::LambdaAssignment) {
                    if let [target] = &targets[..] {
                        pycodestyle::rules::lambda_assignment(self, target, value, None, stmt);
                    }
                }
                if self.enabled(Rule::AssignmentToOsEnviron) {
                    flake8_bugbear::rules::assignment_to_os_environ(self, targets);
                }
                if self.enabled(Rule::HardcodedPasswordString) {
                    if let Some(diagnostic) =
                        flake8_bandit::rules::assign_hardcoded_password_string(value, targets)
                    {
                        self.diagnostics.push(diagnostic);
                    }
                }
                if self.enabled(Rule::GlobalStatement) {
                    for target in targets.iter() {
                        if let Expr::Name(ast::ExprName { id, .. }) = target {
                            pylint::rules::global_statement(self, id);
                        }
                    }
                }
                if self.enabled(Rule::UselessMetaclassType) {
                    pyupgrade::rules::useless_metaclass_type(self, stmt, value, targets);
                }
                if self.enabled(Rule::ConvertTypedDictFunctionalToClass) {
                    pyupgrade::rules::convert_typed_dict_functional_to_class(
                        self, stmt, targets, value,
                    );
                }
                if self.enabled(Rule::ConvertNamedTupleFunctionalToClass) {
                    pyupgrade::rules::convert_named_tuple_functional_to_class(
                        self, stmt, targets, value,
                    );
                }
                if self.enabled(Rule::UnpackedListComprehension) {
                    pyupgrade::rules::unpacked_list_comprehension(self, targets, value);
                }
                if self.enabled(Rule::PandasDfVariableName) {
                    if let Some(diagnostic) = pandas_vet::rules::assignment_to_df(targets) {
                        self.diagnostics.push(diagnostic);
                    }
                }
                if self
                    .settings
                    .rules
                    .enabled(Rule::AirflowVariableNameTaskIdMismatch)
                {
                    if let Some(diagnostic) =
                        airflow::rules::variable_name_task_id(self, targets, value)
                    {
                        self.diagnostics.push(diagnostic);
                    }
                }
                if self.is_stub {
                    if self.any_enabled(&[
                        Rule::UnprefixedTypeParam,
                        Rule::AssignmentDefaultInStub,
                        Rule::UnannotatedAssignmentInStub,
                    ]) {
                        // Ignore assignments in function bodies; those are covered by other rules.
                        if !self
                            .semantic_model
                            .scopes()
                            .any(|scope| scope.kind.is_any_function())
                        {
                            if self.enabled(Rule::UnprefixedTypeParam) {
                                flake8_pyi::rules::prefix_type_params(self, value, targets);
                            }
                            if self.enabled(Rule::AssignmentDefaultInStub) {
                                flake8_pyi::rules::assignment_default_in_stub(self, targets, value);
                            }
                            if self.enabled(Rule::UnannotatedAssignmentInStub) {
                                flake8_pyi::rules::unannotated_assignment_in_stub(
                                    self, targets, value,
                                );
                            }
                        }
                    }
                }
            }
            Stmt::AnnAssign(ast::StmtAnnAssign {
                target,
                value,
                annotation,
                ..
            }) => {
                if self.enabled(Rule::LambdaAssignment) {
                    if let Some(value) = value {
                        pycodestyle::rules::lambda_assignment(
                            self,
                            target,
                            value,
                            Some(annotation),
                            stmt,
                        );
                    }
                }
                if self.enabled(Rule::UnintentionalTypeAnnotation) {
                    flake8_bugbear::rules::unintentional_type_annotation(
                        self,
                        target,
                        value.as_deref(),
                        stmt,
                    );
                }
                if self.is_stub {
                    if let Some(value) = value {
                        if self.enabled(Rule::AssignmentDefaultInStub) {
                            // Ignore assignments in function bodies; those are covered by other rules.
                            if !self
                                .semantic_model
                                .scopes()
                                .any(|scope| scope.kind.is_any_function())
                            {
                                flake8_pyi::rules::annotated_assignment_default_in_stub(
                                    self, target, value, annotation,
                                );
                            }
                        }
                    } else {
                        if self.enabled(Rule::UnassignedSpecialVariableInStub) {
                            flake8_pyi::rules::unassigned_special_variable_in_stub(
                                self, target, stmt,
                            );
                        }
                    }
                    if self
                        .semantic_model
                        .match_typing_expr(annotation, "TypeAlias")
                    {
                        if self.enabled(Rule::SnakeCaseTypeAlias) {
                            flake8_pyi::rules::snake_case_type_alias(self, target);
                        }
                        if self.enabled(Rule::TSuffixedTypeAlias) {
                            flake8_pyi::rules::t_suffixed_type_alias(self, target);
                        }
                    }
                }
            }
            Stmt::Delete(ast::StmtDelete { targets, range: _ }) => {
                if self.enabled(Rule::GlobalStatement) {
                    for target in targets.iter() {
                        if let Expr::Name(ast::ExprName { id, .. }) = target {
                            pylint::rules::global_statement(self, id);
                        }
                    }
                }
            }
            Stmt::Expr(ast::StmtExpr { value, range: _ }) => {
                if self.enabled(Rule::UselessComparison) {
                    flake8_bugbear::rules::useless_comparison(self, value);
                }
                if self.enabled(Rule::UselessExpression) {
                    flake8_bugbear::rules::useless_expression(self, value);
                }
                if self.enabled(Rule::InvalidMockAccess) {
                    pygrep_hooks::rules::uncalled_mock_method(self, value);
                }
                if self.enabled(Rule::NamedExprWithoutContext) {
                    pylint::rules::named_expr_without_context(self, value);
                }
                if self.enabled(Rule::AsyncioDanglingTask) {
                    if let Some(diagnostic) = ruff::rules::asyncio_dangling_task(value, |expr| {
                        self.semantic_model.resolve_call_path(expr)
                    }) {
                        self.diagnostics.push(diagnostic);
                    }
                }
            }
            _ => {}
        }

        // Recurse.
        match stmt {
            Stmt::FunctionDef(ast::StmtFunctionDef {
                body,
                name,
                args,
                decorator_list,
                returns,
                ..
            })
            | Stmt::AsyncFunctionDef(ast::StmtAsyncFunctionDef {
                body,
                name,
                args,
                decorator_list,
                returns,
                ..
            }) => {
                // Visit the decorators and arguments, but avoid the body, which will be
                // deferred.
                for decorator in decorator_list {
                    self.visit_decorator(decorator);
                }

                // Function annotations are always evaluated at runtime, unless future annotations
                // are enabled.
                let runtime_annotation = !self.semantic_model.future_annotations();

                for arg in &args.posonlyargs {
                    if let Some(expr) = &arg.annotation {
                        if runtime_annotation {
                            self.visit_type_definition(expr);
                        } else {
                            self.visit_annotation(expr);
                        };
                    }
                }
                for arg in &args.args {
                    if let Some(expr) = &arg.annotation {
                        if runtime_annotation {
                            self.visit_type_definition(expr);
                        } else {
                            self.visit_annotation(expr);
                        };
                    }
                }
                if let Some(arg) = &args.vararg {
                    if let Some(expr) = &arg.annotation {
                        if runtime_annotation {
                            self.visit_type_definition(expr);
                        } else {
                            self.visit_annotation(expr);
                        };
                    }
                }
                for arg in &args.kwonlyargs {
                    if let Some(expr) = &arg.annotation {
                        if runtime_annotation {
                            self.visit_type_definition(expr);
                        } else {
                            self.visit_annotation(expr);
                        };
                    }
                }
                if let Some(arg) = &args.kwarg {
                    if let Some(expr) = &arg.annotation {
                        if runtime_annotation {
                            self.visit_type_definition(expr);
                        } else {
                            self.visit_annotation(expr);
                        };
                    }
                }
                for expr in returns {
                    if runtime_annotation {
                        self.visit_type_definition(expr);
                    } else {
                        self.visit_annotation(expr);
                    };
                }
                for expr in &args.kw_defaults {
                    self.visit_expr(expr);
                }
                for expr in &args.defaults {
                    self.visit_expr(expr);
                }

                self.add_binding(
                    name,
                    stmt.range(),
                    BindingKind::FunctionDefinition,
                    BindingFlags::empty(),
                );

                let definition = docstrings::extraction::extract_definition(
                    ExtractionTarget::Function,
                    stmt,
                    self.semantic_model.definition_id,
                    &self.semantic_model.definitions,
                );
                self.semantic_model.push_definition(definition);

                self.semantic_model.push_scope(match &stmt {
                    Stmt::FunctionDef(stmt) => ScopeKind::Function(stmt),
                    Stmt::AsyncFunctionDef(stmt) => ScopeKind::AsyncFunction(stmt),
                    _ => unreachable!("Expected Stmt::FunctionDef | Stmt::AsyncFunctionDef"),
                });

                self.deferred.functions.push(self.semantic_model.snapshot());

                // Extract any global bindings from the function body.
                if let Some(globals) = Globals::from_body(body) {
                    self.semantic_model.set_globals(globals);
                }
            }
            Stmt::ClassDef(
                class_def @ ast::StmtClassDef {
                    body,
                    bases,
                    keywords,
                    decorator_list,
                    ..
                },
            ) => {
                for expr in bases {
                    self.visit_expr(expr);
                }
                for keyword in keywords {
                    self.visit_keyword(keyword);
                }
                for decorator in decorator_list {
                    self.visit_decorator(decorator);
                }

                let definition = docstrings::extraction::extract_definition(
                    ExtractionTarget::Class,
                    stmt,
                    self.semantic_model.definition_id,
                    &self.semantic_model.definitions,
                );
                self.semantic_model.push_definition(definition);

                self.semantic_model.push_scope(ScopeKind::Class(class_def));

                // Extract any global bindings from the class body.
                if let Some(globals) = Globals::from_body(body) {
                    self.semantic_model.set_globals(globals);
                }

                self.visit_body(body);
            }
            Stmt::Try(ast::StmtTry {
                body,
                handlers,
                orelse,
                finalbody,
                range: _,
            })
            | Stmt::TryStar(ast::StmtTryStar {
                body,
                handlers,
                orelse,
                finalbody,
                range: _,
            }) => {
                let mut handled_exceptions = Exceptions::empty();
                for type_ in extract_handled_exceptions(handlers) {
                    if let Some(call_path) = self.semantic_model.resolve_call_path(type_) {
                        match call_path.as_slice() {
                            ["", "NameError"] => {
                                handled_exceptions |= Exceptions::NAME_ERROR;
                            }
                            ["", "ModuleNotFoundError"] => {
                                handled_exceptions |= Exceptions::MODULE_NOT_FOUND_ERROR;
                            }
                            ["", "ImportError"] => {
                                handled_exceptions |= Exceptions::IMPORT_ERROR;
                            }
                            _ => {}
                        }
                    }
                }

                self.semantic_model
                    .handled_exceptions
                    .push(handled_exceptions);

                if self.enabled(Rule::JumpStatementInFinally) {
                    flake8_bugbear::rules::jump_statement_in_finally(self, finalbody);
                }

                if self.enabled(Rule::ContinueInFinally) {
                    if self.settings.target_version <= PythonVersion::Py38 {
                        pylint::rules::continue_in_finally(self, finalbody);
                    }
                }

                self.visit_body(body);
                self.semantic_model.handled_exceptions.pop();

                self.semantic_model.flags |= SemanticModelFlags::EXCEPTION_HANDLER;
                for excepthandler in handlers {
                    self.visit_excepthandler(excepthandler);
                }

                self.visit_body(orelse);
                self.visit_body(finalbody);
            }
            Stmt::AnnAssign(ast::StmtAnnAssign {
                target,
                annotation,
                value,
                ..
            }) => {
                // If we're in a class or module scope, then the annotation needs to be
                // available at runtime.
                // See: https://docs.python.org/3/reference/simple_stmts.html#annotated-assignment-statements
                let runtime_annotation = if self.semantic_model.future_annotations() {
                    if self.semantic_model.scope().kind.is_class() {
                        let baseclasses = &self
                            .settings
                            .flake8_type_checking
                            .runtime_evaluated_base_classes;
                        let decorators = &self
                            .settings
                            .flake8_type_checking
                            .runtime_evaluated_decorators;
                        flake8_type_checking::helpers::runtime_evaluated(
                            &self.semantic_model,
                            baseclasses,
                            decorators,
                        )
                    } else {
                        false
                    }
                } else {
                    matches!(
                        self.semantic_model.scope().kind,
                        ScopeKind::Class(_) | ScopeKind::Module
                    )
                };

                if runtime_annotation {
                    self.visit_type_definition(annotation);
                } else {
                    self.visit_annotation(annotation);
                }
                if let Some(expr) = value {
                    if self
                        .semantic_model
                        .match_typing_expr(annotation, "TypeAlias")
                    {
                        self.visit_type_definition(expr);
                    } else {
                        self.visit_expr(expr);
                    }
                }
                self.visit_expr(target);
            }
            Stmt::Assert(ast::StmtAssert {
                test,
                msg,
                range: _,
            }) => {
                self.visit_boolean_test(test);
                if let Some(expr) = msg {
                    self.visit_expr(expr);
                }
            }
            Stmt::While(ast::StmtWhile {
                test,
                body,
                orelse,
                range: _,
            }) => {
                self.visit_boolean_test(test);
                self.visit_body(body);
                self.visit_body(orelse);
            }
            Stmt::If(
                stmt_if @ ast::StmtIf {
                    test,
                    body,
                    orelse,
                    range: _,
                },
            ) => {
                self.visit_boolean_test(test);

                if analyze::typing::is_type_checking_block(stmt_if, &self.semantic_model) {
                    if self.semantic_model.at_top_level() {
                        self.importer.visit_type_checking_block(stmt);
                    }

                    if self.enabled(Rule::EmptyTypeCheckingBlock) {
                        flake8_type_checking::rules::empty_type_checking_block(self, stmt_if);
                    }

                    self.visit_type_checking_block(body);
                } else {
                    self.visit_body(body);
                }

                self.visit_body(orelse);
            }
            _ => visitor::walk_stmt(self, stmt),
        };

        // Post-visit.
        match stmt {
            Stmt::FunctionDef(_) | Stmt::AsyncFunctionDef(_) => {
                self.semantic_model.pop_scope();
                self.semantic_model.pop_definition();
            }
            Stmt::ClassDef(ast::StmtClassDef { name, .. }) => {
                self.semantic_model.pop_scope();
                self.semantic_model.pop_definition();
                self.add_binding(
                    name,
                    stmt.range(),
                    BindingKind::ClassDefinition,
                    BindingFlags::empty(),
                );
            }
            _ => {}
        }

        self.semantic_model.flags = flags_snapshot;
        self.semantic_model.pop_stmt();
    }

    fn visit_annotation(&mut self, expr: &'b Expr) {
        let flags_snapshot = self.semantic_model.flags;
        self.semantic_model.flags |= SemanticModelFlags::ANNOTATION;
        self.visit_type_definition(expr);
        self.semantic_model.flags = flags_snapshot;
    }

    fn visit_expr(&mut self, expr: &'b Expr) {
        if !self.semantic_model.in_f_string()
            && !self.semantic_model.in_deferred_type_definition()
            && self.semantic_model.in_type_definition()
            && self.semantic_model.future_annotations()
        {
            if let Expr::Constant(ast::ExprConstant {
                value: Constant::Str(value),
                ..
            }) = expr
            {
                self.deferred.string_type_definitions.push((
                    expr.range(),
                    value,
                    self.semantic_model.snapshot(),
                ));
            } else {
                self.deferred
                    .future_type_definitions
                    .push((expr, self.semantic_model.snapshot()));
            }
            return;
        }

        self.semantic_model.push_expr(expr);

        // Store the flags prior to any further descent, so that we can restore them after visiting
        // the node.
        let flags_snapshot = self.semantic_model.flags;

        // If we're in a boolean test (e.g., the `test` of a `Stmt::If`), but now within a
        // subexpression (e.g., `a` in `f(a)`), then we're no longer in a boolean test.
        if !matches!(
            expr,
            Expr::BoolOp(_)
                | Expr::UnaryOp(ast::ExprUnaryOp {
                    op: Unaryop::Not,
                    ..
                })
        ) {
            self.semantic_model.flags -= SemanticModelFlags::BOOLEAN_TEST;
        }

        // Pre-visit.
        match expr {
            Expr::Subscript(ast::ExprSubscript { value, slice, .. }) => {
                // Ex) Optional[...], Union[...]
                if self.any_enabled(&[
                    Rule::FutureRewritableTypeAnnotation,
                    Rule::NonPEP604Annotation,
                ]) {
                    if let Some(operator) =
                        analyze::typing::to_pep604_operator(value, slice, &self.semantic_model)
                    {
                        if self.enabled(Rule::FutureRewritableTypeAnnotation) {
                            if self.settings.target_version < PythonVersion::Py310
                                && self.settings.target_version >= PythonVersion::Py37
                                && !self.semantic_model.future_annotations()
                                && self.semantic_model.in_annotation()
                            {
                                flake8_future_annotations::rules::future_rewritable_type_annotation(
                                    self, value,
                                );
                            }
                        }
                        if self.enabled(Rule::NonPEP604Annotation) {
                            if self.settings.target_version >= PythonVersion::Py310
                                || (self.settings.target_version >= PythonVersion::Py37
                                    && self.semantic_model.future_annotations()
                                    && self.semantic_model.in_annotation())
                            {
                                pyupgrade::rules::use_pep604_annotation(
                                    self, expr, slice, operator,
                                );
                            }
                        }
                    }
                }

                // Ex) list[...]
                if self.enabled(Rule::FutureRequiredTypeAnnotation) {
                    if self.settings.target_version < PythonVersion::Py39
                        && !self.semantic_model.future_annotations()
                        && self.semantic_model.in_annotation()
                        && analyze::typing::is_pep585_generic(value, &self.semantic_model)
                    {
                        flake8_future_annotations::rules::future_required_type_annotation(
                            self,
                            expr,
                            flake8_future_annotations::rules::Reason::PEP585,
                        );
                    }
                }

                if self.semantic_model.match_typing_expr(value, "Literal") {
                    self.semantic_model.flags |= SemanticModelFlags::LITERAL;
                }

                if self.any_enabled(&[
                    Rule::SysVersionSlice3,
                    Rule::SysVersion2,
                    Rule::SysVersion0,
                    Rule::SysVersionSlice1,
                ]) {
                    flake8_2020::rules::subscript(self, value, slice);
                }

                if self.enabled(Rule::UncapitalizedEnvironmentVariables) {
                    flake8_simplify::rules::use_capital_environment_variables(self, expr);
                }

                pandas_vet::rules::subscript(self, value, expr);
            }
            Expr::Tuple(ast::ExprTuple {
                elts,
                ctx,
                range: _,
            })
            | Expr::List(ast::ExprList {
                elts,
                ctx,
                range: _,
            }) => {
                if matches!(ctx, ExprContext::Store) {
                    let check_too_many_expressions =
                        self.enabled(Rule::ExpressionsInStarAssignment);
                    let check_two_starred_expressions =
                        self.enabled(Rule::MultipleStarredExpressions);
                    if let Some(diagnostic) = pyflakes::rules::starred_expressions(
                        elts,
                        check_too_many_expressions,
                        check_two_starred_expressions,
                        expr.range(),
                    ) {
                        self.diagnostics.push(diagnostic);
                    }
                }
            }
            Expr::Name(ast::ExprName { id, ctx, range: _ }) => {
                match ctx {
                    ExprContext::Load => {
                        if self.enabled(Rule::TypingTextStrAlias) {
                            pyupgrade::rules::typing_text_str_alias(self, expr);
                        }
                        if self.enabled(Rule::NumpyDeprecatedTypeAlias) {
                            numpy::rules::deprecated_type_alias(self, expr);
                        }
                        if self.is_stub {
                            if self.enabled(Rule::CollectionsNamedTuple) {
                                flake8_pyi::rules::collections_named_tuple(self, expr);
                            }
                        }

                        // Ex) List[...]
                        if self.any_enabled(&[
                            Rule::FutureRewritableTypeAnnotation,
                            Rule::NonPEP585Annotation,
                        ]) {
                            if let Some(replacement) =
                                analyze::typing::to_pep585_generic(expr, &self.semantic_model)
                            {
                                if self.enabled(Rule::FutureRewritableTypeAnnotation) {
                                    if self.settings.target_version < PythonVersion::Py39
                                        && self.settings.target_version >= PythonVersion::Py37
                                        && !self.semantic_model.future_annotations()
                                        && self.semantic_model.in_annotation()
                                    {
                                        flake8_future_annotations::rules::future_rewritable_type_annotation(
                                            self, expr,
                                        );
                                    }
                                }
                                if self.enabled(Rule::NonPEP585Annotation) {
                                    if self.settings.target_version >= PythonVersion::Py39
                                        || (self.settings.target_version >= PythonVersion::Py37
                                            && self.semantic_model.future_annotations()
                                            && self.semantic_model.in_annotation())
                                    {
                                        pyupgrade::rules::use_pep585_annotation(
                                            self,
                                            expr,
                                            &replacement,
                                        );
                                    }
                                }
                            }
                        }

                        self.handle_node_load(expr);
                    }
                    ExprContext::Store => {
                        if self.enabled(Rule::AmbiguousVariableName) {
                            if let Some(diagnostic) =
                                pycodestyle::rules::ambiguous_variable_name(id, expr.range())
                            {
                                self.diagnostics.push(diagnostic);
                            }
                        }

                        if self.semantic_model.scope().kind.is_class() {
                            if self.enabled(Rule::BuiltinAttributeShadowing) {
                                flake8_builtins::rules::builtin_attribute_shadowing(
                                    self,
                                    id,
                                    AnyShadowing::from(expr),
                                );
                            }
                        } else {
                            if self.enabled(Rule::BuiltinVariableShadowing) {
                                flake8_builtins::rules::builtin_variable_shadowing(
                                    self,
                                    id,
                                    AnyShadowing::from(expr),
                                );
                            }
                        }

                        self.handle_node_store(id, expr);
                    }
                    ExprContext::Del => self.handle_node_delete(expr),
                }

                if self.enabled(Rule::SixPY3) {
                    flake8_2020::rules::name_or_attribute(self, expr);
                }

                if self.enabled(Rule::LoadBeforeGlobalDeclaration) {
                    pylint::rules::load_before_global_declaration(self, id, expr);
                }
            }
            Expr::Attribute(ast::ExprAttribute { attr, value, .. }) => {
                // Ex) typing.List[...]
                if self.any_enabled(&[
                    Rule::FutureRewritableTypeAnnotation,
                    Rule::NonPEP585Annotation,
                ]) {
                    if let Some(replacement) =
                        analyze::typing::to_pep585_generic(expr, &self.semantic_model)
                    {
                        if self.enabled(Rule::FutureRewritableTypeAnnotation) {
                            if self.settings.target_version < PythonVersion::Py39
                                && self.settings.target_version >= PythonVersion::Py37
                                && !self.semantic_model.future_annotations()
                                && self.semantic_model.in_annotation()
                            {
                                flake8_future_annotations::rules::future_rewritable_type_annotation(
                                    self, expr,
                                );
                            }
                        }
                        if self.enabled(Rule::NonPEP585Annotation) {
                            if self.settings.target_version >= PythonVersion::Py39
                                || (self.settings.target_version >= PythonVersion::Py37
                                    && self.semantic_model.future_annotations()
                                    && self.semantic_model.in_annotation())
                            {
                                pyupgrade::rules::use_pep585_annotation(self, expr, &replacement);
                            }
                        }
                    }
                }
                if self.enabled(Rule::DatetimeTimezoneUTC)
                    && self.settings.target_version >= PythonVersion::Py311
                {
                    pyupgrade::rules::datetime_utc_alias(self, expr);
                }
                if self.enabled(Rule::TypingTextStrAlias) {
                    pyupgrade::rules::typing_text_str_alias(self, expr);
                }
                if self.enabled(Rule::NumpyDeprecatedTypeAlias) {
                    numpy::rules::deprecated_type_alias(self, expr);
                }
                if self.enabled(Rule::DeprecatedMockImport) {
                    pyupgrade::rules::deprecated_mock_attribute(self, expr);
                }
                if self.enabled(Rule::SixPY3) {
                    flake8_2020::rules::name_or_attribute(self, expr);
                }
                if self.enabled(Rule::BannedApi) {
                    flake8_tidy_imports::rules::banned_attribute_access(self, expr);
                }
                if self.enabled(Rule::PrivateMemberAccess) {
                    flake8_self::rules::private_member_access(self, expr);
                }
                if self.is_stub {
                    if self.enabled(Rule::CollectionsNamedTuple) {
                        flake8_pyi::rules::collections_named_tuple(self, expr);
                    }
                }
                pandas_vet::rules::attr(self, attr, value, expr);
            }
            Expr::Call(ast::ExprCall {
                func,
                args,
                keywords,
                range: _,
            }) => {
                if self.any_enabled(&[
                    // pyflakes
                    Rule::StringDotFormatInvalidFormat,
                    Rule::StringDotFormatExtraNamedArguments,
                    Rule::StringDotFormatExtraPositionalArguments,
                    Rule::StringDotFormatMissingArguments,
                    Rule::StringDotFormatMixingAutomatic,
                    // pyupgrade
                    Rule::FormatLiterals,
                    Rule::FString,
                    // flynt
                    Rule::StaticJoinToFString,
                ]) {
                    if let Expr::Attribute(ast::ExprAttribute { value, attr, .. }) = func.as_ref() {
                        let attr = attr.as_str();
                        if let Expr::Constant(ast::ExprConstant {
                            value: Constant::Str(value),
                            ..
                        }) = value.as_ref()
                        {
                            if attr == "join" {
                                // "...".join(...) call
                                if self.enabled(Rule::StaticJoinToFString) {
                                    flynt::rules::static_join_to_fstring(self, expr, value);
                                }
                            } else if attr == "format" {
                                // "...".format(...) call
                                let location = expr.range();
                                match pyflakes::format::FormatSummary::try_from(value.as_ref()) {
                                    Err(e) => {
                                        if self.enabled(Rule::StringDotFormatInvalidFormat) {
                                            self.diagnostics.push(Diagnostic::new(
                                                pyflakes::rules::StringDotFormatInvalidFormat {
                                                    message: pyflakes::format::error_to_string(&e),
                                                },
                                                location,
                                            ));
                                        }
                                    }
                                    Ok(summary) => {
                                        if self.enabled(Rule::StringDotFormatExtraNamedArguments) {
                                            pyflakes::rules::string_dot_format_extra_named_arguments(
                                                self, &summary, keywords, location,
                                            );
                                        }

                                        if self
                                            .enabled(Rule::StringDotFormatExtraPositionalArguments)
                                        {
                                            pyflakes::rules::string_dot_format_extra_positional_arguments(
                                                self,
                                                &summary, args, location,
                                            );
                                        }

                                        if self.enabled(Rule::StringDotFormatMissingArguments) {
                                            pyflakes::rules::string_dot_format_missing_argument(
                                                self, &summary, args, keywords, location,
                                            );
                                        }

                                        if self.enabled(Rule::StringDotFormatMixingAutomatic) {
                                            pyflakes::rules::string_dot_format_mixing_automatic(
                                                self, &summary, location,
                                            );
                                        }

                                        if self.enabled(Rule::FormatLiterals) {
                                            pyupgrade::rules::format_literals(self, &summary, expr);
                                        }

                                        if self.enabled(Rule::FString) {
                                            pyupgrade::rules::f_strings(self, &summary, expr);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                // pyupgrade
                if self.enabled(Rule::TypeOfPrimitive) {
                    pyupgrade::rules::type_of_primitive(self, expr, func, args);
                }
                if self.enabled(Rule::DeprecatedUnittestAlias) {
                    pyupgrade::rules::deprecated_unittest_alias(self, func);
                }
                if self.enabled(Rule::SuperCallWithParameters) {
                    pyupgrade::rules::super_call_with_parameters(self, expr, func, args);
                }
                if self.enabled(Rule::UnnecessaryEncodeUTF8) {
                    pyupgrade::rules::unnecessary_encode_utf8(self, expr, func, args, keywords);
                }
                if self.enabled(Rule::RedundantOpenModes) {
                    pyupgrade::rules::redundant_open_modes(self, expr);
                }
                if self.enabled(Rule::NativeLiterals) {
                    pyupgrade::rules::native_literals(self, expr, func, args, keywords);
                }
                if self.enabled(Rule::OpenAlias) {
                    pyupgrade::rules::open_alias(self, expr, func);
                }
                if self.enabled(Rule::ReplaceUniversalNewlines) {
                    pyupgrade::rules::replace_universal_newlines(self, func, keywords);
                }
                if self.enabled(Rule::ReplaceStdoutStderr) {
                    pyupgrade::rules::replace_stdout_stderr(self, expr, func, args, keywords);
                }
                if self.enabled(Rule::OSErrorAlias) {
                    pyupgrade::rules::os_error_alias_call(self, func);
                }
                if self.enabled(Rule::NonPEP604Isinstance)
                    && self.settings.target_version >= PythonVersion::Py310
                {
                    pyupgrade::rules::use_pep604_isinstance(self, expr, func, args);
                }

                // flake8-async
                if self.enabled(Rule::BlockingHttpCallInAsyncFunction) {
                    flake8_async::rules::blocking_http_call(self, expr);
                }
                if self.enabled(Rule::OpenSleepOrSubprocessInAsyncFunction) {
                    flake8_async::rules::open_sleep_or_subprocess_call(self, expr);
                }
                if self.enabled(Rule::BlockingOsCallInAsyncFunction) {
                    flake8_async::rules::blocking_os_call(self, expr);
                }

                // flake8-print
                if self.any_enabled(&[Rule::Print, Rule::PPrint]) {
                    flake8_print::rules::print_call(self, func, keywords);
                }

                // flake8-bandit
                if self.any_enabled(&[
                    Rule::SuspiciousPickleUsage,
                    Rule::SuspiciousMarshalUsage,
                    Rule::SuspiciousInsecureHashUsage,
                    Rule::SuspiciousInsecureCipherUsage,
                    Rule::SuspiciousInsecureCipherModeUsage,
                    Rule::SuspiciousMktempUsage,
                    Rule::SuspiciousEvalUsage,
                    Rule::SuspiciousMarkSafeUsage,
                    Rule::SuspiciousURLOpenUsage,
                    Rule::SuspiciousNonCryptographicRandomUsage,
                    Rule::SuspiciousXMLCElementTreeUsage,
                    Rule::SuspiciousXMLElementTreeUsage,
                    Rule::SuspiciousXMLExpatReaderUsage,
                    Rule::SuspiciousXMLExpatBuilderUsage,
                    Rule::SuspiciousXMLSaxUsage,
                    Rule::SuspiciousXMLMiniDOMUsage,
                    Rule::SuspiciousXMLPullDOMUsage,
                    Rule::SuspiciousXMLETreeUsage,
                    Rule::SuspiciousUnverifiedContextUsage,
                    Rule::SuspiciousTelnetUsage,
                    Rule::SuspiciousFTPLibUsage,
                ]) {
                    flake8_bandit::rules::suspicious_function_call(self, expr);
                }

                // flake8-bugbear
                if self.enabled(Rule::UnreliableCallableCheck) {
                    flake8_bugbear::rules::unreliable_callable_check(self, expr, func, args);
                }
                if self.enabled(Rule::StripWithMultiCharacters) {
                    flake8_bugbear::rules::strip_with_multi_characters(self, expr, func, args);
                }
                if self.enabled(Rule::GetAttrWithConstant) {
                    flake8_bugbear::rules::getattr_with_constant(self, expr, func, args);
                }
                if self.enabled(Rule::SetAttrWithConstant) {
                    flake8_bugbear::rules::setattr_with_constant(self, expr, func, args);
                }
                if self.enabled(Rule::UselessContextlibSuppress) {
                    flake8_bugbear::rules::useless_contextlib_suppress(self, expr, func, args);
                }
                if self.enabled(Rule::StarArgUnpackingAfterKeywordArg) {
                    flake8_bugbear::rules::star_arg_unpacking_after_keyword_arg(
                        self, args, keywords,
                    );
                }
                if self.enabled(Rule::ZipWithoutExplicitStrict)
                    && self.settings.target_version >= PythonVersion::Py310
                {
                    flake8_bugbear::rules::zip_without_explicit_strict(
                        self, expr, func, args, keywords,
                    );
                }
                if self.enabled(Rule::NoExplicitStacklevel) {
                    flake8_bugbear::rules::no_explicit_stacklevel(self, func, args, keywords);
                }

                // flake8-pie
                if self.enabled(Rule::UnnecessaryDictKwargs) {
                    flake8_pie::rules::unnecessary_dict_kwargs(self, expr, keywords);
                }

                // flake8-bandit
                if self.enabled(Rule::ExecBuiltin) {
                    if let Some(diagnostic) = flake8_bandit::rules::exec_used(expr, func) {
                        self.diagnostics.push(diagnostic);
                    }
                }
                if self.enabled(Rule::BadFilePermissions) {
                    flake8_bandit::rules::bad_file_permissions(self, func, args, keywords);
                }
                if self.enabled(Rule::RequestWithNoCertValidation) {
                    flake8_bandit::rules::request_with_no_cert_validation(
                        self, func, args, keywords,
                    );
                }
                if self.enabled(Rule::UnsafeYAMLLoad) {
                    flake8_bandit::rules::unsafe_yaml_load(self, func, args, keywords);
                }
                if self.enabled(Rule::SnmpInsecureVersion) {
                    flake8_bandit::rules::snmp_insecure_version(self, func, args, keywords);
                }
                if self.enabled(Rule::SnmpWeakCryptography) {
                    flake8_bandit::rules::snmp_weak_cryptography(self, func, args, keywords);
                }
                if self.enabled(Rule::Jinja2AutoescapeFalse) {
                    flake8_bandit::rules::jinja2_autoescape_false(self, func, args, keywords);
                }
                if self.enabled(Rule::HardcodedPasswordFuncArg) {
                    self.diagnostics
                        .extend(flake8_bandit::rules::hardcoded_password_func_arg(keywords));
                }
                if self.enabled(Rule::HardcodedSQLExpression) {
                    flake8_bandit::rules::hardcoded_sql_expression(self, expr);
                }
                if self.enabled(Rule::HashlibInsecureHashFunction) {
                    flake8_bandit::rules::hashlib_insecure_hash_functions(
                        self, func, args, keywords,
                    );
                }
                if self.enabled(Rule::RequestWithoutTimeout) {
                    flake8_bandit::rules::request_without_timeout(self, func, args, keywords);
                }
                if self.enabled(Rule::ParamikoCall) {
                    flake8_bandit::rules::paramiko_call(self, func);
                }
                if self.enabled(Rule::LoggingConfigInsecureListen) {
                    flake8_bandit::rules::logging_config_insecure_listen(
                        self, func, args, keywords,
                    );
                }
                if self.any_enabled(&[
                    Rule::SubprocessWithoutShellEqualsTrue,
                    Rule::SubprocessPopenWithShellEqualsTrue,
                    Rule::CallWithShellEqualsTrue,
                    Rule::StartProcessWithAShell,
                    Rule::StartProcessWithNoShell,
                    Rule::StartProcessWithPartialPath,
                    Rule::UnixCommandWildcardInjection,
                ]) {
                    flake8_bandit::rules::shell_injection(self, func, args, keywords);
                }

                // flake8-comprehensions
                if self.enabled(Rule::UnnecessaryGeneratorList) {
                    flake8_comprehensions::rules::unnecessary_generator_list(
                        self, expr, func, args, keywords,
                    );
                }
                if self.enabled(Rule::UnnecessaryGeneratorSet) {
                    flake8_comprehensions::rules::unnecessary_generator_set(
                        self, expr, func, args, keywords,
                    );
                }
                if self.enabled(Rule::UnnecessaryGeneratorDict) {
                    flake8_comprehensions::rules::unnecessary_generator_dict(
                        self, expr, func, args, keywords,
                    );
                }
                if self.enabled(Rule::UnnecessaryListComprehensionSet) {
                    flake8_comprehensions::rules::unnecessary_list_comprehension_set(
                        self, expr, func, args, keywords,
                    );
                }
                if self.enabled(Rule::UnnecessaryListComprehensionDict) {
                    flake8_comprehensions::rules::unnecessary_list_comprehension_dict(
                        self, expr, func, args, keywords,
                    );
                }
                if self.enabled(Rule::UnnecessaryLiteralSet) {
                    flake8_comprehensions::rules::unnecessary_literal_set(
                        self, expr, func, args, keywords,
                    );
                }
                if self.enabled(Rule::UnnecessaryLiteralDict) {
                    flake8_comprehensions::rules::unnecessary_literal_dict(
                        self, expr, func, args, keywords,
                    );
                }
                if self.enabled(Rule::UnnecessaryCollectionCall) {
                    flake8_comprehensions::rules::unnecessary_collection_call(
                        self,
                        expr,
                        func,
                        args,
                        keywords,
                        &self.settings.flake8_comprehensions,
                    );
                }
                if self.enabled(Rule::UnnecessaryLiteralWithinTupleCall) {
                    flake8_comprehensions::rules::unnecessary_literal_within_tuple_call(
                        self, expr, func, args, keywords,
                    );
                }
                if self.enabled(Rule::UnnecessaryLiteralWithinListCall) {
                    flake8_comprehensions::rules::unnecessary_literal_within_list_call(
                        self, expr, func, args, keywords,
                    );
                }
                if self.enabled(Rule::UnnecessaryLiteralWithinDictCall) {
                    flake8_comprehensions::rules::unnecessary_literal_within_dict_call(
                        self, expr, func, args, keywords,
                    );
                }
                if self.enabled(Rule::UnnecessaryListCall) {
                    flake8_comprehensions::rules::unnecessary_list_call(self, expr, func, args);
                }
                if self.enabled(Rule::UnnecessaryCallAroundSorted) {
                    flake8_comprehensions::rules::unnecessary_call_around_sorted(
                        self, expr, func, args,
                    );
                }
                if self.enabled(Rule::UnnecessaryDoubleCastOrProcess) {
                    flake8_comprehensions::rules::unnecessary_double_cast_or_process(
                        self, expr, func, args,
                    );
                }
                if self.enabled(Rule::UnnecessarySubscriptReversal) {
                    flake8_comprehensions::rules::unnecessary_subscript_reversal(
                        self, expr, func, args,
                    );
                }
                if self.enabled(Rule::UnnecessaryMap) {
                    flake8_comprehensions::rules::unnecessary_map(
                        self,
                        expr,
                        self.semantic_model.expr_parent(),
                        func,
                        args,
                    );
                }
                if self.enabled(Rule::UnnecessaryComprehensionAnyAll) {
                    flake8_comprehensions::rules::unnecessary_comprehension_any_all(
                        self, expr, func, args, keywords,
                    );
                }

                // flake8-boolean-trap
                if self.enabled(Rule::BooleanPositionalValueInFunctionCall) {
                    flake8_boolean_trap::rules::check_boolean_positional_value_in_function_call(
                        self, args, func,
                    );
                }
                if let Expr::Name(ast::ExprName { id, ctx, range: _ }) = func.as_ref() {
                    if id == "locals" && matches!(ctx, ExprContext::Load) {
                        let scope = self.semantic_model.scope_mut();
                        scope.set_uses_locals();
                    }
                }

                // flake8-debugger
                if self.enabled(Rule::Debugger) {
                    flake8_debugger::rules::debugger_call(self, expr, func);
                }

                // pandas-vet
                if self.enabled(Rule::PandasUseOfInplaceArgument) {
                    self.diagnostics.extend(
                        pandas_vet::rules::inplace_argument(self, expr, func, args, keywords)
                            .into_iter(),
                    );
                }
                pandas_vet::rules::call(self, func);

                if self.enabled(Rule::PandasUseOfPdMerge) {
                    if let Some(diagnostic) = pandas_vet::rules::use_of_pd_merge(func) {
                        self.diagnostics.push(diagnostic);
                    };
                }

                // flake8-datetimez
                if self.enabled(Rule::CallDatetimeWithoutTzinfo) {
                    flake8_datetimez::rules::call_datetime_without_tzinfo(
                        self,
                        func,
                        args,
                        keywords,
                        expr.range(),
                    );
                }
                if self.enabled(Rule::CallDatetimeToday) {
                    flake8_datetimez::rules::call_datetime_today(self, func, expr.range());
                }
                if self.enabled(Rule::CallDatetimeUtcnow) {
                    flake8_datetimez::rules::call_datetime_utcnow(self, func, expr.range());
                }
                if self.enabled(Rule::CallDatetimeUtcfromtimestamp) {
                    flake8_datetimez::rules::call_datetime_utcfromtimestamp(
                        self,
                        func,
                        expr.range(),
                    );
                }
                if self.enabled(Rule::CallDatetimeNowWithoutTzinfo) {
                    flake8_datetimez::rules::call_datetime_now_without_tzinfo(
                        self,
                        func,
                        args,
                        keywords,
                        expr.range(),
                    );
                }
                if self.enabled(Rule::CallDatetimeFromtimestamp) {
                    flake8_datetimez::rules::call_datetime_fromtimestamp(
                        self,
                        func,
                        args,
                        keywords,
                        expr.range(),
                    );
                }
                if self.enabled(Rule::CallDatetimeStrptimeWithoutZone) {
                    flake8_datetimez::rules::call_datetime_strptime_without_zone(
                        self,
                        func,
                        args,
                        expr.range(),
                    );
                }
                if self.enabled(Rule::CallDateToday) {
                    flake8_datetimez::rules::call_date_today(self, func, expr.range());
                }
                if self.enabled(Rule::CallDateFromtimestamp) {
                    flake8_datetimez::rules::call_date_fromtimestamp(self, func, expr.range());
                }

                // pygrep-hooks
                if self.enabled(Rule::Eval) {
                    pygrep_hooks::rules::no_eval(self, func);
                }
                if self.enabled(Rule::DeprecatedLogWarn) {
                    pygrep_hooks::rules::deprecated_log_warn(self, func);
                }

                // pylint
                if self.enabled(Rule::UnnecessaryDirectLambdaCall) {
                    pylint::rules::unnecessary_direct_lambda_call(self, expr, func);
                }
                if self.enabled(Rule::SysExitAlias) {
                    pylint::rules::sys_exit_alias(self, func);
                }
                if self.enabled(Rule::BadStrStripCall) {
                    pylint::rules::bad_str_strip_call(self, func, args);
                }
                if self.enabled(Rule::InvalidEnvvarDefault) {
                    pylint::rules::invalid_envvar_default(self, func, args, keywords);
                }
                if self.enabled(Rule::InvalidEnvvarValue) {
                    pylint::rules::invalid_envvar_value(self, func, args, keywords);
                }
                if self.enabled(Rule::NestedMinMax) {
                    pylint::rules::nested_min_max(self, expr, func, args, keywords);
                }

                // flake8-pytest-style
                if self.enabled(Rule::PytestPatchWithLambda) {
                    if let Some(diagnostic) =
                        flake8_pytest_style::rules::patch_with_lambda(func, args, keywords)
                    {
                        self.diagnostics.push(diagnostic);
                    }
                }
                if self.enabled(Rule::PytestUnittestAssertion) {
                    if let Some(diagnostic) = flake8_pytest_style::rules::unittest_assertion(
                        self, expr, func, args, keywords,
                    ) {
                        self.diagnostics.push(diagnostic);
                    }
                }

                if self.any_enabled(&[
                    Rule::PytestRaisesWithoutException,
                    Rule::PytestRaisesTooBroad,
                ]) {
                    flake8_pytest_style::rules::raises_call(self, func, args, keywords);
                }

                if self.enabled(Rule::PytestFailWithoutMessage) {
                    flake8_pytest_style::rules::fail_call(self, func, args, keywords);
                }

                if self.enabled(Rule::PairwiseOverZipped) {
                    if self.settings.target_version >= PythonVersion::Py310 {
                        ruff::rules::pairwise_over_zipped(self, func, args);
                    }
                }

                // flake8-gettext
                if self.any_enabled(&[
                    Rule::FStringInGetTextFuncCall,
                    Rule::FormatInGetTextFuncCall,
                    Rule::PrintfInGetTextFuncCall,
                ]) && flake8_gettext::rules::is_gettext_func_call(
                    func,
                    &self.settings.flake8_gettext.functions_names,
                ) {
                    if self.enabled(Rule::FStringInGetTextFuncCall) {
                        self.diagnostics
                            .extend(flake8_gettext::rules::f_string_in_gettext_func_call(args));
                    }
                    if self.enabled(Rule::FormatInGetTextFuncCall) {
                        self.diagnostics
                            .extend(flake8_gettext::rules::format_in_gettext_func_call(args));
                    }
                    if self.enabled(Rule::PrintfInGetTextFuncCall) {
                        self.diagnostics
                            .extend(flake8_gettext::rules::printf_in_gettext_func_call(args));
                    }
                }

                // flake8-simplify
                if self.enabled(Rule::UncapitalizedEnvironmentVariables) {
                    flake8_simplify::rules::use_capital_environment_variables(self, expr);
                }

                if self.enabled(Rule::OpenFileWithContextHandler) {
                    flake8_simplify::rules::open_file_with_context_handler(self, func);
                }

                if self.enabled(Rule::DictGetWithNoneDefault) {
                    flake8_simplify::rules::dict_get_with_none_default(self, expr);
                }

                // flake8-use-pathlib
                if self.any_enabled(&[
                    Rule::OsPathAbspath,
                    Rule::OsChmod,
                    Rule::OsMkdir,
                    Rule::OsMakedirs,
                    Rule::OsRename,
                    Rule::OsReplace,
                    Rule::OsRmdir,
                    Rule::OsRemove,
                    Rule::OsUnlink,
                    Rule::OsGetcwd,
                    Rule::OsPathExists,
                    Rule::OsPathExpanduser,
                    Rule::OsPathIsdir,
                    Rule::OsPathIsfile,
                    Rule::OsPathIslink,
                    Rule::OsReadlink,
                    Rule::OsStat,
                    Rule::OsPathIsabs,
                    Rule::OsPathJoin,
                    Rule::OsPathBasename,
                    Rule::OsPathSamefile,
                    Rule::OsPathSplitext,
                    Rule::BuiltinOpen,
                    Rule::PyPath,
                ]) {
                    flake8_use_pathlib::rules::replaceable_by_pathlib(self, func);
                }

                // numpy
                if self.enabled(Rule::NumpyLegacyRandom) {
                    numpy::rules::numpy_legacy_random(self, func);
                }

                // flake8-logging-format
                if self.any_enabled(&[
                    Rule::LoggingStringFormat,
                    Rule::LoggingPercentFormat,
                    Rule::LoggingStringConcat,
                    Rule::LoggingFString,
                    Rule::LoggingWarn,
                    Rule::LoggingExtraAttrClash,
                    Rule::LoggingExcInfo,
                    Rule::LoggingRedundantExcInfo,
                ]) {
                    flake8_logging_format::rules::logging_call(self, func, args, keywords);
                }

                // pylint logging checker
                if self.any_enabled(&[Rule::LoggingTooFewArgs, Rule::LoggingTooManyArgs]) {
                    pylint::rules::logging_call(self, func, args, keywords);
                }

                // flake8-django
                if self.enabled(Rule::DjangoLocalsInRenderFunction) {
                    flake8_django::rules::locals_in_render_function(self, func, args, keywords);
                }
            }
            Expr::Dict(ast::ExprDict {
                keys,
                values,
                range: _,
            }) => {
                if self.any_enabled(&[
                    Rule::MultiValueRepeatedKeyLiteral,
                    Rule::MultiValueRepeatedKeyVariable,
                ]) {
                    pyflakes::rules::repeated_keys(self, keys, values);
                }

                if self.enabled(Rule::UnnecessarySpread) {
                    flake8_pie::rules::unnecessary_spread(self, keys, values);
                }
            }
            Expr::Set(ast::ExprSet { elts, range: _ }) => {
                if self.enabled(Rule::DuplicateValue) {
                    flake8_bugbear::rules::duplicate_value(self, elts);
                }
            }
            Expr::Yield(_) => {
                if self.enabled(Rule::YieldOutsideFunction) {
                    pyflakes::rules::yield_outside_function(self, expr);
                }
                if self.enabled(Rule::YieldInInit) {
                    pylint::rules::yield_in_init(self, expr);
                }
            }
            Expr::YieldFrom(yield_from) => {
                if self.enabled(Rule::YieldOutsideFunction) {
                    pyflakes::rules::yield_outside_function(self, expr);
                }
                if self.enabled(Rule::YieldInInit) {
                    pylint::rules::yield_in_init(self, expr);
                }
                if self.enabled(Rule::YieldFromInAsyncFunction) {
                    pylint::rules::yield_from_in_async_function(self, yield_from);
                }
            }
            Expr::Await(_) => {
                if self.enabled(Rule::YieldOutsideFunction) {
                    pyflakes::rules::yield_outside_function(self, expr);
                }
                if self.enabled(Rule::AwaitOutsideAsync) {
                    pylint::rules::await_outside_async(self, expr);
                }
            }
            Expr::JoinedStr(ast::ExprJoinedStr { values, range: _ }) => {
                if self.enabled(Rule::FStringMissingPlaceholders) {
                    pyflakes::rules::f_string_missing_placeholders(expr, values, self);
                }
                if self.enabled(Rule::HardcodedSQLExpression) {
                    flake8_bandit::rules::hardcoded_sql_expression(self, expr);
                }
                if self.enabled(Rule::ExplicitFStringTypeConversion) {
                    ruff::rules::explicit_f_string_type_conversion(self, expr, values);
                }
            }
            Expr::BinOp(ast::ExprBinOp {
                left,
                op: Operator::RShift,
                ..
            }) => {
                if self.enabled(Rule::InvalidPrintSyntax) {
                    pyflakes::rules::invalid_print_syntax(self, left);
                }
            }
            Expr::BinOp(ast::ExprBinOp {
                left,
                op: Operator::Mod,
                right,
                range: _,
            }) => {
                if let Expr::Constant(ast::ExprConstant {
                    value: Constant::Str(value),
                    ..
                }) = left.as_ref()
                {
                    if self.any_enabled(&[
                        Rule::PercentFormatInvalidFormat,
                        Rule::PercentFormatExpectedMapping,
                        Rule::PercentFormatExpectedSequence,
                        Rule::PercentFormatExtraNamedArguments,
                        Rule::PercentFormatMissingArgument,
                        Rule::PercentFormatMixedPositionalAndNamed,
                        Rule::PercentFormatPositionalCountMismatch,
                        Rule::PercentFormatStarRequiresSequence,
                        Rule::PercentFormatUnsupportedFormatCharacter,
                    ]) {
                        let location = expr.range();
                        match pyflakes::cformat::CFormatSummary::try_from(value.as_str()) {
                            Err(CFormatError {
                                typ: CFormatErrorType::UnsupportedFormatChar(c),
                                ..
                            }) => {
                                if self.enabled(Rule::PercentFormatUnsupportedFormatCharacter) {
                                    self.diagnostics.push(Diagnostic::new(
                                        pyflakes::rules::PercentFormatUnsupportedFormatCharacter {
                                            char: c,
                                        },
                                        location,
                                    ));
                                }
                            }
                            Err(e) => {
                                if self.enabled(Rule::PercentFormatInvalidFormat) {
                                    self.diagnostics.push(Diagnostic::new(
                                        pyflakes::rules::PercentFormatInvalidFormat {
                                            message: e.to_string(),
                                        },
                                        location,
                                    ));
                                }
                            }
                            Ok(summary) => {
                                if self.enabled(Rule::PercentFormatExpectedMapping) {
                                    pyflakes::rules::percent_format_expected_mapping(
                                        self, &summary, right, location,
                                    );
                                }
                                if self.enabled(Rule::PercentFormatExpectedSequence) {
                                    pyflakes::rules::percent_format_expected_sequence(
                                        self, &summary, right, location,
                                    );
                                }
                                if self.enabled(Rule::PercentFormatExtraNamedArguments) {
                                    pyflakes::rules::percent_format_extra_named_arguments(
                                        self, &summary, right, location,
                                    );
                                }
                                if self.enabled(Rule::PercentFormatMissingArgument) {
                                    pyflakes::rules::percent_format_missing_arguments(
                                        self, &summary, right, location,
                                    );
                                }
                                if self.enabled(Rule::PercentFormatMixedPositionalAndNamed) {
                                    pyflakes::rules::percent_format_mixed_positional_and_named(
                                        self, &summary, location,
                                    );
                                }
                                if self.enabled(Rule::PercentFormatPositionalCountMismatch) {
                                    pyflakes::rules::percent_format_positional_count_mismatch(
                                        self, &summary, right, location,
                                    );
                                }
                                if self.enabled(Rule::PercentFormatStarRequiresSequence) {
                                    pyflakes::rules::percent_format_star_requires_sequence(
                                        self, &summary, right, location,
                                    );
                                }
                            }
                        }
                    }

                    if self.enabled(Rule::PrintfStringFormatting) {
                        pyupgrade::rules::printf_string_formatting(self, expr, right, self.locator);
                    }
                    if self.enabled(Rule::BadStringFormatType) {
                        pylint::rules::bad_string_format_type(self, expr, right);
                    }
                    if self.enabled(Rule::HardcodedSQLExpression) {
                        flake8_bandit::rules::hardcoded_sql_expression(self, expr);
                    }
                }
            }
            Expr::BinOp(ast::ExprBinOp {
                op: Operator::Add, ..
            }) => {
                if self.enabled(Rule::ExplicitStringConcatenation) {
                    if let Some(diagnostic) = flake8_implicit_str_concat::rules::explicit(expr) {
                        self.diagnostics.push(diagnostic);
                    }
                }
                if self.enabled(Rule::CollectionLiteralConcatenation) {
                    ruff::rules::collection_literal_concatenation(self, expr);
                }
                if self.enabled(Rule::HardcodedSQLExpression) {
                    flake8_bandit::rules::hardcoded_sql_expression(self, expr);
                }
            }
            Expr::BinOp(ast::ExprBinOp {
                op: Operator::BitOr,
                ..
            }) => {
                // Ex) `str | None`
                if self.enabled(Rule::FutureRequiredTypeAnnotation) {
                    if self.settings.target_version < PythonVersion::Py310
                        && !self.semantic_model.future_annotations()
                        && self.semantic_model.in_annotation()
                    {
                        flake8_future_annotations::rules::future_required_type_annotation(
                            self,
                            expr,
                            flake8_future_annotations::rules::Reason::PEP604,
                        );
                    }
                }

                if self.is_stub {
                    if self.enabled(Rule::DuplicateUnionMember)
                        && self.semantic_model.in_type_definition()
                        && self.semantic_model.expr_parent().map_or(true, |parent| {
                            !matches!(
                                parent,
                                Expr::BinOp(ast::ExprBinOp {
                                    op: Operator::BitOr,
                                    ..
                                })
                            )
                        })
                    {
                        flake8_pyi::rules::duplicate_union_member(self, expr);
                    }
                }
            }
            Expr::UnaryOp(ast::ExprUnaryOp {
                op,
                operand,
                range: _,
            }) => {
                let check_not_in = self.enabled(Rule::NotInTest);
                let check_not_is = self.enabled(Rule::NotIsTest);
                if check_not_in || check_not_is {
                    pycodestyle::rules::not_tests(
                        self,
                        expr,
                        *op,
                        operand,
                        check_not_in,
                        check_not_is,
                    );
                }

                if self.enabled(Rule::UnaryPrefixIncrement) {
                    flake8_bugbear::rules::unary_prefix_increment(self, expr, *op, operand);
                }

                if self.enabled(Rule::NegateEqualOp) {
                    flake8_simplify::rules::negation_with_equal_op(self, expr, *op, operand);
                }
                if self.enabled(Rule::NegateNotEqualOp) {
                    flake8_simplify::rules::negation_with_not_equal_op(self, expr, *op, operand);
                }
                if self.enabled(Rule::DoubleNegation) {
                    flake8_simplify::rules::double_negation(self, expr, *op, operand);
                }
            }
            Expr::Compare(ast::ExprCompare {
                left,
                ops,
                comparators,
                range: _,
            }) => {
                let check_none_comparisons = self.enabled(Rule::NoneComparison);
                let check_true_false_comparisons = self.enabled(Rule::TrueFalseComparison);
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

                if self.enabled(Rule::IsLiteral) {
                    pyflakes::rules::invalid_literal_comparison(self, left, ops, comparators, expr);
                }

                if self.enabled(Rule::TypeComparison) {
                    pycodestyle::rules::type_comparison(self, expr, ops, comparators);
                }

                if self.any_enabled(&[
                    Rule::SysVersionCmpStr3,
                    Rule::SysVersionInfo0Eq3,
                    Rule::SysVersionInfo1CmpInt,
                    Rule::SysVersionInfoMinorCmpInt,
                    Rule::SysVersionCmpStr10,
                ]) {
                    flake8_2020::rules::compare(self, left, ops, comparators);
                }

                if self.enabled(Rule::HardcodedPasswordString) {
                    self.diagnostics.extend(
                        flake8_bandit::rules::compare_to_hardcoded_password_string(
                            left,
                            comparators,
                        ),
                    );
                }

                if self.enabled(Rule::ComparisonWithItself) {
                    pylint::rules::comparison_with_itself(self, left, ops, comparators);
                }

                if self.enabled(Rule::ComparisonOfConstant) {
                    pylint::rules::comparison_of_constant(self, left, ops, comparators);
                }

                if self.enabled(Rule::CompareToEmptyString) {
                    pylint::rules::compare_to_empty_string(self, left, ops, comparators);
                }

                if self.enabled(Rule::MagicValueComparison) {
                    pylint::rules::magic_value_comparison(self, left, comparators);
                }

                if self.enabled(Rule::InDictKeys) {
                    flake8_simplify::rules::key_in_dict_compare(self, expr, left, ops, comparators);
                }

                if self.enabled(Rule::YodaConditions) {
                    flake8_simplify::rules::yoda_conditions(self, expr, left, ops, comparators);
                }

                if self.is_stub {
                    if self.any_enabled(&[
                        Rule::UnrecognizedPlatformCheck,
                        Rule::UnrecognizedPlatformName,
                    ]) {
                        flake8_pyi::rules::unrecognized_platform(
                            self,
                            expr,
                            left,
                            ops,
                            comparators,
                        );
                    }

                    if self.enabled(Rule::BadVersionInfoComparison) {
                        flake8_pyi::rules::bad_version_info_comparison(
                            self,
                            expr,
                            left,
                            ops,
                            comparators,
                        );
                    }
                }
            }
            Expr::Constant(ast::ExprConstant {
                value: Constant::Int(_) | Constant::Float(_) | Constant::Complex { .. },
                kind: _,
                range: _,
            }) => {
                if self.is_stub && self.enabled(Rule::NumericLiteralTooLong) {
                    flake8_pyi::rules::numeric_literal_too_long(self, expr);
                }
            }
            Expr::Constant(ast::ExprConstant {
                value: Constant::Bytes(_),
                kind: _,
                range: _,
            }) => {
                if self.is_stub && self.enabled(Rule::StringOrBytesTooLong) {
                    flake8_pyi::rules::string_or_bytes_too_long(self, expr);
                }
            }
            Expr::Constant(ast::ExprConstant {
                value: Constant::Str(value),
                kind,
                range: _,
            }) => {
                if self.semantic_model.in_type_definition()
                    && !self.semantic_model.in_literal()
                    && !self.semantic_model.in_f_string()
                {
                    self.deferred.string_type_definitions.push((
                        expr.range(),
                        value,
                        self.semantic_model.snapshot(),
                    ));
                }
                if self.enabled(Rule::HardcodedBindAllInterfaces) {
                    if let Some(diagnostic) =
                        flake8_bandit::rules::hardcoded_bind_all_interfaces(value, expr.range())
                    {
                        self.diagnostics.push(diagnostic);
                    }
                }
                if self.enabled(Rule::HardcodedTempFile) {
                    if let Some(diagnostic) = flake8_bandit::rules::hardcoded_tmp_directory(
                        expr,
                        value,
                        &self.settings.flake8_bandit.hardcoded_tmp_directory,
                    ) {
                        self.diagnostics.push(diagnostic);
                    }
                }
                if self.enabled(Rule::UnicodeKindPrefix) {
                    pyupgrade::rules::unicode_kind_prefix(self, expr, kind.as_deref());
                }
                if self.is_stub && self.enabled(Rule::StringOrBytesTooLong) {
                    flake8_pyi::rules::string_or_bytes_too_long(self, expr);
                }
            }
            Expr::Lambda(
                lambda @ ast::ExprLambda {
                    args,
                    body: _,
                    range: _,
                },
            ) => {
                if self.enabled(Rule::ReimplementedListBuiltin) {
                    flake8_pie::rules::reimplemented_list_builtin(self, lambda);
                }

                // Visit the default arguments, but avoid the body, which will be deferred.
                for expr in &args.kw_defaults {
                    self.visit_expr(expr);
                }
                for expr in &args.defaults {
                    self.visit_expr(expr);
                }
                self.semantic_model.push_scope(ScopeKind::Lambda(lambda));
            }
            Expr::IfExp(ast::ExprIfExp {
                test,
                body,
                orelse,
                range: _,
            }) => {
                if self.enabled(Rule::IfExprWithTrueFalse) {
                    flake8_simplify::rules::explicit_true_false_in_ifexpr(
                        self, expr, test, body, orelse,
                    );
                }
                if self.enabled(Rule::IfExprWithFalseTrue) {
                    flake8_simplify::rules::explicit_false_true_in_ifexpr(
                        self, expr, test, body, orelse,
                    );
                }
                if self.enabled(Rule::IfExprWithTwistedArms) {
                    flake8_simplify::rules::twisted_arms_in_ifexpr(self, expr, test, body, orelse);
                }
            }
            Expr::ListComp(ast::ExprListComp {
                elt,
                generators,
                range: _,
            })
            | Expr::SetComp(ast::ExprSetComp {
                elt,
                generators,
                range: _,
            }) => {
                if self.enabled(Rule::UnnecessaryComprehension) {
                    flake8_comprehensions::rules::unnecessary_list_set_comprehension(
                        self, expr, elt, generators,
                    );
                }
                if self.enabled(Rule::FunctionUsesLoopVariable) {
                    flake8_bugbear::rules::function_uses_loop_variable(self, &Node::Expr(expr));
                }
                if self.enabled(Rule::InDictKeys) {
                    for generator in generators {
                        flake8_simplify::rules::key_in_dict_for(
                            self,
                            &generator.target,
                            &generator.iter,
                        );
                    }
                }
                if self.enabled(Rule::IterationOverSet) {
                    for generator in generators {
                        pylint::rules::iteration_over_set(self, &generator.iter);
                    }
                }
            }
            Expr::DictComp(ast::ExprDictComp {
                key,
                value,
                generators,
                range: _,
            }) => {
                if self.enabled(Rule::UnnecessaryComprehension) {
                    flake8_comprehensions::rules::unnecessary_dict_comprehension(
                        self, expr, key, value, generators,
                    );
                }
                if self.enabled(Rule::FunctionUsesLoopVariable) {
                    flake8_bugbear::rules::function_uses_loop_variable(self, &Node::Expr(expr));
                }
                if self.enabled(Rule::InDictKeys) {
                    for generator in generators {
                        flake8_simplify::rules::key_in_dict_for(
                            self,
                            &generator.target,
                            &generator.iter,
                        );
                    }
                }
                if self.enabled(Rule::IterationOverSet) {
                    for generator in generators {
                        pylint::rules::iteration_over_set(self, &generator.iter);
                    }
                }
                if self.enabled(Rule::StaticKeyDictComprehension) {
                    ruff::rules::static_key_dict_comprehension(self, key);
                }
            }
            Expr::GeneratorExp(ast::ExprGeneratorExp {
                generators,
                elt: _,
                range: _,
            }) => {
                if self.enabled(Rule::FunctionUsesLoopVariable) {
                    flake8_bugbear::rules::function_uses_loop_variable(self, &Node::Expr(expr));
                }
                if self.enabled(Rule::InDictKeys) {
                    for generator in generators {
                        flake8_simplify::rules::key_in_dict_for(
                            self,
                            &generator.target,
                            &generator.iter,
                        );
                    }
                }
                if self.enabled(Rule::IterationOverSet) {
                    for generator in generators {
                        pylint::rules::iteration_over_set(self, &generator.iter);
                    }
                }
            }
            Expr::BoolOp(ast::ExprBoolOp {
                op,
                values,
                range: _,
            }) => {
                if self.enabled(Rule::RepeatedIsinstanceCalls) {
                    pylint::rules::repeated_isinstance_calls(self, expr, *op, values);
                }
                if self.enabled(Rule::MultipleStartsEndsWith) {
                    flake8_pie::rules::multiple_starts_ends_with(self, expr);
                }
                if self.enabled(Rule::DuplicateIsinstanceCall) {
                    flake8_simplify::rules::duplicate_isinstance_call(self, expr);
                }
                if self.enabled(Rule::CompareWithTuple) {
                    flake8_simplify::rules::compare_with_tuple(self, expr);
                }
                if self.enabled(Rule::ExprAndNotExpr) {
                    flake8_simplify::rules::expr_and_not_expr(self, expr);
                }
                if self.enabled(Rule::ExprOrNotExpr) {
                    flake8_simplify::rules::expr_or_not_expr(self, expr);
                }
                if self.enabled(Rule::ExprOrTrue) {
                    flake8_simplify::rules::expr_or_true(self, expr);
                }
                if self.enabled(Rule::ExprAndFalse) {
                    flake8_simplify::rules::expr_and_false(self, expr);
                }
            }
            _ => {}
        };

        // Recurse.
        match expr {
            Expr::ListComp(ast::ExprListComp {
                elt,
                generators,
                range: _,
            })
            | Expr::SetComp(ast::ExprSetComp {
                elt,
                generators,
                range: _,
            })
            | Expr::GeneratorExp(ast::ExprGeneratorExp {
                elt,
                generators,
                range: _,
            }) => {
                self.visit_generators(generators);
                self.visit_expr(elt);
            }
            Expr::DictComp(ast::ExprDictComp {
                key,
                value,
                generators,
                range: _,
            }) => {
                self.visit_generators(generators);
                self.visit_expr(key);
                self.visit_expr(value);
            }
            Expr::Lambda(_) => {
                self.deferred
                    .lambdas
                    .push((expr, self.semantic_model.snapshot()));
            }
            Expr::IfExp(ast::ExprIfExp {
                test,
                body,
                orelse,
                range: _,
            }) => {
                self.visit_boolean_test(test);
                self.visit_expr(body);
                self.visit_expr(orelse);
            }
            Expr::Call(ast::ExprCall {
                func,
                args,
                keywords,
                range: _,
            }) => {
                let callable = self
                    .semantic_model
                    .resolve_call_path(func)
                    .and_then(|call_path| {
                        if self
                            .semantic_model
                            .match_typing_call_path(&call_path, "cast")
                        {
                            Some(Callable::Cast)
                        } else if self
                            .semantic_model
                            .match_typing_call_path(&call_path, "NewType")
                        {
                            Some(Callable::NewType)
                        } else if self
                            .semantic_model
                            .match_typing_call_path(&call_path, "TypeVar")
                        {
                            Some(Callable::TypeVar)
                        } else if self
                            .semantic_model
                            .match_typing_call_path(&call_path, "NamedTuple")
                        {
                            Some(Callable::NamedTuple)
                        } else if self
                            .semantic_model
                            .match_typing_call_path(&call_path, "TypedDict")
                        {
                            Some(Callable::TypedDict)
                        } else if [
                            "Arg",
                            "DefaultArg",
                            "NamedArg",
                            "DefaultNamedArg",
                            "VarArg",
                            "KwArg",
                        ]
                        .iter()
                        .any(|target| call_path.as_slice() == ["mypy_extensions", target])
                        {
                            Some(Callable::MypyExtension)
                        } else if call_path.as_slice() == ["", "bool"] {
                            Some(Callable::Bool)
                        } else {
                            None
                        }
                    });
                match callable {
                    Some(Callable::Bool) => {
                        self.visit_expr(func);
                        let mut args = args.iter();
                        if let Some(arg) = args.next() {
                            self.visit_boolean_test(arg);
                        }
                        for arg in args {
                            self.visit_expr(arg);
                        }
                    }
                    Some(Callable::Cast) => {
                        self.visit_expr(func);
                        let mut args = args.iter();
                        if let Some(arg) = args.next() {
                            self.visit_type_definition(arg);
                        }
                        for arg in args {
                            self.visit_expr(arg);
                        }
                    }
                    Some(Callable::NewType) => {
                        self.visit_expr(func);
                        let mut args = args.iter();
                        if let Some(arg) = args.next() {
                            self.visit_non_type_definition(arg);
                        }
                        for arg in args {
                            self.visit_type_definition(arg);
                        }
                    }
                    Some(Callable::TypeVar) => {
                        self.visit_expr(func);
                        let mut args = args.iter();
                        if let Some(arg) = args.next() {
                            self.visit_non_type_definition(arg);
                        }
                        for arg in args {
                            self.visit_type_definition(arg);
                        }
                        for keyword in keywords {
                            let Keyword {
                                arg,
                                value,
                                range: _,
                            } = keyword;
                            if let Some(id) = arg {
                                if id == "bound" {
                                    self.visit_type_definition(value);
                                } else {
                                    self.visit_non_type_definition(value);
                                }
                            }
                        }
                    }
                    Some(Callable::NamedTuple) => {
                        self.visit_expr(func);

                        // Ex) NamedTuple("a", [("a", int)])
                        let mut args = args.iter();
                        if let Some(arg) = args.next() {
                            self.visit_non_type_definition(arg);
                        }
                        for arg in args {
                            if let Expr::List(ast::ExprList { elts, .. })
                            | Expr::Tuple(ast::ExprTuple { elts, .. }) = arg
                            {
                                for elt in elts {
                                    match elt {
                                        Expr::List(ast::ExprList { elts, .. })
                                        | Expr::Tuple(ast::ExprTuple { elts, .. })
                                            if elts.len() == 2 =>
                                        {
                                            self.visit_non_type_definition(&elts[0]);
                                            self.visit_type_definition(&elts[1]);
                                        }
                                        _ => {
                                            self.visit_non_type_definition(elt);
                                        }
                                    }
                                }
                            } else {
                                self.visit_non_type_definition(arg);
                            }
                        }

                        // Ex) NamedTuple("a", a=int)
                        for keyword in keywords {
                            let Keyword { value, .. } = keyword;
                            self.visit_type_definition(value);
                        }
                    }
                    Some(Callable::TypedDict) => {
                        self.visit_expr(func);

                        // Ex) TypedDict("a", {"a": int})
                        let mut args = args.iter();
                        if let Some(arg) = args.next() {
                            self.visit_non_type_definition(arg);
                        }
                        for arg in args {
                            if let Expr::Dict(ast::ExprDict {
                                keys,
                                values,
                                range: _,
                            }) = arg
                            {
                                for key in keys.iter().flatten() {
                                    self.visit_non_type_definition(key);
                                }
                                for value in values {
                                    self.visit_type_definition(value);
                                }
                            } else {
                                self.visit_non_type_definition(arg);
                            }
                        }

                        // Ex) TypedDict("a", a=int)
                        for keyword in keywords {
                            let Keyword { value, .. } = keyword;
                            self.visit_type_definition(value);
                        }
                    }
                    Some(Callable::MypyExtension) => {
                        self.visit_expr(func);

                        let mut args = args.iter();
                        if let Some(arg) = args.next() {
                            // Ex) DefaultNamedArg(bool | None, name="some_prop_name")
                            self.visit_type_definition(arg);

                            for arg in args {
                                self.visit_non_type_definition(arg);
                            }
                            for keyword in keywords {
                                let Keyword { value, .. } = keyword;
                                self.visit_non_type_definition(value);
                            }
                        } else {
                            // Ex) DefaultNamedArg(type="bool", name="some_prop_name")
                            for keyword in keywords {
                                let Keyword {
                                    value,
                                    arg,
                                    range: _,
                                } = keyword;
                                if arg.as_ref().map_or(false, |arg| arg == "type") {
                                    self.visit_type_definition(value);
                                } else {
                                    self.visit_non_type_definition(value);
                                }
                            }
                        }
                    }
                    None => {
                        // If we're in a type definition, we need to treat the arguments to any
                        // other callables as non-type definitions (i.e., we don't want to treat
                        // any strings as deferred type definitions).
                        self.visit_expr(func);
                        for arg in args {
                            self.visit_non_type_definition(arg);
                        }
                        for keyword in keywords {
                            let Keyword { value, .. } = keyword;
                            self.visit_non_type_definition(value);
                        }
                    }
                }
            }
            Expr::Subscript(ast::ExprSubscript {
                value,
                slice,
                ctx,
                range: _,
            }) => {
                // Only allow annotations in `ExprContext::Load`. If we have, e.g.,
                // `obj["foo"]["bar"]`, we need to avoid treating the `obj["foo"]`
                // portion as an annotation, despite having `ExprContext::Load`. Thus, we track
                // the `ExprContext` at the top-level.
                if self.semantic_model.in_subscript() {
                    visitor::walk_expr(self, expr);
                } else if matches!(ctx, ExprContext::Store | ExprContext::Del) {
                    self.semantic_model.flags |= SemanticModelFlags::SUBSCRIPT;
                    visitor::walk_expr(self, expr);
                } else {
                    match analyze::typing::match_annotated_subscript(
                        value,
                        &self.semantic_model,
                        self.settings.typing_modules.iter().map(String::as_str),
                        &self.settings.pyflakes.extend_generics,
                    ) {
                        Some(subscript) => {
                            match subscript {
                                // Ex) Optional[int]
                                SubscriptKind::AnnotatedSubscript => {
                                    self.visit_expr(value);
                                    self.visit_type_definition(slice);
                                    self.visit_expr_context(ctx);
                                }
                                // Ex) Annotated[int, "Hello, world!"]
                                SubscriptKind::PEP593AnnotatedSubscript => {
                                    // First argument is a type (including forward references); the
                                    // rest are arbitrary Python objects.
                                    self.visit_expr(value);
                                    if let Expr::Tuple(ast::ExprTuple {
                                        elts,
                                        ctx,
                                        range: _,
                                    }) = slice.as_ref()
                                    {
                                        if let Some(expr) = elts.first() {
                                            self.visit_expr(expr);
                                            for expr in elts.iter().skip(1) {
                                                self.visit_non_type_definition(expr);
                                            }
                                            self.visit_expr_context(ctx);
                                        }
                                    } else {
                                        error!(
                                            "Found non-Expr::Tuple argument to PEP 593 \
                                             Annotation."
                                        );
                                    }
                                }
                            }
                        }
                        None => visitor::walk_expr(self, expr),
                    }
                }
            }
            Expr::JoinedStr(_) => {
                self.semantic_model.flags |= if self.semantic_model.in_f_string() {
                    SemanticModelFlags::NESTED_F_STRING
                } else {
                    SemanticModelFlags::F_STRING
                };
                visitor::walk_expr(self, expr);
            }
            _ => visitor::walk_expr(self, expr),
        }

        // Post-visit.
        match expr {
            Expr::Lambda(_)
            | Expr::GeneratorExp(_)
            | Expr::ListComp(_)
            | Expr::DictComp(_)
            | Expr::SetComp(_) => {
                self.semantic_model.pop_scope();
            }
            _ => {}
        };

        self.semantic_model.flags = flags_snapshot;
        self.semantic_model.pop_expr();
    }

    fn visit_excepthandler(&mut self, excepthandler: &'b Excepthandler) {
        match excepthandler {
            Excepthandler::ExceptHandler(ast::ExcepthandlerExceptHandler {
                type_,
                name,
                body,
                range: _,
            }) => {
                let name = name.as_deref();
                if self.enabled(Rule::BareExcept) {
                    if let Some(diagnostic) = pycodestyle::rules::bare_except(
                        type_.as_deref(),
                        body,
                        excepthandler,
                        self.locator,
                    ) {
                        self.diagnostics.push(diagnostic);
                    }
                }
                if self.enabled(Rule::RaiseWithoutFromInsideExcept) {
                    flake8_bugbear::rules::raise_without_from_inside_except(self, body);
                }
                if self.enabled(Rule::BlindExcept) {
                    flake8_blind_except::rules::blind_except(self, type_.as_deref(), name, body);
                }
                if self.enabled(Rule::TryExceptPass) {
                    flake8_bandit::rules::try_except_pass(
                        self,
                        excepthandler,
                        type_.as_deref(),
                        name,
                        body,
                        self.settings.flake8_bandit.check_typed_exception,
                    );
                }
                if self.enabled(Rule::TryExceptContinue) {
                    flake8_bandit::rules::try_except_continue(
                        self,
                        excepthandler,
                        type_.as_deref(),
                        name,
                        body,
                        self.settings.flake8_bandit.check_typed_exception,
                    );
                }
                if self.enabled(Rule::ExceptWithEmptyTuple) {
                    flake8_bugbear::rules::except_with_empty_tuple(self, excepthandler);
                }
                if self.enabled(Rule::ExceptWithNonExceptionClasses) {
                    flake8_bugbear::rules::except_with_non_exception_classes(self, excepthandler);
                }
                if self.enabled(Rule::ReraiseNoCause) {
                    tryceratops::rules::reraise_no_cause(self, body);
                }

                if self.enabled(Rule::BinaryOpException) {
                    pylint::rules::binary_op_exception(self, excepthandler);
                }
                match name {
                    Some(name) => {
                        if self.enabled(Rule::AmbiguousVariableName) {
                            if let Some(diagnostic) = pycodestyle::rules::ambiguous_variable_name(
                                name,
                                helpers::excepthandler_name_range(excepthandler, self.locator)
                                    .expect("Failed to find `name` range"),
                            ) {
                                self.diagnostics.push(diagnostic);
                            }
                        }

                        if self.enabled(Rule::BuiltinVariableShadowing) {
                            flake8_builtins::rules::builtin_variable_shadowing(
                                self,
                                name,
                                AnyShadowing::from(excepthandler),
                            );
                        }

                        let name_range =
                            helpers::excepthandler_name_range(excepthandler, self.locator).unwrap();

                        if self.semantic_model.scope().has(name) {
                            self.handle_node_store(
                                name,
                                &Expr::Name(ast::ExprName {
                                    id: name.into(),
                                    ctx: ExprContext::Store,
                                    range: name_range,
                                }),
                            );
                        }

                        let definition = self.semantic_model.scope().get(name);
                        self.handle_node_store(
                            name,
                            &Expr::Name(ast::ExprName {
                                id: name.into(),
                                ctx: ExprContext::Store,
                                range: name_range,
                            }),
                        );

                        walk_excepthandler(self, excepthandler);

                        if let Some(binding_id) = {
                            let scope = self.semantic_model.scope_mut();
                            scope.delete(name)
                        } {
                            if !self.semantic_model.is_used(binding_id) {
                                if self.enabled(Rule::UnusedVariable) {
                                    let mut diagnostic = Diagnostic::new(
                                        pyflakes::rules::UnusedVariable { name: name.into() },
                                        name_range,
                                    );
                                    if self.patch(Rule::UnusedVariable) {
                                        #[allow(deprecated)]
                                        diagnostic.try_set_fix_from_edit(|| {
                                            pyflakes::fixes::remove_exception_handler_assignment(
                                                excepthandler,
                                                self.locator,
                                            )
                                        });
                                    }
                                    self.diagnostics.push(diagnostic);
                                }
                            }
                        }

                        if let Some(binding_id) = definition {
                            let scope = self.semantic_model.scope_mut();
                            scope.add(name, binding_id);
                        }
                    }
                    None => walk_excepthandler(self, excepthandler),
                }
            }
        }
    }

    fn visit_format_spec(&mut self, format_spec: &'b Expr) {
        match format_spec {
            Expr::JoinedStr(ast::ExprJoinedStr { values, range: _ }) => {
                for value in values {
                    self.visit_expr(value);
                }
            }
            _ => unreachable!("Unexpected expression for format_spec"),
        }
    }

    fn visit_arguments(&mut self, arguments: &'b Arguments) {
        if self.enabled(Rule::MutableArgumentDefault) {
            flake8_bugbear::rules::mutable_argument_default(self, arguments);
        }
        if self.enabled(Rule::FunctionCallInDefaultArgument) {
            flake8_bugbear::rules::function_call_argument_default(self, arguments);
        }

        if self.is_stub {
            if self.enabled(Rule::TypedArgumentDefaultInStub) {
                flake8_pyi::rules::typed_argument_simple_defaults(self, arguments);
            }
        }
        if self.is_stub {
            if self.enabled(Rule::ArgumentDefaultInStub) {
                flake8_pyi::rules::argument_simple_defaults(self, arguments);
            }
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
            &arg.arg,
            arg.range(),
            BindingKind::Argument,
            BindingFlags::empty(),
        );

        if self.enabled(Rule::AmbiguousVariableName) {
            if let Some(diagnostic) =
                pycodestyle::rules::ambiguous_variable_name(&arg.arg, arg.range())
            {
                self.diagnostics.push(diagnostic);
            }
        }

        if self.enabled(Rule::InvalidArgumentName) {
            if let Some(diagnostic) = pep8_naming::rules::invalid_argument_name(
                &arg.arg,
                arg,
                &self.settings.pep8_naming.ignore_names,
            ) {
                self.diagnostics.push(diagnostic);
            }
        }

        if self.enabled(Rule::BuiltinArgumentShadowing) {
            flake8_builtins::rules::builtin_argument_shadowing(self, arg);
        }
    }

    fn visit_pattern(&mut self, pattern: &'b Pattern) {
        if let Pattern::MatchAs(ast::PatternMatchAs {
            name: Some(name), ..
        })
        | Pattern::MatchStar(ast::PatternMatchStar {
            name: Some(name),
            range: _,
        })
        | Pattern::MatchMapping(ast::PatternMatchMapping {
            rest: Some(name), ..
        }) = pattern
        {
            self.add_binding(
                name,
                pattern.range(),
                BindingKind::Assignment,
                BindingFlags::empty(),
            );
        }

        walk_pattern(self, pattern);
    }

    fn visit_body(&mut self, body: &'b [Stmt]) {
        if self.enabled(Rule::UnnecessaryPass) {
            flake8_pie::rules::no_unnecessary_pass(self, body);
        }

        let prev_body = self.semantic_model.body;
        let prev_body_index = self.semantic_model.body_index;
        self.semantic_model.body = body;
        self.semantic_model.body_index = 0;

        for stmt in body {
            self.visit_stmt(stmt);
            self.semantic_model.body_index += 1;
        }

        self.semantic_model.body = prev_body;
        self.semantic_model.body_index = prev_body_index;
    }
}

impl<'a> Checker<'a> {
    /// Visit a [`Module`]. Returns `true` if the module contains a module-level docstring.
    fn visit_module(&mut self, python_ast: &'a Suite) -> bool {
        if self.enabled(Rule::FStringDocstring) {
            flake8_bugbear::rules::f_string_docstring(self, python_ast);
        }
        let docstring = docstrings::extraction::docstring_from(python_ast);
        docstring.is_some()
    }

    /// Visit a list of [`Comprehension`] nodes, assumed to be the comprehensions that compose a
    /// generator expression, like a list or set comprehension.
    fn visit_generators(&mut self, generators: &'a [Comprehension]) {
        let mut generators = generators.iter();

        let Some(generator) = generators.next() else {
            unreachable!("Generator expression must contain at least one generator");
        };

        // Generators are compiled as nested functions. (This may change with PEP 709.)
        // As such, the `iter` of the first generator is evaluated in the outer scope, while all
        // subsequent nodes are evaluated in the inner scope.
        //
        // For example, given:
        // ```py
        // class A:
        //     T = range(10)
        //
        //     L = [x for x in T for y in T]
        // ```
        //
        // Conceptually, this is compiled as:
        // ```py
        // class A:
        //     T = range(10)
        //
        //     def foo(x=T):
        //         def bar(y=T):
        //             pass
        //         return bar()
        //     foo()
        // ```
        //
        // Following Python's scoping rules, the `T` in `x=T` is thus evaluated in the outer scope,
        // while all subsequent reads and writes are evaluated in the inner scope. In particular,
        // `x` is local to `foo`, and the `T` in `y=T` skips the class scope when resolving.
        self.visit_expr(&generator.iter);
        self.semantic_model.push_scope(ScopeKind::Generator);
        self.visit_expr(&generator.target);
        for expr in &generator.ifs {
            self.visit_boolean_test(expr);
        }

        for generator in generators {
            self.visit_expr(&generator.iter);
            self.visit_expr(&generator.target);
            for expr in &generator.ifs {
                self.visit_boolean_test(expr);
            }
        }
    }

    /// Visit an body of [`Stmt`] nodes within a type-checking block.
    fn visit_type_checking_block(&mut self, body: &'a [Stmt]) {
        let snapshot = self.semantic_model.flags;
        self.semantic_model.flags |= SemanticModelFlags::TYPE_CHECKING_BLOCK;
        self.visit_body(body);
        self.semantic_model.flags = snapshot;
    }

    /// Visit an [`Expr`], and treat it as a type definition.
    fn visit_type_definition(&mut self, expr: &'a Expr) {
        let snapshot = self.semantic_model.flags;
        self.semantic_model.flags |= SemanticModelFlags::TYPE_DEFINITION;
        self.visit_expr(expr);
        self.semantic_model.flags = snapshot;
    }

    /// Visit an [`Expr`], and treat it as _not_ a type definition.
    fn visit_non_type_definition(&mut self, expr: &'a Expr) {
        let snapshot = self.semantic_model.flags;
        self.semantic_model.flags -= SemanticModelFlags::TYPE_DEFINITION;
        self.visit_expr(expr);
        self.semantic_model.flags = snapshot;
    }

    /// Visit an [`Expr`], and treat it as a boolean test. This is useful for detecting whether an
    /// expressions return value is significant, or whether the calling context only relies on
    /// its truthiness.
    fn visit_boolean_test(&mut self, expr: &'a Expr) {
        let snapshot = self.semantic_model.flags;
        self.semantic_model.flags |= SemanticModelFlags::BOOLEAN_TEST;
        self.visit_expr(expr);
        self.semantic_model.flags = snapshot;
    }

    /// Add a [`Binding`] to the current scope, bound to the given name.
    fn add_binding(
        &mut self,
        name: &'a str,
        range: TextRange,
        kind: BindingKind<'a>,
        flags: BindingFlags,
    ) -> BindingId {
        // Determine the scope to which the binding belongs.
        // Per [PEP 572](https://peps.python.org/pep-0572/#scope-of-the-target), named
        // expressions in generators and comprehensions bind to the scope that contains the
        // outermost comprehension.
        let scope_id = if kind.is_named_expr_assignment() {
            self.semantic_model
                .scopes
                .ancestor_ids(self.semantic_model.scope_id)
                .find_or_last(|scope_id| !self.semantic_model.scopes[*scope_id].kind.is_generator())
                .unwrap_or(self.semantic_model.scope_id)
        } else {
            self.semantic_model.scope_id
        };

        // Create the `Binding`.
        let binding_id = self.semantic_model.push_binding(range, kind, flags);
        let binding = &self.semantic_model.bindings[binding_id];

        // Determine whether the binding shadows any existing bindings.
        if let Some((stack_index, shadowed_id)) = self
            .semantic_model
            .scopes
            .ancestors(self.semantic_model.scope_id)
            .enumerate()
            .find_map(|(stack_index, scope)| {
                scope.get(name).map(|binding_id| (stack_index, binding_id))
            })
        {
            let shadowed = &self.semantic_model.bindings[shadowed_id];
            let in_current_scope = stack_index == 0;
            if !shadowed.kind.is_builtin()
                && shadowed.source.map_or(true, |left| {
                    binding.source.map_or(true, |right| {
                        !branch_detection::different_forks(left, right, &self.semantic_model.stmts)
                    })
                })
            {
                let shadows_import = matches!(
                    shadowed.kind,
                    BindingKind::Importation(..)
                        | BindingKind::FromImportation(..)
                        | BindingKind::SubmoduleImportation(..)
                        | BindingKind::FutureImportation
                );
                if binding.kind.is_loop_var() && shadows_import {
                    if self.enabled(Rule::ImportShadowedByLoopVar) {
                        #[allow(deprecated)]
                        let line = self.locator.compute_line_index(shadowed.range.start());

                        self.diagnostics.push(Diagnostic::new(
                            pyflakes::rules::ImportShadowedByLoopVar {
                                name: name.to_string(),
                                line,
                            },
                            binding.range,
                        ));
                    }
                } else if in_current_scope {
                    if !shadowed.is_used()
                        && binding.redefines(shadowed)
                        && (!self.settings.dummy_variable_rgx.is_match(name) || shadows_import)
                        && !(shadowed.kind.is_function_definition()
                            && analyze::visibility::is_overload(
                                &self.semantic_model,
                                cast::decorator_list(
                                    self.semantic_model.stmts[shadowed.source.unwrap()],
                                ),
                            ))
                    {
                        if self.enabled(Rule::RedefinedWhileUnused) {
                            #[allow(deprecated)]
                            let line = self.locator.compute_line_index(
                                shadowed
                                    .trimmed_range(&self.semantic_model, self.locator)
                                    .start(),
                            );

                            let mut diagnostic = Diagnostic::new(
                                pyflakes::rules::RedefinedWhileUnused {
                                    name: name.to_string(),
                                    line,
                                },
                                binding.trimmed_range(&self.semantic_model, self.locator),
                            );
                            if let Some(range) = binding.parent_range(&self.semantic_model) {
                                diagnostic.set_parent(range.start());
                            }
                            self.diagnostics.push(diagnostic);
                        }
                    }
                } else if shadows_import && binding.redefines(shadowed) {
                    self.semantic_model
                        .shadowed_bindings
                        .insert(binding_id, shadowed_id);
                }
            }
        }

        // If there's an existing binding in this scope, copy its references.
        if let Some(shadowed) = self.semantic_model.scopes[scope_id]
            .get(name)
            .map(|binding_id| &self.semantic_model.bindings[binding_id])
        {
            match &shadowed.kind {
                BindingKind::Builtin => {
                    // Avoid overriding builtins.
                }
                kind @ (BindingKind::Global | BindingKind::Nonlocal) => {
                    // If the original binding was a global or nonlocal, then the new binding is
                    // too.
                    let references = shadowed.references.clone();
                    self.semantic_model.bindings[binding_id].kind = kind.clone();
                    self.semantic_model.bindings[binding_id].references = references;
                }
                _ => {
                    let references = shadowed.references.clone();
                    self.semantic_model.bindings[binding_id].references = references;
                }
            }

            // If this is an annotation, and we already have an existing value in the same scope,
            // don't treat it as an assignment (i.e., avoid adding it to the scope).
            if self.semantic_model.bindings[binding_id]
                .kind
                .is_annotation()
            {
                return binding_id;
            }
        }

        // Add the binding to the scope.
        let scope = &mut self.semantic_model.scopes[scope_id];
        scope.add(name, binding_id);

        binding_id
    }

    fn bind_builtins(&mut self) {
        for builtin in BUILTINS
            .iter()
            .chain(MAGIC_GLOBALS.iter())
            .copied()
            .chain(self.settings.builtins.iter().map(String::as_str))
        {
            // Add the builtin to the scope.
            let binding_id = self.semantic_model.push_builtin();
            let scope = self.semantic_model.scope_mut();
            scope.add(builtin, binding_id);
        }
    }

    fn handle_node_load(&mut self, expr: &Expr) {
        let Expr::Name(ast::ExprName { id, .. } )= expr else {
            return;
        };
        match self.semantic_model.resolve_read(id, expr.range()) {
            ResolvedRead::Resolved(..) | ResolvedRead::ImplicitGlobal => {
                // Nothing to do.
            }
            ResolvedRead::StarImport => {
                // F405
                if self.enabled(Rule::UndefinedLocalWithImportStarUsage) {
                    let sources: Vec<String> = self
                        .semantic_model
                        .scopes
                        .iter()
                        .flat_map(Scope::star_imports)
                        .map(|StarImportation { level, module }| {
                            helpers::format_import_from(*level, *module)
                        })
                        .sorted()
                        .dedup()
                        .collect();
                    self.diagnostics.push(Diagnostic::new(
                        pyflakes::rules::UndefinedLocalWithImportStarUsage {
                            name: id.to_string(),
                            sources,
                        },
                        expr.range(),
                    ));
                }
            }
            ResolvedRead::NotFound => {
                // F821
                if self.enabled(Rule::UndefinedName) {
                    // Allow __path__.
                    if self.path.ends_with("__init__.py") && id == "__path__" {
                        return;
                    }

                    // Avoid flagging if `NameError` is handled.
                    if self
                        .semantic_model
                        .handled_exceptions
                        .iter()
                        .any(|handler_names| handler_names.contains(Exceptions::NAME_ERROR))
                    {
                        return;
                    }

                    self.diagnostics.push(Diagnostic::new(
                        pyflakes::rules::UndefinedName {
                            name: id.to_string(),
                        },
                        expr.range(),
                    ));
                }
            }
        }
    }

    fn handle_node_store(&mut self, id: &'a str, expr: &Expr) {
        let parent = self.semantic_model.stmt();

        if self.enabled(Rule::UndefinedLocal) {
            pyflakes::rules::undefined_local(self, id);
        }

        if self.enabled(Rule::NonLowercaseVariableInFunction) {
            if self.semantic_model.scope().kind.is_any_function() {
                // Ignore globals.
                if !self
                    .semantic_model
                    .scope()
                    .get(id)
                    .map_or(false, |binding_id| {
                        self.semantic_model.bindings[binding_id].kind.is_global()
                    })
                {
                    pep8_naming::rules::non_lowercase_variable_in_function(self, expr, parent, id);
                }
            }
        }

        if self.enabled(Rule::MixedCaseVariableInClassScope) {
            if let ScopeKind::Class(ast::StmtClassDef { bases, .. }) =
                &self.semantic_model.scope().kind
            {
                pep8_naming::rules::mixed_case_variable_in_class_scope(
                    self, expr, parent, id, bases,
                );
            }
        }

        if self.enabled(Rule::MixedCaseVariableInGlobalScope) {
            if matches!(self.semantic_model.scope().kind, ScopeKind::Module) {
                pep8_naming::rules::mixed_case_variable_in_global_scope(self, expr, parent, id);
            }
        }

        if matches!(
            parent,
            Stmt::AnnAssign(ast::StmtAnnAssign { value: None, .. })
        ) {
            self.add_binding(
                id,
                expr.range(),
                BindingKind::Annotation,
                BindingFlags::empty(),
            );
            return;
        }

        if matches!(parent, Stmt::For(_) | Stmt::AsyncFor(_)) {
            self.add_binding(
                id,
                expr.range(),
                BindingKind::LoopVar,
                BindingFlags::empty(),
            );
            return;
        }

        if helpers::is_unpacking_assignment(parent, expr) {
            self.add_binding(
                id,
                expr.range(),
                BindingKind::UnpackedAssignment,
                BindingFlags::empty(),
            );
            return;
        }

        let scope = self.semantic_model.scope();

        if scope.kind.is_module()
            && match parent {
                Stmt::Assign(ast::StmtAssign { targets, .. }) => {
                    if let Some(Expr::Name(ast::ExprName { id, .. })) = targets.first() {
                        id == "__all__"
                    } else {
                        false
                    }
                }
                Stmt::AugAssign(ast::StmtAugAssign { target, .. }) => {
                    if let Expr::Name(ast::ExprName { id, .. }) = target.as_ref() {
                        id == "__all__"
                    } else {
                        false
                    }
                }
                Stmt::AnnAssign(ast::StmtAnnAssign { target, .. }) => {
                    if let Expr::Name(ast::ExprName { id, .. }) = target.as_ref() {
                        id == "__all__"
                    } else {
                        false
                    }
                }
                _ => false,
            }
        {
            let (names, flags) =
                extract_all_names(parent, |name| self.semantic_model.is_builtin(name));

            if self.enabled(Rule::InvalidAllFormat) {
                if matches!(flags, AllNamesFlags::INVALID_FORMAT) {
                    self.diagnostics
                        .push(pylint::rules::invalid_all_format(expr));
                }
            }

            if self.enabled(Rule::InvalidAllObject) {
                if matches!(flags, AllNamesFlags::INVALID_OBJECT) {
                    self.diagnostics
                        .push(pylint::rules::invalid_all_object(expr));
                }
            }

            self.add_binding(
                id,
                expr.range(),
                BindingKind::Export(Export { names }),
                BindingFlags::empty(),
            );
            return;
        }

        if self
            .semantic_model
            .expr_ancestors()
            .any(|expr| matches!(expr, Expr::NamedExpr(_)))
        {
            self.add_binding(
                id,
                expr.range(),
                BindingKind::NamedExprAssignment,
                BindingFlags::empty(),
            );
            return;
        }

        self.add_binding(
            id,
            expr.range(),
            BindingKind::Assignment,
            BindingFlags::empty(),
        );
    }

    fn handle_node_delete(&mut self, expr: &'a Expr) {
        let Expr::Name(ast::ExprName { id, .. } )= expr else {
            return;
        };
        if helpers::on_conditional_branch(&mut self.semantic_model.parents()) {
            return;
        }

        let scope = self.semantic_model.scope_mut();
        if scope.delete(id.as_str()).is_none() {
            if self.enabled(Rule::UndefinedName) {
                self.diagnostics.push(Diagnostic::new(
                    pyflakes::rules::UndefinedName {
                        name: id.to_string(),
                    },
                    expr.range(),
                ));
            }
        }
    }

    fn check_deferred_future_type_definitions(&mut self) {
        while !self.deferred.future_type_definitions.is_empty() {
            let type_definitions = std::mem::take(&mut self.deferred.future_type_definitions);
            for (expr, snapshot) in type_definitions {
                self.semantic_model.restore(snapshot);

                self.semantic_model.flags |= SemanticModelFlags::TYPE_DEFINITION
                    | SemanticModelFlags::FUTURE_TYPE_DEFINITION;
                self.visit_expr(expr);
            }
        }
    }

    fn check_deferred_string_type_definitions(&mut self, allocator: &'a typed_arena::Arena<Expr>) {
        while !self.deferred.string_type_definitions.is_empty() {
            let type_definitions = std::mem::take(&mut self.deferred.string_type_definitions);
            for (range, value, snapshot) in type_definitions {
                if let Ok((expr, kind)) = parse_type_annotation(value, range, self.locator) {
                    let expr = allocator.alloc(expr);

                    self.semantic_model.restore(snapshot);

                    if self.semantic_model.in_annotation()
                        && self.semantic_model.future_annotations()
                    {
                        if self.enabled(Rule::QuotedAnnotation) {
                            pyupgrade::rules::quoted_annotation(self, value, range);
                        }
                    }
                    if self.is_stub {
                        if self.enabled(Rule::QuotedAnnotationInStub) {
                            flake8_pyi::rules::quoted_annotation_in_stub(self, value, range);
                        }
                    }

                    let type_definition_flag = match kind {
                        AnnotationKind::Simple => SemanticModelFlags::SIMPLE_STRING_TYPE_DEFINITION,
                        AnnotationKind::Complex => {
                            SemanticModelFlags::COMPLEX_STRING_TYPE_DEFINITION
                        }
                    };

                    self.semantic_model.flags |=
                        SemanticModelFlags::TYPE_DEFINITION | type_definition_flag;
                    self.visit_expr(expr);
                } else {
                    if self.enabled(Rule::ForwardAnnotationSyntaxError) {
                        self.diagnostics.push(Diagnostic::new(
                            pyflakes::rules::ForwardAnnotationSyntaxError {
                                body: value.to_string(),
                            },
                            range,
                        ));
                    }
                }
            }
        }
    }

    fn check_deferred_functions(&mut self) {
        while !self.deferred.functions.is_empty() {
            let deferred_functions = std::mem::take(&mut self.deferred.functions);
            for snapshot in deferred_functions {
                self.semantic_model.restore(snapshot);

                match &self.semantic_model.stmt() {
                    Stmt::FunctionDef(ast::StmtFunctionDef { body, args, .. })
                    | Stmt::AsyncFunctionDef(ast::StmtAsyncFunctionDef { body, args, .. }) => {
                        self.visit_arguments(args);
                        self.visit_body(body);
                    }
                    _ => {
                        unreachable!("Expected Stmt::FunctionDef | Stmt::AsyncFunctionDef")
                    }
                }

                self.deferred.assignments.push(snapshot);
            }
        }
    }

    fn check_deferred_lambdas(&mut self) {
        while !self.deferred.lambdas.is_empty() {
            let lambdas = std::mem::take(&mut self.deferred.lambdas);
            for (expr, snapshot) in lambdas {
                self.semantic_model.restore(snapshot);

                if let Expr::Lambda(ast::ExprLambda {
                    args,
                    body,
                    range: _,
                }) = expr
                {
                    self.visit_arguments(args);
                    self.visit_expr(body);
                } else {
                    unreachable!("Expected Expr::Lambda");
                }

                self.deferred.assignments.push(snapshot);
            }
        }
    }

    fn check_deferred_assignments(&mut self) {
        while !self.deferred.assignments.is_empty() {
            let assignments = std::mem::take(&mut self.deferred.assignments);
            for snapshot in assignments {
                self.semantic_model.restore(snapshot);

                // pyflakes
                if self.enabled(Rule::UnusedVariable) {
                    pyflakes::rules::unused_variable(self, self.semantic_model.scope_id);
                }
                if self.enabled(Rule::UnusedAnnotation) {
                    pyflakes::rules::unused_annotation(self, self.semantic_model.scope_id);
                }

                if !self.is_stub {
                    // flake8-unused-arguments
                    if self.any_enabled(&[
                        Rule::UnusedFunctionArgument,
                        Rule::UnusedMethodArgument,
                        Rule::UnusedClassMethodArgument,
                        Rule::UnusedStaticMethodArgument,
                        Rule::UnusedLambdaArgument,
                    ]) {
                        let scope = &self.semantic_model.scopes[self.semantic_model.scope_id];
                        let parent = &self.semantic_model.scopes[scope.parent.unwrap()];
                        self.diagnostics
                            .extend(flake8_unused_arguments::rules::unused_arguments(
                                self,
                                parent,
                                scope,
                                &self.semantic_model.bindings,
                            ));
                    }
                }
            }
        }
    }

    fn check_deferred_for_loops(&mut self) {
        while !self.deferred.for_loops.is_empty() {
            let for_loops = std::mem::take(&mut self.deferred.for_loops);

            for snapshot in for_loops {
                self.semantic_model.restore(snapshot);

                if let Stmt::For(ast::StmtFor { target, body, .. })
                | Stmt::AsyncFor(ast::StmtAsyncFor { target, body, .. }) =
                    &self.semantic_model.stmt()
                {
                    if self.enabled(Rule::UnusedLoopControlVariable) {
                        flake8_bugbear::rules::unused_loop_control_variable(self, target, body);
                    }
                } else {
                    unreachable!("Expected Expr::For | Expr::AsyncFor");
                }
            }
        }
    }

    fn check_dead_scopes(&mut self) {
        let enforce_typing_imports = !self.is_stub
            && self.any_enabled(&[
                Rule::GlobalVariableNotAssigned,
                Rule::RuntimeImportInTypeCheckingBlock,
                Rule::TypingOnlyFirstPartyImport,
                Rule::TypingOnlyThirdPartyImport,
                Rule::TypingOnlyStandardLibraryImport,
            ]);

        if !(enforce_typing_imports
            || self.any_enabled(&[
                Rule::UnusedImport,
                Rule::UndefinedLocalWithImportStarUsage,
                Rule::RedefinedWhileUnused,
                Rule::UndefinedExport,
            ]))
        {
            return;
        }

        // Mark anything referenced in `__all__` as used.
        let exports: Vec<(&str, TextRange)> = {
            let global_scope = self.semantic_model.global_scope();
            global_scope
                .bindings_for_name("__all__")
                .map(|binding_id| &self.semantic_model.bindings[binding_id])
                .filter_map(|binding| match &binding.kind {
                    BindingKind::Export(Export { names }) => {
                        Some(names.iter().map(|name| (*name, binding.range)))
                    }
                    _ => None,
                })
                .flatten()
                .collect()
        };

        for (name, range) in &exports {
            if let Some(binding_id) = self.semantic_model.global_scope().get(name) {
                self.semantic_model.add_global_reference(
                    binding_id,
                    *range,
                    ExecutionContext::Runtime,
                );
            }
        }

        // Identify any valid runtime imports. If a module is imported at runtime, and
        // used at runtime, then by default, we avoid flagging any other
        // imports from that model as typing-only.
        let runtime_imports: Vec<Vec<&Binding>> = if enforce_typing_imports {
            if self.settings.flake8_type_checking.strict {
                vec![]
            } else {
                self.semantic_model
                    .scopes
                    .iter()
                    .map(|scope| {
                        scope
                            .binding_ids()
                            .map(|binding_id| &self.semantic_model.bindings[binding_id])
                            .filter(|binding| {
                                flake8_type_checking::helpers::is_valid_runtime_import(
                                    &self.semantic_model,
                                    binding,
                                )
                            })
                            .collect()
                    })
                    .collect::<Vec<_>>()
            }
        } else {
            vec![]
        };

        let mut diagnostics: Vec<Diagnostic> = vec![];
        for scope_id in self.semantic_model.dead_scopes.iter().rev() {
            let scope = &self.semantic_model.scopes[*scope_id];

            if scope.kind.is_module() {
                // F822
                if self.enabled(Rule::UndefinedExport) {
                    if !self.path.ends_with("__init__.py") {
                        for (name, range) in &exports {
                            diagnostics
                                .extend(pyflakes::rules::undefined_export(name, *range, scope));
                        }
                    }
                }

                // F405
                if self.enabled(Rule::UndefinedLocalWithImportStarUsage) {
                    let sources: Vec<String> = scope
                        .star_imports()
                        .map(|StarImportation { level, module }| {
                            helpers::format_import_from(*level, *module)
                        })
                        .sorted()
                        .dedup()
                        .collect();
                    if !sources.is_empty() {
                        for (name, range) in &exports {
                            if !scope.has(name) {
                                diagnostics.push(Diagnostic::new(
                                    pyflakes::rules::UndefinedLocalWithImportStarUsage {
                                        name: (*name).to_string(),
                                        sources: sources.clone(),
                                    },
                                    *range,
                                ));
                            }
                        }
                    }
                }
            }

            // PLW0602
            if self.enabled(Rule::GlobalVariableNotAssigned) {
                for (name, binding_id) in scope.bindings() {
                    let binding = &self.semantic_model.bindings[binding_id];
                    if binding.kind.is_global() {
                        if let Some(source) = binding.source {
                            let stmt = &self.semantic_model.stmts[source];
                            if stmt.is_global_stmt() {
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
            if scope.kind.is_class() {
                continue;
            }

            // Look for any bindings that were redefined in another scope, and remain
            // unused. Note that we only store references in `shadowed_bindings` if
            // the bindings are in different scopes.
            if self.enabled(Rule::RedefinedWhileUnused) {
                for (name, binding_id) in scope.bindings() {
                    if let Some(shadowed_id) = self.semantic_model.shadowed_binding(binding_id) {
                        let shadowed = &self.semantic_model.bindings[shadowed_id];
                        if shadowed.is_used() {
                            continue;
                        }

                        #[allow(deprecated)]
                        let line = self.locator.compute_line_index(
                            shadowed
                                .trimmed_range(&self.semantic_model, self.locator)
                                .start(),
                        );

                        let binding = &self.semantic_model.bindings[binding_id];
                        let mut diagnostic = Diagnostic::new(
                            pyflakes::rules::RedefinedWhileUnused {
                                name: (*name).to_string(),
                                line,
                            },
                            binding.trimmed_range(&self.semantic_model, self.locator),
                        );
                        if let Some(range) = binding.parent_range(&self.semantic_model) {
                            diagnostic.set_parent(range.start());
                        }
                        diagnostics.push(diagnostic);
                    }
                }
            }

            if enforce_typing_imports {
                let runtime_imports: Vec<&Binding> = if self.settings.flake8_type_checking.strict {
                    vec![]
                } else {
                    self.semantic_model
                        .scopes
                        .ancestor_ids(*scope_id)
                        .flat_map(|scope_id| runtime_imports[scope_id.as_usize()].iter())
                        .copied()
                        .collect()
                };

                flake8_type_checking::rules::runtime_import_in_type_checking_block(
                    self,
                    scope,
                    &mut diagnostics,
                );

                flake8_type_checking::rules::typing_only_runtime_import(
                    self,
                    scope,
                    &runtime_imports,
                    &mut diagnostics,
                );
            }

            if self.enabled(Rule::UnusedImport) {
                pyflakes::rules::unused_import(self, scope, &mut diagnostics);
            }
        }
        self.diagnostics.extend(diagnostics);
    }

    /// Visit all the [`Definition`] nodes in the AST.
    ///
    /// This phase is expected to run after the AST has been traversed in its entirety; as such,
    /// it is expected that all [`Definition`] nodes have been visited by the time, and that this
    /// method will not recurse into any other nodes.
    fn check_definitions(&mut self) {
        let enforce_annotations = self.any_enabled(&[
            Rule::MissingTypeFunctionArgument,
            Rule::MissingTypeArgs,
            Rule::MissingTypeKwargs,
            Rule::MissingTypeSelf,
            Rule::MissingTypeCls,
            Rule::MissingReturnTypeUndocumentedPublicFunction,
            Rule::MissingReturnTypePrivateFunction,
            Rule::MissingReturnTypeSpecialMethod,
            Rule::MissingReturnTypeStaticMethod,
            Rule::MissingReturnTypeClassMethod,
            Rule::AnyType,
        ]);
        let enforce_stubs = self.is_stub
            && self.any_enabled(&[Rule::DocstringInStub, Rule::IterMethodReturnIterable]);
        let enforce_docstrings = self.any_enabled(&[
            Rule::UndocumentedPublicModule,
            Rule::UndocumentedPublicClass,
            Rule::UndocumentedPublicMethod,
            Rule::UndocumentedPublicFunction,
            Rule::UndocumentedPublicPackage,
            Rule::UndocumentedMagicMethod,
            Rule::UndocumentedPublicNestedClass,
            Rule::UndocumentedPublicInit,
            Rule::FitsOnOneLine,
            Rule::NoBlankLineBeforeFunction,
            Rule::NoBlankLineAfterFunction,
            Rule::OneBlankLineBeforeClass,
            Rule::OneBlankLineAfterClass,
            Rule::BlankLineAfterSummary,
            Rule::IndentWithSpaces,
            Rule::UnderIndentation,
            Rule::OverIndentation,
            Rule::NewLineAfterLastParagraph,
            Rule::SurroundingWhitespace,
            Rule::BlankLineBeforeClass,
            Rule::MultiLineSummaryFirstLine,
            Rule::MultiLineSummarySecondLine,
            Rule::SectionNotOverIndented,
            Rule::SectionUnderlineNotOverIndented,
            Rule::TripleSingleQuotes,
            Rule::EscapeSequenceInDocstring,
            Rule::EndsInPeriod,
            Rule::NonImperativeMood,
            Rule::NoSignature,
            Rule::FirstLineCapitalized,
            Rule::DocstringStartsWithThis,
            Rule::CapitalizeSectionName,
            Rule::NewLineAfterSectionName,
            Rule::DashedUnderlineAfterSection,
            Rule::SectionUnderlineAfterName,
            Rule::SectionUnderlineMatchesSectionLength,
            Rule::NoBlankLineAfterSection,
            Rule::NoBlankLineBeforeSection,
            Rule::BlankLinesBetweenHeaderAndContent,
            Rule::BlankLineAfterLastSection,
            Rule::EmptyDocstringSection,
            Rule::EndsInPunctuation,
            Rule::SectionNameEndsInColon,
            Rule::UndocumentedParam,
            Rule::OverloadWithDocstring,
            Rule::EmptyDocstring,
        ]);

        if !enforce_annotations && !enforce_docstrings && !enforce_stubs {
            return;
        }

        // Compute visibility of all definitions.
        let global_scope = self.semantic_model.global_scope();
        let exports: Option<&[&str]> = global_scope
            .get("__all__")
            .map(|binding_id| &self.semantic_model.bindings[binding_id])
            .and_then(|binding| match &binding.kind {
                BindingKind::Export(Export { names }) => Some(names.as_slice()),
                _ => None,
            });
        let definitions = std::mem::take(&mut self.semantic_model.definitions);

        let mut overloaded_name: Option<String> = None;
        for ContextualizedDefinition {
            definition,
            visibility,
        } in definitions.resolve(exports).iter()
        {
            let docstring = docstrings::extraction::extract_docstring(definition);

            // flake8-annotations
            if enforce_annotations {
                // TODO(charlie): This should be even stricter, in that an overload
                // implementation should come immediately after the overloaded
                // interfaces, without any AST nodes in between. Right now, we
                // only error when traversing definition boundaries (functions,
                // classes, etc.).
                if !overloaded_name.map_or(false, |overloaded_name| {
                    flake8_annotations::helpers::is_overload_impl(
                        &self.semantic_model,
                        definition,
                        &overloaded_name,
                    )
                }) {
                    self.diagnostics
                        .extend(flake8_annotations::rules::definition(
                            self,
                            definition,
                            *visibility,
                        ));
                }
                overloaded_name =
                    flake8_annotations::helpers::overloaded_name(&self.semantic_model, definition);
            }

            // flake8-pyi
            if enforce_stubs {
                if self.is_stub {
                    if self.enabled(Rule::DocstringInStub) {
                        flake8_pyi::rules::docstring_in_stubs(self, docstring);
                    }
                    if self.enabled(Rule::IterMethodReturnIterable) {
                        flake8_pyi::rules::iter_method_return_iterable(self, definition);
                    }
                }
            }

            // pydocstyle
            if enforce_docstrings {
                if pydocstyle::helpers::should_ignore_definition(
                    &self.semantic_model,
                    definition,
                    &self.settings.pydocstyle.ignore_decorators,
                ) {
                    continue;
                }

                // Extract a `Docstring` from a `Definition`.
                let Some(expr) = docstring else {
                    pydocstyle::rules::not_missing(self, definition, *visibility);
                    continue;
                };

                let contents = self.locator.slice(expr.range());

                let indentation = self.locator.slice(TextRange::new(
                    self.locator.line_start(expr.start()),
                    expr.start(),
                ));

                if pydocstyle::helpers::should_ignore_docstring(contents) {
                    #[allow(deprecated)]
                    let location = self.locator.compute_source_location(expr.start());
                    warn_user!(
                        "Docstring at {}:{}:{} contains implicit string concatenation; ignoring...",
                        relativize_path(self.path),
                        location.row,
                        location.column
                    );
                    continue;
                }

                // SAFETY: Safe for docstrings that pass `should_ignore_docstring`.
                let body_range = str::raw_contents_range(contents).unwrap();
                let docstring = Docstring {
                    definition,
                    expr,
                    contents,
                    body_range,
                    indentation,
                };

                if !pydocstyle::rules::not_empty(self, &docstring) {
                    continue;
                }

                if self.enabled(Rule::FitsOnOneLine) {
                    pydocstyle::rules::one_liner(self, &docstring);
                }
                if self.any_enabled(&[
                    Rule::NoBlankLineBeforeFunction,
                    Rule::NoBlankLineAfterFunction,
                ]) {
                    pydocstyle::rules::blank_before_after_function(self, &docstring);
                }
                if self.any_enabled(&[
                    Rule::OneBlankLineBeforeClass,
                    Rule::OneBlankLineAfterClass,
                    Rule::BlankLineBeforeClass,
                ]) {
                    pydocstyle::rules::blank_before_after_class(self, &docstring);
                }
                if self.enabled(Rule::BlankLineAfterSummary) {
                    pydocstyle::rules::blank_after_summary(self, &docstring);
                }
                if self.any_enabled(&[
                    Rule::IndentWithSpaces,
                    Rule::UnderIndentation,
                    Rule::OverIndentation,
                ]) {
                    pydocstyle::rules::indent(self, &docstring);
                }
                if self.enabled(Rule::NewLineAfterLastParagraph) {
                    pydocstyle::rules::newline_after_last_paragraph(self, &docstring);
                }
                if self.enabled(Rule::SurroundingWhitespace) {
                    pydocstyle::rules::no_surrounding_whitespace(self, &docstring);
                }
                if self.any_enabled(&[
                    Rule::MultiLineSummaryFirstLine,
                    Rule::MultiLineSummarySecondLine,
                ]) {
                    pydocstyle::rules::multi_line_summary_start(self, &docstring);
                }
                if self.enabled(Rule::TripleSingleQuotes) {
                    pydocstyle::rules::triple_quotes(self, &docstring);
                }
                if self.enabled(Rule::EscapeSequenceInDocstring) {
                    pydocstyle::rules::backslashes(self, &docstring);
                }
                if self.enabled(Rule::EndsInPeriod) {
                    pydocstyle::rules::ends_with_period(self, &docstring);
                }
                if self.enabled(Rule::NonImperativeMood) {
                    pydocstyle::rules::non_imperative_mood(
                        self,
                        &docstring,
                        &self.settings.pydocstyle.property_decorators,
                    );
                }
                if self.enabled(Rule::NoSignature) {
                    pydocstyle::rules::no_signature(self, &docstring);
                }
                if self.enabled(Rule::FirstLineCapitalized) {
                    pydocstyle::rules::capitalized(self, &docstring);
                }
                if self.enabled(Rule::DocstringStartsWithThis) {
                    pydocstyle::rules::starts_with_this(self, &docstring);
                }
                if self.enabled(Rule::EndsInPunctuation) {
                    pydocstyle::rules::ends_with_punctuation(self, &docstring);
                }
                if self.enabled(Rule::OverloadWithDocstring) {
                    pydocstyle::rules::if_needed(self, &docstring);
                }
                if self.any_enabled(&[
                    Rule::MultiLineSummaryFirstLine,
                    Rule::SectionNotOverIndented,
                    Rule::SectionUnderlineNotOverIndented,
                    Rule::CapitalizeSectionName,
                    Rule::NewLineAfterSectionName,
                    Rule::DashedUnderlineAfterSection,
                    Rule::SectionUnderlineAfterName,
                    Rule::SectionUnderlineMatchesSectionLength,
                    Rule::NoBlankLineAfterSection,
                    Rule::NoBlankLineBeforeSection,
                    Rule::BlankLinesBetweenHeaderAndContent,
                    Rule::BlankLineAfterLastSection,
                    Rule::EmptyDocstringSection,
                    Rule::SectionNameEndsInColon,
                    Rule::UndocumentedParam,
                ]) {
                    pydocstyle::rules::sections(
                        self,
                        &docstring,
                        self.settings.pydocstyle.convention.as_ref(),
                    );
                }
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn check_ast(
    python_ast: &Suite,
    locator: &Locator,
    stylist: &Stylist,
    indexer: &Indexer,
    noqa_line_for: &NoqaMapping,
    settings: &Settings,
    noqa: flags::Noqa,
    path: &Path,
    package: Option<&Path>,
) -> Vec<Diagnostic> {
    let module_path = package.and_then(|package| to_module_path(package, path));
    let module = Module {
        kind: if path.ends_with("__init__.py") {
            ModuleKind::Package
        } else {
            ModuleKind::Module
        },
        source: if let Some(module_path) = module_path.as_ref() {
            ModuleSource::Path(module_path)
        } else {
            ModuleSource::File(path)
        },
        python_ast,
    };

    let mut checker = Checker::new(
        settings,
        noqa_line_for,
        noqa,
        path,
        package,
        module,
        locator,
        stylist,
        indexer,
        Importer::new(python_ast, locator, stylist),
    );
    checker.bind_builtins();

    // Check for module docstring.
    let python_ast = if checker.visit_module(python_ast) {
        &python_ast[1..]
    } else {
        python_ast
    };

    // Iterate over the AST.
    checker.visit_body(python_ast);

    // Check any deferred statements.
    checker.check_deferred_functions();
    checker.check_deferred_lambdas();
    checker.check_deferred_future_type_definitions();
    let allocator = typed_arena::Arena::new();
    checker.check_deferred_string_type_definitions(&allocator);
    checker.check_deferred_assignments();
    checker.check_deferred_for_loops();

    // Check docstrings.
    checker.check_definitions();

    // Reset the scope to module-level, and check all consumed scopes.
    checker.semantic_model.scope_id = ScopeId::global();
    checker.semantic_model.dead_scopes.push(ScopeId::global());
    checker.check_dead_scopes();

    checker.diagnostics
}
