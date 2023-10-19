use ruff_diagnostics::Diagnostic;
use ruff_python_ast::helpers;
use ruff_python_ast::types::Node;
use ruff_python_ast::{self as ast, Expr, Stmt};
use ruff_python_semantic::analyze::typing;
use ruff_python_semantic::ScopeKind;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::registry::Rule;
use crate::rules::{
    airflow, flake8_bandit, flake8_boolean_trap, flake8_bugbear, flake8_builtins, flake8_debugger,
    flake8_django, flake8_errmsg, flake8_import_conventions, flake8_pie, flake8_pyi,
    flake8_pytest_style, flake8_raise, flake8_return, flake8_simplify, flake8_slots,
    flake8_tidy_imports, flake8_type_checking, mccabe, pandas_vet, pep8_naming, perflint,
    pycodestyle, pyflakes, pygrep_hooks, pylint, pyupgrade, refurb, ruff, tryceratops,
};
use crate::settings::types::PythonVersion;

/// Run lint rules over a [`Stmt`] syntax node.
pub(crate) fn statement(stmt: &Stmt, checker: &mut Checker) {
    match stmt {
        Stmt::Global(ast::StmtGlobal { names, range: _ }) => {
            if checker.enabled(Rule::GlobalAtModuleLevel) {
                pylint::rules::global_at_module_level(checker, stmt);
            }
            if checker.enabled(Rule::AmbiguousVariableName) {
                checker.diagnostics.extend(names.iter().filter_map(|name| {
                    pycodestyle::rules::ambiguous_variable_name(name, name.range())
                }));
            }
        }
        Stmt::Nonlocal(ast::StmtNonlocal { names, range: _ }) => {
            if checker.enabled(Rule::AmbiguousVariableName) {
                checker.diagnostics.extend(names.iter().filter_map(|name| {
                    pycodestyle::rules::ambiguous_variable_name(name, name.range())
                }));
            }
            if checker.enabled(Rule::NonlocalWithoutBinding) {
                if !checker.semantic.scope_id.is_global() {
                    for name in names {
                        if checker.semantic.nonlocal(name).is_none() {
                            checker.diagnostics.push(Diagnostic::new(
                                pylint::rules::NonlocalWithoutBinding {
                                    name: name.to_string(),
                                },
                                name.range(),
                            ));
                        }
                    }
                }
            }
        }
        Stmt::Break(_) => {
            if checker.enabled(Rule::BreakOutsideLoop) {
                if let Some(diagnostic) = pyflakes::rules::break_outside_loop(
                    stmt,
                    &mut checker.semantic.current_statements().skip(1),
                ) {
                    checker.diagnostics.push(diagnostic);
                }
            }
        }
        Stmt::Continue(_) => {
            if checker.enabled(Rule::ContinueOutsideLoop) {
                if let Some(diagnostic) = pyflakes::rules::continue_outside_loop(
                    stmt,
                    &mut checker.semantic.current_statements().skip(1),
                ) {
                    checker.diagnostics.push(diagnostic);
                }
            }
        }
        Stmt::FunctionDef(
            function_def @ ast::StmtFunctionDef {
                is_async,
                name,
                decorator_list,
                returns,
                parameters,
                body,
                type_params,
                range: _,
            },
        ) => {
            if checker.enabled(Rule::DjangoNonLeadingReceiverDecorator) {
                flake8_django::rules::non_leading_receiver_decorator(checker, decorator_list);
            }
            if checker.enabled(Rule::AmbiguousFunctionName) {
                if let Some(diagnostic) = pycodestyle::rules::ambiguous_function_name(name) {
                    checker.diagnostics.push(diagnostic);
                }
            }
            if checker.enabled(Rule::InvalidStrReturnType) {
                pylint::rules::invalid_str_return(checker, name, body);
            }
            if checker.enabled(Rule::InvalidFunctionName) {
                if let Some(diagnostic) = pep8_naming::rules::invalid_function_name(
                    stmt,
                    name,
                    decorator_list,
                    &checker.settings.pep8_naming.ignore_names,
                    &checker.semantic,
                ) {
                    checker.diagnostics.push(diagnostic);
                }
            }
            if checker.enabled(Rule::InvalidFirstArgumentNameForClassMethod) {
                if let Some(diagnostic) =
                    pep8_naming::rules::invalid_first_argument_name_for_class_method(
                        checker,
                        checker.semantic.current_scope(),
                        name,
                        decorator_list,
                        parameters,
                    )
                {
                    checker.diagnostics.push(diagnostic);
                }
            }
            if checker.enabled(Rule::InvalidFirstArgumentNameForMethod) {
                if let Some(diagnostic) = pep8_naming::rules::invalid_first_argument_name_for_method(
                    checker,
                    checker.semantic.current_scope(),
                    name,
                    decorator_list,
                    parameters,
                ) {
                    checker.diagnostics.push(diagnostic);
                }
            }
            if checker.source_type.is_stub() {
                if checker.enabled(Rule::PassStatementStubBody) {
                    flake8_pyi::rules::pass_statement_stub_body(checker, body);
                }
                if checker.enabled(Rule::NonEmptyStubBody) {
                    flake8_pyi::rules::non_empty_stub_body(checker, body);
                }
                if checker.enabled(Rule::StubBodyMultipleStatements) {
                    flake8_pyi::rules::stub_body_multiple_statements(checker, stmt, body);
                }
            }
            if checker.enabled(Rule::AnyEqNeAnnotation) {
                flake8_pyi::rules::any_eq_ne_annotation(checker, name, parameters);
            }
            if checker.enabled(Rule::NonSelfReturnType) {
                flake8_pyi::rules::non_self_return_type(
                    checker,
                    stmt,
                    *is_async,
                    name,
                    decorator_list,
                    returns.as_ref().map(AsRef::as_ref),
                    parameters,
                );
            }
            if checker.enabled(Rule::CustomTypeVarReturnType) {
                flake8_pyi::rules::custom_type_var_return_type(
                    checker,
                    name,
                    decorator_list,
                    returns.as_ref().map(AsRef::as_ref),
                    parameters,
                    type_params.as_ref(),
                );
            }
            if checker.source_type.is_stub() {
                if checker.enabled(Rule::StrOrReprDefinedInStub) {
                    flake8_pyi::rules::str_or_repr_defined_in_stub(checker, stmt);
                }
            }
            if checker.source_type.is_stub()
                || checker.settings.target_version >= PythonVersion::Py311
            {
                if checker.enabled(Rule::NoReturnArgumentAnnotationInStub) {
                    flake8_pyi::rules::no_return_argument_annotation(checker, parameters);
                }
            }
            if checker.enabled(Rule::BadExitAnnotation) {
                flake8_pyi::rules::bad_exit_annotation(checker, *is_async, name, parameters);
            }
            if checker.enabled(Rule::RedundantNumericUnion) {
                flake8_pyi::rules::redundant_numeric_union(checker, parameters);
            }
            if checker.enabled(Rule::DunderFunctionName) {
                if let Some(diagnostic) = pep8_naming::rules::dunder_function_name(
                    checker.semantic.current_scope(),
                    stmt,
                    name,
                    &checker.settings.pep8_naming.ignore_names,
                ) {
                    checker.diagnostics.push(diagnostic);
                }
            }
            if checker.enabled(Rule::GlobalStatement) {
                pylint::rules::global_statement(checker, name);
            }
            if checker.enabled(Rule::LRUCacheWithoutParameters) {
                if checker.settings.target_version >= PythonVersion::Py38 {
                    pyupgrade::rules::lru_cache_without_parameters(checker, decorator_list);
                }
            }
            if checker.enabled(Rule::LRUCacheWithMaxsizeNone) {
                if checker.settings.target_version >= PythonVersion::Py39 {
                    pyupgrade::rules::lru_cache_with_maxsize_none(checker, decorator_list);
                }
            }
            if checker.enabled(Rule::CachedInstanceMethod) {
                flake8_bugbear::rules::cached_instance_method(checker, decorator_list);
            }
            if checker.enabled(Rule::MutableArgumentDefault) {
                flake8_bugbear::rules::mutable_argument_default(checker, function_def);
            }
            if checker.any_enabled(&[
                Rule::UnnecessaryReturnNone,
                Rule::ImplicitReturnValue,
                Rule::ImplicitReturn,
                Rule::UnnecessaryAssign,
                Rule::SuperfluousElseReturn,
                Rule::SuperfluousElseRaise,
                Rule::SuperfluousElseContinue,
                Rule::SuperfluousElseBreak,
            ]) {
                flake8_return::rules::function(checker, body, returns.as_ref().map(AsRef::as_ref));
            }
            if checker.enabled(Rule::UselessReturn) {
                pylint::rules::useless_return(
                    checker,
                    stmt,
                    body,
                    returns.as_ref().map(AsRef::as_ref),
                );
            }
            if checker.enabled(Rule::ComplexStructure) {
                if let Some(diagnostic) = mccabe::rules::function_is_too_complex(
                    stmt,
                    name,
                    body,
                    checker.settings.mccabe.max_complexity,
                ) {
                    checker.diagnostics.push(diagnostic);
                }
            }
            if checker.enabled(Rule::HardcodedPasswordDefault) {
                flake8_bandit::rules::hardcoded_password_default(checker, parameters);
            }
            if checker.enabled(Rule::PropertyWithParameters) {
                pylint::rules::property_with_parameters(checker, stmt, decorator_list, parameters);
            }
            if checker.enabled(Rule::TooManyArguments) {
                pylint::rules::too_many_arguments(checker, parameters, stmt);
            }
            if checker.enabled(Rule::TooManyReturnStatements) {
                if let Some(diagnostic) = pylint::rules::too_many_return_statements(
                    stmt,
                    body,
                    checker.settings.pylint.max_returns,
                ) {
                    checker.diagnostics.push(diagnostic);
                }
            }
            if checker.enabled(Rule::TooManyBranches) {
                if let Some(diagnostic) = pylint::rules::too_many_branches(
                    stmt,
                    body,
                    checker.settings.pylint.max_branches,
                ) {
                    checker.diagnostics.push(diagnostic);
                }
            }
            if checker.enabled(Rule::TooManyStatements) {
                if let Some(diagnostic) = pylint::rules::too_many_statements(
                    stmt,
                    body,
                    checker.settings.pylint.max_statements,
                ) {
                    checker.diagnostics.push(diagnostic);
                }
            }
            if checker.any_enabled(&[
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
                    checker,
                    stmt,
                    name,
                    parameters,
                    returns.as_deref(),
                    decorator_list,
                    body,
                );
            }
            if checker.any_enabled(&[
                Rule::PytestParametrizeNamesWrongType,
                Rule::PytestParametrizeValuesWrongType,
                Rule::PytestDuplicateParametrizeTestCases,
            ]) {
                flake8_pytest_style::rules::parametrize(checker, decorator_list);
            }
            if checker.any_enabled(&[
                Rule::PytestIncorrectMarkParenthesesStyle,
                Rule::PytestUseFixturesWithoutParameters,
            ]) {
                flake8_pytest_style::rules::marks(checker, decorator_list);
            }
            if checker.enabled(Rule::BooleanTypeHintPositionalArgument) {
                flake8_boolean_trap::rules::boolean_type_hint_positional_argument(
                    checker,
                    name,
                    decorator_list,
                    parameters,
                );
            }
            if checker.enabled(Rule::BooleanDefaultValuePositionalArgument) {
                flake8_boolean_trap::rules::boolean_default_value_positional_argument(
                    checker,
                    name,
                    decorator_list,
                    parameters,
                );
            }
            if checker.enabled(Rule::UnexpectedSpecialMethodSignature) {
                pylint::rules::unexpected_special_method_signature(
                    checker,
                    stmt,
                    name,
                    decorator_list,
                    parameters,
                );
            }
            if checker.enabled(Rule::FStringDocstring) {
                flake8_bugbear::rules::f_string_docstring(checker, body);
            }
            if let ScopeKind::Class(class_def) = checker.semantic.current_scope().kind {
                if checker.enabled(Rule::BuiltinAttributeShadowing) {
                    flake8_builtins::rules::builtin_method_shadowing(
                        checker,
                        class_def,
                        name,
                        decorator_list,
                        name.range(),
                    );
                }
            } else {
                if checker.enabled(Rule::BuiltinVariableShadowing) {
                    flake8_builtins::rules::builtin_variable_shadowing(checker, name, name.range());
                }
            }
            #[cfg(feature = "unreachable-code")]
            if checker.enabled(Rule::UnreachableCode) {
                checker
                    .diagnostics
                    .extend(ruff::rules::unreachable::in_function(name, body));
            }
        }
        Stmt::Return(_) => {
            if checker.enabled(Rule::ReturnOutsideFunction) {
                pyflakes::rules::return_outside_function(checker, stmt);
            }
            if checker.enabled(Rule::ReturnInInit) {
                pylint::rules::return_in_init(checker, stmt);
            }
        }
        Stmt::ClassDef(
            class_def @ ast::StmtClassDef {
                name,
                arguments,
                type_params: _,
                decorator_list,
                body,
                range: _,
            },
        ) => {
            if checker.enabled(Rule::DjangoNullableModelStringField) {
                flake8_django::rules::nullable_model_string_field(checker, body);
            }
            if checker.enabled(Rule::DjangoExcludeWithModelForm) {
                if let Some(diagnostic) = flake8_django::rules::exclude_with_model_form(
                    checker,
                    arguments.as_deref(),
                    body,
                ) {
                    checker.diagnostics.push(diagnostic);
                }
            }
            if checker.enabled(Rule::DjangoAllWithModelForm) {
                if let Some(diagnostic) =
                    flake8_django::rules::all_with_model_form(checker, arguments.as_deref(), body)
                {
                    checker.diagnostics.push(diagnostic);
                }
            }
            if checker.enabled(Rule::DjangoUnorderedBodyContentInModel) {
                flake8_django::rules::unordered_body_content_in_model(
                    checker,
                    arguments.as_deref(),
                    body,
                );
            }
            if !checker.source_type.is_stub() {
                if checker.enabled(Rule::DjangoModelWithoutDunderStr) {
                    flake8_django::rules::model_without_dunder_str(checker, class_def);
                }
            }
            if checker.enabled(Rule::EqWithoutHash) {
                pylint::rules::object_without_hash_method(checker, class_def);
            }
            if checker.enabled(Rule::TooManyPublicMethods) {
                pylint::rules::too_many_public_methods(
                    checker,
                    class_def,
                    checker.settings.pylint.max_public_methods,
                );
            }
            if checker.enabled(Rule::GlobalStatement) {
                pylint::rules::global_statement(checker, name);
            }
            if checker.enabled(Rule::UselessObjectInheritance) {
                pyupgrade::rules::useless_object_inheritance(checker, class_def);
            }
            if checker.enabled(Rule::UnnecessaryClassParentheses) {
                pyupgrade::rules::unnecessary_class_parentheses(checker, class_def);
            }
            if checker.enabled(Rule::AmbiguousClassName) {
                if let Some(diagnostic) = pycodestyle::rules::ambiguous_class_name(name) {
                    checker.diagnostics.push(diagnostic);
                }
            }
            if checker.enabled(Rule::InvalidClassName) {
                if let Some(diagnostic) = pep8_naming::rules::invalid_class_name(
                    stmt,
                    name,
                    &checker.settings.pep8_naming.ignore_names,
                ) {
                    checker.diagnostics.push(diagnostic);
                }
            }
            if checker.enabled(Rule::ErrorSuffixOnExceptionName) {
                if let Some(diagnostic) = pep8_naming::rules::error_suffix_on_exception_name(
                    stmt,
                    arguments.as_deref(),
                    name,
                    &checker.settings.pep8_naming.ignore_names,
                ) {
                    checker.diagnostics.push(diagnostic);
                }
            }
            if !checker.source_type.is_stub() {
                if checker.any_enabled(&[
                    Rule::AbstractBaseClassWithoutAbstractMethod,
                    Rule::EmptyMethodWithoutAbstractDecorator,
                ]) {
                    flake8_bugbear::rules::abstract_base_class(
                        checker,
                        stmt,
                        name,
                        arguments.as_deref(),
                        body,
                    );
                }
            }
            if checker.source_type.is_stub() {
                if checker.enabled(Rule::PassStatementStubBody) {
                    flake8_pyi::rules::pass_statement_stub_body(checker, body);
                }
                if checker.enabled(Rule::PassInClassBody) {
                    flake8_pyi::rules::pass_in_class_body(checker, class_def);
                }
            }
            if checker.enabled(Rule::EllipsisInNonEmptyClassBody) {
                flake8_pyi::rules::ellipsis_in_non_empty_class_body(checker, body);
            }
            if checker.enabled(Rule::PytestIncorrectMarkParenthesesStyle) {
                flake8_pytest_style::rules::marks(checker, decorator_list);
            }
            if checker.enabled(Rule::DuplicateClassFieldDefinition) {
                flake8_pie::rules::duplicate_class_field_definition(checker, body);
            }
            if checker.enabled(Rule::NonUniqueEnums) {
                flake8_pie::rules::non_unique_enums(checker, stmt, body);
            }
            if checker.enabled(Rule::MutableClassDefault) {
                ruff::rules::mutable_class_default(checker, class_def);
            }
            if checker.enabled(Rule::MutableDataclassDefault) {
                ruff::rules::mutable_dataclass_default(checker, class_def);
            }
            if checker.enabled(Rule::FunctionCallInDataclassDefaultArgument) {
                ruff::rules::function_call_in_dataclass_default(checker, class_def);
            }
            if checker.enabled(Rule::FStringDocstring) {
                flake8_bugbear::rules::f_string_docstring(checker, body);
            }
            if checker.enabled(Rule::BuiltinVariableShadowing) {
                flake8_builtins::rules::builtin_variable_shadowing(checker, name, name.range());
            }
            if checker.enabled(Rule::DuplicateBases) {
                pylint::rules::duplicate_bases(checker, name, arguments.as_deref());
            }
            if checker.enabled(Rule::NoSlotsInStrSubclass) {
                flake8_slots::rules::no_slots_in_str_subclass(checker, stmt, class_def);
            }
            if checker.enabled(Rule::NoSlotsInTupleSubclass) {
                flake8_slots::rules::no_slots_in_tuple_subclass(checker, stmt, class_def);
            }
            if checker.enabled(Rule::NoSlotsInNamedtupleSubclass) {
                flake8_slots::rules::no_slots_in_namedtuple_subclass(checker, stmt, class_def);
            }
            if checker.enabled(Rule::SingleStringSlots) {
                pylint::rules::single_string_slots(checker, class_def);
            }
            if checker.enabled(Rule::BadDunderMethodName) {
                pylint::rules::bad_dunder_method_name(checker, body);
            }
        }
        Stmt::Import(ast::StmtImport { names, range: _ }) => {
            if checker.enabled(Rule::MultipleImportsOnOneLine) {
                pycodestyle::rules::multiple_imports_on_one_line(checker, stmt, names);
            }
            if checker.enabled(Rule::ModuleImportNotAtTopOfFile) {
                pycodestyle::rules::module_import_not_at_top_of_file(checker, stmt);
            }
            if checker.enabled(Rule::GlobalStatement) {
                for name in names {
                    if let Some(asname) = name.asname.as_ref() {
                        pylint::rules::global_statement(checker, asname);
                    } else {
                        pylint::rules::global_statement(checker, &name.name);
                    }
                }
            }
            if checker.enabled(Rule::DeprecatedCElementTree) {
                pyupgrade::rules::deprecated_c_element_tree(checker, stmt);
            }
            if checker.enabled(Rule::DeprecatedMockImport) {
                pyupgrade::rules::deprecated_mock_import(checker, stmt);
            }

            for alias in names {
                if let Some(asname) = &alias.asname {
                    if checker.enabled(Rule::BuiltinVariableShadowing) {
                        flake8_builtins::rules::builtin_variable_shadowing(
                            checker,
                            asname,
                            asname.range(),
                        );
                    }
                }
                if checker.enabled(Rule::Debugger) {
                    if let Some(diagnostic) =
                        flake8_debugger::rules::debugger_import(stmt, None, &alias.name)
                    {
                        checker.diagnostics.push(diagnostic);
                    }
                }
                if checker.enabled(Rule::BannedApi) {
                    flake8_tidy_imports::rules::banned_api(
                        checker,
                        &flake8_tidy_imports::matchers::NameMatchPolicy::MatchNameOrParent(
                            flake8_tidy_imports::matchers::MatchNameOrParent {
                                module: &alias.name,
                            },
                        ),
                        &alias,
                    );
                }

                if checker.enabled(Rule::BannedModuleLevelImports) {
                    flake8_tidy_imports::rules::banned_module_level_imports(
                        checker,
                        &flake8_tidy_imports::matchers::NameMatchPolicy::MatchNameOrParent(
                            flake8_tidy_imports::matchers::MatchNameOrParent {
                                module: &alias.name,
                            },
                        ),
                        &alias,
                    );
                }

                if !checker.source_type.is_stub() {
                    if checker.enabled(Rule::UselessImportAlias) {
                        pylint::rules::useless_import_alias(checker, alias);
                    }
                }
                if checker.enabled(Rule::ManualFromImport) {
                    pylint::rules::manual_from_import(checker, stmt, alias, names);
                }
                if checker.enabled(Rule::ImportSelf) {
                    if let Some(diagnostic) = pylint::rules::import_self(alias, checker.module_path)
                    {
                        checker.diagnostics.push(diagnostic);
                    }
                }
                if let Some(asname) = &alias.asname {
                    let name = alias.name.split('.').last().unwrap();
                    if checker.enabled(Rule::ConstantImportedAsNonConstant) {
                        if let Some(diagnostic) =
                            pep8_naming::rules::constant_imported_as_non_constant(
                                name,
                                asname,
                                alias,
                                stmt,
                                &checker.settings.pep8_naming.ignore_names,
                            )
                        {
                            checker.diagnostics.push(diagnostic);
                        }
                    }
                    if checker.enabled(Rule::LowercaseImportedAsNonLowercase) {
                        if let Some(diagnostic) =
                            pep8_naming::rules::lowercase_imported_as_non_lowercase(
                                name,
                                asname,
                                alias,
                                stmt,
                                &checker.settings.pep8_naming.ignore_names,
                            )
                        {
                            checker.diagnostics.push(diagnostic);
                        }
                    }
                    if checker.enabled(Rule::CamelcaseImportedAsLowercase) {
                        if let Some(diagnostic) =
                            pep8_naming::rules::camelcase_imported_as_lowercase(
                                name,
                                asname,
                                alias,
                                stmt,
                                &checker.settings.pep8_naming.ignore_names,
                            )
                        {
                            checker.diagnostics.push(diagnostic);
                        }
                    }
                    if checker.enabled(Rule::CamelcaseImportedAsConstant) {
                        if let Some(diagnostic) = pep8_naming::rules::camelcase_imported_as_constant(
                            name,
                            asname,
                            alias,
                            stmt,
                            &checker.settings.pep8_naming.ignore_names,
                        ) {
                            checker.diagnostics.push(diagnostic);
                        }
                    }
                    if checker.enabled(Rule::CamelcaseImportedAsAcronym) {
                        if let Some(diagnostic) = pep8_naming::rules::camelcase_imported_as_acronym(
                            name,
                            asname,
                            alias,
                            stmt,
                            &checker.settings.pep8_naming.ignore_names,
                        ) {
                            checker.diagnostics.push(diagnostic);
                        }
                    }
                }
                if checker.enabled(Rule::BannedImportAlias) {
                    if let Some(asname) = &alias.asname {
                        if let Some(diagnostic) =
                            flake8_import_conventions::rules::banned_import_alias(
                                stmt,
                                &alias.name,
                                asname,
                                &checker.settings.flake8_import_conventions.banned_aliases,
                            )
                        {
                            checker.diagnostics.push(diagnostic);
                        }
                    }
                }
                if checker.enabled(Rule::PytestIncorrectPytestImport) {
                    if let Some(diagnostic) = flake8_pytest_style::rules::import(
                        stmt,
                        &alias.name,
                        alias.asname.as_deref(),
                    ) {
                        checker.diagnostics.push(diagnostic);
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
            let level = *level;
            if checker.enabled(Rule::ModuleImportNotAtTopOfFile) {
                pycodestyle::rules::module_import_not_at_top_of_file(checker, stmt);
            }
            if checker.enabled(Rule::GlobalStatement) {
                for name in names {
                    if let Some(asname) = name.asname.as_ref() {
                        pylint::rules::global_statement(checker, asname);
                    } else {
                        pylint::rules::global_statement(checker, &name.name);
                    }
                }
            }
            if checker.enabled(Rule::UnnecessaryFutureImport) {
                if checker.settings.target_version >= PythonVersion::Py37 {
                    if let Some("__future__") = module {
                        pyupgrade::rules::unnecessary_future_import(checker, stmt, names);
                    }
                }
            }
            if checker.enabled(Rule::DeprecatedMockImport) {
                pyupgrade::rules::deprecated_mock_import(checker, stmt);
            }
            if checker.enabled(Rule::DeprecatedCElementTree) {
                pyupgrade::rules::deprecated_c_element_tree(checker, stmt);
            }
            if checker.enabled(Rule::DeprecatedImport) {
                pyupgrade::rules::deprecated_import(checker, stmt, names, module, level);
            }
            if checker.enabled(Rule::UnnecessaryBuiltinImport) {
                if let Some(module) = module {
                    pyupgrade::rules::unnecessary_builtin_import(checker, stmt, module, names);
                }
            }
            if checker.enabled(Rule::BannedApi) {
                if let Some(module) =
                    helpers::resolve_imported_module_path(level, module, checker.module_path)
                {
                    flake8_tidy_imports::rules::banned_api(
                        checker,
                        &flake8_tidy_imports::matchers::NameMatchPolicy::MatchNameOrParent(
                            flake8_tidy_imports::matchers::MatchNameOrParent { module: &module },
                        ),
                        &stmt,
                    );

                    for alias in names {
                        if &alias.name == "*" {
                            continue;
                        }
                        flake8_tidy_imports::rules::banned_api(
                            checker,
                            &flake8_tidy_imports::matchers::NameMatchPolicy::MatchName(
                                flake8_tidy_imports::matchers::MatchName {
                                    module: &module,
                                    member: &alias.name,
                                },
                            ),
                            &alias,
                        );
                    }
                }
            }
            if checker.enabled(Rule::BannedModuleLevelImports) {
                if let Some(module) =
                    helpers::resolve_imported_module_path(level, module, checker.module_path)
                {
                    flake8_tidy_imports::rules::banned_module_level_imports(
                        checker,
                        &flake8_tidy_imports::matchers::NameMatchPolicy::MatchNameOrParent(
                            flake8_tidy_imports::matchers::MatchNameOrParent { module: &module },
                        ),
                        &stmt,
                    );

                    for alias in names {
                        if &alias.name == "*" {
                            continue;
                        }
                        flake8_tidy_imports::rules::banned_module_level_imports(
                            checker,
                            &flake8_tidy_imports::matchers::NameMatchPolicy::MatchName(
                                flake8_tidy_imports::matchers::MatchName {
                                    module: &module,
                                    member: &alias.name,
                                },
                            ),
                            &alias,
                        );
                    }
                }
            }
            if checker.enabled(Rule::PytestIncorrectPytestImport) {
                if let Some(diagnostic) =
                    flake8_pytest_style::rules::import_from(stmt, module, level)
                {
                    checker.diagnostics.push(diagnostic);
                }
            }
            if checker.source_type.is_stub() {
                if checker.enabled(Rule::FutureAnnotationsInStub) {
                    flake8_pyi::rules::from_future_import(checker, import_from);
                }
            }
            for alias in names {
                if let Some("__future__") = module {
                    if checker.enabled(Rule::FutureFeatureNotDefined) {
                        pyflakes::rules::future_feature_not_defined(checker, alias);
                    }
                    if checker.enabled(Rule::LateFutureImport) {
                        if checker.semantic.seen_futures_boundary() {
                            checker.diagnostics.push(Diagnostic::new(
                                pyflakes::rules::LateFutureImport,
                                stmt.range(),
                            ));
                        }
                    }
                } else if &alias.name == "*" {
                    if checker.enabled(Rule::UndefinedLocalWithNestedImportStarUsage) {
                        if !matches!(checker.semantic.current_scope().kind, ScopeKind::Module) {
                            checker.diagnostics.push(Diagnostic::new(
                                pyflakes::rules::UndefinedLocalWithNestedImportStarUsage {
                                    name: helpers::format_import_from(level, module),
                                },
                                stmt.range(),
                            ));
                        }
                    }
                    if checker.enabled(Rule::UndefinedLocalWithImportStar) {
                        checker.diagnostics.push(Diagnostic::new(
                            pyflakes::rules::UndefinedLocalWithImportStar {
                                name: helpers::format_import_from(level, module),
                            },
                            stmt.range(),
                        ));
                    }
                } else {
                    if let Some(asname) = &alias.asname {
                        if checker.enabled(Rule::BuiltinVariableShadowing) {
                            flake8_builtins::rules::builtin_variable_shadowing(
                                checker,
                                asname,
                                asname.range(),
                            );
                        }
                    }
                }
                if checker.enabled(Rule::RelativeImports) {
                    if let Some(diagnostic) = flake8_tidy_imports::rules::banned_relative_import(
                        checker,
                        stmt,
                        level,
                        module,
                        checker.module_path,
                        checker.settings.flake8_tidy_imports.ban_relative_imports,
                    ) {
                        checker.diagnostics.push(diagnostic);
                    }
                }
                if checker.enabled(Rule::Debugger) {
                    if let Some(diagnostic) =
                        flake8_debugger::rules::debugger_import(stmt, module, &alias.name)
                    {
                        checker.diagnostics.push(diagnostic);
                    }
                }
                if checker.enabled(Rule::BannedImportAlias) {
                    if let Some(asname) = &alias.asname {
                        let qualified_name =
                            helpers::format_import_from_member(level, module, &alias.name);
                        if let Some(diagnostic) =
                            flake8_import_conventions::rules::banned_import_alias(
                                stmt,
                                &qualified_name,
                                asname,
                                &checker.settings.flake8_import_conventions.banned_aliases,
                            )
                        {
                            checker.diagnostics.push(diagnostic);
                        }
                    }
                }
                if let Some(asname) = &alias.asname {
                    if checker.enabled(Rule::ConstantImportedAsNonConstant) {
                        if let Some(diagnostic) =
                            pep8_naming::rules::constant_imported_as_non_constant(
                                &alias.name,
                                asname,
                                alias,
                                stmt,
                                &checker.settings.pep8_naming.ignore_names,
                            )
                        {
                            checker.diagnostics.push(diagnostic);
                        }
                    }
                    if checker.enabled(Rule::LowercaseImportedAsNonLowercase) {
                        if let Some(diagnostic) =
                            pep8_naming::rules::lowercase_imported_as_non_lowercase(
                                &alias.name,
                                asname,
                                alias,
                                stmt,
                                &checker.settings.pep8_naming.ignore_names,
                            )
                        {
                            checker.diagnostics.push(diagnostic);
                        }
                    }
                    if checker.enabled(Rule::CamelcaseImportedAsLowercase) {
                        if let Some(diagnostic) =
                            pep8_naming::rules::camelcase_imported_as_lowercase(
                                &alias.name,
                                asname,
                                alias,
                                stmt,
                                &checker.settings.pep8_naming.ignore_names,
                            )
                        {
                            checker.diagnostics.push(diagnostic);
                        }
                    }
                    if checker.enabled(Rule::CamelcaseImportedAsConstant) {
                        if let Some(diagnostic) = pep8_naming::rules::camelcase_imported_as_constant(
                            &alias.name,
                            asname,
                            alias,
                            stmt,
                            &checker.settings.pep8_naming.ignore_names,
                        ) {
                            checker.diagnostics.push(diagnostic);
                        }
                    }
                    if checker.enabled(Rule::CamelcaseImportedAsAcronym) {
                        if let Some(diagnostic) = pep8_naming::rules::camelcase_imported_as_acronym(
                            &alias.name,
                            asname,
                            alias,
                            stmt,
                            &checker.settings.pep8_naming.ignore_names,
                        ) {
                            checker.diagnostics.push(diagnostic);
                        }
                    }
                    if !checker.source_type.is_stub() {
                        if checker.enabled(Rule::UselessImportAlias) {
                            pylint::rules::useless_import_alias(checker, alias);
                        }
                    }
                }
            }
            if checker.enabled(Rule::ImportSelf) {
                if let Some(diagnostic) =
                    pylint::rules::import_from_self(level, module, names, checker.module_path)
                {
                    checker.diagnostics.push(diagnostic);
                }
            }
            if checker.enabled(Rule::BannedImportFrom) {
                if let Some(diagnostic) = flake8_import_conventions::rules::banned_import_from(
                    stmt,
                    &helpers::format_import_from(level, module),
                    &checker.settings.flake8_import_conventions.banned_from,
                ) {
                    checker.diagnostics.push(diagnostic);
                }
            }
        }
        Stmt::Raise(raise @ ast::StmtRaise { exc, .. }) => {
            if checker.enabled(Rule::RaiseNotImplemented) {
                if let Some(expr) = exc {
                    pyflakes::rules::raise_not_implemented(checker, expr);
                }
            }
            if checker.enabled(Rule::RaiseLiteral) {
                if let Some(exc) = exc {
                    flake8_bugbear::rules::raise_literal(checker, exc);
                }
            }
            if checker.any_enabled(&[
                Rule::RawStringInException,
                Rule::FStringInException,
                Rule::DotFormatInException,
            ]) {
                if let Some(exc) = exc {
                    flake8_errmsg::rules::string_in_exception(checker, stmt, exc);
                }
            }
            if checker.enabled(Rule::OSErrorAlias) {
                if let Some(item) = exc {
                    pyupgrade::rules::os_error_alias_raise(checker, item);
                }
            }
            if checker.enabled(Rule::RaiseVanillaClass) {
                if let Some(expr) = exc {
                    tryceratops::rules::raise_vanilla_class(checker, expr);
                }
            }
            if checker.enabled(Rule::RaiseVanillaArgs) {
                if let Some(expr) = exc {
                    tryceratops::rules::raise_vanilla_args(checker, expr);
                }
            }
            if checker.enabled(Rule::UnnecessaryParenOnRaiseException) {
                if let Some(expr) = exc {
                    flake8_raise::rules::unnecessary_paren_on_raise_exception(checker, expr);
                }
            }
            if checker.enabled(Rule::MisplacedBareRaise) {
                pylint::rules::misplaced_bare_raise(checker, raise);
            }
        }
        Stmt::AugAssign(ast::StmtAugAssign { target, .. }) => {
            if checker.enabled(Rule::GlobalStatement) {
                if let Expr::Name(ast::ExprName { id, .. }) = target.as_ref() {
                    pylint::rules::global_statement(checker, id);
                }
            }
        }
        Stmt::If(
            if_ @ ast::StmtIf {
                test,
                elif_else_clauses,
                ..
            },
        ) => {
            if checker.enabled(Rule::EmptyTypeCheckingBlock) {
                if typing::is_type_checking_block(if_, &checker.semantic) {
                    flake8_type_checking::rules::empty_type_checking_block(checker, if_);
                }
            }
            if checker.enabled(Rule::IfTuple) {
                pyflakes::rules::if_tuple(checker, if_);
            }
            if checker.enabled(Rule::CollapsibleIf) {
                flake8_simplify::rules::nested_if_statements(
                    checker,
                    if_,
                    checker.semantic.current_statement_parent(),
                );
            }
            if checker.enabled(Rule::IfWithSameArms) {
                flake8_simplify::rules::if_with_same_arms(checker, checker.locator, if_);
            }
            if checker.enabled(Rule::NeedlessBool) {
                flake8_simplify::rules::needless_bool(checker, stmt);
            }
            if checker.enabled(Rule::IfElseBlockInsteadOfDictLookup) {
                flake8_simplify::rules::manual_dict_lookup(checker, if_);
            }
            if checker.enabled(Rule::IfElseBlockInsteadOfIfExp) {
                flake8_simplify::rules::use_ternary_operator(checker, stmt);
            }
            if checker.enabled(Rule::IfElseBlockInsteadOfDictGet) {
                flake8_simplify::rules::use_dict_get_with_default(checker, if_);
            }
            if checker.enabled(Rule::TypeCheckWithoutTypeError) {
                tryceratops::rules::type_check_without_type_error(
                    checker,
                    if_,
                    checker.semantic.current_statement_parent(),
                );
            }
            if checker.enabled(Rule::OutdatedVersionBlock) {
                pyupgrade::rules::outdated_version_block(checker, if_);
            }
            if checker.enabled(Rule::CollapsibleElseIf) {
                if let Some(diagnostic) = pylint::rules::collapsible_else_if(elif_else_clauses) {
                    checker.diagnostics.push(diagnostic);
                }
            }
            if checker.enabled(Rule::CheckAndRemoveFromSet) {
                refurb::rules::check_and_remove_from_set(checker, if_);
            }
            if checker.enabled(Rule::TooManyBooleanExpressions) {
                pylint::rules::too_many_boolean_expressions(checker, if_);
            }
            if checker.source_type.is_stub() {
                if checker.any_enabled(&[
                    Rule::UnrecognizedVersionInfoCheck,
                    Rule::PatchVersionComparison,
                    Rule::WrongTupleLengthVersionComparison,
                ]) {
                    if let Expr::BoolOp(ast::ExprBoolOp { values, .. }) = test.as_ref() {
                        for value in values {
                            flake8_pyi::rules::unrecognized_version_info(checker, value);
                        }
                    } else {
                        flake8_pyi::rules::unrecognized_version_info(checker, test);
                    }
                }
                if checker.any_enabled(&[
                    Rule::UnrecognizedPlatformCheck,
                    Rule::UnrecognizedPlatformName,
                ]) {
                    if let Expr::BoolOp(ast::ExprBoolOp { values, .. }) = test.as_ref() {
                        for value in values {
                            flake8_pyi::rules::unrecognized_platform(checker, value);
                        }
                    } else {
                        flake8_pyi::rules::unrecognized_platform(checker, test);
                    }
                }
                if checker.enabled(Rule::BadVersionInfoComparison) {
                    if let Expr::BoolOp(ast::ExprBoolOp { values, .. }) = test.as_ref() {
                        for value in values {
                            flake8_pyi::rules::bad_version_info_comparison(checker, value);
                        }
                    } else {
                        flake8_pyi::rules::bad_version_info_comparison(checker, test);
                    }
                }
                if checker.enabled(Rule::ComplexIfStatementInStub) {
                    if let Expr::BoolOp(ast::ExprBoolOp { values, .. }) = test.as_ref() {
                        for value in values {
                            flake8_pyi::rules::complex_if_statement_in_stub(checker, value);
                        }
                    } else {
                        flake8_pyi::rules::complex_if_statement_in_stub(checker, test);
                    }
                }
            }
        }
        Stmt::Assert(ast::StmtAssert {
            test,
            msg,
            range: _,
        }) => {
            if !checker.semantic.in_type_checking_block() {
                if checker.enabled(Rule::Assert) {
                    checker
                        .diagnostics
                        .push(flake8_bandit::rules::assert_used(stmt));
                }
            }
            if checker.enabled(Rule::AssertTuple) {
                pyflakes::rules::assert_tuple(checker, stmt, test);
            }
            if checker.enabled(Rule::AssertFalse) {
                flake8_bugbear::rules::assert_false(checker, stmt, test, msg.as_deref());
            }
            if checker.enabled(Rule::PytestAssertAlwaysFalse) {
                flake8_pytest_style::rules::assert_falsy(checker, stmt, test);
            }
            if checker.enabled(Rule::PytestCompositeAssertion) {
                flake8_pytest_style::rules::composite_condition(
                    checker,
                    stmt,
                    test,
                    msg.as_deref(),
                );
            }
            if checker.enabled(Rule::AssertOnStringLiteral) {
                pylint::rules::assert_on_string_literal(checker, test);
            }
            if checker.enabled(Rule::InvalidMockAccess) {
                pygrep_hooks::rules::non_existent_mock_method(checker, test);
            }
        }
        Stmt::With(with_stmt @ ast::StmtWith { items, body, .. }) => {
            if checker.enabled(Rule::AssertRaisesException) {
                flake8_bugbear::rules::assert_raises_exception(checker, items);
            }
            if checker.enabled(Rule::PytestRaisesWithMultipleStatements) {
                flake8_pytest_style::rules::complex_raises(checker, stmt, items, body);
            }
            if checker.enabled(Rule::MultipleWithStatements) {
                flake8_simplify::rules::multiple_with_statements(
                    checker,
                    with_stmt,
                    checker.semantic.current_statement_parent(),
                );
            }
            if checker.enabled(Rule::RedefinedLoopName) {
                pylint::rules::redefined_loop_name(checker, stmt);
            }
        }
        Stmt::While(ast::StmtWhile { body, orelse, .. }) => {
            if checker.enabled(Rule::FunctionUsesLoopVariable) {
                flake8_bugbear::rules::function_uses_loop_variable(checker, &Node::Stmt(stmt));
            }
            if checker.enabled(Rule::UselessElseOnLoop) {
                pylint::rules::useless_else_on_loop(checker, stmt, body, orelse);
            }
            if checker.enabled(Rule::TryExceptInLoop) {
                perflint::rules::try_except_in_loop(checker, body);
            }
        }
        Stmt::For(
            for_stmt @ ast::StmtFor {
                target,
                body,
                iter,
                orelse,
                is_async,
                range: _,
            },
        ) => {
            if checker.any_enabled(&[
                Rule::UnusedLoopControlVariable,
                Rule::IncorrectDictIterator,
                Rule::UnnecessaryEnumerate,
                Rule::YieldInForLoop,
            ]) {
                checker.deferred.for_loops.push(checker.semantic.snapshot());
            }
            if checker.enabled(Rule::LoopVariableOverridesIterator) {
                flake8_bugbear::rules::loop_variable_overrides_iterator(checker, target, iter);
            }
            if checker.enabled(Rule::FunctionUsesLoopVariable) {
                flake8_bugbear::rules::function_uses_loop_variable(checker, &Node::Stmt(stmt));
            }
            if checker.enabled(Rule::ReuseOfGroupbyGenerator) {
                flake8_bugbear::rules::reuse_of_groupby_generator(checker, target, body, iter);
            }
            if checker.enabled(Rule::UselessElseOnLoop) {
                pylint::rules::useless_else_on_loop(checker, stmt, body, orelse);
            }
            if checker.enabled(Rule::RedefinedLoopName) {
                pylint::rules::redefined_loop_name(checker, stmt);
            }
            if checker.enabled(Rule::IterationOverSet) {
                pylint::rules::iteration_over_set(checker, iter);
            }
            if checker.enabled(Rule::ManualListComprehension) {
                perflint::rules::manual_list_comprehension(checker, target, body);
            }
            if checker.enabled(Rule::ManualListCopy) {
                perflint::rules::manual_list_copy(checker, target, body);
            }
            if checker.enabled(Rule::ManualDictComprehension) {
                perflint::rules::manual_dict_comprehension(checker, target, body);
            }
            if checker.enabled(Rule::UnnecessaryListCast) {
                perflint::rules::unnecessary_list_cast(checker, iter);
            }
            if !is_async {
                if checker.enabled(Rule::ReimplementedBuiltin) {
                    flake8_simplify::rules::convert_for_loop_to_any_all(checker, stmt);
                }
                if checker.enabled(Rule::InDictKeys) {
                    flake8_simplify::rules::key_in_dict_for(checker, for_stmt);
                }
                if checker.enabled(Rule::TryExceptInLoop) {
                    perflint::rules::try_except_in_loop(checker, body);
                }
            }
        }
        Stmt::Try(ast::StmtTry {
            body,
            handlers,
            orelse,
            finalbody,
            ..
        }) => {
            if checker.enabled(Rule::JumpStatementInFinally) {
                flake8_bugbear::rules::jump_statement_in_finally(checker, finalbody);
            }
            if checker.enabled(Rule::ContinueInFinally) {
                if checker.settings.target_version <= PythonVersion::Py38 {
                    pylint::rules::continue_in_finally(checker, finalbody);
                }
            }
            if checker.enabled(Rule::DefaultExceptNotLast) {
                if let Some(diagnostic) =
                    pyflakes::rules::default_except_not_last(handlers, checker.locator)
                {
                    checker.diagnostics.push(diagnostic);
                }
            }
            if checker.any_enabled(&[
                Rule::DuplicateHandlerException,
                Rule::DuplicateTryBlockException,
            ]) {
                flake8_bugbear::rules::duplicate_exceptions(checker, handlers);
            }
            if checker.enabled(Rule::RedundantTupleInExceptionHandler) {
                flake8_bugbear::rules::redundant_tuple_in_exception_handler(checker, handlers);
            }
            if checker.enabled(Rule::OSErrorAlias) {
                pyupgrade::rules::os_error_alias_handlers(checker, handlers);
            }
            if checker.enabled(Rule::PytestAssertInExcept) {
                flake8_pytest_style::rules::assert_in_exception_handler(checker, handlers);
            }
            if checker.enabled(Rule::SuppressibleException) {
                flake8_simplify::rules::suppressible_exception(
                    checker, stmt, body, handlers, orelse, finalbody,
                );
            }
            if checker.enabled(Rule::ReturnInTryExceptFinally) {
                flake8_simplify::rules::return_in_try_except_finally(
                    checker, body, handlers, finalbody,
                );
            }
            if checker.enabled(Rule::TryConsiderElse) {
                tryceratops::rules::try_consider_else(checker, body, orelse, handlers);
            }
            if checker.enabled(Rule::VerboseRaise) {
                tryceratops::rules::verbose_raise(checker, handlers);
            }
            if checker.enabled(Rule::VerboseLogMessage) {
                tryceratops::rules::verbose_log_message(checker, handlers);
            }
            if checker.enabled(Rule::RaiseWithinTry) {
                tryceratops::rules::raise_within_try(checker, body, handlers);
            }
            if checker.enabled(Rule::UselessTryExcept) {
                tryceratops::rules::useless_try_except(checker, handlers);
            }
            if checker.enabled(Rule::ErrorInsteadOfException) {
                tryceratops::rules::error_instead_of_exception(checker, handlers);
            }
        }
        Stmt::Assign(assign @ ast::StmtAssign { targets, value, .. }) => {
            if checker.enabled(Rule::LambdaAssignment) {
                if let [target] = &targets[..] {
                    pycodestyle::rules::lambda_assignment(checker, target, value, None, stmt);
                }
            }
            if checker.enabled(Rule::AssignmentToOsEnviron) {
                flake8_bugbear::rules::assignment_to_os_environ(checker, targets);
            }
            if checker.enabled(Rule::HardcodedPasswordString) {
                flake8_bandit::rules::assign_hardcoded_password_string(checker, value, targets);
            }
            if checker.enabled(Rule::GlobalStatement) {
                for target in targets {
                    if let Expr::Name(ast::ExprName { id, .. }) = target {
                        pylint::rules::global_statement(checker, id);
                    }
                }
            }
            if checker.enabled(Rule::UselessMetaclassType) {
                pyupgrade::rules::useless_metaclass_type(checker, stmt, value, targets);
            }
            if checker.enabled(Rule::ConvertTypedDictFunctionalToClass) {
                pyupgrade::rules::convert_typed_dict_functional_to_class(
                    checker, stmt, targets, value,
                );
            }
            if checker.enabled(Rule::ConvertNamedTupleFunctionalToClass) {
                pyupgrade::rules::convert_named_tuple_functional_to_class(
                    checker, stmt, targets, value,
                );
            }
            if checker.enabled(Rule::UnpackedListComprehension) {
                pyupgrade::rules::unpacked_list_comprehension(checker, targets, value);
            }
            if checker.enabled(Rule::PandasDfVariableName) {
                if let Some(diagnostic) = pandas_vet::rules::assignment_to_df(targets) {
                    checker.diagnostics.push(diagnostic);
                }
            }
            if checker
                .settings
                .rules
                .enabled(Rule::AirflowVariableNameTaskIdMismatch)
            {
                if let Some(diagnostic) =
                    airflow::rules::variable_name_task_id(checker, targets, value)
                {
                    checker.diagnostics.push(diagnostic);
                }
            }
            if checker.settings.rules.enabled(Rule::SelfAssigningVariable) {
                if let [target] = targets.as_slice() {
                    pylint::rules::self_assigning_variable(checker, target, value);
                }
            }
            if checker.settings.rules.enabled(Rule::TypeParamNameMismatch) {
                pylint::rules::type_param_name_mismatch(checker, value, targets);
            }
            if checker
                .settings
                .rules
                .enabled(Rule::TypeNameIncorrectVariance)
            {
                pylint::rules::type_name_incorrect_variance(checker, value);
            }
            if checker.settings.rules.enabled(Rule::TypeBivariance) {
                pylint::rules::type_bivariance(checker, value);
            }
            if checker.source_type.is_stub() {
                if checker.any_enabled(&[
                    Rule::UnprefixedTypeParam,
                    Rule::AssignmentDefaultInStub,
                    Rule::UnannotatedAssignmentInStub,
                    Rule::ComplexAssignmentInStub,
                    Rule::TypeAliasWithoutAnnotation,
                ]) {
                    // Ignore assignments in function bodies; those are covered by other rules.
                    if !checker
                        .semantic
                        .current_scopes()
                        .any(|scope| scope.kind.is_function())
                    {
                        if checker.enabled(Rule::UnprefixedTypeParam) {
                            flake8_pyi::rules::prefix_type_params(checker, value, targets);
                        }
                        if checker.enabled(Rule::AssignmentDefaultInStub) {
                            flake8_pyi::rules::assignment_default_in_stub(checker, targets, value);
                        }
                        if checker.enabled(Rule::UnannotatedAssignmentInStub) {
                            flake8_pyi::rules::unannotated_assignment_in_stub(
                                checker, targets, value,
                            );
                        }
                        if checker.enabled(Rule::ComplexAssignmentInStub) {
                            flake8_pyi::rules::complex_assignment_in_stub(checker, assign);
                        }
                        if checker.enabled(Rule::TypeAliasWithoutAnnotation) {
                            flake8_pyi::rules::type_alias_without_annotation(
                                checker, value, targets,
                            );
                        }
                    }
                }
            }
        }
        Stmt::AnnAssign(
            assign_stmt @ ast::StmtAnnAssign {
                target,
                value,
                annotation,
                ..
            },
        ) => {
            if let Some(value) = value {
                if checker.enabled(Rule::LambdaAssignment) {
                    pycodestyle::rules::lambda_assignment(
                        checker,
                        target,
                        value,
                        Some(annotation),
                        stmt,
                    );
                }
                if checker.enabled(Rule::SelfAssigningVariable) {
                    pylint::rules::self_assigning_variable(checker, target, value);
                }
            }
            if checker.enabled(Rule::UnintentionalTypeAnnotation) {
                flake8_bugbear::rules::unintentional_type_annotation(
                    checker,
                    target,
                    value.as_deref(),
                    stmt,
                );
            }
            if checker.enabled(Rule::NonPEP695TypeAlias) {
                pyupgrade::rules::non_pep695_type_alias(checker, assign_stmt);
            }
            if checker.source_type.is_stub() {
                if let Some(value) = value {
                    if checker.enabled(Rule::AssignmentDefaultInStub) {
                        // Ignore assignments in function bodies; those are covered by other rules.
                        if !checker
                            .semantic
                            .current_scopes()
                            .any(|scope| scope.kind.is_function())
                        {
                            flake8_pyi::rules::annotated_assignment_default_in_stub(
                                checker, target, value, annotation,
                            );
                        }
                    }
                } else {
                    if checker.enabled(Rule::UnassignedSpecialVariableInStub) {
                        flake8_pyi::rules::unassigned_special_variable_in_stub(
                            checker, target, stmt,
                        );
                    }
                }
            }
            if checker.semantic.match_typing_expr(annotation, "TypeAlias") {
                if checker.enabled(Rule::SnakeCaseTypeAlias) {
                    flake8_pyi::rules::snake_case_type_alias(checker, target);
                }
                if checker.enabled(Rule::TSuffixedTypeAlias) {
                    flake8_pyi::rules::t_suffixed_type_alias(checker, target);
                }
            }
        }
        Stmt::Delete(delete @ ast::StmtDelete { targets, range: _ }) => {
            if checker.enabled(Rule::GlobalStatement) {
                for target in targets {
                    if let Expr::Name(ast::ExprName { id, .. }) = target {
                        pylint::rules::global_statement(checker, id);
                    }
                }
            }
            if checker.enabled(Rule::DeleteFullSlice) {
                refurb::rules::delete_full_slice(checker, delete);
            }
        }
        Stmt::Expr(ast::StmtExpr { value, range: _ }) => {
            if checker.enabled(Rule::UselessComparison) {
                flake8_bugbear::rules::useless_comparison(checker, value);
            }
            if checker.enabled(Rule::UselessExpression) {
                flake8_bugbear::rules::useless_expression(checker, value);
            }
            if checker.enabled(Rule::InvalidMockAccess) {
                pygrep_hooks::rules::uncalled_mock_method(checker, value);
            }
            if checker.enabled(Rule::NamedExprWithoutContext) {
                pylint::rules::named_expr_without_context(checker, value);
            }
            if checker.enabled(Rule::AsyncioDanglingTask) {
                ruff::rules::asyncio_dangling_task(checker, value);
            }
            if checker.enabled(Rule::RepeatedAppend) {
                refurb::rules::repeated_append(checker, stmt);
            }
        }
        _ => {}
    }
}
