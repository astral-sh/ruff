use itertools::join;
use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

use ruff_python_semantic::{Imported, ResolvedReference, Scope};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for import statements that import a private name (a name starting
/// with an underscore `_`) from another module.
///
/// ## Why is this bad?
/// [PEP 8] states that names starting with an underscore are private. Thus,
/// they are not intended to be used outside of the module in which they are
/// defined.
///
/// Further, as private imports are not considered part of the public API, they
/// are prone to unexpected changes, even in a minor version bump.
///
/// Instead, consider using the public API of the module.
///
/// ## Known problems
/// Does not ignore private name imports from within the module that defines
/// the private name if the module is defined with [PEP 420] namespace packages
/// (directories that omit the `__init__.py` file). Instead, namespace packages
/// must be made known to ruff using the [`namespace-packages`] setting.
///
/// ## Example
/// ```python
/// from foo import _bar
/// ```
///
/// ## Options
/// - [`namespace-packages`]: List of namespace packages that are known to ruff
///
/// ## References
/// - [PEP 8: Naming Conventions](https://peps.python.org/pep-0008/#naming-conventions)
/// - [Semantic Versioning](https://semver.org/)
///
/// [PEP 8]: https://www.python.org/dev/peps/pep-0008/
/// [PEP 420]: https://www.python.org/dev/peps/pep-0420/
/// [`namespace-packages`]: https://beta.ruff.rs/docs/settings/#namespace-packages
#[violation]
pub struct ImportPrivateName {
    name: String,
    module: Option<String>,
}

impl Violation for ImportPrivateName {
    #[derive_message_formats]
    fn message(&self) -> String {
        let ImportPrivateName { name, module } = self;
        match module {
            Some(module) => {
                format!("Private name import `{name}` from external module `{module}`")
            }
            None => format!("Private name import `{name}`"),
        }
    }
}

/// PLC2701
pub(crate) fn import_private_name(
    checker: &Checker,
    scope: &Scope,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for binding_id in scope.binding_ids() {
        let binding = checker.semantic().binding(binding_id);
        let Some(import) = binding.as_any_import() else {
            continue;
        };
        let Some(import) = import.from_import() else {
            continue;
        };

        let module = import.module_name();
        let Some(root_module) = module.first() else {
            continue;
        };

        // Relative imports are not a public API.
        // Ex) `from . import foo`
        if module.starts_with(&["."]) {
            continue;
        }

        // We can also ignore dunder names.
        // Ex) `from __future__ import annotations`
        // Ex) `from foo import __version__`
        if root_module.starts_with("__") || import.member_name().starts_with("__") {
            continue;
        }

        // Ignore private imports from the same module.
        // Ex) `from foo import _bar` within `foo/baz.py`
        if checker
            .package()
            .is_some_and(|path| path.ends_with(root_module))
        {
            continue;
        }

        let call_path = import.call_path();
        if call_path.iter().any(|name| name.starts_with('_')) {
            // Ignore private imports used for typing.
            if binding.context.is_runtime()
                && binding
                    .references()
                    .map(|reference_id| checker.semantic().reference(reference_id))
                    .any(is_typing)
            {
                continue;
            }

            let private_name = call_path.iter().find(|name| name.starts_with('_')).unwrap();
            let external_module = Some(join(
                call_path.iter().take_while(|name| name != &private_name),
                ".",
            ))
            .filter(|module| !module.is_empty());
            diagnostics.push(Diagnostic::new(
                ImportPrivateName {
                    name: (*private_name).to_string(),
                    module: external_module,
                },
                binding.range(),
            ));
        }
    }
}

/// Returns `true` if the [`ResolvedReference`] is in a typing context.
fn is_typing(reference: &ResolvedReference) -> bool {
    reference.in_type_checking_block()
        || reference.in_typing_only_annotation()
        || reference.in_complex_string_type_definition()
        || reference.in_simple_string_type_definition()
        || reference.in_runtime_evaluated_annotation()
}
