use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_semantic::analyze::visibility;
use ruff_python_semantic::{BindingKind, Imported, Scope, ScopeId};
use ruff_source_file::SourceRow;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::fix::edits;
use crate::{Fix, FixAvailability, Violation};

use rustc_hash::FxHashMap;

/// ## What it does
/// Checks for variable definitions that redefine (or "shadow") unused
/// variables.
///
/// ## Why is this bad?
/// Redefinitions of unused names are unnecessary and often indicative of a
/// mistake.
///
/// ## Example
/// ```python
/// import foo
/// import bar
/// import foo  # Redefinition of unused `foo` from line 1
/// ```
///
/// Use instead:
/// ```python
/// import foo
/// import bar
/// ```
#[derive(ViolationMetadata)]
pub(crate) struct RedefinedWhileUnused {
    pub name: String,
    pub row: SourceRow,
}

impl Violation for RedefinedWhileUnused {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let RedefinedWhileUnused { name, row } = self;
        format!("Redefinition of unused `{name}` from {row}")
    }

    fn fix_title(&self) -> Option<String> {
        let RedefinedWhileUnused { name, .. } = self;
        Some(format!("Remove definition: `{name}`"))
    }
}

/// F811
pub(crate) fn redefined_while_unused(checker: &Checker, scope_id: ScopeId, scope: &Scope) {
    // Index the redefined bindings by statement.
    let mut redefinitions = FxHashMap::default();

    for (name, binding_id) in scope.bindings() {
        for shadow in checker.semantic().shadowed_bindings(scope_id, binding_id) {
            // If the shadowing binding is a loop variable, abort, to avoid overlap
            // with F402.
            let binding = &checker.semantic().bindings[shadow.binding_id()];
            if binding.kind.is_loop_var() {
                continue;
            }

            // If the shadowed binding is used, abort.
            let shadowed = &checker.semantic().bindings[shadow.shadowed_id()];
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
                ) && checker.settings().dummy_variable_rgx.is_match(name)
                {
                    continue;
                }

                let Some(node_id) = shadowed.source else {
                    continue;
                };

                // If this is an overloaded function, abort.
                if shadowed.kind.is_function_definition() {
                    if checker
                        .semantic()
                        .statement(node_id)
                        .as_function_def_stmt()
                        .is_some_and(|function| {
                            visibility::is_overload(&function.decorator_list, checker.semantic())
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
                    .is_none_or(|right| !checker.semantic().same_branch(left, right))
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
            let statement = checker.semantic().statement(*source);
            let parent = checker.semantic().parent_statement(*source);
            let Ok(edit) = edits::remove_unused_imports(
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
            let mut diagnostic = checker.report_diagnostic(
                RedefinedWhileUnused {
                    name: binding.name(checker.source()).to_string(),
                    row: checker.compute_source_row(shadowed.start()),
                },
                binding.range(),
            );

            if let Some(range) = binding.parent_range(checker.semantic()) {
                diagnostic.set_parent(range.start());
            }

            if let Some(fix) = source.as_ref().and_then(|source| fixes.get(source)) {
                diagnostic.set_fix(fix.clone());
            }
        }
    }
}
