use ruff_diagnostics::{Diagnostic, Fix};
use ruff_python_semantic::analyze::visibility;
use ruff_python_semantic::{Binding, BindingKind, Imported, ScopeKind};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::codes::Rule;
use crate::fix;
use crate::rules::{
    flake8_builtins, flake8_pyi, flake8_type_checking, flake8_unused_arguments, pyflakes, pylint,
    ruff,
};

/// Run lint rules over all deferred scopes in the [`SemanticModel`].
pub(crate) fn deferred_scopes(checker: &mut Checker) {
    if !checker.any_enabled(&[
        Rule::AsyncioDanglingTask,
        Rule::GlobalVariableNotAssigned,
        Rule::ImportPrivateName,
        Rule::ImportShadowedByLoopVar,
        Rule::NoSelfUse,
        Rule::RedefinedArgumentFromLocal,
        Rule::RedefinedWhileUnused,
        Rule::RuntimeImportInTypeCheckingBlock,
        Rule::TooManyLocals,
        Rule::TypingOnlyFirstPartyImport,
        Rule::TypingOnlyStandardLibraryImport,
        Rule::TypingOnlyThirdPartyImport,
        Rule::UndefinedLocal,
        Rule::UnusedAnnotation,
        Rule::UnusedClassMethodArgument,
        Rule::BuiltinAttributeShadowing,
        Rule::UnusedFunctionArgument,
        Rule::UnusedImport,
        Rule::UnusedLambdaArgument,
        Rule::UnusedMethodArgument,
        Rule::UnusedPrivateProtocol,
        Rule::UnusedPrivateTypeAlias,
        Rule::UnusedPrivateTypeVar,
        Rule::UnusedPrivateTypedDict,
        Rule::UnusedStaticMethodArgument,
        Rule::UnusedVariable,
    ]) {
        return;
    }

    // Identify any valid runtime imports. If a module is imported at runtime, and
    // used at runtime, then by default, we avoid flagging any other
    // imports from that model as typing-only.
    let enforce_typing_imports = !checker.source_type.is_stub()
        && checker.any_enabled(&[
            Rule::RuntimeImportInTypeCheckingBlock,
            Rule::TypingOnlyFirstPartyImport,
            Rule::TypingOnlyStandardLibraryImport,
            Rule::TypingOnlyThirdPartyImport,
        ]);
    let runtime_imports: Vec<Vec<&Binding>> = if enforce_typing_imports {
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

    let mut diagnostics: Vec<Diagnostic> = vec![];
    for scope_id in checker.analyze.scopes.iter().rev().copied() {
        let scope = &checker.semantic.scopes[scope_id];

        if checker.enabled(Rule::UndefinedLocal) {
            pyflakes::rules::undefined_local(checker, scope_id, scope, &mut diagnostics);
        }

        if checker.enabled(Rule::GlobalVariableNotAssigned) {
            for (name, binding_id) in scope.bindings() {
                let binding = checker.semantic.binding(binding_id);
                if binding.kind.is_global() {
                    diagnostics.push(Diagnostic::new(
                        pylint::rules::GlobalVariableNotAssigned {
                            name: (*name).to_string(),
                        },
                        binding.range(),
                    ));
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
                    checker.diagnostics.push(Diagnostic::new(
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
                    if shadowed.source.map_or(true, |left| {
                        binding
                            .source
                            .map_or(true, |right| !checker.semantic.same_branch(left, right))
                    }) {
                        continue;
                    }

                    checker.diagnostics.push(Diagnostic::new(
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
                    if shadowed.source.map_or(true, |left| {
                        binding
                            .source
                            .map_or(true, |right| !checker.semantic.same_branch(left, right))
                    }) {
                        continue;
                    }

                    let mut diagnostic = Diagnostic::new(
                        pyflakes::rules::RedefinedWhileUnused {
                            name: (*name).to_string(),
                            row: checker.compute_source_row(shadowed.start()),
                        },
                        binding.range(),
                    );

                    if let Some(range) = binding.parent_range(&checker.semantic) {
                        diagnostic.set_parent(range.start());
                    }

                    if let Some(import) = binding.as_any_import() {
                        if let Some(source) = binding.source {
                            diagnostic.try_set_fix(|| {
                                let statement = checker.semantic().statement(source);
                                let parent = checker.semantic().parent_statement(source);
                                let edit = fix::edits::remove_unused_imports(
                                    std::iter::once(import.member_name().as_ref()),
                                    statement,
                                    parent,
                                    checker.locator(),
                                    checker.stylist(),
                                    checker.indexer(),
                                )?;
                                Ok(Fix::safe_edit(edit).isolate(Checker::isolation(
                                    checker.semantic().parent_statement_id(source),
                                )))
                            });
                        }
                    }

                    diagnostics.push(diagnostic);
                }
            }
        }

        if checker.source_type.is_stub()
            || matches!(scope.kind, ScopeKind::Module | ScopeKind::Function(_))
        {
            if checker.enabled(Rule::UnusedPrivateTypeVar) {
                flake8_pyi::rules::unused_private_type_var(checker, scope, &mut diagnostics);
            }
            if checker.enabled(Rule::UnusedPrivateProtocol) {
                flake8_pyi::rules::unused_private_protocol(checker, scope, &mut diagnostics);
            }
            if checker.enabled(Rule::UnusedPrivateTypeAlias) {
                flake8_pyi::rules::unused_private_type_alias(checker, scope, &mut diagnostics);
            }
            if checker.enabled(Rule::UnusedPrivateTypedDict) {
                flake8_pyi::rules::unused_private_typed_dict(checker, scope, &mut diagnostics);
            }
        }

        if checker.enabled(Rule::AsyncioDanglingTask) {
            ruff::rules::asyncio_dangling_binding(scope, &checker.semantic, &mut diagnostics);
        }

        if let Some(class_def) = scope.kind.as_class() {
            if checker.enabled(Rule::BuiltinAttributeShadowing) {
                flake8_builtins::rules::builtin_attribute_shadowing(
                    checker,
                    scope_id,
                    scope,
                    class_def,
                    &mut diagnostics,
                );
            }
        }

        if matches!(scope.kind, ScopeKind::Function(_) | ScopeKind::Lambda(_)) {
            if checker.enabled(Rule::UnusedVariable) {
                pyflakes::rules::unused_variable(checker, scope, &mut diagnostics);
            }

            if checker.enabled(Rule::UnusedAnnotation) {
                pyflakes::rules::unused_annotation(checker, scope, &mut diagnostics);
            }

            if !checker.source_type.is_stub() {
                if checker.any_enabled(&[
                    Rule::UnusedClassMethodArgument,
                    Rule::UnusedFunctionArgument,
                    Rule::UnusedLambdaArgument,
                    Rule::UnusedMethodArgument,
                    Rule::UnusedStaticMethodArgument,
                ]) {
                    flake8_unused_arguments::rules::unused_arguments(
                        checker,
                        scope,
                        &mut diagnostics,
                    );
                }
            }
        }

        if matches!(scope.kind, ScopeKind::Function(_) | ScopeKind::Module) {
            if enforce_typing_imports {
                let runtime_imports: Vec<&Binding> = checker
                    .semantic
                    .scopes
                    .ancestor_ids(scope_id)
                    .flat_map(|scope_id| runtime_imports[scope_id.as_usize()].iter())
                    .copied()
                    .collect();

                if checker.enabled(Rule::RuntimeImportInTypeCheckingBlock) {
                    flake8_type_checking::rules::runtime_import_in_type_checking_block(
                        checker,
                        scope,
                        &mut diagnostics,
                    );
                }

                if checker.any_enabled(&[
                    Rule::TypingOnlyFirstPartyImport,
                    Rule::TypingOnlyStandardLibraryImport,
                    Rule::TypingOnlyThirdPartyImport,
                ]) {
                    flake8_type_checking::rules::typing_only_runtime_import(
                        checker,
                        scope,
                        &runtime_imports,
                        &mut diagnostics,
                    );
                }
            }

            if checker.enabled(Rule::UnusedImport) {
                pyflakes::rules::unused_import(checker, scope, &mut diagnostics);
            }

            if checker.enabled(Rule::ImportPrivateName) {
                pylint::rules::import_private_name(checker, scope, &mut diagnostics);
            }
        }

        if scope.kind.is_function() {
            if checker.enabled(Rule::NoSelfUse) {
                pylint::rules::no_self_use(checker, scope_id, scope, &mut diagnostics);
            }

            if checker.enabled(Rule::TooManyLocals) {
                pylint::rules::too_many_locals(checker, scope, &mut diagnostics);
            }
        }
    }
    checker.diagnostics.extend(diagnostics);
}
