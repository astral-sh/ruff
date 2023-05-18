use std::path::Path;

use itertools::Itertools;
use log::error;
use ruff_text_size::{TextRange, TextSize};
use rustc_hash::{FxHashMap, FxHashSet};
use rustpython_format::cformat::{CFormatError, CFormatErrorType};
use rustpython_parser::ast::{
    self, Arg, Arguments, Comprehension, Constant, Excepthandler, Expr, ExprContext, Keyword,
    Operator, Pattern, Ranged, Stmt, Suite, Unaryop,
};

use ruff_diagnostics::{Diagnostic, Fix};
use ruff_python_ast::all::{extract_all_names, AllNamesFlags};
use ruff_python_ast::helpers::{extract_handled_exceptions, to_module_path};
use ruff_python_ast::source_code::{Generator, Indexer, Locator, Quote, Stylist};
use ruff_python_ast::str::trailing_quote;
use ruff_python_ast::types::{Node, RefEquality};
use ruff_python_ast::typing::{parse_type_annotation, AnnotationKind};
use ruff_python_ast::visitor::{walk_excepthandler, walk_pattern, Visitor};
use ruff_python_ast::{cast, helpers, str, visitor};
use ruff_python_semantic::analyze;
use ruff_python_semantic::analyze::branch_detection;
use ruff_python_semantic::analyze::typing::{Callable, SubscriptKind};
use ruff_python_semantic::analyze::visibility::ModuleSource;
use ruff_python_semantic::binding::{
    Binding, BindingId, BindingKind, Exceptions, ExecutionContext, Export, FromImportation,
    Importation, StarImportation, SubmoduleImportation,
};
use ruff_python_semantic::context::{Context, ContextFlags};
use ruff_python_semantic::definition::{ContextualizedDefinition, Module, ModuleKind};
use ruff_python_semantic::node::NodeId;
use ruff_python_semantic::scope::{ClassDef, FunctionDef, Lambda, Scope, ScopeId, ScopeKind};
use ruff_python_stdlib::builtins::{BUILTINS, MAGIC_GLOBALS};
use ruff_python_stdlib::path::is_python_stub_file;

use crate::checkers::ast::deferred::Deferred;
use crate::docstrings::extraction::ExtractionTarget;
use crate::docstrings::Docstring;
use crate::fs::relativize_path;
use crate::importer::Importer;
use crate::noqa::NoqaMapping;
use crate::registry::{AsRule, Rule};
use crate::rules::{
    flake8_2020, flake8_annotations, flake8_async, flake8_bandit, flake8_blind_except,
    flake8_boolean_trap, flake8_bugbear, flake8_builtins, flake8_comprehensions, flake8_datetimez,
    flake8_debugger, flake8_django, flake8_errmsg, flake8_future_annotations, flake8_gettext,
    flake8_implicit_str_concat, flake8_import_conventions, flake8_logging_format, flake8_pie,
    flake8_print, flake8_pyi, flake8_pytest_style, flake8_raise, flake8_return, flake8_self,
    flake8_simplify, flake8_tidy_imports, flake8_type_checking, flake8_unused_arguments,
    flake8_use_pathlib, flynt, mccabe, numpy, pandas_vet, pep8_naming, pycodestyle, pydocstyle,
    pyflakes, pygrep_hooks, pylint, pyupgrade, ruff, tryceratops,
};
use crate::settings::types::PythonVersion;
use crate::settings::{flags, Settings};
use crate::{autofix, docstrings, noqa, warn_user};

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
    pub(crate) ctx: Context<'a>,
    pub(crate) diagnostics: Vec<Diagnostic>,
    pub(crate) deletions: FxHashSet<RefEquality<'a, Stmt>>,
    deferred: Deferred<'a>,
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
            ctx: Context::new(&settings.typing_modules, path, module),
            deferred: Deferred::default(),
            diagnostics: Vec::default(),
            deletions: FxHashSet::default(),
            flake8_bugbear_seen: Vec::default(),
        }
    }
}

impl<'a> Checker<'a> {
    /// Return `true` if a patch should be generated under the given autofix
    /// `Mode`.
    pub(crate) fn patch(&self, code: Rule) -> bool {
        self.settings.rules.should_fix(code)
    }

    /// Return `true` if a `Rule` is disabled by a `noqa` directive.
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
        fn quote_style(context: &Context, locator: &Locator, indexer: &Indexer) -> Option<Quote> {
            if !context.in_f_string() {
                return None;
            }

            // Find the quote character used to start the containing f-string.
            let expr = context.expr()?;
            let string_range = indexer.f_string_range(expr.start())?;
            let trailing_quote = trailing_quote(locator.slice(string_range))?;

            // Invert the quote character, if it's a single quote.
            match *trailing_quote {
                "'" => Some(Quote::Double),
                "\"" => Some(Quote::Single),
                _ => None,
            }
        }

        Generator::new(
            self.stylist.indentation(),
            quote_style(&self.ctx, self.locator, self.indexer).unwrap_or(self.stylist.quote()),
            self.stylist.line_ending(),
        )
    }
}

