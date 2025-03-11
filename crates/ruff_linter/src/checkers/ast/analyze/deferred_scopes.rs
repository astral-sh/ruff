use ruff_diagnostics::{Diagnostic, Fix};
use ruff_python_semantic::analyze::visibility;
use ruff_python_semantic::{Binding, BindingKind, Imported, ResolvedReference, ScopeKind};
use ruff_text_size::Ranged;
use rustc_hash::FxHashMap;

use crate::checkers::ast::Checker;
use crate::codes::Rule;
use crate::fix;
use crate::rules::{
    flake8_builtins, flake8_pyi, flake8_type_checking, flake8_unused_arguments, pep8_naming,
    pyflakes, pylint, ruff,
};

/// Run lint rules over all deferred scopes in the [`SemanticModel`].
pub(crate) fn deferred_scopes(checker: &Checker) {
    if !checker.any_enabled(&[
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
        && checker.any_enabled(&[
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
                            &checker.settings.flake8_type_checking,
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

        if checker.enabled(Rule::UndefinedLocal) {
            pyflakes::rules::undefined_local(checker, scope_id, scope);
        }

        if checker.enabled(Rule::GlobalVariableNotAssigned) {
            for (name, binding_id) in scope.bindings() {
                let binding = checker.semantic.binding(binding_id);
                // If the binding is a `global`, then it's a top-level `global` that was never
                // assigned in the current scope. If it were assigned, the `global` would be
                // shadowed by the assignment.
                if binding.kind.is_global() {
                    // If the binding was conditionally deleted, it will include a reference within
                    // a `Del` context, but won't be shadowed by a `BindingKind::Deletion`, as in:
                    // ```python
                    // if condition:
                    //     del var
                    // ```
                    if binding
                        .references
                        .iter()
                        .map(|id| checker.semantic.reference(*id))
                        .all(ResolvedReference::is_load)
                    {
                        checker.report_diagnostic(Diagnostic::new(
                            pylint::rules::GlobalVariableNotAssigned {
                                name: (*name).to_string(),
                            },
                            binding.range(),
                        ));
                    }
                }
            }
        }

        if checker.enabled(Rule::RedefinedArgumentFromLocal) {
            for (name, binding_id) in scope.bindings() {
                for shadow in checker.semantic.shadowed_bindings(scope_id, binding_id) {
                    let binding = &checker.semantic.bindings[shadow.binding_id()];
                    if !matches!(
                        binding.kind,
                        BindingKind::LoopVar
                            | BindingKind::BoundException
                            | BindingKind::WithItemVar
                    ) {
                        continue;
                    }
                    let shadowed = &checker.semantic.bindings[shadow.shadowed_id()];
                    if !shadowed.kind.is_argument() {
                        continue;
                    }
                    if checker.settings.dummy_variable_rgx.is_match(name) {
                        continue;
                    }
                    let scope = &checker.semantic.scopes[binding.scope];
                    if scope.kind.is_generator() {
                        continue;
                    }
                    checker.report_diagnostic(Diagnostic::new(
                        pylint::rules::RedefinedArgumentFromLocal {
                            name: name.to_string(),
                        },
                        binding.range(),
                    ));
                }
            }
        }

        if checker.enabled(Rule::ImportShadowedByLoopVar) {
            for (name, binding_id) in scope.bindings() {
                for shadow in checker.semantic.shadowed_bindings(scope_id, binding_id) {
                    // If the shadowing binding isn't a loop variable, abort.
                    let binding = &checker.semantic.bindings[shadow.binding_id()];
                    if !binding.kind.is_loop_var() {
                        continue;
                    }

                    // If the shadowed binding isn't an import, abort.
                    let shadowed = &checker.semantic.bindings[shadow.shadowed_id()];
                    if !matches!(
                        shadowed.kind,
                        BindingKind::Import(..)
                            | BindingKind::FromImport(..)
                            | BindingKind::SubmoduleImport(..)
                            | BindingKind::FutureImport
                    ) {
                        continue;
                    }

                    // If the bindings are in different forks, abort.
                    if shadowed.source.is_none_or(|left| {
                        binding
                            .source
                            .is_none_or(|right| !checker.semantic.same_branch(left, right))
                    }) {
                        continue;
                    }

                    checker.report_diagnostic(Diagnostic::new(
                        pyflakes::rules::ImportShadowedByLoopVar {
                            name: name.to_string(),
                            row: checker.compute_source_row(shadowed.start()),
                        },
                        binding.range(),
                    ));
                }
            }
        }

        if checker.enabled(Rule::RedefinedWhileUnused) {
            // Index the redefined bindings by statement.
            let mut redefinitions = FxHashMap::default();

            for (name, binding_id) in scope.bindings() {
                for shadow in checker.semantic.shadowed_bindings(scope_id, binding_id) {
                    // If the shadowing binding is a loop variable, abort, to avoid overlap
                    // with F402.
                    let binding = &checker.semantic.bindings[shadow.binding_id()];
                    if binding.kind.is_loop_var() {
                        continue;
                    }

                    // If the shadowed binding is used, abort.
                    let shadowed = &checker.semantic.bindings[shadow.shadowed_id()];
                    if shadowed.is_used() {
                        continue;
                    }

                    // If the shadowing binding isn't considered a "redefinition" of the
                    // shadowed binding, abort.
                    if !binding.redefines(shadowed) {
                        continue;
                    }

                    if shadow.same_scope() {
                        // If the symbol is a dummy variable, abort, unless the shadowed
                        // binding is an import.
                        if !matches!(
                            shadowed.kind,
                            BindingKind::Import(..)
                                | BindingKind::FromImport(..)
                                | BindingKind::SubmoduleImport(..)
                                | BindingKind::FutureImport
                        ) && checker.settings.dummy_variable_rgx.is_match(name)
                        {
                            continue;
                        }

                        let Some(node_id) = shadowed.source else {
                            continue;
                        };

                        // If this is an overloaded function, abort.
                        if shadowed.kind.is_function_definition() {
                            if checker
                                .semantic
                                .statement(node_id)
                                .as_function_def_stmt()
                                .is_some_and(|function| {
                                    visibility::is_overload(
                                        &function.decorator_list,
                                        &checker.semantic,
                                    )
                                })
                            {
                                continue;
                            }
                        }
                    } else {
                        // Only enforce cross-scope shadowing for imports.
                        if !matches!(
                            shadowed.kind,
                            BindingKind::Import(..)
                                | BindingKind::FromImport(..)
                                | BindingKind::SubmoduleImport(..)
                                | BindingKind::FutureImport
                        ) {
                            continue;
                        }
                    }

                    // If the bindings are in different forks, abort.
                    if shadowed.source.is_none_or(|left| {
                        binding
                            .source
                            .is_none_or(|right| !checker.semantic.same_branch(left, right))
                    }) {
                        continue;
                    }

                    redefinitions
                        .entry(binding.source)
                        .or_insert_with(Vec::new)
                        .push((shadowed, binding));
                }
            }

            // Create a fix for each source statement.
            let mut fixes = FxHashMap::default();
            for (source, entries) in &redefinitions {
                let Some(source) = source else {
                    continue;
                };

                let member_names = entries
                    .iter()
                    .filter_map(|(shadowed, binding)| {
                        if let Some(shadowed_import) = shadowed.as_any_import() {
                            if let Some(import) = binding.as_any_import() {
                                if shadowed_import.qualified_name() == import.qualified_name() {
                                    return Some(import.member_name());
                                }
                            }
                        }
                        None
                    })
                    .collect::<Vec<_>>();

                if !member_names.is_empty() {
                    let statement = checker.semantic.statement(*source);
                    let parent = checker.semantic.parent_statement(*source);
                    let Ok(edit) = fix::edits::remove_unused_imports(
                        member_names.iter().map(std::convert::AsRef::as_ref),
                        statement,
                        parent,
                        checker.locator(),
                        checker.stylist(),
                        checker.indexer(),
                    ) else {
                        continue;
                    };
                    fixes.insert(
                        *source,
                        Fix::safe_edit(edit).isolate(Checker::isolation(
                            checker.semantic().parent_statement_id(*source),
                        )),
                    );
                }
            }

            // Create diagnostics for each statement.
            for (source, entries) in &redefinitions {
                for (shadowed, binding) in entries {
                    let mut diagnostic = Diagnostic::new(
                        pyflakes::rules::RedefinedWhileUnused {
                            name: binding.name(checker.source()).to_string(),
                            row: checker.compute_source_row(shadowed.start()),
                        },
                        binding.range(),
                    );

                    if let Some(range) = binding.parent_range(&checker.semantic) {
                        diagnostic.set_parent(range.start());
                    }

                    if let Some(fix) = source.as_ref().and_then(|source| fixes.get(source)) {
                        diagnostic.set_fix(fix.clone());
                    }

                    checker.report_diagnostic(diagnostic);
                }
            }
        }

        if checker.source_type.is_stub()
            || matches!(scope.kind, ScopeKind::Module | ScopeKind::Function(_))
        {
            if checker.enabled(Rule::UnusedPrivateTypeVar) {
                flake8_pyi::rules::unused_private_type_var(checker, scope);
            }
            if checker.enabled(Rule::UnusedPrivateProtocol) {
                flake8_pyi::rules::unused_private_protocol(checker, scope);
            }
            if checker.enabled(Rule::UnusedPrivateTypeAlias) {
                flake8_pyi::rules::unused_private_type_alias(checker, scope);
            }
            if checker.enabled(Rule::UnusedPrivateTypedDict) {
                flake8_pyi::rules::unused_private_typed_dict(checker, scope);
            }
        }

        if checker.enabled(Rule::AsyncioDanglingTask) {
            ruff::rules::asyncio_dangling_binding(scope, checker);
        }

        if let Some(class_def) = scope.kind.as_class() {
            if checker.enabled(Rule::BuiltinAttributeShadowing) {
                flake8_builtins::rules::builtin_attribute_shadowing(
                    checker, scope_id, scope, class_def,
                );
            }
            if checker.enabled(Rule::FunctionCallInDataclassDefaultArgument) {
                ruff::rules::function_call_in_dataclass_default(checker, class_def);
            }
            if checker.enabled(Rule::MutableClassDefault) {
                ruff::rules::mutable_class_default(checker, class_def);
            }
            if checker.enabled(Rule::MutableDataclassDefault) {
                ruff::rules::mutable_dataclass_default(checker, class_def);
            }
        }

        if matches!(scope.kind, ScopeKind::Function(_) | ScopeKind::Lambda(_)) {
            if checker.any_enabled(&[Rule::UnusedVariable, Rule::UnusedUnpackedVariable])
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
                            && !checker.settings.dummy_variable_rgx.is_match(name)
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
                    if checker.enabled(Rule::UnusedVariable) {
                        pyflakes::rules::unused_variable(checker, unused_name, unused_binding);
                    }

                    if checker.enabled(Rule::UnusedUnpackedVariable) {
                        ruff::rules::unused_unpacked_variable(checker, unused_name, unused_binding);
                    }
                }
            }

            if checker.enabled(Rule::UnusedAnnotation) {
                pyflakes::rules::unused_annotation(checker, scope);
            }

            if !checker.source_type.is_stub() {
                if checker.any_enabled(&[
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
                && checker.enabled(Rule::RuntimeImportInTypeCheckingBlock)
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

            if checker.enabled(Rule::UnusedImport) {
                pyflakes::rules::unused_import(checker, scope);
            }

            if checker.enabled(Rule::ImportPrivateName) {
                pylint::rules::import_private_name(checker, scope);
            }
        }

        if scope.kind.is_function() {
            if checker.enabled(Rule::NoSelfUse) {
                pylint::rules::no_self_use(checker, scope_id, scope);
            }

            if checker.enabled(Rule::TooManyLocals) {
                pylint::rules::too_many_locals(checker, scope);
            }

            if checker.enabled(Rule::SingledispatchMethod) {
                pylint::rules::singledispatch_method(checker, scope);
            }

            if checker.enabled(Rule::SingledispatchmethodFunction) {
                pylint::rules::singledispatchmethod_function(checker, scope);
            }

            if checker.enabled(Rule::BadStaticmethodArgument) {
                pylint::rules::bad_staticmethod_argument(checker, scope);
            }

            if checker.any_enabled(&[
                Rule::InvalidFirstArgumentNameForClassMethod,
                Rule::InvalidFirstArgumentNameForMethod,
            ]) {
                pep8_naming::rules::invalid_first_argument_name(checker, scope);
            }
        }
    }
}
