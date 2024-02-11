use crate::checkers::ast::Checker;
use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_semantic::{Binding, Scope};
use ruff_text_size::Ranged;
use std::collections::HashMap;

/// ## What it does
/// Checks for the presence of unused variables when unpacking tuples.
///
/// ## Why is this bad?
/// A variable that is defined but not used is likely a mistake.
///
/// If a variable is intentionally defined-but-not-used, it should be
/// prefixed with an underscore, or some other value that adheres to the
/// [`dummy-variable-rgx`] pattern.
///
/// ## Example
/// ```python
/// def foo():
///     return (1, 2)
///
/// x, y = foo()
///
/// print(x)
/// ```
///
/// Use instead:
/// ```python
/// def foo():
///    return (1, 2)
///
/// x, _y = foo()
///
/// print(x)
/// ```
///
/// ## Options
/// - `lint.dummy-variable-rgx`

#[violation]
pub struct UnusedTupleElement {
    name: String,
}

impl Violation for UnusedTupleElement {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let UnusedTupleElement { name } = self;
        format!("Local variable `{name}` is assigned to but never used. Consider renaming to `_{name}`.")
    }

    fn fix_title(&self) -> Option<String> {
        let UnusedTupleElement { name } = self;
        Some(format!(
            "Replace assignment of unused variable from `{name}` to `_{name}`."
        ))
    }
}

/// RUF028
pub(crate) fn unused_tuple_element(
    checker: &Checker,
    scope: &Scope,
    diagnostics: &mut Vec<Diagnostic>,
) {
    // Don't apply rule if `locals()` is used because we don't know if variables are be dynamically used.
    if scope.uses_locals() && scope.kind.is_function() {
        return;
    }

    let mut statement_bindings_map = HashMap::new();
    let bindings = scope
        .bindings()
        .map(|(name, binding_id)| (name, checker.semantic().binding(binding_id)));

    // Group bindings by statement.
    for (name, binding) in bindings {
        if !binding.is_unpacked_assignment() {
            continue;
        }
        if let Some(statement_id) = binding
            .source
            .map(|node_id| checker.semantic().statement_id(node_id))
        {
            statement_bindings_map
                .entry(statement_id)
                .or_insert_with(Vec::new)
                .push((name, binding));
        };
    }

    let bindings_to_fix = statement_bindings_map
        .iter()
        .filter_map(|(_, bindings)| {
            let all_bindings_unused = bindings.iter().all(|(_, binding)| !binding.is_used());
            // Don't apply the rule if all bindings are unused since rule F841 will apply instead.
            if all_bindings_unused {
                return None;
            }
            let unused_bindings = bindings.iter().filter_map(|(name, binding)| {
                if !binding.is_used()
                    && !checker.settings.dummy_variable_rgx.is_match(name)
                    && !matches!(
                        *name,
                        "__tracebackhide__"
                            | "__traceback_info__"
                            | "__traceback_supplement__"
                            | "__debuggerskip__"
                    )
                {
                    return Some((*name, *binding));
                }
                None
            });
            Some(unused_bindings)
        })
        .flatten();

    for (name, binding) in bindings_to_fix {
        let mut diagnostic = Diagnostic::new(
            UnusedTupleElement {
                name: name.to_string(),
            },
            binding.range(),
        );
        if let Some(fix) = generate_fix(binding, checker) {
            diagnostic.set_fix(fix);
        }
        diagnostics.push(diagnostic);
    }
}

fn generate_fix(binding: &Binding, checker: &Checker) -> Option<Fix> {
    let node_id = binding.source?;
    let name = binding.name(checker.locator());
    let renamed = format!("_{name}");
    if checker.settings.dummy_variable_rgx.is_match(&renamed) {
        let edit = Edit::range_replacement(renamed, binding.range());
        let isolation = Checker::isolation(checker.semantic().parent_statement_id(node_id));
        return Some(Fix::unsafe_edit(edit).isolate(isolation));
    }
    None
}