impl<'a, 'b> Visitor<'b> for Checker<'a>
where
    'b: 'a,
{
    fn visit_stmt(&mut self, stmt: &'b Stmt) {
        self.ctx.push_stmt(stmt);

        // Track whether we've seen docstrings, non-imports, etc.
        match stmt {
            Stmt::ImportFrom(ast::StmtImportFrom { module, names, .. }) => {
                // Allow __future__ imports until we see a non-__future__ import.
                if let Some("__future__") = module.as_deref() {
                    if names
                        .iter()
                        .any(|alias| alias.name.as_str() == "annotations")
                    {
                        self.ctx.flags |= ContextFlags::FUTURE_ANNOTATIONS;
                    }
                } else {
                    self.ctx.flags |= ContextFlags::FUTURES_BOUNDARY;
                }
            }
            Stmt::Import(_) => {
                self.ctx.flags |= ContextFlags::FUTURES_BOUNDARY;
            }
            _ => {
                self.ctx.flags |= ContextFlags::FUTURES_BOUNDARY;
                if !self.ctx.seen_import_boundary()
                    && !helpers::is_assignment_to_a_dunder(stmt)
                    && !helpers::in_nested_block(self.ctx.parents())
                {
                    self.ctx.flags |= ContextFlags::IMPORT_BOUNDARY;
                }
            }
        }

        // Track each top-level import, to guide import insertions.
        if matches!(stmt, Stmt::Import(_) | Stmt::ImportFrom(_)) {
            if self.ctx.at_top_level() {
                self.importer.visit_import(stmt);
            }
        }

        // Store the flags prior to any further descent, so that we can restore them after visiting
        // the node.
        let flags_snapshot = self.ctx.flags;

        // Pre-visit.
        match stmt {
            Stmt::Global(ast::StmtGlobal { names, range: _ }) => {
                let ranges: Vec<TextRange> = helpers::find_names(stmt, self.locator).collect();
                if !self.ctx.scope_id.is_global() {
                    // Add the binding to the current scope.
                    let context = self.ctx.execution_context();
                    let exceptions = self.ctx.exceptions();
                    let scope = &mut self.ctx.scopes[self.ctx.scope_id];
                    let usage = Some((self.ctx.scope_id, stmt.range()));
                    for (name, range) in names.iter().zip(ranges.iter()) {
                        let id = self.ctx.bindings.push(Binding {
                            kind: BindingKind::Global,
                            runtime_usage: None,
                            synthetic_usage: usage,
                            typing_usage: None,
                            range: *range,
                            source: self.ctx.stmt_id,
                            context,
                            exceptions,
                        });
                        scope.add(name, id);
                    }
                }

                if self.settings.rules.enabled(Rule::AmbiguousVariableName) {
                    self.diagnostics
                        .extend(names.iter().zip(ranges.iter()).filter_map(|(name, range)| {
                            pycodestyle::rules::ambiguous_variable_name(name, *range)
                        }));
                }
            }
            Stmt::Nonlocal(ast::StmtNonlocal { names, range: _ }) => {
                let ranges: Vec<TextRange> = helpers::find_names(stmt, self.locator).collect();
                if !self.ctx.scope_id.is_global() {
                    let context = self.ctx.execution_context();
                    let exceptions = self.ctx.exceptions();
                    let scope = &mut self.ctx.scopes[self.ctx.scope_id];
                    let usage = Some((self.ctx.scope_id, stmt.range()));
                    for (name, range) in names.iter().zip(ranges.iter()) {
                        // Add a binding to the current scope.
                        let id = self.ctx.bindings.push(Binding {
                            kind: BindingKind::Nonlocal,
                            runtime_usage: None,
                            synthetic_usage: usage,
                            typing_usage: None,
                            range: *range,
                            source: self.ctx.stmt_id,
                            context,
                            exceptions,
                        });
                        scope.add(name, id);
                    }

                    // Mark the binding in the defining scopes as used too. (Skip the global scope
                    // and the current scope.)
                    for (name, range) in names.iter().zip(ranges.iter()) {
                        let binding_id = self
                            .ctx
                            .scopes
                            .ancestors(self.ctx.scope_id)
                            .skip(1)
                            .take_while(|scope| !scope.kind.is_module())
                            .find_map(|scope| scope.get(name.as_str()));

                        if let Some(binding_id) = binding_id {
                            self.ctx.bindings[*binding_id].runtime_usage = usage;
                        } else {
                            // Ensure that every nonlocal has an existing binding from a parent scope.
                            if self.settings.rules.enabled(Rule::NonlocalWithoutBinding) {
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

                if self.settings.rules.enabled(Rule::AmbiguousVariableName) {
                    self.diagnostics
                        .extend(names.iter().zip(ranges.iter()).filter_map(|(name, range)| {
                            pycodestyle::rules::ambiguous_variable_name(name, *range)
                        }));
                }
            }
            Stmt::Break(_) => {
                if self.settings.rules.enabled(Rule::BreakOutsideLoop) {
                    if let Some(diagnostic) =
                        pyflakes::rules::break_outside_loop(stmt, &mut self.ctx.parents().skip(1))
                    {
                        self.diagnostics.push(diagnostic);
                    }
                }
            }
            Stmt::Continue(_) => {
                if self.settings.rules.enabled(Rule::ContinueOutsideLoop) {
                    if let Some(diagnostic) = pyflakes::rules::continue_outside_loop(
                        stmt,
                        &mut self.ctx.parents().skip(1),
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
                if self
                    .settings
                    .rules
                    .enabled(Rule::DjangoNonLeadingReceiverDecorator)
                {
                    self.diagnostics
                        .extend(flake8_django::rules::non_leading_receiver_decorator(
                            decorator_list,
                            |expr| self.ctx.resolve_call_path(expr),
                        ));
                }

                if self.settings.rules.enabled(Rule::AmbiguousFunctionName) {
                    if let Some(diagnostic) =
                        pycodestyle::rules::ambiguous_function_name(name, || {
                            helpers::identifier_range(stmt, self.locator)
                        })
                    {
                        self.diagnostics.push(diagnostic);
                    }
                }

                if self.settings.rules.enabled(Rule::InvalidFunctionName) {
                    if let Some(diagnostic) = pep8_naming::rules::invalid_function_name(
                        stmt,
                        name,
                        decorator_list,
                        &self.settings.pep8_naming.ignore_names,
                        &self.ctx,
                        self.locator,
                    ) {
                        self.diagnostics.push(diagnostic);
                    }
                }

                if self
                    .settings
                    .rules
                    .enabled(Rule::InvalidFirstArgumentNameForClassMethod)
                {
                    if let Some(diagnostic) =
                        pep8_naming::rules::invalid_first_argument_name_for_class_method(
                            self,
                            self.ctx.scope(),
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
                    .enabled(Rule::InvalidFirstArgumentNameForMethod)
                {
                    if let Some(diagnostic) =
                        pep8_naming::rules::invalid_first_argument_name_for_method(
                            self,
                            self.ctx.scope(),
                            name,
                            decorator_list,
                            args,
                        )
                    {
                        self.diagnostics.push(diagnostic);
                    }
                }

                if self.is_stub {
                    if self.settings.rules.enabled(Rule::PassStatementStubBody) {
                        flake8_pyi::rules::pass_statement_stub_body(self, body);
                    }
                    if self.settings.rules.enabled(Rule::NonEmptyStubBody) {
                        flake8_pyi::rules::non_empty_stub_body(self, body);
                    }
                }

                if self.settings.rules.enabled(Rule::DunderFunctionName) {
                    if let Some(diagnostic) = pep8_naming::rules::dunder_function_name(
                        self.ctx.scope(),
                        stmt,
                        name,
                        self.locator,
                    ) {
                        self.diagnostics.push(diagnostic);
                    }
                }

                if self.settings.rules.enabled(Rule::GlobalStatement) {
                    pylint::rules::global_statement(self, name);
                }

                if self.settings.rules.enabled(Rule::LRUCacheWithoutParameters)
                    && self.settings.target_version >= PythonVersion::Py38
                {
                    pyupgrade::rules::lru_cache_without_parameters(self, decorator_list);
                }
                if self.settings.rules.enabled(Rule::LRUCacheWithMaxsizeNone)
                    && self.settings.target_version >= PythonVersion::Py39
                {
                    pyupgrade::rules::lru_cache_with_maxsize_none(self, decorator_list);
                }

                if self.settings.rules.enabled(Rule::CachedInstanceMethod) {
                    flake8_bugbear::rules::cached_instance_method(self, decorator_list);
                }

                if self.settings.rules.any_enabled(&[
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

                if self.settings.rules.enabled(Rule::UselessReturn) {
                    pylint::rules::useless_return(
                        self,
                        stmt,
                        body,
                        returns.as_ref().map(|expr| &**expr),
                    );
                }

                if self.settings.rules.enabled(Rule::ComplexStructure) {
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

                if self.settings.rules.enabled(Rule::HardcodedPasswordDefault) {
                    self.diagnostics
                        .extend(flake8_bandit::rules::hardcoded_password_default(args));
                }

                if self.settings.rules.enabled(Rule::PropertyWithParameters) {
                    pylint::rules::property_with_parameters(self, stmt, decorator_list, args);
                }

                if self.settings.rules.enabled(Rule::TooManyArguments) {
                    pylint::rules::too_many_arguments(self, args, stmt);
                }

                if self.settings.rules.enabled(Rule::TooManyReturnStatements) {
                    if let Some(diagnostic) = pylint::rules::too_many_return_statements(
                        stmt,
                        body,
                        self.settings.pylint.max_returns,
                        self.locator,
                    ) {
                        self.diagnostics.push(diagnostic);
                    }
                }

                if self.settings.rules.enabled(Rule::TooManyBranches) {
                    if let Some(diagnostic) = pylint::rules::too_many_branches(
                        stmt,
                        body,
                        self.settings.pylint.max_branches,
                        self.locator,
                    ) {
                        self.diagnostics.push(diagnostic);
                    }
                }

                if self.settings.rules.enabled(Rule::TooManyStatements) {
                    if let Some(diagnostic) = pylint::rules::too_many_statements(
                        stmt,
                        body,
                        self.settings.pylint.max_statements,
                        self.locator,
                    ) {
                        self.diagnostics.push(diagnostic);
                    }
                }

                if self.settings.rules.any_enabled(&[
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

                if self.settings.rules.any_enabled(&[
                    Rule::PytestParametrizeNamesWrongType,
                    Rule::PytestParametrizeValuesWrongType,
                ]) {
                    flake8_pytest_style::rules::parametrize(self, decorator_list);
                }

                if self.settings.rules.any_enabled(&[
                    Rule::PytestIncorrectMarkParenthesesStyle,
                    Rule::PytestUseFixturesWithoutParameters,
                ]) {
                    flake8_pytest_style::rules::marks(self, decorator_list);
                }

                if self
                    .settings
                    .rules
                    .enabled(Rule::BooleanPositionalArgInFunctionDefinition)
                {
                    flake8_boolean_trap::rules::check_positional_boolean_in_def(
                        self,
                        name,
                        decorator_list,
                        args,
                    );
                }

                if self
                    .settings
                    .rules
                    .enabled(Rule::BooleanDefaultValueInFunctionDefinition)
                {
                    flake8_boolean_trap::rules::check_boolean_default_value_in_function_definition(
                        self,
                        name,
                        decorator_list,
                        args,
                    );
                }

                if self
                    .settings
                    .rules
                    .enabled(Rule::UnexpectedSpecialMethodSignature)
                {
                    pylint::rules::unexpected_special_method_signature(
                        self,
                        stmt,
                        name,
                        decorator_list,
                        args,
                        self.locator,
                    );
                }

                if self.settings.rules.enabled(Rule::FStringDocstring) {
                    flake8_bugbear::rules::f_string_docstring(self, body);
                }

                if self.settings.rules.enabled(Rule::YieldInForLoop) {
                    pyupgrade::rules::yield_in_for_loop(self, stmt);
                }

                if self.ctx.scope().kind.is_class() {
                    if self.settings.rules.enabled(Rule::BuiltinAttributeShadowing) {
                        flake8_builtins::rules::builtin_attribute_shadowing(self, name, stmt);
                    }
                } else {
                    if self.settings.rules.enabled(Rule::BuiltinVariableShadowing) {
                        flake8_builtins::rules::builtin_variable_shadowing(self, name, stmt);
                    }
                }
            }
            Stmt::Return(_) => {
                if self.settings.rules.enabled(Rule::ReturnOutsideFunction) {
                    pyflakes::rules::return_outside_function(self, stmt);
                }
                if self.settings.rules.enabled(Rule::ReturnInInit) {
                    pylint::rules::return_in_init(self, stmt);
                }
            }
            Stmt::ClassDef(ast::StmtClassDef {
                name,
                bases,
                keywords,
                decorator_list,
                body,
                range: _,
            }) => {
                if self
                    .settings
                    .rules
                    .enabled(Rule::DjangoNullableModelStringField)
                {
                    self.diagnostics
                        .extend(flake8_django::rules::nullable_model_string_field(
                            self, body,
                        ));
                }

                if self
                    .settings
                    .rules
                    .enabled(Rule::DjangoExcludeWithModelForm)
                {
                    if let Some(diagnostic) =
                        flake8_django::rules::exclude_with_model_form(self, bases, body)
                    {
                        self.diagnostics.push(diagnostic);
                    }
                }
                if self.settings.rules.enabled(Rule::DjangoAllWithModelForm) {
                    if let Some(diagnostic) =
                        flake8_django::rules::all_with_model_form(self, bases, body)
                    {
                        self.diagnostics.push(diagnostic);
                    }
                }
                if self
                    .settings
                    .rules
                    .enabled(Rule::DjangoModelWithoutDunderStr)
                {
                    if let Some(diagnostic) =
                        flake8_django::rules::model_without_dunder_str(self, bases, body, stmt)
                    {
                        self.diagnostics.push(diagnostic);
                    }
                }
                if self
                    .settings
                    .rules
                    .enabled(Rule::DjangoUnorderedBodyContentInModel)
                {
                    flake8_django::rules::unordered_body_content_in_model(self, bases, body);
                }
                if self.settings.rules.enabled(Rule::GlobalStatement) {
                    pylint::rules::global_statement(self, name);
                }
                if self.settings.rules.enabled(Rule::UselessObjectInheritance) {
                    pyupgrade::rules::useless_object_inheritance(self, stmt, name, bases, keywords);
                }

                if self.settings.rules.enabled(Rule::AmbiguousClassName) {
                    if let Some(diagnostic) = pycodestyle::rules::ambiguous_class_name(name, || {
                        helpers::identifier_range(stmt, self.locator)
                    }) {
                        self.diagnostics.push(diagnostic);
                    }
                }

                if self.settings.rules.enabled(Rule::InvalidClassName) {
                    if let Some(diagnostic) =
                        pep8_naming::rules::invalid_class_name(stmt, name, self.locator)
                    {
                        self.diagnostics.push(diagnostic);
                    }
                }

                if self
                    .settings
                    .rules
                    .enabled(Rule::ErrorSuffixOnExceptionName)
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

                if !self.is_stub {
                    if self.settings.rules.any_enabled(&[
                        Rule::AbstractBaseClassWithoutAbstractMethod,
                        Rule::EmptyMethodWithoutAbstractDecorator,
                    ]) {
                        flake8_bugbear::rules::abstract_base_class(
                            self, stmt, name, bases, keywords, body,
                        );
                    }
                }
                if self.is_stub {
                    if self.settings.rules.enabled(Rule::PassStatementStubBody) {
                        flake8_pyi::rules::pass_statement_stub_body(self, body);
                    }
                    if self.settings.rules.enabled(Rule::PassInClassBody) {
                        flake8_pyi::rules::pass_in_class_body(self, stmt, body);
                    }
                }

                if self
                    .settings
                    .rules
                    .enabled(Rule::PytestIncorrectMarkParenthesesStyle)
                {
                    flake8_pytest_style::rules::marks(self, decorator_list);
                }

                if self
                    .settings
                    .rules
                    .enabled(Rule::DuplicateClassFieldDefinition)
                {
                    flake8_pie::rules::duplicate_class_field_definition(self, stmt, body);
                }

                if self.settings.rules.enabled(Rule::NonUniqueEnums) {
                    flake8_pie::rules::non_unique_enums(self, stmt, body);
                }

                if self.settings.rules.any_enabled(&[
                    Rule::MutableDataclassDefault,
                    Rule::FunctionCallInDataclassDefaultArgument,
                ]) && ruff::rules::is_dataclass(self, decorator_list)
                {
                    if self.settings.rules.enabled(Rule::MutableDataclassDefault) {
                        ruff::rules::mutable_dataclass_default(self, body);
                    }

                    if self
                        .settings
                        .rules
                        .enabled(Rule::FunctionCallInDataclassDefaultArgument)
                    {
                        ruff::rules::function_call_in_dataclass_defaults(self, body);
                    }
                }

                if self.settings.rules.enabled(Rule::FStringDocstring) {
                    flake8_bugbear::rules::f_string_docstring(self, body);
                }

                if self.settings.rules.enabled(Rule::BuiltinVariableShadowing) {
                    flake8_builtins::rules::builtin_variable_shadowing(self, name, stmt);
                }

                if self.settings.rules.enabled(Rule::DuplicateBases) {
                    pylint::rules::duplicate_bases(self, name, bases);
                }
            }
            Stmt::Import(ast::StmtImport { names, range: _ }) => {
                if self.settings.rules.enabled(Rule::MultipleImportsOnOneLine) {
                    pycodestyle::rules::multiple_imports_on_one_line(self, stmt, names);
                }
                if self
                    .settings
                    .rules
                    .enabled(Rule::ModuleImportNotAtTopOfFile)
                {
                    pycodestyle::rules::module_import_not_at_top_of_file(self, stmt, self.locator);
                }

                if self.settings.rules.enabled(Rule::GlobalStatement) {
                    for name in names.iter() {
                        if let Some(asname) = name.asname.as_ref() {
                            pylint::rules::global_statement(self, asname);
                        } else {
                            pylint::rules::global_statement(self, &name.name);
                        }
                    }
                }

                if self.settings.rules.enabled(Rule::DeprecatedCElementTree) {
                    pyupgrade::rules::deprecated_c_element_tree(self, stmt);
                }
                if self.settings.rules.enabled(Rule::DeprecatedMockImport) {
                    pyupgrade::rules::deprecated_mock_import(self, stmt);
                }

                for alias in names {
                    if &alias.name == "__future__" {
                        let name = alias.asname.as_ref().unwrap_or(&alias.name);
                        self.add_binding(
                            name,
                            Binding {
                                kind: BindingKind::FutureImportation,
                                runtime_usage: None,
                                // Always mark `__future__` imports as used.
                                synthetic_usage: Some((self.ctx.scope_id, alias.range())),
                                typing_usage: None,
                                range: alias.range(),
                                source: self.ctx.stmt_id,
                                context: self.ctx.execution_context(),
                                exceptions: self.ctx.exceptions(),
                            },
                        );

                        if self.settings.rules.enabled(Rule::LateFutureImport) {
                            if self.ctx.seen_futures_boundary() {
                                self.diagnostics.push(Diagnostic::new(
                                    pyflakes::rules::LateFutureImport,
                                    stmt.range(),
                                ));
                            }
                        }
                    } else if alias.name.contains('.') && alias.asname.is_none() {
                        // Given `import foo.bar`, `name` would be "foo", and `full_name` would be
                        // "foo.bar".
                        let name = alias.name.split('.').next().unwrap();
                        let full_name = &alias.name;
                        self.add_binding(
                            name,
                            Binding {
                                kind: BindingKind::SubmoduleImportation(SubmoduleImportation {
                                    name,
                                    full_name,
                                }),
                                runtime_usage: None,
                                synthetic_usage: None,
                                typing_usage: None,
                                range: alias.range(),
                                source: self.ctx.stmt_id,
                                context: self.ctx.execution_context(),
                                exceptions: self.ctx.exceptions(),
                            },
                        );
                    } else {
                        // Treat explicit re-export as usage (e.g., `from .applications
                        // import FastAPI as FastAPI`).
                        let is_explicit_reexport = alias
                            .asname
                            .as_ref()
                            .map_or(false, |asname| asname == &alias.name);

                        let name = alias.asname.as_ref().unwrap_or(&alias.name);
                        let full_name = &alias.name;
                        self.add_binding(
                            name,
                            Binding {
                                kind: BindingKind::Importation(Importation { name, full_name }),
                                runtime_usage: None,
                                synthetic_usage: if is_explicit_reexport {
                                    Some((self.ctx.scope_id, alias.range()))
                                } else {
                                    None
                                },
                                typing_usage: None,
                                range: alias.range(),
                                source: self.ctx.stmt_id,
                                context: self.ctx.execution_context(),
                                exceptions: self.ctx.exceptions(),
                            },
                        );

                        if let Some(asname) = &alias.asname {
                            if self.settings.rules.enabled(Rule::BuiltinVariableShadowing) {
                                flake8_builtins::rules::builtin_variable_shadowing(
                                    self, asname, stmt,
                                );
                            }
                        }
                    }

                    // flake8-debugger
                    if self.settings.rules.enabled(Rule::Debugger) {
                        if let Some(diagnostic) =
                            flake8_debugger::rules::debugger_import(stmt, None, &alias.name)
                        {
                            self.diagnostics.push(diagnostic);
                        }
                    }

                    // flake8_tidy_imports
                    if self.settings.rules.enabled(Rule::BannedApi) {
                        flake8_tidy_imports::banned_api::name_or_parent_is_banned(
                            self,
                            &alias.name,
                            alias,
                        );
                    }

                    // pylint
                    if !self.is_stub {
                        if self.settings.rules.enabled(Rule::UselessImportAlias) {
                            pylint::rules::useless_import_alias(self, alias);
                        }
                    }
                    if self.settings.rules.enabled(Rule::ManualFromImport) {
                        pylint::rules::manual_from_import(self, stmt, alias, names);
                    }
                    if self.settings.rules.enabled(Rule::ImportSelf) {
                        if let Some(diagnostic) =
                            pylint::rules::import_self(alias, self.module_path)
                        {
                            self.diagnostics.push(diagnostic);
                        }
                    }

                    if let Some(asname) = &alias.asname {
                        let name = alias.name.split('.').last().unwrap();
                        if self
                            .settings
                            .rules
                            .enabled(Rule::ConstantImportedAsNonConstant)
                        {
                            if let Some(diagnostic) =
                                pep8_naming::rules::constant_imported_as_non_constant(
                                    name, asname, alias, stmt,
                                )
                            {
                                self.diagnostics.push(diagnostic);
                            }
                        }

                        if self
                            .settings
                            .rules
                            .enabled(Rule::LowercaseImportedAsNonLowercase)
                        {
                            if let Some(diagnostic) =
                                pep8_naming::rules::lowercase_imported_as_non_lowercase(
                                    name, asname, alias, stmt,
                                )
                            {
                                self.diagnostics.push(diagnostic);
                            }
                        }

                        if self
                            .settings
                            .rules
                            .enabled(Rule::CamelcaseImportedAsLowercase)
                        {
                            if let Some(diagnostic) =
                                pep8_naming::rules::camelcase_imported_as_lowercase(
                                    name, asname, alias, stmt,
                                )
                            {
                                self.diagnostics.push(diagnostic);
                            }
                        }

                        if self
                            .settings
                            .rules
                            .enabled(Rule::CamelcaseImportedAsConstant)
                        {
                            if let Some(diagnostic) =
                                pep8_naming::rules::camelcase_imported_as_constant(
                                    name, asname, alias, stmt,
                                )
                            {
                                self.diagnostics.push(diagnostic);
                            }
                        }

                        if self
                            .settings
                            .rules
                            .enabled(Rule::CamelcaseImportedAsAcronym)
                        {
                            if let Some(diagnostic) =
                                pep8_naming::rules::camelcase_imported_as_acronym(
                                    name, asname, alias, stmt,
                                )
                            {
                                self.diagnostics.push(diagnostic);
                            }
                        }
                    }

                    if self.settings.rules.enabled(Rule::UnconventionalImportAlias) {
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

                    if self.settings.rules.enabled(Rule::BannedImportAlias) {
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

                    if self
                        .settings
                        .rules
                        .enabled(Rule::PytestIncorrectPytestImport)
                    {
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
            Stmt::ImportFrom(ast::StmtImportFrom {
                names,
                module,
                level,
                range: _,
            }) => {
                let module = module.as_deref();
                let level = level.map(|level| level.to_u32());
                if self
                    .settings
                    .rules
                    .enabled(Rule::ModuleImportNotAtTopOfFile)
                {
                    pycodestyle::rules::module_import_not_at_top_of_file(self, stmt, self.locator);
                }

                if self.settings.rules.enabled(Rule::GlobalStatement) {
                    for name in names.iter() {
                        if let Some(asname) = name.asname.as_ref() {
                            pylint::rules::global_statement(self, asname);
                        } else {
                            pylint::rules::global_statement(self, &name.name);
                        }
                    }
                }

                if self.settings.rules.enabled(Rule::UnnecessaryFutureImport)
                    && self.settings.target_version >= PythonVersion::Py37
                {
                    if let Some("__future__") = module {
                        pyupgrade::rules::unnecessary_future_import(self, stmt, names);
                    }
                }
                if self.settings.rules.enabled(Rule::DeprecatedMockImport) {
                    pyupgrade::rules::deprecated_mock_import(self, stmt);
                }
                if self.settings.rules.enabled(Rule::DeprecatedCElementTree) {
                    pyupgrade::rules::deprecated_c_element_tree(self, stmt);
                }
                if self.settings.rules.enabled(Rule::DeprecatedImport) {
                    pyupgrade::rules::deprecated_import(self, stmt, names, module, level);
                }
                if self.settings.rules.enabled(Rule::UnnecessaryBuiltinImport) {
                    if let Some(module) = module {
                        pyupgrade::rules::unnecessary_builtin_import(self, stmt, module, names);
                    }
                }
                if self.settings.rules.enabled(Rule::BannedApi) {
                    if let Some(module) =
                        helpers::resolve_imported_module_path(level, module, self.module_path)
                    {
                        flake8_tidy_imports::banned_api::name_or_parent_is_banned(
                            self, &module, stmt,
                        );

                        for alias in names {
                            if &alias.name == "*" {
                                continue;
                            }
                            flake8_tidy_imports::banned_api::name_is_banned(
                                self,
                                format!("{module}.{}", alias.name),
                                alias,
                            );
                        }
                    }
                }

                if self
                    .settings
                    .rules
                    .enabled(Rule::PytestIncorrectPytestImport)
                {
                    if let Some(diagnostic) =
                        flake8_pytest_style::rules::import_from(stmt, module, level)
                    {
                        self.diagnostics.push(diagnostic);
                    }
                }

                for alias in names {
                    if let Some("__future__") = module {
                        let name = alias.asname.as_ref().unwrap_or(&alias.name);
                        self.add_binding(
                            name,
                            Binding {
                                kind: BindingKind::FutureImportation,
                                runtime_usage: None,
                                // Always mark `__future__` imports as used.
                                synthetic_usage: Some((self.ctx.scope_id, alias.range())),
                                typing_usage: None,
                                range: alias.range(),
                                source: self.ctx.stmt_id,
                                context: self.ctx.execution_context(),
                                exceptions: self.ctx.exceptions(),
                            },
                        );

                        if self.settings.rules.enabled(Rule::FutureFeatureNotDefined) {
                            pyflakes::rules::future_feature_not_defined(self, alias);
                        }

                        if self.settings.rules.enabled(Rule::LateFutureImport) {
                            if self.ctx.seen_futures_boundary() {
                                self.diagnostics.push(Diagnostic::new(
                                    pyflakes::rules::LateFutureImport,
                                    stmt.range(),
                                ));
                            }
                        }
                    } else if &alias.name == "*" {
                        self.ctx
                            .scope_mut()
                            .add_star_import(StarImportation { level, module });

                        if self
                            .settings
                            .rules
                            .enabled(Rule::UndefinedLocalWithNestedImportStarUsage)
                        {
                            let scope = self.ctx.scope();
                            if !matches!(scope.kind, ScopeKind::Module) {
                                self.diagnostics.push(Diagnostic::new(
                                    pyflakes::rules::UndefinedLocalWithNestedImportStarUsage {
                                        name: helpers::format_import_from(level, module),
                                    },
                                    stmt.range(),
                                ));
                            }
                        }

                        if self
                            .settings
                            .rules
                            .enabled(Rule::UndefinedLocalWithImportStar)
                        {
                            self.diagnostics.push(Diagnostic::new(
                                pyflakes::rules::UndefinedLocalWithImportStar {
                                    name: helpers::format_import_from(level, module),
                                },
                                stmt.range(),
                            ));
                        }
                    } else {
                        if let Some(asname) = &alias.asname {
                            if self.settings.rules.enabled(Rule::BuiltinVariableShadowing) {
                                flake8_builtins::rules::builtin_variable_shadowing(
                                    self, asname, stmt,
                                );
                            }
                        }

                        // Treat explicit re-export as usage (e.g., `from .applications
                        // import FastAPI as FastAPI`).
                        let is_explicit_reexport = alias
                            .asname
                            .as_ref()
                            .map_or(false, |asname| asname == &alias.name);

                        // Given `from foo import bar`, `name` would be "bar" and `full_name` would
                        // be "foo.bar". Given `from foo import bar as baz`, `name` would be "baz"
                        // and `full_name` would be "foo.bar".
                        let name = alias.asname.as_ref().unwrap_or(&alias.name);
                        let full_name =
                            helpers::format_import_from_member(level, module, &alias.name);
                        self.add_binding(
                            name,
                            Binding {
                                kind: BindingKind::FromImportation(FromImportation {
                                    name,
                                    full_name,
                                }),
                                runtime_usage: None,
                                synthetic_usage: if is_explicit_reexport {
                                    Some((self.ctx.scope_id, alias.range()))
                                } else {
                                    None
                                },
                                typing_usage: None,
                                range: alias.range(),
                                source: self.ctx.stmt_id,
                                context: self.ctx.execution_context(),
                                exceptions: self.ctx.exceptions(),
                            },
                        );
                    }

                    if self.settings.rules.enabled(Rule::RelativeImports) {
                        if let Some(diagnostic) =
                            flake8_tidy_imports::relative_imports::banned_relative_import(
                                self,
                                stmt,
                                level,
                                module,
                                self.module_path,
                                self.settings.flake8_tidy_imports.ban_relative_imports,
                            )
                        {
                            self.diagnostics.push(diagnostic);
                        }
                    }

                    // flake8-debugger
                    if self.settings.rules.enabled(Rule::Debugger) {
                        if let Some(diagnostic) =
                            flake8_debugger::rules::debugger_import(stmt, module, &alias.name)
                        {
                            self.diagnostics.push(diagnostic);
                        }
                    }

                    if self.settings.rules.enabled(Rule::UnconventionalImportAlias) {
                        let full_name =
                            helpers::format_import_from_member(level, module, &alias.name);
                        if let Some(diagnostic) =
                            flake8_import_conventions::rules::conventional_import_alias(
                                stmt,
                                &full_name,
                                alias.asname.as_deref(),
                                &self.settings.flake8_import_conventions.aliases,
                            )
                        {
                            self.diagnostics.push(diagnostic);
                        }
                    }

                    if self.settings.rules.enabled(Rule::BannedImportAlias) {
                        if let Some(asname) = &alias.asname {
                            let full_name =
                                helpers::format_import_from_member(level, module, &alias.name);
                            if let Some(diagnostic) =
                                flake8_import_conventions::rules::banned_import_alias(
                                    stmt,
                                    &full_name,
                                    asname,
                                    &self.settings.flake8_import_conventions.banned_aliases,
                                )
                            {
                                self.diagnostics.push(diagnostic);
                            }
                        }
                    }

                    if let Some(asname) = &alias.asname {
                        if self
                            .settings
                            .rules
                            .enabled(Rule::ConstantImportedAsNonConstant)
                        {
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

                        if self
                            .settings
                            .rules
                            .enabled(Rule::LowercaseImportedAsNonLowercase)
                        {
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

                        if self
                            .settings
                            .rules
                            .enabled(Rule::CamelcaseImportedAsLowercase)
                        {
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

                        if self
                            .settings
                            .rules
                            .enabled(Rule::CamelcaseImportedAsConstant)
                        {
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

                        if self
                            .settings
                            .rules
                            .enabled(Rule::CamelcaseImportedAsAcronym)
                        {
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
                            if self.settings.rules.enabled(Rule::UselessImportAlias) {
                                pylint::rules::useless_import_alias(self, alias);
                            }
                        }
                    }
                }

                if self.settings.rules.enabled(Rule::ImportSelf) {
                    if let Some(diagnostic) =
                        pylint::rules::import_from_self(level, module, names, self.module_path)
                    {
                        self.diagnostics.push(diagnostic);
                    }
                }

                if self.settings.rules.enabled(Rule::BannedImportFrom) {
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
                if self.settings.rules.enabled(Rule::RaiseNotImplemented) {
                    if let Some(expr) = exc {
                        pyflakes::rules::raise_not_implemented(self, expr);
                    }
                }
                if self.settings.rules.enabled(Rule::CannotRaiseLiteral) {
                    if let Some(exc) = exc {
                        flake8_bugbear::rules::cannot_raise_literal(self, exc);
                    }
                }
                if self.settings.rules.any_enabled(&[
                    Rule::RawStringInException,
                    Rule::FStringInException,
                    Rule::DotFormatInException,
                ]) {
                    if let Some(exc) = exc {
                        flake8_errmsg::rules::string_in_exception(self, stmt, exc);
                    }
                }
                if self.settings.rules.enabled(Rule::OSErrorAlias) {
                    if let Some(item) = exc {
                        pyupgrade::rules::os_error_alias_raise(self, item);
                    }
                }
                if self.settings.rules.enabled(Rule::RaiseVanillaClass) {
                    if let Some(expr) = exc {
                        tryceratops::rules::raise_vanilla_class(self, expr);
                    }
                }
                if self.settings.rules.enabled(Rule::RaiseVanillaArgs) {
                    if let Some(expr) = exc {
                        tryceratops::rules::raise_vanilla_args(self, expr);
                    }
                }
                if self
                    .settings
                    .rules
                    .enabled(Rule::UnnecessaryParenOnRaiseException)
                {
                    if let Some(expr) = exc {
                        flake8_raise::rules::unnecessary_paren_on_raise_exception(self, expr);
                    }
                }
            }
            Stmt::AugAssign(ast::StmtAugAssign { target, .. }) => {
                self.handle_node_load(target);

                if self.settings.rules.enabled(Rule::GlobalStatement) {
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
                if self.settings.rules.enabled(Rule::IfTuple) {
                    pyflakes::rules::if_tuple(self, stmt, test);
                }
                if self.settings.rules.enabled(Rule::CollapsibleIf) {
                    flake8_simplify::rules::nested_if_statements(
                        self,
                        stmt,
                        test,
                        body,
                        orelse,
                        self.ctx.stmt_parent(),
                    );
                }
                if self.settings.rules.enabled(Rule::IfWithSameArms) {
                    flake8_simplify::rules::if_with_same_arms(self, stmt, self.ctx.stmt_parent());
                }
                if self.settings.rules.enabled(Rule::NeedlessBool) {
                    flake8_simplify::rules::needless_bool(self, stmt);
                }
                if self
                    .settings
                    .rules
                    .enabled(Rule::IfElseBlockInsteadOfDictLookup)
                {
                    flake8_simplify::rules::manual_dict_lookup(
                        self,
                        stmt,
                        test,
                        body,
                        orelse,
                        self.ctx.stmt_parent(),
                    );
                }
                if self.settings.rules.enabled(Rule::IfElseBlockInsteadOfIfExp) {
                    flake8_simplify::rules::use_ternary_operator(
                        self,
                        stmt,
                        self.ctx.stmt_parent(),
                    );
                }
                if self
                    .settings
                    .rules
                    .enabled(Rule::IfElseBlockInsteadOfDictGet)
                {
                    flake8_simplify::rules::use_dict_get_with_default(
                        self,
                        stmt,
                        test,
                        body,
                        orelse,
                        self.ctx.stmt_parent(),
                    );
                }
                if self.settings.rules.enabled(Rule::TypeCheckWithoutTypeError) {
                    tryceratops::rules::type_check_without_type_error(
                        self,
                        body,
                        test,
                        orelse,
                        self.ctx.stmt_parent(),
                    );
                }
                if self.settings.rules.enabled(Rule::OutdatedVersionBlock) {
                    pyupgrade::rules::outdated_version_block(self, stmt, test, body, orelse);
                }
                if self.settings.rules.enabled(Rule::CollapsibleElseIf) {
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
                if !self.ctx.in_type_checking_block() {
                    if self.settings.rules.enabled(Rule::Assert) {
                        self.diagnostics
                            .push(flake8_bandit::rules::assert_used(stmt));
                    }
                }
                if self.settings.rules.enabled(Rule::AssertTuple) {
                    pyflakes::rules::assert_tuple(self, stmt, test);
                }
                if self.settings.rules.enabled(Rule::AssertFalse) {
                    flake8_bugbear::rules::assert_false(self, stmt, test, msg.as_deref());
                }
                if self.settings.rules.enabled(Rule::PytestAssertAlwaysFalse) {
                    flake8_pytest_style::rules::assert_falsy(self, stmt, test);
                }
                if self.settings.rules.enabled(Rule::PytestCompositeAssertion) {
                    flake8_pytest_style::rules::composite_condition(
                        self,
                        stmt,
                        test,
                        msg.as_deref(),
                    );
                }
                if self.settings.rules.enabled(Rule::AssertOnStringLiteral) {
                    pylint::rules::assert_on_string_literal(self, test);
                }
                if self.settings.rules.enabled(Rule::InvalidMockAccess) {
                    pygrep_hooks::rules::non_existent_mock_method(self, test);
                }
            }
            Stmt::With(ast::StmtWith { items, body, .. }) => {
                if self.settings.rules.enabled(Rule::AssertRaisesException) {
                    flake8_bugbear::rules::assert_raises_exception(self, stmt, items);
                }
                if self
                    .settings
                    .rules
                    .enabled(Rule::PytestRaisesWithMultipleStatements)
                {
                    flake8_pytest_style::rules::complex_raises(self, stmt, items, body);
                }
                if self.settings.rules.enabled(Rule::MultipleWithStatements) {
                    flake8_simplify::rules::multiple_with_statements(
                        self,
                        stmt,
                        body,
                        self.ctx.stmt_parent(),
                    );
                }
                if self.settings.rules.enabled(Rule::RedefinedLoopName) {
                    pylint::rules::redefined_loop_name(self, &Node::Stmt(stmt));
                }
            }
            Stmt::While(ast::StmtWhile { body, orelse, .. }) => {
                if self.settings.rules.enabled(Rule::FunctionUsesLoopVariable) {
                    flake8_bugbear::rules::function_uses_loop_variable(self, &Node::Stmt(stmt));
                }
                if self.settings.rules.enabled(Rule::UselessElseOnLoop) {
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
                if self.settings.rules.enabled(Rule::UnusedLoopControlVariable) {
                    self.deferred.for_loops.push(self.ctx.snapshot());
                }
                if self
                    .settings
                    .rules
                    .enabled(Rule::LoopVariableOverridesIterator)
                {
                    flake8_bugbear::rules::loop_variable_overrides_iterator(self, target, iter);
                }
                if self.settings.rules.enabled(Rule::FunctionUsesLoopVariable) {
                    flake8_bugbear::rules::function_uses_loop_variable(self, &Node::Stmt(stmt));
                }
                if self.settings.rules.enabled(Rule::ReuseOfGroupbyGenerator) {
                    flake8_bugbear::rules::reuse_of_groupby_generator(self, target, body, iter);
                }
                if self.settings.rules.enabled(Rule::UselessElseOnLoop) {
                    pylint::rules::useless_else_on_loop(self, stmt, body, orelse);
                }
                if self.settings.rules.enabled(Rule::RedefinedLoopName) {
                    pylint::rules::redefined_loop_name(self, &Node::Stmt(stmt));
                }
                if matches!(stmt, Stmt::For(_)) {
                    if self.settings.rules.enabled(Rule::ReimplementedBuiltin) {
                        flake8_simplify::rules::convert_for_loop_to_any_all(
                            self,
                            stmt,
                            self.ctx.sibling_stmt(),
                        );
                    }
                    if self.settings.rules.enabled(Rule::InDictKeys) {
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
                if self.settings.rules.enabled(Rule::DefaultExceptNotLast) {
                    if let Some(diagnostic) =
                        pyflakes::rules::default_except_not_last(handlers, self.locator)
                    {
                        self.diagnostics.push(diagnostic);
                    }
                }
                if self.settings.rules.any_enabled(&[
                    Rule::DuplicateHandlerException,
                    Rule::DuplicateTryBlockException,
                ]) {
                    flake8_bugbear::rules::duplicate_exceptions(self, handlers);
                }
                if self
                    .settings
                    .rules
                    .enabled(Rule::RedundantTupleInExceptionHandler)
                {
                    flake8_bugbear::rules::redundant_tuple_in_exception_handler(self, handlers);
                }
                if self.settings.rules.enabled(Rule::OSErrorAlias) {
                    pyupgrade::rules::os_error_alias_handlers(self, handlers);
                }
                if self.settings.rules.enabled(Rule::PytestAssertInExcept) {
                    self.diagnostics.extend(
                        flake8_pytest_style::rules::assert_in_exception_handler(handlers),
                    );
                }
                if self.settings.rules.enabled(Rule::SuppressibleException) {
                    flake8_simplify::rules::suppressible_exception(
                        self, stmt, body, handlers, orelse, finalbody,
                    );
                }
                if self.settings.rules.enabled(Rule::ReturnInTryExceptFinally) {
                    flake8_simplify::rules::return_in_try_except_finally(
                        self, body, handlers, finalbody,
                    );
                }
                if self.settings.rules.enabled(Rule::TryConsiderElse) {
                    tryceratops::rules::try_consider_else(self, body, orelse, handlers);
                }
                if self.settings.rules.enabled(Rule::VerboseRaise) {
                    tryceratops::rules::verbose_raise(self, handlers);
                }
                if self.settings.rules.enabled(Rule::VerboseLogMessage) {
                    tryceratops::rules::verbose_log_message(self, handlers);
                }
                if self.settings.rules.enabled(Rule::RaiseWithinTry) {
                    tryceratops::rules::raise_within_try(self, body, handlers);
                }
                if self.settings.rules.enabled(Rule::UselessTryExcept) {
                    tryceratops::rules::useless_try_except(self, handlers);
                }
                if self.settings.rules.enabled(Rule::ErrorInsteadOfException) {
                    tryceratops::rules::error_instead_of_exception(self, handlers);
                }
            }
            Stmt::Assign(ast::StmtAssign { targets, value, .. }) => {
                if self.settings.rules.enabled(Rule::LambdaAssignment) {
                    if let [target] = &targets[..] {
                        pycodestyle::rules::lambda_assignment(self, target, value, None, stmt);
                    }
                }

                if self.settings.rules.enabled(Rule::AssignmentToOsEnviron) {
                    flake8_bugbear::rules::assignment_to_os_environ(self, targets);
                }

                if self.settings.rules.enabled(Rule::HardcodedPasswordString) {
                    if let Some(diagnostic) =
                        flake8_bandit::rules::assign_hardcoded_password_string(value, targets)
                    {
                        self.diagnostics.push(diagnostic);
                    }
                }

                if self.settings.rules.enabled(Rule::GlobalStatement) {
                    for target in targets.iter() {
                        if let Expr::Name(ast::ExprName { id, .. }) = target {
                            pylint::rules::global_statement(self, id);
                        }
                    }
                }

                if self.settings.rules.enabled(Rule::UselessMetaclassType) {
                    pyupgrade::rules::useless_metaclass_type(self, stmt, value, targets);
                }
                if self
                    .settings
                    .rules
                    .enabled(Rule::ConvertTypedDictFunctionalToClass)
                {
                    pyupgrade::rules::convert_typed_dict_functional_to_class(
                        self, stmt, targets, value,
                    );
                }
                if self
                    .settings
                    .rules
                    .enabled(Rule::ConvertNamedTupleFunctionalToClass)
                {
                    pyupgrade::rules::convert_named_tuple_functional_to_class(
                        self, stmt, targets, value,
                    );
                }
                if self.settings.rules.enabled(Rule::UnpackedListComprehension) {
                    pyupgrade::rules::unpacked_list_comprehension(self, targets, value);
                }

                if self.settings.rules.enabled(Rule::PandasDfVariableName) {
                    if let Some(diagnostic) = pandas_vet::rules::assignment_to_df(targets) {
                        self.diagnostics.push(diagnostic);
                    }
                }

                if self.is_stub {
                    if self.settings.rules.any_enabled(&[
                        Rule::UnprefixedTypeParam,
                        Rule::AssignmentDefaultInStub,
                        Rule::UnannotatedAssignmentInStub,
                    ]) {
                        // Ignore assignments in function bodies; those are covered by other rules.
                        if !self.ctx.scopes().any(|scope| scope.kind.is_function()) {
                            if self.settings.rules.enabled(Rule::UnprefixedTypeParam) {
                                flake8_pyi::rules::prefix_type_params(self, value, targets);
                            }
                            if self.settings.rules.enabled(Rule::AssignmentDefaultInStub) {
                                flake8_pyi::rules::assignment_default_in_stub(self, targets, value);
                            }
                            if self
                                .settings
                                .rules
                                .enabled(Rule::UnannotatedAssignmentInStub)
                            {
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
                if self.settings.rules.enabled(Rule::LambdaAssignment) {
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
                if self
                    .settings
                    .rules
                    .enabled(Rule::UnintentionalTypeAnnotation)
                {
                    flake8_bugbear::rules::unintentional_type_annotation(
                        self,
                        target,
                        value.as_deref(),
                        stmt,
                    );
                }
                if self.is_stub {
                    if let Some(value) = value {
                        if self.settings.rules.enabled(Rule::AssignmentDefaultInStub) {
                            // Ignore assignments in function bodies; those are covered by other rules.
                            if !self.ctx.scopes().any(|scope| scope.kind.is_function()) {
                                flake8_pyi::rules::annotated_assignment_default_in_stub(
                                    self, target, value, annotation,
                                );
                            }
                        }
                    }
                    if self.ctx.match_typing_expr(annotation, "TypeAlias") {
                        if self.settings.rules.enabled(Rule::SnakeCaseTypeAlias) {
                            flake8_pyi::rules::snake_case_type_alias(self, target);
                        }
                        if self.settings.rules.enabled(Rule::TSuffixedTypeAlias) {
                            flake8_pyi::rules::t_suffixed_type_alias(self, target);
                        }
                    }
                }
            }
            Stmt::Delete(ast::StmtDelete { targets, range: _ }) => {
                if self.settings.rules.enabled(Rule::GlobalStatement) {
                    for target in targets.iter() {
                        if let Expr::Name(ast::ExprName { id, .. }) = target {
                            pylint::rules::global_statement(self, id);
                        }
                    }
                }
            }
            Stmt::Expr(ast::StmtExpr { value, range: _ }) => {
                if self.settings.rules.enabled(Rule::UselessComparison) {
                    flake8_bugbear::rules::useless_comparison(self, value);
                }
                if self.settings.rules.enabled(Rule::UselessExpression) {
                    flake8_bugbear::rules::useless_expression(self, value);
                }
                if self.settings.rules.enabled(Rule::InvalidMockAccess) {
                    pygrep_hooks::rules::uncalled_mock_method(self, value);
                }
                if self.settings.rules.enabled(Rule::AsyncioDanglingTask) {
                    if let Some(diagnostic) = ruff::rules::asyncio_dangling_task(value, |expr| {
                        self.ctx.resolve_call_path(expr)
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
                for expr in decorator_list {
                    self.visit_expr(expr);
                }

                // Function annotations are always evaluated at runtime, unless future annotations
                // are enabled.
                let runtime_annotation = !self.ctx.future_annotations();

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
                    Binding {
                        kind: BindingKind::FunctionDefinition,
                        runtime_usage: None,
                        synthetic_usage: None,
                        typing_usage: None,
                        range: stmt.range(),
                        source: self.ctx.stmt_id,
                        context: self.ctx.execution_context(),
                        exceptions: self.ctx.exceptions(),
                    },
                );

                // If any global bindings don't already exist in the global scope, add it.
                let globals = helpers::extract_globals(body);
                for (name, stmt) in helpers::extract_globals(body) {
                    if self
                        .ctx
                        .global_scope()
                        .get(name)
                        .map_or(true, |index| self.ctx.bindings[*index].kind.is_annotation())
                    {
                        let id = self.ctx.bindings.push(Binding {
                            kind: BindingKind::Assignment,
                            runtime_usage: None,
                            synthetic_usage: None,
                            typing_usage: None,
                            range: stmt.range(),
                            source: self.ctx.stmt_id,
                            context: self.ctx.execution_context(),
                            exceptions: self.ctx.exceptions(),
                        });
                        self.ctx.global_scope_mut().add(name, id);
                    }
                }

                let definition = docstrings::extraction::extract_definition(
                    ExtractionTarget::Function,
                    stmt,
                    self.ctx.definition_id,
                    &self.ctx.definitions,
                );
                self.ctx.push_definition(definition);

                self.ctx.push_scope(ScopeKind::Function(FunctionDef {
                    name,
                    body,
                    args,
                    decorator_list,
                    async_: matches!(stmt, Stmt::AsyncFunctionDef(_)),
                    globals,
                }));

                self.deferred.functions.push(self.ctx.snapshot());
            }
            Stmt::ClassDef(ast::StmtClassDef {
                body,
                name,
                bases,
                keywords,
                decorator_list,
                range: _,
            }) => {
                for expr in bases {
                    self.visit_expr(expr);
                }
                for keyword in keywords {
                    self.visit_keyword(keyword);
                }
                for expr in decorator_list {
                    self.visit_expr(expr);
                }

                // If any global bindings don't already exist in the global scope, add it.
                let globals = helpers::extract_globals(body);
                for (name, stmt) in &globals {
                    if self
                        .ctx
                        .global_scope()
                        .get(name)
                        .map_or(true, |index| self.ctx.bindings[*index].kind.is_annotation())
                    {
                        let id = self.ctx.bindings.push(Binding {
                            kind: BindingKind::Assignment,
                            runtime_usage: None,
                            synthetic_usage: None,
                            typing_usage: None,
                            range: stmt.range(),
                            source: self.ctx.stmt_id,
                            context: self.ctx.execution_context(),
                            exceptions: self.ctx.exceptions(),
                        });
                        self.ctx.global_scope_mut().add(name, id);
                    }
                }

                let definition = docstrings::extraction::extract_definition(
                    ExtractionTarget::Class,
                    stmt,
                    self.ctx.definition_id,
                    &self.ctx.definitions,
                );
                self.ctx.push_definition(definition);

                self.ctx.push_scope(ScopeKind::Class(ClassDef {
                    name,
                    bases,
                    keywords,
                    decorator_list,
                    globals,
                }));

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
                    if let Some(call_path) = self.ctx.resolve_call_path(type_) {
                        if call_path.as_slice() == ["", "NameError"] {
                            handled_exceptions |= Exceptions::NAME_ERROR;
                        } else if call_path.as_slice() == ["", "ModuleNotFoundError"] {
                            handled_exceptions |= Exceptions::MODULE_NOT_FOUND_ERROR;
                        }
                    }
                }

                self.ctx.handled_exceptions.push(handled_exceptions);

                if self.settings.rules.enabled(Rule::JumpStatementInFinally) {
                    flake8_bugbear::rules::jump_statement_in_finally(self, finalbody);
                }

                if self.settings.rules.enabled(Rule::ContinueInFinally) {
                    if self.settings.target_version <= PythonVersion::Py38 {
                        pylint::rules::continue_in_finally(self, finalbody);
                    }
                }

                self.visit_body(body);
                self.ctx.handled_exceptions.pop();

                self.ctx.flags |= ContextFlags::EXCEPTION_HANDLER;
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
                let runtime_annotation = if self.ctx.future_annotations() {
                    if matches!(self.ctx.scope().kind, ScopeKind::Class(..)) {
                        let baseclasses = &self
                            .settings
                            .flake8_type_checking
                            .runtime_evaluated_base_classes;
                        let decorators = &self
                            .settings
                            .flake8_type_checking
                            .runtime_evaluated_decorators;
                        flake8_type_checking::helpers::runtime_evaluated(
                            &self.ctx,
                            baseclasses,
                            decorators,
                        )
                    } else {
                        false
                    }
                } else {
                    matches!(
                        self.ctx.scope().kind,
                        ScopeKind::Class(..) | ScopeKind::Module
                    )
                };

                if runtime_annotation {
                    self.visit_type_definition(annotation);
                } else {
                    self.visit_annotation(annotation);
                }
                if let Some(expr) = value {
                    if self.ctx.match_typing_expr(annotation, "TypeAlias") {
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
            Stmt::If(ast::StmtIf {
                test,
                body,
                orelse,
                range: _,
            }) => {
                self.visit_boolean_test(test);

                if flake8_type_checking::helpers::is_type_checking_block(&self.ctx, test) {
                    if self.settings.rules.enabled(Rule::EmptyTypeCheckingBlock) {
                        flake8_type_checking::rules::empty_type_checking_block(self, stmt, body);
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
                self.ctx.pop_scope();
                self.ctx.pop_definition();
            }
            Stmt::ClassDef(ast::StmtClassDef { name, .. }) => {
                self.ctx.pop_scope();
                self.ctx.pop_definition();
                self.add_binding(
                    name,
                    Binding {
                        kind: BindingKind::ClassDefinition,
                        runtime_usage: None,
                        synthetic_usage: None,
                        typing_usage: None,
                        range: stmt.range(),
                        source: self.ctx.stmt_id,
                        context: self.ctx.execution_context(),
                        exceptions: self.ctx.exceptions(),
                    },
                );
            }
            _ => {}
        }

        self.ctx.flags = flags_snapshot;
        self.ctx.pop_stmt();
    }

    fn visit_annotation(&mut self, expr: &'b Expr) {
        let flags_snapshot = self.ctx.flags;
        self.ctx.flags |= ContextFlags::ANNOTATION;
        self.visit_type_definition(expr);
        self.ctx.flags = flags_snapshot;
    }

    fn visit_expr(&mut self, expr: &'b Expr) {
        if !self.ctx.in_f_string()
            && !self.ctx.in_deferred_type_definition()
            && self.ctx.in_type_definition()
            && self.ctx.future_annotations()
        {
            if let Expr::Constant(ast::ExprConstant {
                value: Constant::Str(value),
                ..
            }) = expr
            {
                self.deferred.string_type_definitions.push((
                    expr.range(),
                    value,
                    self.ctx.snapshot(),
                ));
            } else {
                self.deferred
                    .future_type_definitions
                    .push((expr, self.ctx.snapshot()));
            }
            return;
        }

        self.ctx.push_expr(expr);

        // Store the flags prior to any further descent, so that we can restore them after visiting
        // the node.
        let flags_snapshot = self.ctx.flags;

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
            self.ctx.flags -= ContextFlags::BOOLEAN_TEST;
        }

        // Pre-visit.
        match expr {
            Expr::Subscript(ast::ExprSubscript { value, slice, .. }) => {
                // Ex) Optional[...], Union[...]
                if self.settings.rules.any_enabled(&[
                    Rule::MissingFutureAnnotationsImport,
                    Rule::NonPEP604Annotation,
                ]) {
                    if let Some(operator) =
                        analyze::typing::to_pep604_operator(value, slice, &self.ctx)
                    {
                        if self
                            .settings
                            .rules
                            .enabled(Rule::MissingFutureAnnotationsImport)
                        {
                            if self.settings.target_version < PythonVersion::Py310
                                && self.settings.target_version >= PythonVersion::Py37
                                && !self.ctx.future_annotations()
                                && self.ctx.in_annotation()
                            {
                                flake8_future_annotations::rules::missing_future_annotations(
                                    self, value,
                                );
                            }
                        }
                        if self.settings.rules.enabled(Rule::NonPEP604Annotation) {
                            if self.settings.target_version >= PythonVersion::Py310
                                || (self.settings.target_version >= PythonVersion::Py37
                                    && self.ctx.future_annotations()
                                    && self.ctx.in_annotation())
                            {
                                pyupgrade::rules::use_pep604_annotation(
                                    self, expr, slice, operator,
                                );
                            }
                        }
                    }
                }

                if self.ctx.match_typing_expr(value, "Literal") {
                    self.ctx.flags |= ContextFlags::LITERAL;
                }

                if self.settings.rules.any_enabled(&[
                    Rule::SysVersionSlice3,
                    Rule::SysVersion2,
                    Rule::SysVersion0,
                    Rule::SysVersionSlice1,
                ]) {
                    flake8_2020::rules::subscript(self, value, slice);
                }

                if self
                    .settings
                    .rules
                    .enabled(Rule::UncapitalizedEnvironmentVariables)
                {
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
                    let check_too_many_expressions = self
                        .settings
                        .rules
                        .enabled(Rule::ExpressionsInStarAssignment);
                    let check_two_starred_expressions = self
                        .settings
                        .rules
                        .enabled(Rule::MultipleStarredExpressions);
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
                        if self.settings.rules.enabled(Rule::TypingTextStrAlias) {
                            pyupgrade::rules::typing_text_str_alias(self, expr);
                        }
                        if self.settings.rules.enabled(Rule::NumpyDeprecatedTypeAlias) {
                            numpy::rules::deprecated_type_alias(self, expr);
                        }

                        // Ex) List[...]
                        if self.settings.rules.any_enabled(&[
                            Rule::MissingFutureAnnotationsImport,
                            Rule::NonPEP585Annotation,
                        ]) {
                            if let Some(replacement) =
                                analyze::typing::to_pep585_generic(expr, &self.ctx)
                            {
                                if self
                                    .settings
                                    .rules
                                    .enabled(Rule::MissingFutureAnnotationsImport)
                                {
                                    if self.settings.target_version < PythonVersion::Py39
                                        && self.settings.target_version >= PythonVersion::Py37
                                        && !self.ctx.future_annotations()
                                        && self.ctx.in_annotation()
                                    {
                                        flake8_future_annotations::rules::missing_future_annotations(
                                            self, expr,
                                        );
                                    }
                                }
                                if self.settings.rules.enabled(Rule::NonPEP585Annotation) {
                                    if self.settings.target_version >= PythonVersion::Py39
                                        || (self.settings.target_version >= PythonVersion::Py37
                                            && self.ctx.future_annotations()
                                            && self.ctx.in_annotation())
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
                        if self.settings.rules.enabled(Rule::AmbiguousVariableName) {
                            if let Some(diagnostic) =
                                pycodestyle::rules::ambiguous_variable_name(id, expr.range())
                            {
                                self.diagnostics.push(diagnostic);
                            }
                        }

                        if self.ctx.scope().kind.is_class() {
                            if self.settings.rules.enabled(Rule::BuiltinAttributeShadowing) {
                                flake8_builtins::rules::builtin_attribute_shadowing(self, id, expr);
                            }
                        } else {
                            if self.settings.rules.enabled(Rule::BuiltinVariableShadowing) {
                                flake8_builtins::rules::builtin_variable_shadowing(self, id, expr);
                            }
                        }

                        self.handle_node_store(id, expr);
                    }
                    ExprContext::Del => self.handle_node_delete(expr),
                }

                if self.settings.rules.enabled(Rule::SixPY3) {
                    flake8_2020::rules::name_or_attribute(self, expr);
                }

                if self
                    .settings
                    .rules
                    .enabled(Rule::LoadBeforeGlobalDeclaration)
                {
                    pylint::rules::load_before_global_declaration(self, id, expr);
                }
            }
            Expr::Attribute(ast::ExprAttribute { attr, value, .. }) => {
                // Ex) typing.List[...]
                if self.settings.rules.any_enabled(&[
                    Rule::MissingFutureAnnotationsImport,
                    Rule::NonPEP585Annotation,
                ]) {
                    if let Some(replacement) = analyze::typing::to_pep585_generic(expr, &self.ctx) {
                        if self
                            .settings
                            .rules
                            .enabled(Rule::MissingFutureAnnotationsImport)
                        {
                            if self.settings.target_version < PythonVersion::Py39
                                && self.settings.target_version >= PythonVersion::Py37
                                && !self.ctx.future_annotations()
                                && self.ctx.in_annotation()
                            {
                                flake8_future_annotations::rules::missing_future_annotations(
                                    self, expr,
                                );
                            }
                        }
                        if self.settings.rules.enabled(Rule::NonPEP585Annotation) {
                            if self.settings.target_version >= PythonVersion::Py39
                                || (self.settings.target_version >= PythonVersion::Py37
                                    && self.ctx.future_annotations()
                                    && self.ctx.in_annotation())
                            {
                                pyupgrade::rules::use_pep585_annotation(self, expr, &replacement);
                            }
                        }
                    }
                }
                if self.settings.rules.enabled(Rule::DatetimeTimezoneUTC)
                    && self.settings.target_version >= PythonVersion::Py311
                {
                    pyupgrade::rules::datetime_utc_alias(self, expr);
                }
                if self.settings.rules.enabled(Rule::TypingTextStrAlias) {
                    pyupgrade::rules::typing_text_str_alias(self, expr);
                }
                if self.settings.rules.enabled(Rule::NumpyDeprecatedTypeAlias) {
                    numpy::rules::deprecated_type_alias(self, expr);
                }
                if self.settings.rules.enabled(Rule::DeprecatedMockImport) {
                    pyupgrade::rules::deprecated_mock_attribute(self, expr);
                }
                if self.settings.rules.enabled(Rule::SixPY3) {
                    flake8_2020::rules::name_or_attribute(self, expr);
                }
                if self.settings.rules.enabled(Rule::BannedApi) {
                    flake8_tidy_imports::banned_api::banned_attribute_access(self, expr);
                }
                if self.settings.rules.enabled(Rule::PrivateMemberAccess) {
                    flake8_self::rules::private_member_access(self, expr);
                }
                pandas_vet::rules::attr(self, attr, value, expr);
            }
            Expr::Call(ast::ExprCall {
                func,
                args,
                keywords,
                range: _,
            }) => {
                if self.settings.rules.any_enabled(&[
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
                                if self.settings.rules.enabled(Rule::StaticJoinToFString) {
                                    flynt::rules::static_join_to_fstring(self, expr, value);
                                }
                            } else if attr == "format" {
                                // "...".format(...) call
                                let location = expr.range();
                                match pyflakes::format::FormatSummary::try_from(value.as_ref()) {
                                    Err(e) => {
                                        if self
                                            .settings
                                            .rules
                                            .enabled(Rule::StringDotFormatInvalidFormat)
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
                                            .enabled(Rule::StringDotFormatExtraNamedArguments)
                                        {
                                            pyflakes::rules::string_dot_format_extra_named_arguments(
                                                self, &summary, keywords, location,
                                            );
                                        }

                                        if self
                                            .settings
                                            .rules
                                            .enabled(Rule::StringDotFormatExtraPositionalArguments)
                                        {
                                            pyflakes::rules::string_dot_format_extra_positional_arguments(
                                                self,
                                                &summary, args, location,
                                            );
                                        }

                                        if self
                                            .settings
                                            .rules
                                            .enabled(Rule::StringDotFormatMissingArguments)
                                        {
                                            pyflakes::rules::string_dot_format_missing_argument(
                                                self, &summary, args, keywords, location,
                                            );
                                        }

                                        if self
                                            .settings
                                            .rules
                                            .enabled(Rule::StringDotFormatMixingAutomatic)
                                        {
                                            pyflakes::rules::string_dot_format_mixing_automatic(
                                                self, &summary, location,
                                            );
                                        }

                                        if self.settings.rules.enabled(Rule::FormatLiterals) {
                                            pyupgrade::rules::format_literals(self, &summary, expr);
                                        }

                                        if self.settings.rules.enabled(Rule::FString) {
                                            pyupgrade::rules::f_strings(self, &summary, expr);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                // pyupgrade
                if self.settings.rules.enabled(Rule::TypeOfPrimitive) {
                    pyupgrade::rules::type_of_primitive(self, expr, func, args);
                }
                if self.settings.rules.enabled(Rule::DeprecatedUnittestAlias) {
                    pyupgrade::rules::deprecated_unittest_alias(self, func);
                }
                if self.settings.rules.enabled(Rule::SuperCallWithParameters) {
                    pyupgrade::rules::super_call_with_parameters(self, expr, func, args);
                }
                if self.settings.rules.enabled(Rule::UnnecessaryEncodeUTF8) {
                    pyupgrade::rules::unnecessary_encode_utf8(self, expr, func, args, keywords);
                }
                if self.settings.rules.enabled(Rule::RedundantOpenModes) {
                    pyupgrade::rules::redundant_open_modes(self, expr);
                }
                if self.settings.rules.enabled(Rule::NativeLiterals) {
                    pyupgrade::rules::native_literals(self, expr, func, args, keywords);
                }
                if self.settings.rules.enabled(Rule::OpenAlias) {
                    pyupgrade::rules::open_alias(self, expr, func);
                }
                if self.settings.rules.enabled(Rule::ReplaceUniversalNewlines) {
                    pyupgrade::rules::replace_universal_newlines(self, func, keywords);
                }
                if self.settings.rules.enabled(Rule::ReplaceStdoutStderr) {
                    pyupgrade::rules::replace_stdout_stderr(self, expr, func, args, keywords);
                }
                if self.settings.rules.enabled(Rule::OSErrorAlias) {
                    pyupgrade::rules::os_error_alias_call(self, func);
                }
                if self.settings.rules.enabled(Rule::NonPEP604Isinstance)
                    && self.settings.target_version >= PythonVersion::Py310
                {
                    pyupgrade::rules::use_pep604_isinstance(self, expr, func, args);
                }

                // flake8-async
                if self
                    .settings
                    .rules
                    .enabled(Rule::BlockingHttpCallInAsyncFunction)
                {
                    flake8_async::rules::blocking_http_call(self, expr);
                }
                if self
                    .settings
                    .rules
                    .enabled(Rule::OpenSleepOrSubprocessInAsyncFunction)
                {
                    flake8_async::rules::open_sleep_or_subprocess_call(self, expr);
                }
                if self
                    .settings
                    .rules
                    .enabled(Rule::BlockingOsCallInAsyncFunction)
                {
                    flake8_async::rules::blocking_os_call(self, expr);
                }

                // flake8-print
                if self
                    .settings
                    .rules
                    .any_enabled(&[Rule::Print, Rule::PPrint])
                {
                    flake8_print::rules::print_call(self, func, keywords);
                }

                // flake8-bandit
                if self.settings.rules.any_enabled(&[
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
                if self.settings.rules.enabled(Rule::UnreliableCallableCheck) {
                    flake8_bugbear::rules::unreliable_callable_check(self, expr, func, args);
                }
                if self.settings.rules.enabled(Rule::StripWithMultiCharacters) {
                    flake8_bugbear::rules::strip_with_multi_characters(self, expr, func, args);
                }
                if self.settings.rules.enabled(Rule::GetAttrWithConstant) {
                    flake8_bugbear::rules::getattr_with_constant(self, expr, func, args);
                }
                if self.settings.rules.enabled(Rule::SetAttrWithConstant) {
                    flake8_bugbear::rules::setattr_with_constant(self, expr, func, args);
                }
                if self.settings.rules.enabled(Rule::UselessContextlibSuppress) {
                    flake8_bugbear::rules::useless_contextlib_suppress(self, expr, func, args);
                }
                if self
                    .settings
                    .rules
                    .enabled(Rule::StarArgUnpackingAfterKeywordArg)
                {
                    flake8_bugbear::rules::star_arg_unpacking_after_keyword_arg(
                        self, args, keywords,
                    );
                }
                if self.settings.rules.enabled(Rule::ZipWithoutExplicitStrict)
                    && self.settings.target_version >= PythonVersion::Py310
                {
                    flake8_bugbear::rules::zip_without_explicit_strict(self, expr, func, keywords);
                }
                if self.settings.rules.enabled(Rule::NoExplicitStacklevel) {
                    flake8_bugbear::rules::no_explicit_stacklevel(self, func, args, keywords);
                }

                // flake8-pie
                if self.settings.rules.enabled(Rule::UnnecessaryDictKwargs) {
                    flake8_pie::rules::unnecessary_dict_kwargs(self, expr, keywords);
                }

                // flake8-bandit
                if self.settings.rules.enabled(Rule::ExecBuiltin) {
                    if let Some(diagnostic) = flake8_bandit::rules::exec_used(expr, func) {
                        self.diagnostics.push(diagnostic);
                    }
                }
                if self.settings.rules.enabled(Rule::BadFilePermissions) {
                    flake8_bandit::rules::bad_file_permissions(self, func, args, keywords);
                }
                if self
                    .settings
                    .rules
                    .enabled(Rule::RequestWithNoCertValidation)
                {
                    flake8_bandit::rules::request_with_no_cert_validation(
                        self, func, args, keywords,
                    );
                }
                if self.settings.rules.enabled(Rule::UnsafeYAMLLoad) {
                    flake8_bandit::rules::unsafe_yaml_load(self, func, args, keywords);
                }
                if self.settings.rules.enabled(Rule::SnmpInsecureVersion) {
                    flake8_bandit::rules::snmp_insecure_version(self, func, args, keywords);
                }
                if self.settings.rules.enabled(Rule::SnmpWeakCryptography) {
                    flake8_bandit::rules::snmp_weak_cryptography(self, func, args, keywords);
                }
                if self.settings.rules.enabled(Rule::Jinja2AutoescapeFalse) {
                    flake8_bandit::rules::jinja2_autoescape_false(self, func, args, keywords);
                }
                if self.settings.rules.enabled(Rule::HardcodedPasswordFuncArg) {
                    self.diagnostics
                        .extend(flake8_bandit::rules::hardcoded_password_func_arg(keywords));
                }
                if self.settings.rules.enabled(Rule::HardcodedSQLExpression) {
                    flake8_bandit::rules::hardcoded_sql_expression(self, expr);
                }
                if self
                    .settings
                    .rules
                    .enabled(Rule::HashlibInsecureHashFunction)
                {
                    flake8_bandit::rules::hashlib_insecure_hash_functions(
                        self, func, args, keywords,
                    );
                }
                if self.settings.rules.enabled(Rule::RequestWithoutTimeout) {
                    flake8_bandit::rules::request_without_timeout(self, func, args, keywords);
                }
                if self
                    .settings
                    .rules
                    .enabled(Rule::LoggingConfigInsecureListen)
                {
                    flake8_bandit::rules::logging_config_insecure_listen(
                        self, func, args, keywords,
                    );
                }
                if self.settings.rules.any_enabled(&[
                    Rule::SubprocessWithoutShellEqualsTrue,
                    Rule::SubprocessPopenWithShellEqualsTrue,
                    Rule::CallWithShellEqualsTrue,
                    Rule::StartProcessWithAShell,
                    Rule::StartProcessWithNoShell,
                    Rule::StartProcessWithPartialPath,
                ]) {
                    flake8_bandit::rules::shell_injection(self, func, args, keywords);
                }

                // flake8-comprehensions
                if self.settings.rules.enabled(Rule::UnnecessaryGeneratorList) {
                    flake8_comprehensions::rules::unnecessary_generator_list(
                        self, expr, func, args, keywords,
                    );
                }
                if self.settings.rules.enabled(Rule::UnnecessaryGeneratorSet) {
                    flake8_comprehensions::rules::unnecessary_generator_set(
                        self,
                        expr,
                        self.ctx.expr_parent(),
                        func,
                        args,
                        keywords,
                    );
                }
                if self.settings.rules.enabled(Rule::UnnecessaryGeneratorDict) {
                    flake8_comprehensions::rules::unnecessary_generator_dict(
                        self,
                        expr,
                        self.ctx.expr_parent(),
                        func,
                        args,
                        keywords,
                    );
                }
                if self
                    .settings
                    .rules
                    .enabled(Rule::UnnecessaryListComprehensionSet)
                {
                    flake8_comprehensions::rules::unnecessary_list_comprehension_set(
                        self, expr, func, args, keywords,
                    );
                }
                if self
                    .settings
                    .rules
                    .enabled(Rule::UnnecessaryListComprehensionDict)
                {
                    flake8_comprehensions::rules::unnecessary_list_comprehension_dict(
                        self, expr, func, args, keywords,
                    );
                }
                if self.settings.rules.enabled(Rule::UnnecessaryLiteralSet) {
                    flake8_comprehensions::rules::unnecessary_literal_set(
                        self, expr, func, args, keywords,
                    );
                }
                if self.settings.rules.enabled(Rule::UnnecessaryLiteralDict) {
                    flake8_comprehensions::rules::unnecessary_literal_dict(
                        self, expr, func, args, keywords,
                    );
                }
                if self.settings.rules.enabled(Rule::UnnecessaryCollectionCall) {
                    flake8_comprehensions::rules::unnecessary_collection_call(
                        self,
                        expr,
                        func,
                        args,
                        keywords,
                        &self.settings.flake8_comprehensions,
                    );
                }
                if self
                    .settings
                    .rules
                    .enabled(Rule::UnnecessaryLiteralWithinTupleCall)
                {
                    flake8_comprehensions::rules::unnecessary_literal_within_tuple_call(
                        self, expr, func, args, keywords,
                    );
                }
                if self
                    .settings
                    .rules
                    .enabled(Rule::UnnecessaryLiteralWithinListCall)
                {
                    flake8_comprehensions::rules::unnecessary_literal_within_list_call(
                        self, expr, func, args, keywords,
                    );
                }
                if self
                    .settings
                    .rules
                    .enabled(Rule::UnnecessaryLiteralWithinDictCall)
                {
                    flake8_comprehensions::rules::unnecessary_literal_within_dict_call(
                        self, expr, func, args, keywords,
                    );
                }
                if self.settings.rules.enabled(Rule::UnnecessaryListCall) {
                    flake8_comprehensions::rules::unnecessary_list_call(self, expr, func, args);
                }
                if self
                    .settings
                    .rules
                    .enabled(Rule::UnnecessaryCallAroundSorted)
                {
                    flake8_comprehensions::rules::unnecessary_call_around_sorted(
                        self, expr, func, args,
                    );
                }
                if self
                    .settings
                    .rules
                    .enabled(Rule::UnnecessaryDoubleCastOrProcess)
                {
                    flake8_comprehensions::rules::unnecessary_double_cast_or_process(
                        self, expr, func, args,
                    );
                }
                if self
                    .settings
                    .rules
                    .enabled(Rule::UnnecessarySubscriptReversal)
                {
                    flake8_comprehensions::rules::unnecessary_subscript_reversal(
                        self, expr, func, args,
                    );
                }
                if self.settings.rules.enabled(Rule::UnnecessaryMap) {
                    flake8_comprehensions::rules::unnecessary_map(
                        self,
                        expr,
                        self.ctx.expr_parent(),
                        func,
                        args,
                    );
                }
                if self
                    .settings
                    .rules
                    .enabled(Rule::UnnecessaryComprehensionAnyAll)
                {
                    flake8_comprehensions::rules::unnecessary_comprehension_any_all(
                        self, expr, func, args, keywords,
                    );
                }

                // flake8-boolean-trap
                if self
                    .settings
                    .rules
                    .enabled(Rule::BooleanPositionalValueInFunctionCall)
                {
                    flake8_boolean_trap::rules::check_boolean_positional_value_in_function_call(
                        self, args, func,
                    );
                }
                if let Expr::Name(ast::ExprName { id, ctx, range: _ }) = func.as_ref() {
                    if id == "locals" && matches!(ctx, ExprContext::Load) {
                        let scope = self.ctx.scope_mut();
                        scope.uses_locals = true;
                    }
                }

                // flake8-debugger
                if self.settings.rules.enabled(Rule::Debugger) {
                    flake8_debugger::rules::debugger_call(self, expr, func);
                }

                // pandas-vet
                if self
                    .settings
                    .rules
                    .enabled(Rule::PandasUseOfInplaceArgument)
                {
                    self.diagnostics.extend(
                        pandas_vet::rules::inplace_argument(self, expr, func, args, keywords)
                            .into_iter(),
                    );
                }
                pandas_vet::rules::call(self, func);

                if self.settings.rules.enabled(Rule::PandasUseOfPdMerge) {
                    if let Some(diagnostic) = pandas_vet::rules::use_of_pd_merge(func) {
                        self.diagnostics.push(diagnostic);
                    };
                }

                // flake8-datetimez
                if self.settings.rules.enabled(Rule::CallDatetimeWithoutTzinfo) {
                    flake8_datetimez::rules::call_datetime_without_tzinfo(
                        self,
                        func,
                        args,
                        keywords,
                        expr.range(),
                    );
                }
                if self.settings.rules.enabled(Rule::CallDatetimeToday) {
                    flake8_datetimez::rules::call_datetime_today(self, func, expr.range());
                }
                if self.settings.rules.enabled(Rule::CallDatetimeUtcnow) {
                    flake8_datetimez::rules::call_datetime_utcnow(self, func, expr.range());
                }
                if self
                    .settings
                    .rules
                    .enabled(Rule::CallDatetimeUtcfromtimestamp)
                {
                    flake8_datetimez::rules::call_datetime_utcfromtimestamp(
                        self,
                        func,
                        expr.range(),
                    );
                }
                if self
                    .settings
                    .rules
                    .enabled(Rule::CallDatetimeNowWithoutTzinfo)
                {
                    flake8_datetimez::rules::call_datetime_now_without_tzinfo(
                        self,
                        func,
                        args,
                        keywords,
                        expr.range(),
                    );
                }
                if self.settings.rules.enabled(Rule::CallDatetimeFromtimestamp) {
                    flake8_datetimez::rules::call_datetime_fromtimestamp(
                        self,
                        func,
                        args,
                        keywords,
                        expr.range(),
                    );
                }
                if self
                    .settings
                    .rules
                    .enabled(Rule::CallDatetimeStrptimeWithoutZone)
                {
                    flake8_datetimez::rules::call_datetime_strptime_without_zone(
                        self,
                        func,
                        args,
                        expr.range(),
                    );
                }
                if self.settings.rules.enabled(Rule::CallDateToday) {
                    flake8_datetimez::rules::call_date_today(self, func, expr.range());
                }
                if self.settings.rules.enabled(Rule::CallDateFromtimestamp) {
                    flake8_datetimez::rules::call_date_fromtimestamp(self, func, expr.range());
                }

                // pygrep-hooks
                if self.settings.rules.enabled(Rule::Eval) {
                    pygrep_hooks::rules::no_eval(self, func);
                }
                if self.settings.rules.enabled(Rule::DeprecatedLogWarn) {
                    pygrep_hooks::rules::deprecated_log_warn(self, func);
                }

                // pylint
                if self
                    .settings
                    .rules
                    .enabled(Rule::UnnecessaryDirectLambdaCall)
                {
                    pylint::rules::unnecessary_direct_lambda_call(self, expr, func);
                }
                if self.settings.rules.enabled(Rule::SysExitAlias) {
                    pylint::rules::sys_exit_alias(self, func);
                }
                if self.settings.rules.enabled(Rule::BadStrStripCall) {
                    pylint::rules::bad_str_strip_call(self, func, args);
                }
                if self.settings.rules.enabled(Rule::InvalidEnvvarDefault) {
                    pylint::rules::invalid_envvar_default(self, func, args, keywords);
                }
                if self.settings.rules.enabled(Rule::InvalidEnvvarValue) {
                    pylint::rules::invalid_envvar_value(self, func, args, keywords);
                }
                if self.settings.rules.enabled(Rule::NestedMinMax) {
                    pylint::rules::nested_min_max(self, expr, func, args, keywords);
                }

                // flake8-pytest-style
                if self.settings.rules.enabled(Rule::PytestPatchWithLambda) {
                    if let Some(diagnostic) =
                        flake8_pytest_style::rules::patch_with_lambda(func, args, keywords)
                    {
                        self.diagnostics.push(diagnostic);
                    }
                }
                if self.settings.rules.enabled(Rule::PytestUnittestAssertion) {
                    if let Some(diagnostic) = flake8_pytest_style::rules::unittest_assertion(
                        self, expr, func, args, keywords,
                    ) {
                        self.diagnostics.push(diagnostic);
                    }
                }

                if self.settings.rules.any_enabled(&[
                    Rule::PytestRaisesWithoutException,
                    Rule::PytestRaisesTooBroad,
                ]) {
                    flake8_pytest_style::rules::raises_call(self, func, args, keywords);
                }

                if self.settings.rules.enabled(Rule::PytestFailWithoutMessage) {
                    flake8_pytest_style::rules::fail_call(self, func, args, keywords);
                }

                if self.settings.rules.enabled(Rule::PairwiseOverZipped) {
                    if self.settings.target_version >= PythonVersion::Py310 {
                        ruff::rules::pairwise_over_zipped(self, func, args);
                    }
                }

                // flake8-gettext
                if self.settings.rules.any_enabled(&[
                    Rule::FStringInGetTextFuncCall,
                    Rule::FormatInGetTextFuncCall,
                    Rule::PrintfInGetTextFuncCall,
                ]) && flake8_gettext::rules::is_gettext_func_call(
                    func,
                    &self.settings.flake8_gettext.functions_names,
                ) {
                    if self.settings.rules.enabled(Rule::FStringInGetTextFuncCall) {
                        self.diagnostics
                            .extend(flake8_gettext::rules::f_string_in_gettext_func_call(args));
                    }
                    if self.settings.rules.enabled(Rule::FormatInGetTextFuncCall) {
                        self.diagnostics
                            .extend(flake8_gettext::rules::format_in_gettext_func_call(args));
                    }
                    if self.settings.rules.enabled(Rule::PrintfInGetTextFuncCall) {
                        self.diagnostics
                            .extend(flake8_gettext::rules::printf_in_gettext_func_call(args));
                    }
                }

                // flake8-simplify
                if self
                    .settings
                    .rules
                    .enabled(Rule::UncapitalizedEnvironmentVariables)
                {
                    flake8_simplify::rules::use_capital_environment_variables(self, expr);
                }

                if self
                    .settings
                    .rules
                    .enabled(Rule::OpenFileWithContextHandler)
                {
                    flake8_simplify::rules::open_file_with_context_handler(self, func);
                }

                if self.settings.rules.enabled(Rule::DictGetWithNoneDefault) {
                    flake8_simplify::rules::dict_get_with_none_default(self, expr);
                }

                // flake8-use-pathlib
                if self.settings.rules.any_enabled(&[
                    Rule::OsPathAbspath,
                    Rule::OsChmod,
                    Rule::OsMkdir,
                    Rule::OsMakedirs,
                    Rule::OsRename,
                    Rule::PathlibReplace,
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
                    flake8_use_pathlib::helpers::replaceable_by_pathlib(self, func);
                }

                // numpy
                if self.settings.rules.enabled(Rule::NumpyLegacyRandom) {
                    numpy::rules::numpy_legacy_random(self, func);
                }

                // flake8-logging-format
                if self.settings.rules.any_enabled(&[
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
                if self
                    .settings
                    .rules
                    .any_enabled(&[Rule::LoggingTooFewArgs, Rule::LoggingTooManyArgs])
                {
                    pylint::rules::logging_call(self, func, args, keywords);
                }

                // flake8-django
                if self
                    .settings
                    .rules
                    .enabled(Rule::DjangoLocalsInRenderFunction)
                {
                    flake8_django::rules::locals_in_render_function(self, func, args, keywords);
                }
            }
            Expr::Dict(ast::ExprDict {
                keys,
                values,
                range: _,
            }) => {
                if self.settings.rules.any_enabled(&[
                    Rule::MultiValueRepeatedKeyLiteral,
                    Rule::MultiValueRepeatedKeyVariable,
                ]) {
                    pyflakes::rules::repeated_keys(self, keys, values);
                }

                if self.settings.rules.enabled(Rule::UnnecessarySpread) {
                    flake8_pie::rules::unnecessary_spread(self, keys, values);
                }
            }
            Expr::Yield(_) => {
                if self.settings.rules.enabled(Rule::YieldOutsideFunction) {
                    pyflakes::rules::yield_outside_function(self, expr);
                }
                if self.settings.rules.enabled(Rule::YieldInInit) {
                    pylint::rules::yield_in_init(self, expr);
                }
            }
            Expr::YieldFrom(_) => {
                if self.settings.rules.enabled(Rule::YieldOutsideFunction) {
                    pyflakes::rules::yield_outside_function(self, expr);
                }
                if self.settings.rules.enabled(Rule::YieldInInit) {
                    pylint::rules::yield_in_init(self, expr);
                }
            }
            Expr::Await(_) => {
                if self.settings.rules.enabled(Rule::YieldOutsideFunction) {
                    pyflakes::rules::yield_outside_function(self, expr);
                }
                if self.settings.rules.enabled(Rule::AwaitOutsideAsync) {
                    pylint::rules::await_outside_async(self, expr);
                }
            }
            Expr::JoinedStr(ast::ExprJoinedStr { values, range: _ }) => {
                if self
                    .settings
                    .rules
                    .enabled(Rule::FStringMissingPlaceholders)
                {
                    pyflakes::rules::f_string_missing_placeholders(expr, values, self);
                }
                if self.settings.rules.enabled(Rule::HardcodedSQLExpression) {
                    flake8_bandit::rules::hardcoded_sql_expression(self, expr);
                }
            }
            Expr::BinOp(ast::ExprBinOp {
                left,
                op: Operator::RShift,
                ..
            }) => {
                if self.settings.rules.enabled(Rule::InvalidPrintSyntax) {
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
                    if self.settings.rules.any_enabled(&[
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
                                if self
                                    .settings
                                    .rules
                                    .enabled(Rule::PercentFormatUnsupportedFormatCharacter)
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
                                    .enabled(Rule::PercentFormatInvalidFormat)
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
                                    .enabled(Rule::PercentFormatExpectedMapping)
                                {
                                    pyflakes::rules::percent_format_expected_mapping(
                                        self, &summary, right, location,
                                    );
                                }
                                if self
                                    .settings
                                    .rules
                                    .enabled(Rule::PercentFormatExpectedSequence)
                                {
                                    pyflakes::rules::percent_format_expected_sequence(
                                        self, &summary, right, location,
                                    );
                                }
                                if self
                                    .settings
                                    .rules
                                    .enabled(Rule::PercentFormatExtraNamedArguments)
                                {
                                    pyflakes::rules::percent_format_extra_named_arguments(
                                        self, &summary, right, location,
                                    );
                                }
                                if self
                                    .settings
                                    .rules
                                    .enabled(Rule::PercentFormatMissingArgument)
                                {
                                    pyflakes::rules::percent_format_missing_arguments(
                                        self, &summary, right, location,
                                    );
                                }
                                if self
                                    .settings
                                    .rules
                                    .enabled(Rule::PercentFormatMixedPositionalAndNamed)
                                {
                                    pyflakes::rules::percent_format_mixed_positional_and_named(
                                        self, &summary, location,
                                    );
                                }
                                if self
                                    .settings
                                    .rules
                                    .enabled(Rule::PercentFormatPositionalCountMismatch)
                                {
                                    pyflakes::rules::percent_format_positional_count_mismatch(
                                        self, &summary, right, location,
                                    );
                                }
                                if self
                                    .settings
                                    .rules
                                    .enabled(Rule::PercentFormatStarRequiresSequence)
                                {
                                    pyflakes::rules::percent_format_star_requires_sequence(
                                        self, &summary, right, location,
                                    );
                                }
                            }
                        }
                    }

                    if self.settings.rules.enabled(Rule::PrintfStringFormatting) {
                        pyupgrade::rules::printf_string_formatting(self, expr, right, self.locator);
                    }
                    if self.settings.rules.enabled(Rule::BadStringFormatType) {
                        pylint::rules::bad_string_format_type(self, expr, right);
                    }
                    if self.settings.rules.enabled(Rule::HardcodedSQLExpression) {
                        flake8_bandit::rules::hardcoded_sql_expression(self, expr);
                    }
                }
            }
            Expr::BinOp(ast::ExprBinOp {
                op: Operator::Add, ..
            }) => {
                if self
                    .settings
                    .rules
                    .enabled(Rule::ExplicitStringConcatenation)
                {
                    if let Some(diagnostic) = flake8_implicit_str_concat::rules::explicit(expr) {
                        self.diagnostics.push(diagnostic);
                    }
                }
                if self
                    .settings
                    .rules
                    .enabled(Rule::CollectionLiteralConcatenation)
                {
                    ruff::rules::collection_literal_concatenation(self, expr);
                }
                if self.settings.rules.enabled(Rule::HardcodedSQLExpression) {
                    flake8_bandit::rules::hardcoded_sql_expression(self, expr);
                }
            }
            Expr::BinOp(ast::ExprBinOp {
                op: Operator::BitOr,
                ..
            }) => {
                if self.is_stub {
                    if self.settings.rules.enabled(Rule::DuplicateUnionMember)
                        && self.ctx.in_type_definition()
                        && self.ctx.expr_parent().map_or(true, |parent| {
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
                let check_not_in = self.settings.rules.enabled(Rule::NotInTest);
                let check_not_is = self.settings.rules.enabled(Rule::NotIsTest);
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

                if self.settings.rules.enabled(Rule::UnaryPrefixIncrement) {
                    flake8_bugbear::rules::unary_prefix_increment(self, expr, *op, operand);
                }

                if self.settings.rules.enabled(Rule::NegateEqualOp) {
                    flake8_simplify::rules::negation_with_equal_op(self, expr, *op, operand);
                }
                if self.settings.rules.enabled(Rule::NegateNotEqualOp) {
                    flake8_simplify::rules::negation_with_not_equal_op(self, expr, *op, operand);
                }
                if self.settings.rules.enabled(Rule::DoubleNegation) {
                    flake8_simplify::rules::double_negation(self, expr, *op, operand);
                }
            }
            Expr::Compare(ast::ExprCompare {
                left,
                ops,
                comparators,
                range: _,
            }) => {
                let check_none_comparisons = self.settings.rules.enabled(Rule::NoneComparison);
                let check_true_false_comparisons =
                    self.settings.rules.enabled(Rule::TrueFalseComparison);
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

                if self.settings.rules.enabled(Rule::IsLiteral) {
                    pyflakes::rules::invalid_literal_comparison(
                        self,
                        left,
                        ops,
                        comparators,
                        expr.range(),
                    );
                }

                if self.settings.rules.enabled(Rule::TypeComparison) {
                    pycodestyle::rules::type_comparison(self, expr, ops, comparators);
                }

                if self.settings.rules.any_enabled(&[
                    Rule::SysVersionCmpStr3,
                    Rule::SysVersionInfo0Eq3,
                    Rule::SysVersionInfo1CmpInt,
                    Rule::SysVersionInfoMinorCmpInt,
                    Rule::SysVersionCmpStr10,
                ]) {
                    flake8_2020::rules::compare(self, left, ops, comparators);
                }

                if self.settings.rules.enabled(Rule::HardcodedPasswordString) {
                    self.diagnostics.extend(
                        flake8_bandit::rules::compare_to_hardcoded_password_string(
                            left,
                            comparators,
                        ),
                    );
                }

                if self.settings.rules.enabled(Rule::ComparisonOfConstant) {
                    pylint::rules::comparison_of_constant(self, left, ops, comparators);
                }

                if self.settings.rules.enabled(Rule::CompareToEmptyString) {
                    pylint::rules::compare_to_empty_string(self, left, ops, comparators);
                }

                if self.settings.rules.enabled(Rule::MagicValueComparison) {
                    pylint::rules::magic_value_comparison(self, left, comparators);
                }

                if self.settings.rules.enabled(Rule::InDictKeys) {
                    flake8_simplify::rules::key_in_dict_compare(self, expr, left, ops, comparators);
                }

                if self.settings.rules.enabled(Rule::YodaConditions) {
                    flake8_simplify::rules::yoda_conditions(self, expr, left, ops, comparators);
                }

                if self.is_stub {
                    if self.settings.rules.any_enabled(&[
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

                    if self.settings.rules.enabled(Rule::BadVersionInfoComparison) {
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
                value: Constant::Str(value),
                kind,
                range: _,
            }) => {
                if self.ctx.in_type_definition()
                    && !self.ctx.in_literal()
                    && !self.ctx.in_f_string()
                {
                    self.deferred.string_type_definitions.push((
                        expr.range(),
                        value,
                        self.ctx.snapshot(),
                    ));
                }
                if self
                    .settings
                    .rules
                    .enabled(Rule::HardcodedBindAllInterfaces)
                {
                    if let Some(diagnostic) =
                        flake8_bandit::rules::hardcoded_bind_all_interfaces(value, expr.range())
                    {
                        self.diagnostics.push(diagnostic);
                    }
                }
                if self.settings.rules.enabled(Rule::HardcodedTempFile) {
                    if let Some(diagnostic) = flake8_bandit::rules::hardcoded_tmp_directory(
                        expr,
                        value,
                        &self.settings.flake8_bandit.hardcoded_tmp_directory,
                    ) {
                        self.diagnostics.push(diagnostic);
                    }
                }
                if self.settings.rules.enabled(Rule::UnicodeKindPrefix) {
                    pyupgrade::rules::unicode_kind_prefix(self, expr, kind.as_deref());
                }
            }
            Expr::Lambda(
                lambda @ ast::ExprLambda {
                    args,
                    body,
                    range: _,
                },
            ) => {
                if self.settings.rules.enabled(Rule::ReimplementedListBuiltin) {
                    flake8_pie::rules::reimplemented_list_builtin(self, lambda);
                }

                // Visit the default arguments, but avoid the body, which will be deferred.
                for expr in &args.kw_defaults {
                    self.visit_expr(expr);
                }
                for expr in &args.defaults {
                    self.visit_expr(expr);
                }
                self.ctx
                    .push_scope(ScopeKind::Lambda(Lambda { args, body }));
            }
            Expr::IfExp(ast::ExprIfExp {
                test,
                body,
                orelse,
                range: _,
            }) => {
                if self.settings.rules.enabled(Rule::IfExprWithTrueFalse) {
                    flake8_simplify::rules::explicit_true_false_in_ifexpr(
                        self, expr, test, body, orelse,
                    );
                }
                if self.settings.rules.enabled(Rule::IfExprWithFalseTrue) {
                    flake8_simplify::rules::explicit_false_true_in_ifexpr(
                        self, expr, test, body, orelse,
                    );
                }
                if self.settings.rules.enabled(Rule::IfExprWithTwistedArms) {
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
                if self.settings.rules.enabled(Rule::UnnecessaryComprehension) {
                    flake8_comprehensions::rules::unnecessary_list_set_comprehension(
                        self, expr, elt, generators,
                    );
                }
                if self.settings.rules.enabled(Rule::FunctionUsesLoopVariable) {
                    flake8_bugbear::rules::function_uses_loop_variable(self, &Node::Expr(expr));
                }
                if self.settings.rules.enabled(Rule::InDictKeys) {
                    for generator in generators {
                        flake8_simplify::rules::key_in_dict_for(
                            self,
                            &generator.target,
                            &generator.iter,
                        );
                    }
                }
            }
            Expr::DictComp(ast::ExprDictComp {
                key,
                value,
                generators,
                range: _,
            }) => {
                if self.settings.rules.enabled(Rule::UnnecessaryComprehension) {
                    flake8_comprehensions::rules::unnecessary_dict_comprehension(
                        self, expr, key, value, generators,
                    );
                }
                if self.settings.rules.enabled(Rule::FunctionUsesLoopVariable) {
                    flake8_bugbear::rules::function_uses_loop_variable(self, &Node::Expr(expr));
                }
                if self.settings.rules.enabled(Rule::InDictKeys) {
                    for generator in generators {
                        flake8_simplify::rules::key_in_dict_for(
                            self,
                            &generator.target,
                            &generator.iter,
                        );
                    }
                }
            }
            Expr::GeneratorExp(ast::ExprGeneratorExp {
                generators,
                elt: _,
                range: _,
            }) => {
                if self.settings.rules.enabled(Rule::FunctionUsesLoopVariable) {
                    flake8_bugbear::rules::function_uses_loop_variable(self, &Node::Expr(expr));
                }
                if self.settings.rules.enabled(Rule::InDictKeys) {
                    for generator in generators {
                        flake8_simplify::rules::key_in_dict_for(
                            self,
                            &generator.target,
                            &generator.iter,
                        );
                    }
                }
            }
            Expr::BoolOp(ast::ExprBoolOp {
                op,
                values,
                range: _,
            }) => {
                if self.settings.rules.enabled(Rule::RepeatedIsinstanceCalls) {
                    pylint::rules::repeated_isinstance_calls(self, expr, *op, values);
                }
                if self.settings.rules.enabled(Rule::MultipleStartsEndsWith) {
                    flake8_pie::rules::multiple_starts_ends_with(self, expr);
                }
                if self.settings.rules.enabled(Rule::DuplicateIsinstanceCall) {
                    flake8_simplify::rules::duplicate_isinstance_call(self, expr);
                }
                if self.settings.rules.enabled(Rule::CompareWithTuple) {
                    flake8_simplify::rules::compare_with_tuple(self, expr);
                }
                if self.settings.rules.enabled(Rule::ExprAndNotExpr) {
                    flake8_simplify::rules::expr_and_not_expr(self, expr);
                }
                if self.settings.rules.enabled(Rule::ExprOrNotExpr) {
                    flake8_simplify::rules::expr_or_not_expr(self, expr);
                }
                if self.settings.rules.enabled(Rule::ExprOrTrue) {
                    flake8_simplify::rules::expr_or_true(self, expr);
                }
                if self.settings.rules.enabled(Rule::ExprAndFalse) {
                    flake8_simplify::rules::expr_and_false(self, expr);
                }
            }
            Expr::FormattedValue(ast::ExprFormattedValue {
                value, conversion, ..
            }) => {
                if self
                    .settings
                    .rules
                    .enabled(Rule::ExplicitFStringTypeConversion)
                {
                    ruff::rules::explicit_f_string_type_conversion(self, value, *conversion);
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
                self.deferred.lambdas.push((expr, self.ctx.snapshot()));
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
                let callable = self.ctx.resolve_call_path(func).and_then(|call_path| {
                    if self.ctx.match_typing_call_path(&call_path, "cast") {
                        Some(Callable::Cast)
                    } else if self.ctx.match_typing_call_path(&call_path, "NewType") {
                        Some(Callable::NewType)
                    } else if self.ctx.match_typing_call_path(&call_path, "TypeVar") {
                        Some(Callable::TypeVar)
                    } else if self.ctx.match_typing_call_path(&call_path, "NamedTuple") {
                        Some(Callable::NamedTuple)
                    } else if self.ctx.match_typing_call_path(&call_path, "TypedDict") {
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
                        if !args.is_empty() {
                            self.visit_boolean_test(&args[0]);
                        }
                        for expr in args.iter().skip(1) {
                            self.visit_expr(expr);
                        }
                    }
                    Some(Callable::Cast) => {
                        self.visit_expr(func);
                        if !args.is_empty() {
                            self.visit_type_definition(&args[0]);
                        }
                        for expr in args.iter().skip(1) {
                            self.visit_expr(expr);
                        }
                    }
                    Some(Callable::NewType) => {
                        self.visit_expr(func);
                        for expr in args.iter().skip(1) {
                            self.visit_type_definition(expr);
                        }
                    }
                    Some(Callable::TypeVar) => {
                        self.visit_expr(func);
                        for expr in args.iter().skip(1) {
                            self.visit_type_definition(expr);
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
                        if args.len() > 1 {
                            match &args[1] {
                                Expr::List(ast::ExprList { elts, .. })
                                | Expr::Tuple(ast::ExprTuple { elts, .. }) => {
                                    for elt in elts {
                                        match elt {
                                            Expr::List(ast::ExprList { elts, .. })
                                            | Expr::Tuple(ast::ExprTuple { elts, .. }) => {
                                                if elts.len() == 2 {
                                                    self.visit_non_type_definition(&elts[0]);
                                                    self.visit_type_definition(&elts[1]);
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
                            let Keyword { value, .. } = keyword;
                            self.visit_type_definition(value);
                        }
                    }
                    Some(Callable::TypedDict) => {
                        self.visit_expr(func);

                        // Ex) TypedDict("a", {"a": int})
                        if args.len() > 1 {
                            if let Expr::Dict(ast::ExprDict {
                                keys,
                                values,
                                range: _,
                            }) = &args[1]
                            {
                                for key in keys.iter().flatten() {
                                    self.visit_non_type_definition(key);
                                }
                                for value in values {
                                    self.visit_type_definition(value);
                                }
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

                        if let Some(arg) = args.first() {
                            // Ex) DefaultNamedArg(bool | None, name="some_prop_name")
                            self.visit_type_definition(arg);

                            for arg in args.iter().skip(1) {
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
                if self.ctx.in_subscript() {
                    visitor::walk_expr(self, expr);
                } else if matches!(ctx, ExprContext::Store | ExprContext::Del) {
                    self.ctx.flags |= ContextFlags::SUBSCRIPT;
                    visitor::walk_expr(self, expr);
                } else {
                    match analyze::typing::match_annotated_subscript(
                        value,
                        &self.ctx,
                        self.settings.typing_modules.iter().map(String::as_str),
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
                self.ctx.flags |= ContextFlags::F_STRING;
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
                self.ctx.pop_scope();
            }
            _ => {}
        };

        self.ctx.flags = flags_snapshot;
        self.ctx.pop_expr();
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
                if self.settings.rules.enabled(Rule::BareExcept) {
                    if let Some(diagnostic) = pycodestyle::rules::bare_except(
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
                    .enabled(Rule::RaiseWithoutFromInsideExcept)
                {
                    flake8_bugbear::rules::raise_without_from_inside_except(self, body);
                }
                if self.settings.rules.enabled(Rule::BlindExcept) {
                    flake8_blind_except::rules::blind_except(self, type_.as_deref(), name, body);
                }
                if self.settings.rules.enabled(Rule::TryExceptPass) {
                    flake8_bandit::rules::try_except_pass(
                        self,
                        excepthandler,
                        type_.as_deref(),
                        name,
                        body,
                        self.settings.flake8_bandit.check_typed_exception,
                    );
                }
                if self.settings.rules.enabled(Rule::TryExceptContinue) {
                    flake8_bandit::rules::try_except_continue(
                        self,
                        excepthandler,
                        type_.as_deref(),
                        name,
                        body,
                        self.settings.flake8_bandit.check_typed_exception,
                    );
                }
                if self.settings.rules.enabled(Rule::ExceptWithEmptyTuple) {
                    flake8_bugbear::rules::except_with_empty_tuple(self, excepthandler);
                }
                if self
                    .settings
                    .rules
                    .enabled(Rule::ExceptWithNonExceptionClasses)
                {
                    flake8_bugbear::rules::except_with_non_exception_classes(self, excepthandler);
                }
                if self.settings.rules.enabled(Rule::ReraiseNoCause) {
                    tryceratops::rules::reraise_no_cause(self, body);
                }

                if self.settings.rules.enabled(Rule::BinaryOpException) {
                    pylint::rules::binary_op_exception(self, excepthandler);
                }
                match name {
                    Some(name) => {
                        if self.settings.rules.enabled(Rule::AmbiguousVariableName) {
                            if let Some(diagnostic) = pycodestyle::rules::ambiguous_variable_name(
                                name,
                                helpers::excepthandler_name_range(excepthandler, self.locator)
                                    .expect("Failed to find `name` range"),
                            ) {
                                self.diagnostics.push(diagnostic);
                            }
                        }

                        if self.settings.rules.enabled(Rule::BuiltinVariableShadowing) {
                            flake8_builtins::rules::builtin_variable_shadowing(
                                self,
                                name,
                                excepthandler,
                            );
                        }

                        let name_range =
                            helpers::excepthandler_name_range(excepthandler, self.locator).unwrap();

                        if self.ctx.scope().defines(name) {
                            self.handle_node_store(
                                name,
                                &Expr::Name(ast::ExprName {
                                    id: name.into(),
                                    ctx: ExprContext::Store,
                                    range: name_range,
                                }),
                            );
                        }

                        let definition = self.ctx.scope().get(name).copied();
                        self.handle_node_store(
                            name,
                            &Expr::Name(ast::ExprName {
                                id: name.into(),
                                ctx: ExprContext::Store,
                                range: name_range,
                            }),
                        );

                        walk_excepthandler(self, excepthandler);

                        if let Some(index) = {
                            let scope = self.ctx.scope_mut();
                            &scope.remove(name)
                        } {
                            if !self.ctx.bindings[*index].used() {
                                if self.settings.rules.enabled(Rule::UnusedVariable) {
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

                        if let Some(index) = definition {
                            let scope = self.ctx.scope_mut();
                            scope.add(name, index);
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
        if self.settings.rules.enabled(Rule::MutableArgumentDefault) {
            flake8_bugbear::rules::mutable_argument_default(self, arguments);
        }
        if self
            .settings
            .rules
            .enabled(Rule::FunctionCallInDefaultArgument)
        {
            flake8_bugbear::rules::function_call_argument_default(self, arguments);
        }

        if self.is_stub {
            if self
                .settings
                .rules
                .enabled(Rule::TypedArgumentDefaultInStub)
            {
                flake8_pyi::rules::typed_argument_simple_defaults(self, arguments);
            }
        }
        if self.is_stub {
            if self.settings.rules.enabled(Rule::ArgumentDefaultInStub) {
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
            Binding {
                kind: BindingKind::Argument,
                runtime_usage: None,
                synthetic_usage: None,
                typing_usage: None,
                range: arg.range(),
                source: self.ctx.stmt_id,
                context: self.ctx.execution_context(),
                exceptions: self.ctx.exceptions(),
            },
        );

        if self.settings.rules.enabled(Rule::AmbiguousVariableName) {
            if let Some(diagnostic) =
                pycodestyle::rules::ambiguous_variable_name(&arg.arg, arg.range())
            {
                self.diagnostics.push(diagnostic);
            }
        }

        if self.settings.rules.enabled(Rule::InvalidArgumentName) {
            if let Some(diagnostic) = pep8_naming::rules::invalid_argument_name(
                &arg.arg,
                arg,
                &self.settings.pep8_naming.ignore_names,
            ) {
                self.diagnostics.push(diagnostic);
            }
        }

        if self.settings.rules.enabled(Rule::BuiltinArgumentShadowing) {
            flake8_builtins::rules::builtin_argument_shadowing(self, &arg.arg, arg);
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
                Binding {
                    kind: BindingKind::Assignment,
                    runtime_usage: None,
                    synthetic_usage: None,
                    typing_usage: None,
                    range: pattern.range(),
                    source: self.ctx.stmt_id,
                    context: self.ctx.execution_context(),
                    exceptions: self.ctx.exceptions(),
                },
            );
        }

        walk_pattern(self, pattern);
    }

    fn visit_body(&mut self, body: &'b [Stmt]) {
        if self.settings.rules.enabled(Rule::UnnecessaryPass) {
            flake8_pie::rules::no_unnecessary_pass(self, body);
        }

        let prev_body = self.ctx.body;
        let prev_body_index = self.ctx.body_index;
        self.ctx.body = body;
        self.ctx.body_index = 0;

        for stmt in body {
            self.visit_stmt(stmt);
            self.ctx.body_index += 1;
        }

        self.ctx.body = prev_body;
        self.ctx.body_index = prev_body_index;
    }
}

impl<'a> Checker<'a> {
    /// Visit a [`Module`]. Returns `true` if the module contains a module-level docstring.
    fn visit_module(&mut self, python_ast: &'a Suite) -> bool {
        if self.settings.rules.enabled(Rule::FStringDocstring) {
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
        self.ctx.push_scope(ScopeKind::Generator);
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
        let snapshot = self.ctx.flags;
        self.ctx.flags |= ContextFlags::TYPE_CHECKING_BLOCK;
        self.visit_body(body);
        self.ctx.flags = snapshot;
    }

    /// Visit an [`Expr`], and treat it as a type definition.
    pub(crate) fn visit_type_definition(&mut self, expr: &'a Expr) {
        let snapshot = self.ctx.flags;
        self.ctx.flags |= ContextFlags::TYPE_DEFINITION;
        self.visit_expr(expr);
        self.ctx.flags = snapshot;
    }

    /// Visit an [`Expr`], and treat it as _not_ a type definition.
    pub(crate) fn visit_non_type_definition(&mut self, expr: &'a Expr) {
        let snapshot = self.ctx.flags;
        self.ctx.flags -= ContextFlags::TYPE_DEFINITION;
        self.visit_expr(expr);
        self.ctx.flags = snapshot;
    }

    /// Visit an [`Expr`], and treat it as a boolean test. This is useful for detecting whether an
    /// expressions return value is significant, or whether the calling context only relies on
    /// its truthiness.
    pub(crate) fn visit_boolean_test(&mut self, expr: &'a Expr) {
        let snapshot = self.ctx.flags;
        self.ctx.flags |= ContextFlags::BOOLEAN_TEST;
        self.visit_expr(expr);
        self.ctx.flags = snapshot;
    }

    /// Add a [`Binding`] to the current scope, bound to the given name.
    fn add_binding(&mut self, name: &'a str, binding: Binding<'a>) {
        let binding_id = self.ctx.bindings.next_id();
        if let Some((stack_index, existing_binding_id)) = self
            .ctx
            .scopes
            .ancestors(self.ctx.scope_id)
            .enumerate()
            .find_map(|(stack_index, scope)| {
                scope.get(name).map(|binding_id| (stack_index, *binding_id))
            })
        {
            let existing = &self.ctx.bindings[existing_binding_id];
            let in_current_scope = stack_index == 0;
            if !existing.kind.is_builtin()
                && existing.source.map_or(true, |left| {
                    binding.source.map_or(true, |right| {
                        !branch_detection::different_forks(left, right, &self.ctx.stmts)
                    })
                })
            {
                let existing_is_import = matches!(
                    existing.kind,
                    BindingKind::Importation(..)
                        | BindingKind::FromImportation(..)
                        | BindingKind::SubmoduleImportation(..)
                        | BindingKind::FutureImportation
                );
                if binding.kind.is_loop_var() && existing_is_import {
                    if self.settings.rules.enabled(Rule::ImportShadowedByLoopVar) {
                        #[allow(deprecated)]
                        let line = self.locator.compute_line_index(existing.range.start());

                        self.diagnostics.push(Diagnostic::new(
                            pyflakes::rules::ImportShadowedByLoopVar {
                                name: name.to_string(),
                                line,
                            },
                            binding.range,
                        ));
                    }
                } else if in_current_scope {
                    if !existing.used()
                        && binding.redefines(existing)
                        && (!self.settings.dummy_variable_rgx.is_match(name) || existing_is_import)
                        && !(existing.kind.is_function_definition()
                            && analyze::visibility::is_overload(
                                &self.ctx,
                                cast::decorator_list(self.ctx.stmts[existing.source.unwrap()]),
                            ))
                    {
                        if self.settings.rules.enabled(Rule::RedefinedWhileUnused) {
                            #[allow(deprecated)]
                            let line = self.locator.compute_line_index(existing.range.start());

                            let mut diagnostic = Diagnostic::new(
                                pyflakes::rules::RedefinedWhileUnused {
                                    name: name.to_string(),
                                    line,
                                },
                                matches!(
                                    binding.kind,
                                    BindingKind::ClassDefinition | BindingKind::FunctionDefinition
                                )
                                .then(|| {
                                    binding.source.map_or(binding.range, |source| {
                                        helpers::identifier_range(
                                            self.ctx.stmts[source],
                                            self.locator,
                                        )
                                    })
                                })
                                .unwrap_or(binding.range),
                            );
                            if let Some(parent) = binding.source {
                                let parent = self.ctx.stmts[parent];
                                if matches!(parent, Stmt::ImportFrom(_))
                                    && parent.range().contains_range(binding.range)
                                {
                                    diagnostic.set_parent(parent.start());
                                }
                            }
                            self.diagnostics.push(diagnostic);
                        }
                    }
                } else if existing_is_import && binding.redefines(existing) {
                    self.ctx
                        .shadowed_bindings
                        .entry(existing_binding_id)
                        .or_insert_with(Vec::new)
                        .push(binding_id);
                }
            }
        }

        // Per [PEP 572](https://peps.python.org/pep-0572/#scope-of-the-target), named
        // expressions in generators and comprehensions bind to the scope that contains the
        // outermost comprehension.
        let scope_id = if binding.kind.is_named_expr_assignment() {
            self.ctx
                .scopes
                .ancestor_ids(self.ctx.scope_id)
                .find_or_last(|scope_id| !self.ctx.scopes[*scope_id].kind.is_generator())
                .unwrap_or(self.ctx.scope_id)
        } else {
            self.ctx.scope_id
        };
        let scope = &mut self.ctx.scopes[scope_id];

        let binding = if let Some(index) = scope.get(name) {
            let existing = &self.ctx.bindings[*index];
            match &existing.kind {
                BindingKind::Builtin => {
                    // Avoid overriding builtins.
                    binding
                }
                kind @ (BindingKind::Global | BindingKind::Nonlocal) => {
                    // If the original binding was a global or nonlocal, and the new binding conflicts within
                    // the current scope, then the new binding is also as the same.
                    Binding {
                        runtime_usage: existing.runtime_usage,
                        synthetic_usage: existing.synthetic_usage,
                        typing_usage: existing.typing_usage,
                        kind: kind.clone(),
                        ..binding
                    }
                }
                _ => Binding {
                    runtime_usage: existing.runtime_usage,
                    synthetic_usage: existing.synthetic_usage,
                    typing_usage: existing.typing_usage,
                    ..binding
                },
            }
        } else {
            binding
        };

        // Don't treat annotations as assignments if there is an existing value
        // in scope.
        if binding.kind.is_annotation() && scope.defines(name) {
            self.ctx.bindings.push(binding);
            return;
        }

        // Add the binding to the scope.
        scope.add(name, binding_id);

        // Add the binding to the arena.
        self.ctx.bindings.push(binding);
    }

    fn bind_builtins(&mut self) {
        let scope = &mut self.ctx.scopes[self.ctx.scope_id];

        for builtin in BUILTINS
            .iter()
            .chain(MAGIC_GLOBALS.iter())
            .copied()
            .chain(self.settings.builtins.iter().map(String::as_str))
        {
            let id = self.ctx.bindings.push(Binding {
                kind: BindingKind::Builtin,
                range: TextRange::default(),
                runtime_usage: None,
                synthetic_usage: Some((ScopeId::global(), TextRange::default())),
                typing_usage: None,
                source: None,
                context: ExecutionContext::Runtime,
                exceptions: Exceptions::empty(),
            });
            scope.add(builtin, id);
        }
    }

    fn handle_node_load(&mut self, expr: &Expr) {
        let Expr::Name(ast::ExprName { id, .. } )= expr else {
            return;
        };
        let id = id.as_str();

        let mut first_iter = true;
        let mut import_starred = false;

        for scope in self.ctx.scopes.ancestors(self.ctx.scope_id) {
            if scope.kind.is_class() {
                if id == "__class__" {
                    return;
                } else if !first_iter {
                    continue;
                }
            }

            if let Some(index) = scope.get(id) {
                // Mark the binding as used.
                let context = self.ctx.execution_context();
                self.ctx.bindings[*index].mark_used(self.ctx.scope_id, expr.range(), context);

                if !self.ctx.in_deferred_type_definition()
                    && self.ctx.bindings[*index].kind.is_annotation()
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
                match &self.ctx.bindings[*index].kind {
                    BindingKind::Importation(Importation { name, full_name })
                    | BindingKind::SubmoduleImportation(SubmoduleImportation { name, full_name }) =>
                    {
                        let has_alias = full_name
                            .split('.')
                            .last()
                            .map(|segment| &segment != name)
                            .unwrap_or_default();
                        if has_alias {
                            // Mark the sub-importation as used.
                            if let Some(index) = scope.get(full_name) {
                                self.ctx.bindings[*index].mark_used(
                                    self.ctx.scope_id,
                                    expr.range(),
                                    context,
                                );
                            }
                        }
                    }
                    BindingKind::FromImportation(FromImportation { name, full_name }) => {
                        let has_alias = full_name
                            .split('.')
                            .last()
                            .map(|segment| &segment != name)
                            .unwrap_or_default();
                        if has_alias {
                            // Mark the sub-importation as used.
                            if let Some(index) = scope.get(full_name.as_str()) {
                                self.ctx.bindings[*index].mark_used(
                                    self.ctx.scope_id,
                                    expr.range(),
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
            import_starred = import_starred || scope.uses_star_imports();
        }

        if import_starred {
            // F405
            if self
                .settings
                .rules
                .enabled(Rule::UndefinedLocalWithImportStarUsage)
            {
                let sources: Vec<String> = self
                    .ctx
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
            return;
        }

        if self.settings.rules.enabled(Rule::UndefinedName) {
            // Allow __path__.
            if self.path.ends_with("__init__.py") && id == "__path__" {
                return;
            }

            // Allow "__module__" and "__qualname__" in class scopes.
            if (id == "__module__" || id == "__qualname__")
                && matches!(self.ctx.scope().kind, ScopeKind::Class(..))
            {
                return;
            }

            // Avoid flagging if NameError is handled.
            if self
                .ctx
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

    fn handle_node_store(&mut self, id: &'a str, expr: &Expr) {
        let parent = self.ctx.stmt();

        if self.settings.rules.enabled(Rule::UndefinedLocal) {
            pyflakes::rules::undefined_local(self, id);
        }

        if self
            .settings
            .rules
            .enabled(Rule::NonLowercaseVariableInFunction)
        {
            if matches!(self.ctx.scope().kind, ScopeKind::Function(..)) {
                // Ignore globals.
                if !self
                    .ctx
                    .scope()
                    .get(id)
                    .map_or(false, |index| self.ctx.bindings[*index].kind.is_global())
                {
                    pep8_naming::rules::non_lowercase_variable_in_function(self, expr, parent, id);
                }
            }
        }

        if self
            .settings
            .rules
            .enabled(Rule::MixedCaseVariableInClassScope)
        {
            if let ScopeKind::Class(class) = &self.ctx.scope().kind {
                pep8_naming::rules::mixed_case_variable_in_class_scope(
                    self,
                    expr,
                    parent,
                    id,
                    class.bases,
                );
            }
        }

        if self
            .settings
            .rules
            .enabled(Rule::MixedCaseVariableInGlobalScope)
        {
            if matches!(self.ctx.scope().kind, ScopeKind::Module) {
                pep8_naming::rules::mixed_case_variable_in_global_scope(self, expr, parent, id);
            }
        }

        if matches!(
            parent,
            Stmt::AnnAssign(ast::StmtAnnAssign { value: None, .. })
        ) {
            self.add_binding(
                id,
                Binding {
                    kind: BindingKind::Annotation,
                    runtime_usage: None,
                    synthetic_usage: None,
                    typing_usage: None,
                    range: expr.range(),
                    source: self.ctx.stmt_id,
                    context: self.ctx.execution_context(),
                    exceptions: self.ctx.exceptions(),
                },
            );
            return;
        }

        if matches!(parent, Stmt::For(_) | Stmt::AsyncFor(_)) {
            self.add_binding(
                id,
                Binding {
                    kind: BindingKind::LoopVar,
                    runtime_usage: None,
                    synthetic_usage: None,
                    typing_usage: None,
                    range: expr.range(),
                    source: self.ctx.stmt_id,
                    context: self.ctx.execution_context(),
                    exceptions: self.ctx.exceptions(),
                },
            );
            return;
        }

        if helpers::is_unpacking_assignment(parent, expr) {
            self.add_binding(
                id,
                Binding {
                    kind: BindingKind::Binding,
                    runtime_usage: None,
                    synthetic_usage: None,
                    typing_usage: None,
                    range: expr.range(),
                    source: self.ctx.stmt_id,
                    context: self.ctx.execution_context(),
                    exceptions: self.ctx.exceptions(),
                },
            );
            return;
        }

        let scope = self.ctx.scope();

        if id == "__all__"
            && scope.kind.is_module()
            && matches!(
                parent,
                Stmt::Assign(_) | Stmt::AugAssign(_) | Stmt::AnnAssign(_)
            )
        {
            if match parent {
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
            } {
                let (all_names, all_names_flags) = {
                    let (mut names, flags) =
                        extract_all_names(parent, |name| self.ctx.is_builtin(name));

                    // Grab the existing bound __all__ values.
                    if let Stmt::AugAssign(_) = parent {
                        if let Some(index) = scope.get("__all__") {
                            if let BindingKind::Export(Export { names: existing }) =
                                &self.ctx.bindings[*index].kind
                            {
                                names.extend_from_slice(existing);
                            }
                        }
                    }

                    (names, flags)
                };

                if self.settings.rules.enabled(Rule::InvalidAllFormat) {
                    if matches!(all_names_flags, AllNamesFlags::INVALID_FORMAT) {
                        self.diagnostics
                            .push(pylint::rules::invalid_all_format(expr));
                    }
                }

                if self.settings.rules.enabled(Rule::InvalidAllObject) {
                    if matches!(all_names_flags, AllNamesFlags::INVALID_OBJECT) {
                        self.diagnostics
                            .push(pylint::rules::invalid_all_object(expr));
                    }
                }

                self.add_binding(
                    id,
                    Binding {
                        kind: BindingKind::Export(Export { names: all_names }),
                        runtime_usage: None,
                        synthetic_usage: None,
                        typing_usage: None,
                        range: expr.range(),
                        source: self.ctx.stmt_id,
                        context: self.ctx.execution_context(),
                        exceptions: self.ctx.exceptions(),
                    },
                );
                return;
            }
        }

        if self
            .ctx
            .expr_ancestors()
            .any(|expr| matches!(expr, Expr::NamedExpr(_)))
        {
            self.add_binding(
                id,
                Binding {
                    kind: BindingKind::NamedExprAssignment,
                    runtime_usage: None,
                    synthetic_usage: None,
                    typing_usage: None,
                    range: expr.range(),
                    source: self.ctx.stmt_id,
                    context: self.ctx.execution_context(),
                    exceptions: self.ctx.exceptions(),
                },
            );
            return;
        }

        self.add_binding(
            id,
            Binding {
                kind: BindingKind::Assignment,
                runtime_usage: None,
                synthetic_usage: None,
                typing_usage: None,
                range: expr.range(),
                source: self.ctx.stmt_id,
                context: self.ctx.execution_context(),
                exceptions: self.ctx.exceptions(),
            },
        );
    }

    fn handle_node_delete(&mut self, expr: &'a Expr) {
        let Expr::Name(ast::ExprName { id, .. } )= expr else {
            return;
        };
        if helpers::on_conditional_branch(&mut self.ctx.parents()) {
            return;
        }

        let scope = self.ctx.scope_mut();
        if scope.remove(id.as_str()).is_some() {
            return;
        }
        if !self.settings.rules.enabled(Rule::UndefinedName) {
            return;
        }

        self.diagnostics.push(Diagnostic::new(
            pyflakes::rules::UndefinedName {
                name: id.to_string(),
            },
            expr.range(),
        ));
    }

    fn check_deferred_future_type_definitions(&mut self) {
        while !self.deferred.future_type_definitions.is_empty() {
            let type_definitions = std::mem::take(&mut self.deferred.future_type_definitions);
            for (expr, snapshot) in type_definitions {
                self.ctx.restore(snapshot);

                self.ctx.flags |=
                    ContextFlags::TYPE_DEFINITION | ContextFlags::FUTURE_TYPE_DEFINITION;
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

                    self.ctx.restore(snapshot);

                    if self.ctx.in_annotation() && self.ctx.future_annotations() {
                        if self.settings.rules.enabled(Rule::QuotedAnnotation) {
                            pyupgrade::rules::quoted_annotation(self, value, range);
                        }
                    }
                    if self.is_stub {
                        if self.settings.rules.enabled(Rule::QuotedAnnotationInStub) {
                            flake8_pyi::rules::quoted_annotation_in_stub(self, value, range);
                        }
                    }

                    let type_definition_flag = match kind {
                        AnnotationKind::Simple => ContextFlags::SIMPLE_STRING_TYPE_DEFINITION,
                        AnnotationKind::Complex => ContextFlags::COMPLEX_STRING_TYPE_DEFINITION,
                    };

                    self.ctx.flags |= ContextFlags::TYPE_DEFINITION | type_definition_flag;
                    self.visit_expr(expr);
                } else {
                    if self
                        .settings
                        .rules
                        .enabled(Rule::ForwardAnnotationSyntaxError)
                    {
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
                self.ctx.restore(snapshot);

                match &self.ctx.stmt() {
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
                self.ctx.restore(snapshot);

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
                self.ctx.restore(snapshot);

                // pyflakes
                if self.settings.rules.enabled(Rule::UnusedVariable) {
                    pyflakes::rules::unused_variable(self, self.ctx.scope_id);
                }
                if self.settings.rules.enabled(Rule::UnusedAnnotation) {
                    pyflakes::rules::unused_annotation(self, self.ctx.scope_id);
                }

                if !self.is_stub {
                    // flake8-unused-arguments
                    if self.settings.rules.any_enabled(&[
                        Rule::UnusedFunctionArgument,
                        Rule::UnusedMethodArgument,
                        Rule::UnusedClassMethodArgument,
                        Rule::UnusedStaticMethodArgument,
                        Rule::UnusedLambdaArgument,
                    ]) {
                        let scope = &self.ctx.scopes[self.ctx.scope_id];
                        let parent = &self.ctx.scopes[scope.parent.unwrap()];
                        self.diagnostics
                            .extend(flake8_unused_arguments::rules::unused_arguments(
                                self,
                                parent,
                                scope,
                                &self.ctx.bindings,
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
                self.ctx.restore(snapshot);

                if let Stmt::For(ast::StmtFor { target, body, .. })
                | Stmt::AsyncFor(ast::StmtAsyncFor { target, body, .. }) = &self.ctx.stmt()
                {
                    if self.settings.rules.enabled(Rule::UnusedLoopControlVariable) {
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
            && self.settings.rules.any_enabled(&[
                Rule::GlobalVariableNotAssigned,
                Rule::RuntimeImportInTypeCheckingBlock,
                Rule::TypingOnlyFirstPartyImport,
                Rule::TypingOnlyThirdPartyImport,
                Rule::TypingOnlyStandardLibraryImport,
            ]);

        if !(enforce_typing_imports
            || self.settings.rules.any_enabled(&[
                Rule::UnusedImport,
                Rule::UndefinedLocalWithImportStarUsage,
                Rule::RedefinedWhileUnused,
                Rule::UndefinedExport,
            ]))
        {
            return;
        }

        // Mark anything referenced in `__all__` as used.
        let all_bindings: Option<(Vec<BindingId>, TextRange)> = {
            let global_scope = self.ctx.global_scope();
            let all_names: Option<(&[&str], TextRange)> = global_scope
                .get("__all__")
                .map(|index| &self.ctx.bindings[*index])
                .and_then(|binding| match &binding.kind {
                    BindingKind::Export(Export { names }) => {
                        Some((names.as_slice(), binding.range))
                    }
                    _ => None,
                });

            all_names.map(|(names, range)| {
                (
                    names
                        .iter()
                        .filter_map(|name| global_scope.get(name).copied())
                        .collect(),
                    range,
                )
            })
        };

        if let Some((bindings, range)) = all_bindings {
            for index in bindings {
                self.ctx.bindings[index].mark_used(
                    ScopeId::global(),
                    range,
                    ExecutionContext::Runtime,
                );
            }
        }

        // Extract `__all__` names from the global scope.
        let all_names: Option<(&[&str], TextRange)> = self
            .ctx
            .global_scope()
            .get("__all__")
            .map(|index| &self.ctx.bindings[*index])
            .and_then(|binding| match &binding.kind {
                BindingKind::Export(Export { names }) => Some((names.as_slice(), binding.range)),
                _ => None,
            });

        // Identify any valid runtime imports. If a module is imported at runtime, and
        // used at runtime, then by default, we avoid flagging any other
        // imports from that model as typing-only.
        let runtime_imports: Vec<Vec<&Binding>> = if enforce_typing_imports {
            if self.settings.flake8_type_checking.strict {
                vec![]
            } else {
                self.ctx
                    .scopes
                    .iter()
                    .map(|scope| {
                        scope
                            .binding_ids()
                            .map(|index| &self.ctx.bindings[*index])
                            .filter(|binding| {
                                flake8_type_checking::helpers::is_valid_runtime_import(binding)
                            })
                            .collect()
                    })
                    .collect::<Vec<_>>()
            }
        } else {
            vec![]
        };

        let mut diagnostics: Vec<Diagnostic> = vec![];
        for scope_id in self.ctx.dead_scopes.iter().rev() {
            let scope = &self.ctx.scopes[*scope_id];

            if scope.kind.is_module() {
                // F822
                if self.settings.rules.enabled(Rule::UndefinedExport) {
                    if !self.path.ends_with("__init__.py") {
                        if let Some((names, range)) = all_names {
                            diagnostics
                                .extend(pyflakes::rules::undefined_export(names, range, scope));
                        }
                    }
                }

                // F405
                if self
                    .settings
                    .rules
                    .enabled(Rule::UndefinedLocalWithImportStarUsage)
                {
                    if let Some((names, range)) = &all_names {
                        let sources: Vec<String> = scope
                            .star_imports()
                            .map(|StarImportation { level, module }| {
                                helpers::format_import_from(*level, *module)
                            })
                            .sorted()
                            .dedup()
                            .collect();
                        if !sources.is_empty() {
                            for name in names.iter() {
                                if !scope.defines(name) {
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
            }

            // PLW0602
            if self.settings.rules.enabled(Rule::GlobalVariableNotAssigned) {
                for (name, index) in scope.bindings() {
                    let binding = &self.ctx.bindings[*index];
                    if binding.kind.is_global() {
                        if let Some(source) = binding.source {
                            let stmt = &self.ctx.stmts[source];
                            if matches!(stmt, Stmt::Global(_)) {
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

            // Look for any bindings that were redefined in another scope, and remain
            // unused. Note that we only store references in `redefinitions` if
            // the bindings are in different scopes.
            if self.settings.rules.enabled(Rule::RedefinedWhileUnused) {
                for (name, index) in scope.bindings() {
                    let binding = &self.ctx.bindings[*index];

                    if matches!(
                        binding.kind,
                        BindingKind::Importation(..)
                            | BindingKind::FromImportation(..)
                            | BindingKind::SubmoduleImportation(..)
                            | BindingKind::FutureImportation
                    ) {
                        if binding.used() {
                            continue;
                        }

                        if let Some(indices) = self.ctx.shadowed_bindings.get(index) {
                            for index in indices {
                                let rebound = &self.ctx.bindings[*index];
                                #[allow(deprecated)]
                                let line = self.locator.compute_line_index(binding.range.start());

                                let mut diagnostic = Diagnostic::new(
                                    pyflakes::rules::RedefinedWhileUnused {
                                        name: (*name).to_string(),
                                        line,
                                    },
                                    matches!(
                                        rebound.kind,
                                        BindingKind::ClassDefinition
                                            | BindingKind::FunctionDefinition
                                    )
                                    .then(|| {
                                        rebound.source.map_or(rebound.range, |source| {
                                            helpers::identifier_range(
                                                self.ctx.stmts[source],
                                                self.locator,
                                            )
                                        })
                                    })
                                    .unwrap_or(rebound.range),
                                );
                                if let Some(source) = rebound.source {
                                    let parent = &self.ctx.stmts[source];
                                    if matches!(parent, Stmt::ImportFrom(_))
                                        && parent.range().contains_range(rebound.range)
                                    {
                                        diagnostic.set_parent(parent.start());
                                    }
                                };
                                diagnostics.push(diagnostic);
                            }
                        }
                    }
                }
            }

            if enforce_typing_imports {
                let runtime_imports: Vec<&Binding> = if self.settings.flake8_type_checking.strict {
                    vec![]
                } else {
                    self.ctx
                        .scopes
                        .ancestor_ids(*scope_id)
                        .flat_map(|scope_id| runtime_imports[usize::from(scope_id)].iter())
                        .copied()
                        .collect()
                };
                for index in scope.binding_ids() {
                    let binding = &self.ctx.bindings[*index];

                    if let Some(diagnostic) =
                        flake8_type_checking::rules::runtime_import_in_type_checking_block(binding)
                    {
                        if self.settings.rules.enabled(diagnostic.kind.rule()) {
                            diagnostics.push(diagnostic);
                        }
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

            if self.settings.rules.enabled(Rule::UnusedImport) {
                // Collect all unused imports by location. (Multiple unused imports at the same
                // location indicates an `import from`.)
                type UnusedImport<'a> = (&'a str, &'a TextRange);
                type BindingContext<'a> = (NodeId, Option<NodeId>, Exceptions);

                let mut unused: FxHashMap<BindingContext, Vec<UnusedImport>> = FxHashMap::default();
                let mut ignored: FxHashMap<BindingContext, Vec<UnusedImport>> =
                    FxHashMap::default();

                for index in scope.binding_ids() {
                    let binding = &self.ctx.bindings[*index];

                    let full_name = match &binding.kind {
                        BindingKind::Importation(Importation { full_name, .. }) => full_name,
                        BindingKind::FromImportation(FromImportation { full_name, .. }) => {
                            full_name.as_str()
                        }
                        BindingKind::SubmoduleImportation(SubmoduleImportation {
                            full_name,
                            ..
                        }) => full_name,
                        _ => continue,
                    };

                    if binding.used() {
                        continue;
                    }

                    let child_id = binding.source.unwrap();
                    let parent_id = self.ctx.stmts.parent_id(child_id);

                    let exceptions = binding.exceptions;
                    let diagnostic_offset = binding.range.start();
                    let child = &self.ctx.stmts[child_id];
                    let parent_offset = if matches!(child, Stmt::ImportFrom(_)) {
                        Some(child.start())
                    } else {
                        None
                    };

                    if self.rule_is_ignored(Rule::UnusedImport, diagnostic_offset)
                        || parent_offset.map_or(false, |parent_offset| {
                            self.rule_is_ignored(Rule::UnusedImport, parent_offset)
                        })
                    {
                        ignored
                            .entry((child_id, parent_id, exceptions))
                            .or_default()
                            .push((full_name, &binding.range));
                    } else {
                        unused
                            .entry((child_id, parent_id, exceptions))
                            .or_default()
                            .push((full_name, &binding.range));
                    }
                }

                let in_init =
                    self.settings.ignore_init_module_imports && self.path.ends_with("__init__.py");
                for ((defined_by, defined_in, exceptions), unused_imports) in unused
                    .into_iter()
                    .sorted_by_key(|((defined_by, ..), ..)| *defined_by)
                {
                    let child = self.ctx.stmts[defined_by];
                    let parent = defined_in.map(|defined_in| self.ctx.stmts[defined_in]);
                    let multiple = unused_imports.len() > 1;
                    let in_except_handler = exceptions
                        .intersects(Exceptions::MODULE_NOT_FOUND_ERROR | Exceptions::IMPORT_ERROR);

                    let fix = if !in_init && !in_except_handler && self.patch(Rule::UnusedImport) {
                        let deleted: Vec<&Stmt> = self.deletions.iter().map(Into::into).collect();
                        match autofix::actions::remove_unused_imports(
                            unused_imports.iter().map(|(full_name, _)| *full_name),
                            child,
                            parent,
                            &deleted,
                            self.locator,
                            self.indexer,
                            self.stylist,
                        ) {
                            Ok(fix) => {
                                if fix.is_deletion() || fix.content() == Some("pass") {
                                    self.deletions.insert(RefEquality(child));
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
                        let mut diagnostic = Diagnostic::new(
                            pyflakes::rules::UnusedImport {
                                name: full_name.to_string(),
                                context: if in_except_handler {
                                    Some(pyflakes::rules::UnusedImportContext::ExceptHandler)
                                } else if in_init {
                                    Some(pyflakes::rules::UnusedImportContext::Init)
                                } else {
                                    None
                                },
                                multiple,
                            },
                            *range,
                        );
                        if matches!(child, Stmt::ImportFrom(_)) {
                            diagnostic.set_parent(child.start());
                        }

                        if let Some(edit) = &fix {
                            #[allow(deprecated)]
                            diagnostic.set_fix(Fix::unspecified(edit.clone()));
                        }
                        diagnostics.push(diagnostic);
                    }
                }
                for ((child, .., exceptions), unused_imports) in ignored
                    .into_iter()
                    .sorted_by_key(|((defined_by, ..), ..)| *defined_by)
                {
                    let child = self.ctx.stmts[child];
                    let multiple = unused_imports.len() > 1;
                    let in_except_handler = exceptions
                        .intersects(Exceptions::MODULE_NOT_FOUND_ERROR | Exceptions::IMPORT_ERROR);
                    for (full_name, range) in unused_imports {
                        let mut diagnostic = Diagnostic::new(
                            pyflakes::rules::UnusedImport {
                                name: full_name.to_string(),
                                context: if in_except_handler {
                                    Some(pyflakes::rules::UnusedImportContext::ExceptHandler)
                                } else if in_init {
                                    Some(pyflakes::rules::UnusedImportContext::Init)
                                } else {
                                    None
                                },
                                multiple,
                            },
                            *range,
                        );
                        if matches!(child, Stmt::ImportFrom(_)) {
                            diagnostic.set_parent(child.start());
                        }
                        diagnostics.push(diagnostic);
                    }
                }
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
        let enforce_annotations = self.settings.rules.any_enabled(&[
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
        let enforce_stubs =
            self.is_stub && self.settings.rules.any_enabled(&[Rule::DocstringInStub]);
        let enforce_docstrings = self.settings.rules.any_enabled(&[
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
        let global_scope = self.ctx.global_scope();
        let exports: Option<&[&str]> = global_scope
            .get("__all__")
            .map(|index| &self.ctx.bindings[*index])
            .and_then(|binding| match &binding.kind {
                BindingKind::Export(Export { names }) => Some(names.as_slice()),
                _ => None,
            });
        let definitions = std::mem::take(&mut self.ctx.definitions);

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
                        self,
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
                overloaded_name = flake8_annotations::helpers::overloaded_name(self, definition);
            }

            // flake8-pyi
            if enforce_stubs {
                if self.is_stub {
                    if self.settings.rules.enabled(Rule::DocstringInStub) {
                        flake8_pyi::rules::docstring_in_stubs(self, docstring);
                    }
                }
            }

            // pydocstyle
            if enforce_docstrings {
                if pydocstyle::helpers::should_ignore_definition(
                    self,
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

                if self.settings.rules.enabled(Rule::FitsOnOneLine) {
                    pydocstyle::rules::one_liner(self, &docstring);
                }
                if self.settings.rules.any_enabled(&[
                    Rule::NoBlankLineBeforeFunction,
                    Rule::NoBlankLineAfterFunction,
                ]) {
                    pydocstyle::rules::blank_before_after_function(self, &docstring);
                }
                if self.settings.rules.any_enabled(&[
                    Rule::OneBlankLineBeforeClass,
                    Rule::OneBlankLineAfterClass,
                    Rule::BlankLineBeforeClass,
                ]) {
                    pydocstyle::rules::blank_before_after_class(self, &docstring);
                }
                if self.settings.rules.enabled(Rule::BlankLineAfterSummary) {
                    pydocstyle::rules::blank_after_summary(self, &docstring);
                }
                if self.settings.rules.any_enabled(&[
                    Rule::IndentWithSpaces,
                    Rule::UnderIndentation,
                    Rule::OverIndentation,
                ]) {
                    pydocstyle::rules::indent(self, &docstring);
                }
                if self.settings.rules.enabled(Rule::NewLineAfterLastParagraph) {
                    pydocstyle::rules::newline_after_last_paragraph(self, &docstring);
                }
                if self.settings.rules.enabled(Rule::SurroundingWhitespace) {
                    pydocstyle::rules::no_surrounding_whitespace(self, &docstring);
                }
                if self.settings.rules.any_enabled(&[
                    Rule::MultiLineSummaryFirstLine,
                    Rule::MultiLineSummarySecondLine,
                ]) {
                    pydocstyle::rules::multi_line_summary_start(self, &docstring);
                }
                if self.settings.rules.enabled(Rule::TripleSingleQuotes) {
                    pydocstyle::rules::triple_quotes(self, &docstring);
                }
                if self.settings.rules.enabled(Rule::EscapeSequenceInDocstring) {
                    pydocstyle::rules::backslashes(self, &docstring);
                }
                if self.settings.rules.enabled(Rule::EndsInPeriod) {
                    pydocstyle::rules::ends_with_period(self, &docstring);
                }
                if self.settings.rules.enabled(Rule::NonImperativeMood) {
                    pydocstyle::rules::non_imperative_mood(
                        self,
                        &docstring,
                        &self.settings.pydocstyle.property_decorators,
                    );
                }
                if self.settings.rules.enabled(Rule::NoSignature) {
                    pydocstyle::rules::no_signature(self, &docstring);
                }
                if self.settings.rules.enabled(Rule::FirstLineCapitalized) {
                    pydocstyle::rules::capitalized(self, &docstring);
                }
                if self.settings.rules.enabled(Rule::DocstringStartsWithThis) {
                    pydocstyle::rules::starts_with_this(self, &docstring);
                }
                if self.settings.rules.enabled(Rule::EndsInPunctuation) {
                    pydocstyle::rules::ends_with_punctuation(self, &docstring);
                }
                if self.settings.rules.enabled(Rule::OverloadWithDocstring) {
                    pydocstyle::rules::if_needed(self, &docstring);
                }
                if self.settings.rules.any_enabled(&[
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
    checker.ctx.scope_id = ScopeId::global();
    checker.ctx.dead_scopes.push(ScopeId::global());
    checker.check_dead_scopes();

    checker.diagnostics
}
