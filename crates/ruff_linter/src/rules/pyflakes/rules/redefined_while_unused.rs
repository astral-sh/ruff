use rustc_hash::FxHashMap;

use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_semantic::{
    BindingKind, Imported, NodeId, Scope, ScopeId,
    analyze::{typing::is_type_checking_block, visibility},
};
use ruff_source_file::SourceRow;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::fix::edits;
use crate::{Fix, FixAvailability, Violation};

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
///
/// ## Options
///
/// This rule ignores dummy variables, as determined by:
///
/// - `lint.dummy-variable-rgx`
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.0.171")]
pub(crate) struct RedefinedWhileUnused {
    pub name: String,
    pub row: SourceRow,
    pub is_type_checking_duplicate: bool,
}

impl Violation for RedefinedWhileUnused {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let RedefinedWhileUnused {
            name,
            row,
            is_type_checking_duplicate,
        } = self;
        if *is_type_checking_duplicate {
            format!("Import `{name}` is duplicated in `typing.TYPE_CHECKING` block")
        } else {
            format!("Redefinition of unused `{name}` from {row}")
        }
    }

    fn fix_title(&self) -> Option<String> {
        let RedefinedWhileUnused {
            name,
            is_type_checking_duplicate,
            ..
        } = self;
        if *is_type_checking_duplicate {
            Some(format!("Remove runtime `{name}` import"))
        } else {
            Some(format!("Remove definition: `{name}`"))
        }
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
                binding.source.is_none_or(|right| {
                    let left_in_type_checking = is_in_type_checking_block(checker, left);
                    let right_in_type_checking = is_in_type_checking_block(checker, right);

                    if (left_in_type_checking || right_in_type_checking)
                        && !(left_in_type_checking && right_in_type_checking)
                    {
                        let left_binding = &checker.semantic().bindings[shadow.shadowed_id()];
                        let right_binding = &checker.semantic().bindings[shadow.binding_id()];

                        if let (Some(left_import), Some(right_import)) =
                            (left_binding.as_any_import(), right_binding.as_any_import())
                        {
                            if left_import.qualified_name() == right_import.qualified_name() {
                                return false;
                            }
                        }
                    }

                    !checker.semantic().same_branch(left, right)
                })
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
        let runtime_import_source = entries.iter().find_map(|(shadowed, binding)| {
            let shadowed_in_type_checking = is_in_type_checking_block(checker, shadowed.source?);
            let binding_in_type_checking = is_in_type_checking_block(checker, binding.source?);

            if shadowed_in_type_checking && !binding_in_type_checking {
                binding.source
            } else if !shadowed_in_type_checking && binding_in_type_checking {
                shadowed.source
            } else {
                *source
            }
        });

        let Some(source) = runtime_import_source else {
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
            let statement = checker.semantic().statement(source);
            let parent = checker.semantic().parent_statement(source);
            let Ok(edit) = edits::remove_unused_imports(
                member_names.iter().map(AsRef::as_ref),
                statement,
                parent,
                checker.locator(),
                checker.stylist(),
                checker.indexer(),
            ) else {
                continue;
            };
            fixes.insert(
                source,
                Fix::safe_edit(edit).isolate(Checker::isolation(
                    checker.semantic().parent_statement_id(source),
                )),
            );
        }
    }

    // Create diagnostics for each statement.
    for (source, entries) in &redefinitions {
        for (shadowed, binding) in entries {
            let name = binding.name(checker.source());

            let shadowed_in_type_checking = shadowed
                .source
                .map(|source| is_in_type_checking_block(checker, source))
                .unwrap_or(false);
            let binding_in_type_checking = binding
                .source
                .map(|source| is_in_type_checking_block(checker, source))
                .unwrap_or(false);

            let is_type_checking_duplicate = (shadowed_in_type_checking
                || binding_in_type_checking)
                && !(shadowed_in_type_checking && binding_in_type_checking);

            let runtime_import_source = if shadowed_in_type_checking && !binding_in_type_checking {
                binding.source
            } else if !shadowed_in_type_checking && binding_in_type_checking {
                shadowed.source
            } else {
                *source
            };

            let mut diagnostic = checker.report_diagnostic(
                RedefinedWhileUnused {
                    name: name.to_string(),
                    row: checker.compute_source_row(shadowed.start()),
                    is_type_checking_duplicate,
                },
                binding.range(),
            );
            diagnostic.add_primary_tag(ruff_db::diagnostic::DiagnosticTag::Unnecessary);

            if is_type_checking_duplicate {
                diagnostic.secondary_annotation(
                    format_args!("runtime import of `{name}` here"),
                    shadowed,
                );
            } else {
                diagnostic.secondary_annotation(
                    format_args!("previous definition of `{name}` here"),
                    shadowed,
                );
            }

            diagnostic.set_primary_message(format_args!("`{name}` redefined here"));

            if let Some(range) = binding.parent_range(checker.semantic()) {
                diagnostic.set_parent(range.start());
            }

            if let Some(runtime_source) = runtime_import_source {
                if let Some(fix) = fixes.get(&runtime_source) {
                    diagnostic.set_fix(fix.clone());
                }
            }
        }
    }
}

#[inline]
fn is_in_type_checking_block(checker: &Checker, node_id: NodeId) -> bool {
    checker.semantic().statements(node_id).any(|stmt| {
        stmt.as_if_stmt()
            .map(|if_stmt| is_type_checking_block(if_stmt, checker.semantic()))
            .unwrap_or(false)
    })
}
