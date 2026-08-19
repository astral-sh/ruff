use std::collections::VecDeque;

use rustc_hash::FxHashSet;
use ty_python_core::definition::{Definition, DefinitionKind, NestedBindingExecution};
use ty_python_core::semantic_index;

use crate::{Db, FxIndexSet};

/// Returns definitions that correspond to concrete locations in source code.
///
/// This result excludes synthetic definitions such as loop headers and nested
/// bindings.
///
/// For eager synthetic bindings representing comprehension walruses, the
/// concrete definitions they represent are returned instead:
///
/// ```python
/// [(last := item) for item in items]
/// print(last)  # Go to definition should select `last := item` above.
/// ```
///
/// The binding for the use in `print` is synthetic, so follow it into the
/// comprehension's end-of-scope bindings. Nested comprehensions can produce a
/// chain of these proxies. Only follow sources that resolve to the same
/// variable, so `global` and `nonlocal` writes do not become definitions of
/// each other.
pub(crate) fn source_backed_definitions<'db>(
    db: &'db dyn Db,
    definitions: impl IntoIterator<Item = Definition<'db>>,
) -> FxIndexSet<Definition<'db>> {
    let mut pending = definitions.into_iter().collect::<VecDeque<_>>();
    let mut seen = FxHashSet::default();
    let mut result = FxIndexSet::default();

    while let Some(definition) = pending.pop_front() {
        if !seen.insert(definition) {
            continue;
        }

        match definition.kind(db) {
            DefinitionKind::NestedBindings(nested) => {
                let index = semantic_index(db, definition.program_file(db));
                let sources = nested
                    .visible_binding_sources(index, definition.file_scope(db))
                    .flatten()
                    .filter_map(|binding| binding.binding.definition());
                // A lazy function proxy can lead to an eager comprehension proxy. Follow that
                // proxy-only chain without exposing ordinary lazy nested assignments.
                pending.extend(sources.filter(|source| {
                    nested.execution == NestedBindingExecution::Eager
                        || matches!(source.kind(db), DefinitionKind::NestedBindings(_))
                }));
            }
            kind if kind.is_user_visible() => {
                result.insert(definition);
            }
            _ => {}
        }
    }

    result
}
