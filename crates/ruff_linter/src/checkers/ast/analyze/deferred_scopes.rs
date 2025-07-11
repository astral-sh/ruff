use ruff_python_semantic::{Binding, ScopeKind};

use crate::checkers::ast::Checker;
use crate::codes::Rule;
use crate::rules::{
    flake8_builtins, flake8_pyi, flake8_type_checking, flake8_unused_arguments, pep8_naming,
    pyflakes, pylint, ruff,
};

/// Run lint rules over all deferred scopes in the [`SemanticModel`].
pub(crate) fn deferred_scopes(checker: &Checker) {
    if !checker.any_rule_enabled(&[
        Rule::AsyncioDanglingTask,
        Rule::BadStaticmethodArgument,
        Rule::BuiltinAttributeShadowing,
        Rule::FunctionCallInDataclassDefaultArgument,
        Rule::GlobalVariableNotAssigned,
        Rule::ImportPrivateName,
        Rule::ImportShadowedByLoopVar,
        Rule::InvalidFirstArgumentNameForClassMethod,
        Rule::InvalidFirstArgumentNameForMethod,
        Rule::MutableClassDefault,
        Rule::MutableDataclassDefault,
        Rule::NoSelfUse,
        Rule::RedefinedArgumentFromLocal,
        Rule::RedefinedWhileUnused,
        Rule::RuntimeImportInTypeCheckingBlock,
        Rule::SingledispatchMethod,
        Rule::SingledispatchmethodFunction,
        Rule::TooManyLocals,
        Rule::TypingOnlyFirstPartyImport,
        Rule::TypingOnlyStandardLibraryImport,
        Rule::TypingOnlyThirdPartyImport,
        Rule::UndefinedLocal,
        Rule::UnusedAnnotation,
        Rule::UnusedClassMethodArgument,
        Rule::UnusedFunctionArgument,
        Rule::UnusedImport,
        Rule::UnusedLambdaArgument,
        Rule::UnusedMethodArgument,
        Rule::UnusedPrivateProtocol,
        Rule::UnusedPrivateTypeAlias,
        Rule::UnusedPrivateTypedDict,
        Rule::UnusedPrivateTypeVar,
        Rule::UnusedStaticMethodArgument,
        Rule::UnusedUnpackedVariable,
        Rule::UnusedVariable,
    ]) {
        return;
    }

    // Identify any valid runtime imports. If a module is imported at runtime, and
    // used at runtime, then by default, we avoid flagging any other
    // imports from that model as typing-only.
    let enforce_typing_only_imports = !checker.source_type.is_stub()
        && checker.any_rule_enabled(&[
            Rule::TypingOnlyFirstPartyImport,
            Rule::TypingOnlyStandardLibraryImport,
            Rule::TypingOnlyThirdPartyImport,
        ]);
    let runtime_imports: Vec<Vec<&Binding>> = if enforce_typing_only_imports {
        checker
            .semantic
            .scopes
            .iter()
            .map(|scope| {
                scope
                    .binding_ids()
                    .map(|binding_id| checker.semantic.binding(binding_id))
                    .filter(|binding| {
                        flake8_type_checking::helpers::is_valid_runtime_import(
                            binding,
                            &checker.semantic,
                            checker.settings(),
                        )
                    })
                    .collect()
            })
            .collect::<Vec<_>>()
    } else {
        vec![]
    };

    for scope_id in checker.analyze.scopes.iter().rev().copied() {
        let scope = &checker.semantic.scopes[scope_id];

        if checker.is_rule_enabled(Rule::UndefinedLocal) {
            pyflakes::rules::undefined_local(checker, scope_id, scope);
        }

        if checker.is_rule_enabled(Rule::GlobalVariableNotAssigned) {
            pylint::rules::global_variable_not_assigned(checker, scope);
        }

        if checker.is_rule_enabled(Rule::RedefinedArgumentFromLocal) {
            pylint::rules::redefined_argument_from_local(checker, scope_id, scope);
        }

        if checker.is_rule_enabled(Rule::ImportShadowedByLoopVar) {
            pyflakes::rules::import_shadowed_by_loop_var(checker, scope_id, scope);
        }

        if checker.is_rule_enabled(Rule::RedefinedWhileUnused) {
            pyflakes::rules::redefined_while_unused(checker, scope_id, scope);
        }

        if checker.source_type.is_stub()
            || matches!(scope.kind, ScopeKind::Module | ScopeKind::Function(_))
        {
            if checker.is_rule_enabled(Rule::UnusedPrivateTypeVar) {
                flake8_pyi::rules::unused_private_type_var(checker, scope);
            }
            if checker.is_rule_enabled(Rule::UnusedPrivateProtocol) {
                flake8_pyi::rules::unused_private_protocol(checker, scope);
            }
            if checker.is_rule_enabled(Rule::UnusedPrivateTypeAlias) {
                flake8_pyi::rules::unused_private_type_alias(checker, scope);
            }
            if checker.is_rule_enabled(Rule::UnusedPrivateTypedDict) {
                flake8_pyi::rules::unused_private_typed_dict(checker, scope);
            }
        }

        if checker.is_rule_enabled(Rule::AsyncioDanglingTask) {
            ruff::rules::asyncio_dangling_binding(scope, checker);
        }

        if let Some(class_def) = scope.kind.as_class() {
            if checker.is_rule_enabled(Rule::BuiltinAttributeShadowing) {
                flake8_builtins::rules::builtin_attribute_shadowing(
                    checker, scope_id, scope, class_def,
                );
            }
            if checker.is_rule_enabled(Rule::FunctionCallInDataclassDefaultArgument) {
                ruff::rules::function_call_in_dataclass_default(checker, class_def);
            }
            if checker.is_rule_enabled(Rule::MutableClassDefault) {
                ruff::rules::mutable_class_default(checker, class_def);
            }
            if checker.is_rule_enabled(Rule::MutableDataclassDefault) {
                ruff::rules::mutable_dataclass_default(checker, class_def);
            }
        }

        if matches!(scope.kind, ScopeKind::Function(_) | ScopeKind::Lambda(_)) {
            if checker.any_rule_enabled(&[Rule::UnusedVariable, Rule::UnusedUnpackedVariable])
                && !(scope.uses_locals() && scope.kind.is_function())
            {
                let unused_bindings = scope
                    .bindings()
                    .map(|(name, binding_id)| (name, checker.semantic().binding(binding_id)))
                    .filter_map(|(name, binding)| {
                        if (binding.kind.is_assignment()
                            || binding.kind.is_named_expr_assignment()
                            || binding.kind.is_with_item_var())
                            && binding.is_unused()
                            && !binding.is_nonlocal()
                            && !binding.is_global()
                            && !checker.settings().dummy_variable_rgx.is_match(name)
                            && !matches!(
                                name,
                                "__tracebackhide__"
                                    | "__traceback_info__"
                                    | "__traceback_supplement__"
                                    | "__debuggerskip__"
                            )
                        {
                            return Some((name, binding));
                        }

                        None
                    });

                for (unused_name, unused_binding) in unused_bindings {
                    if checker.is_rule_enabled(Rule::UnusedVariable) {
                        pyflakes::rules::unused_variable(checker, unused_name, unused_binding);
                    }

                    if checker.is_rule_enabled(Rule::UnusedUnpackedVariable) {
                        ruff::rules::unused_unpacked_variable(checker, unused_name, unused_binding);
                    }
                }
            }

            if checker.is_rule_enabled(Rule::UnusedAnnotation) {
                pyflakes::rules::unused_annotation(checker, scope);
            }

            if !checker.source_type.is_stub() {
                if checker.any_rule_enabled(&[
                    Rule::UnusedClassMethodArgument,
                    Rule::UnusedFunctionArgument,
                    Rule::UnusedLambdaArgument,
                    Rule::UnusedMethodArgument,
                    Rule::UnusedStaticMethodArgument,
                ]) {
                    flake8_unused_arguments::rules::unused_arguments(checker, scope);
                }
            }
        }

        if matches!(scope.kind, ScopeKind::Function(_) | ScopeKind::Module) {
            if !checker.source_type.is_stub()
                && checker.is_rule_enabled(Rule::RuntimeImportInTypeCheckingBlock)
            {
                flake8_type_checking::rules::runtime_import_in_type_checking_block(checker, scope);
            }
            if enforce_typing_only_imports {
                let runtime_imports: Vec<&Binding> = checker
                    .semantic
                    .scopes
                    .ancestor_ids(scope_id)
                    .flat_map(|scope_id| runtime_imports[scope_id.as_usize()].iter())
                    .copied()
                    .collect();

                flake8_type_checking::rules::typing_only_runtime_import(
                    checker,
                    scope,
                    &runtime_imports,
                );
            }

            if checker.is_rule_enabled(Rule::UnusedImport) {
                pyflakes::rules::unused_import(checker, scope);
            }

            if checker.is_rule_enabled(Rule::ImportPrivateName) {
                pylint::rules::import_private_name(checker, scope);
            }
        }

        if scope.kind.is_function() {
            if checker.is_rule_enabled(Rule::NoSelfUse) {
                pylint::rules::no_self_use(checker, scope_id, scope);
            }

            if checker.is_rule_enabled(Rule::TooManyLocals) {
                pylint::rules::too_many_locals(checker, scope);
            }

            if checker.is_rule_enabled(Rule::SingledispatchMethod) {
                pylint::rules::singledispatch_method(checker, scope);
            }

            if checker.is_rule_enabled(Rule::SingledispatchmethodFunction) {
                pylint::rules::singledispatchmethod_function(checker, scope);
            }

            if checker.is_rule_enabled(Rule::BadStaticmethodArgument) {
                pylint::rules::bad_staticmethod_argument(checker, scope);
            }

            if checker.any_rule_enabled(&[
                Rule::InvalidFirstArgumentNameForClassMethod,
                Rule::InvalidFirstArgumentNameForMethod,
            ]) {
                pep8_naming::rules::invalid_first_argument_name(checker, scope);
            }
        }
    }
}
