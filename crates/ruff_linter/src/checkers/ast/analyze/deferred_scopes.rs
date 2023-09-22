use ruff_diagnostics::Diagnostic;
use ruff_python_semantic::analyze::visibility;
use ruff_python_semantic::{Binding, BindingKind, ScopeKind};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::codes::Rule;
use crate::rules::{flake8_pyi, flake8_type_checking, flake8_unused_arguments, pyflakes, pylint};

/// Run lint rules over all deferred scopes in the [`SemanticModel`].
pub(crate) fn deferred_scopes(checker: &mut Checker) {
    if !checker.any_enabled(&[
        Rule::GlobalVariableNotAssigned,
        Rule::ImportShadowedByLoopVar,
        Rule::RedefinedWhileUnused,
        Rule::RuntimeImportInTypeCheckingBlock,
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
        Rule::UnusedPrivateTypeVar,
        Rule::UnusedPrivateTypedDict,
        Rule::UnusedStaticMethodArgument,
        Rule::UnusedVariable,
        Rule::NoSelfUse,
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
                        )
                    })
                    .collect()
            })
            .collect::<Vec<_>>()
    } else {
        vec![]
    };

    let mut diagnostics: Vec<Diagnostic> = vec![];
    for scope_id in checker.deferred.scopes.iter().rev().copied() {
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
                        binding.source.map_or(true, |right| {
                            checker.semantic.different_branches(left, right)
                        })
                    }) {
                        continue;
                    }

                    #[allow(deprecated)]
                    let line = checker.locator.compute_line_index(shadowed.start());

                    checker.diagnostics.push(Diagnostic::new(
                        pyflakes::rules::ImportShadowedByLoopVar {
                            name: name.to_string(),
                            line,
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
                        binding.source.map_or(true, |right| {
                            checker.semantic.different_branches(left, right)
                        })
                    }) {
                        continue;
                    }

                    #[allow(deprecated)]
                    let line = checker.locator.compute_line_index(shadowed.start());
                    let mut diagnostic = Diagnostic::new(
                        pyflakes::rules::RedefinedWhileUnused {
                            name: (*name).to_string(),
                            line,
                        },
                        binding.range(),
                    );
                    if let Some(range) = binding.parent_range(&checker.semantic) {
                        diagnostic.set_parent(range.start());
                    }
                    diagnostics.push(diagnostic);
                }
            }
        }

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
        }

        if scope.kind.is_function() {
            if checker.enabled(Rule::NoSelfUse) {
                pylint::rules::no_self_use(checker, scope, &mut diagnostics);
            }
        }
    }
    checker.diagnostics.extend(diagnostics);
}
