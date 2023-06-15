//! Code modification struct to support symbol renaming within a scope.

use anyhow::{anyhow, Result};

use ruff_diagnostics::Edit;
use ruff_python_semantic::{Binding, BindingKind, Scope, SemanticModel};

pub(crate) struct Renamer;

impl Renamer {
    /// Rename a symbol (from `name` to `target`) within a [`Scope`].
    ///
    /// ## How it works
    ///
    /// The renaming algorithm is as follows:
    ///
    /// 1. Start with the first [`Binding`] in the scope, for the given name. For example, in the
    ///    following snippet, we'd start by examining the `x = 1` binding:
    ///
    /// ```python
    /// if True:
    ///     x = 1
    ///     print(x)
    /// else:
    ///     x = 2
    ///     print(x)
    ///
    /// print(x)
    /// ```
    ///
    /// 2. Rename the [`Binding`]. In most cases, this is a simple replacement. For example,
    ///    renaming `x` to `y` above would require replacing `x = 1` with `y = 1`. After the
    ///    first replacement in the snippet above, we'd have:
    ///
    /// ```python
    /// if True:
    ///     y = 1
    ///     print(x)
    /// else:
    ///     x = 2
    ///     print(x)
    ///
    /// print(x)
    /// ```
    ///
    ///    Note that, when renaming imports, we need to instead rename (or add) an alias. For
    ///    example, to rename `pandas` to `pd`, we may need to rewrite `import pandas` to
    ///    `import pandas as pd`, rather than `import pd`.
    ///
    /// 3. Rename every reference to the [`Binding`]. For example, renaming the references to the
    ///    `x = 1` binding above would give us:
    ///
    /// ```python
    /// if True:
    ///     y = 1
    ///     print(y)
    /// else:
    ///     x = 2
    ///     print(x)
    ///
    /// print(x)
    /// ```
    ///
    /// 4. Rename every delayed annotation. (See [`SemanticModel::delayed_annotations`].)
    ///
    /// 5. Repeat the above process for every [`Binding`] in the scope with the given name.
    ///    After renaming the `x = 2` binding, we'd have:
    ///
    /// ```python
    /// if True:
    ///     y = 1
    ///     print(y)
    /// else:
    ///     y = 2
    ///     print(y)
    ///
    /// print(y)
    /// ```
    ///
    /// ## Limitations
    ///
    /// `global` and `nonlocal` declarations are not yet supported.
    ///
    /// `global` and `nonlocal` declarations add some additional complexity. If we're renaming a
    /// name that's declared as `global` or `nonlocal` in a child scope, we need to rename the name
    /// in that scope too, repeating the above process.
    ///
    /// If we're renaming a name that's declared as `global` or `nonlocal` in the current scope,
    /// then we need to identify the scope in which the name is declared, and perform the rename
    /// in that scope instead (which will in turn trigger the above process on the current scope).
    pub(crate) fn rename(
        name: &str,
        target: &str,
        scope: &Scope,
        semantic: &SemanticModel,
    ) -> Result<(Edit, Vec<Edit>)> {
        let mut edits = vec![];

        // Iterate over every binding to the name in the scope.
        for binding_id in scope.get_all(name) {
            let binding = semantic.binding(binding_id);

            // Rename the binding.
            if let Some(edit) = Renamer::rename_binding(binding, name, target) {
                edits.push(edit);

                // Rename any delayed annotations.
                if let Some(annotations) = semantic.delayed_annotations(binding_id) {
                    edits.extend(annotations.iter().filter_map(|annotation_id| {
                        let annotation = semantic.binding(*annotation_id);
                        Renamer::rename_binding(annotation, name, target)
                    }));
                }

                // Rename the references to the binding.
                edits.extend(binding.references().map(|reference_id| {
                    let reference = semantic.reference(reference_id);
                    Edit::range_replacement(target.to_string(), reference.range())
                }));
            }
        }

        // Deduplicate any edits. In some cases, a reference can be both a read _and_ a write. For
        // example, `x += 1` is both a read of and a write to `x`.
        edits.sort();
        edits.dedup();

        let edit = edits
            .pop()
            .ok_or(anyhow!("Unable to rename any references to `{name}`"))?;
        Ok((edit, edits))
    }

    /// Rename a [`Binding`] reference.
    fn rename_binding(binding: &Binding, name: &str, target: &str) -> Option<Edit> {
        match &binding.kind {
            BindingKind::Importation(_) | BindingKind::FromImportation(_) => {
                if binding.is_alias() {
                    // Ex) Rename `import pandas as alias` to `import pandas as pd`.
                    Some(Edit::range_replacement(target.to_string(), binding.range))
                } else {
                    // Ex) Rename `import pandas` to `import pandas as pd`.
                    Some(Edit::range_replacement(
                        format!("{name} as {target}"),
                        binding.range,
                    ))
                }
            }
            BindingKind::SubmoduleImportation(import) => {
                // Ex) Rename `import pandas.core` to `import pandas as pd`.
                let module_name = import.qualified_name.split('.').next().unwrap();
                Some(Edit::range_replacement(
                    format!("{module_name} as {target}"),
                    binding.range,
                ))
            }
            // Avoid renaming builtins and other "special" bindings.
            BindingKind::FutureImportation | BindingKind::Builtin | BindingKind::Export(_) => None,
            // By default, replace the binding's name with the target name.
            BindingKind::Annotation
            | BindingKind::Argument
            | BindingKind::NamedExprAssignment
            | BindingKind::UnpackedAssignment
            | BindingKind::Assignment
            | BindingKind::LoopVar
            | BindingKind::Global
            | BindingKind::Nonlocal
            | BindingKind::ClassDefinition
            | BindingKind::FunctionDefinition
            | BindingKind::Deletion
            | BindingKind::UnboundException => {
                Some(Edit::range_replacement(target.to_string(), binding.range))
            }
        }
    }
}
