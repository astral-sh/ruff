use std::borrow::Cow;

use itertools::Itertools;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::name::QualifiedName;
use ruff_python_semantic::{FromImport, Import, Imported, ResolvedReference, Scope};
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
/// are prone to unexpected changes, especially outside of semantic versioning.
///
/// Instead, consider using the public API of the module.
///
/// This rule ignores private name imports that are exclusively used in type
/// annotations. Ideally, types would be public; however, this is not always
/// possible when using third-party libraries.
///
/// ## Known problems
/// Does not ignore private name imports from within the module that defines
/// the private name if the module is defined with [PEP 420] namespace packages
/// (i.e., directories that omit the `__init__.py` file). Namespace packages
/// must be configured via the [`namespace-packages`] setting.
///
/// ## Example
/// ```python
/// from foo import _bar
/// ```
///
/// ## Options
/// - `namespace-packages`
///
/// ## References
/// - [PEP 8: Naming Conventions](https://peps.python.org/pep-0008/#naming-conventions)
/// - [Semantic Versioning](https://semver.org/)
///
/// [PEP 8]: https://www.python.org/dev/peps/pep-0008/
/// [PEP 420]: https://www.python.org/dev/peps/pep-0420/
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

        let import_info = match import {
            import if import.is_import() => ImportInfo::from(import.import().unwrap()),
            import if import.is_from_import() => ImportInfo::from(import.from_import().unwrap()),
            _ => continue,
        };

        let Some(root_module) = import_info.module_name.first() else {
            continue;
        };

        // Relative imports are not a public API.
        // Ex) `from . import foo`
        if import_info.module_name.starts_with(&["."]) {
            continue;
        }

        // We can also ignore dunder names.
        // Ex) `from __future__ import annotations`
        // Ex) `from foo import __version__`
        if root_module.starts_with("__") || import_info.member_name.starts_with("__") {
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

        // Ignore public imports; require at least one private name.
        // Ex) `from foo import bar`
        let Some((index, private_name)) = import_info
            .qualified_name
            .segments()
            .iter()
            .find_position(|name| name.starts_with('_'))
        else {
            continue;
        };

        // Ignore private imports used exclusively for typing.
        if !binding.references.is_empty()
            && binding
                .references()
                .map(|reference_id| checker.semantic().reference(reference_id))
                .all(is_typing)
        {
            continue;
        }

        let name = (*private_name).to_string();
        let module = if index > 0 {
            Some(import_info.qualified_name.segments()[..index].join("."))
        } else {
            None
        };
        diagnostics.push(Diagnostic::new(
            ImportPrivateName { name, module },
            binding.range(),
        ));
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

#[allow(clippy::struct_field_names)]
struct ImportInfo<'a> {
    module_name: &'a [&'a str],
    member_name: Cow<'a, str>,
    qualified_name: &'a QualifiedName<'a>,
}

impl<'a> From<&'a FromImport<'_>> for ImportInfo<'a> {
    fn from(import: &'a FromImport) -> Self {
        let module_name = import.module_name();
        let member_name = import.member_name();
        let qualified_name = import.qualified_name();
        Self {
            module_name,
            member_name,
            qualified_name,
        }
    }
}

impl<'a> From<&'a Import<'_>> for ImportInfo<'a> {
    fn from(import: &'a Import) -> Self {
        let module_name = import.module_name();
        let member_name = import.member_name();
        let qualified_name = import.qualified_name();
        Self {
            module_name,
            member_name,
            qualified_name,
        }
    }
}
