use std::borrow::Cow;

use itertools::Itertools;

use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::{self as ast, helpers::is_dunder, name::QualifiedName};
use ruff_python_semantic::{AnyImport, Imported, ResolvedReference, Scope};
use ruff_python_trivia::{SimpleTokenKind, SimpleTokenizer};
use ruff_text_size::Ranged;

use crate::Violation;
use crate::checkers::ast::Checker;
use crate::codes::Category;
use crate::package::PackageRoot;
use crate::rules::pylint::helpers::is_underscore_prefixed_public_member;

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
/// import _foo.bar
/// ```
///
/// ## Options
/// - `namespace-packages`
///
/// ## References
/// - [PEP 8: Naming Conventions](https://peps.python.org/pep-0008/#naming-conventions)
/// - [Semantic Versioning](https://semver.org/)
///
/// [PEP 8]: https://peps.python.org/pep-0008/
/// [PEP 420]: https://peps.python.org/pep-0420/
#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "v0.1.14", category = Category::Pedantic)]
pub(crate) struct ImportPrivateName {
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
pub(crate) fn import_private_name(checker: &Checker, scope: &Scope) {
    for binding_id in scope.binding_ids() {
        let binding = checker.semantic().binding(binding_id);
        let Some(import) = binding.as_any_import() else {
            continue;
        };

        // `import foo.bar` without an alias is a submodule import. Its qualified
        // name still includes every segment, so `import _foo.bar` is private in
        // the same way as `from _foo import bar` and `import _foo.bar as bar`.
        let import_info = ImportInfo::from_any(&import);

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
        if is_dunder(root_module) || is_dunder(&import_info.member_name) {
            continue;
        }

        // Ignore private imports from the same module.
        // Ex) `from foo import _bar` within `foo/baz.py`
        if checker
            .package()
            .map(PackageRoot::path)
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
            .find_position(|name| name.starts_with('_') && !is_dunder(name))
        else {
            continue;
        };

        if is_underscore_prefixed_public_member(import_info.qualified_name) {
            continue;
        }

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
        let range = match binding.source.map(|id| checker.semantic().statement(id)) {
            Some(ast::Stmt::ImportFrom(ast::StmtImportFrom { module, names, .. })) => {
                if index < import_info.module_name.len() {
                    module.as_ref().map_or(binding.range(), |module| {
                        SimpleTokenizer::starts_at(module.start(), checker.locator().contents())
                            .skip_trivia()
                            .find(|token| {
                                token.kind == SimpleTokenKind::Name
                                    && checker.locator().slice(token.range()) == *private_name
                            })
                            .map_or(binding.range(), |token| token.range())
                    })
                } else {
                    names
                        .iter()
                        .find(|alias| alias.name.as_str() == import_info.member_name)
                        .map_or(binding.range(), |alias| alias.name.range())
                }
            }
            // `import a.b._c` binds the whole dotted path. Point at the private
            // segment. An aliased import binds the alias instead, so the
            // highlight stays on the alias unless that name is the private one.
            Some(ast::Stmt::Import(_)) => {
                SimpleTokenizer::new(checker.locator().contents(), binding.range())
                    .skip_trivia()
                    .find(|token| {
                        token.kind == SimpleTokenKind::Name
                            && checker.locator().slice(token.range()) == *private_name
                    })
                    .map_or(binding.range(), |token| token.range())
            }
            _ => binding.range(),
        };

        checker.report_diagnostic(ImportPrivateName { name, module }, range);
    }
}

/// Returns `true` if the [`ResolvedReference`] is in a typing context.
fn is_typing(reference: &ResolvedReference) -> bool {
    reference.in_typing_context() || reference.in_runtime_evaluated_annotation()
}

#[expect(clippy::struct_field_names)]
struct ImportInfo<'a> {
    module_name: &'a [&'a str],
    member_name: Cow<'a, str>,
    qualified_name: &'a QualifiedName<'a>,
}

impl<'a> ImportInfo<'a> {
    fn from_any(import: &'a AnyImport<'_, 'a>) -> Self {
        Self {
            module_name: import.module_name(),
            member_name: import.member_name(),
            qualified_name: import.qualified_name(),
        }
    }
}
