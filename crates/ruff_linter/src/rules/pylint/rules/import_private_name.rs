use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{Alias, Stmt};

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
    checker: &mut Checker,
    stmt: &Stmt,
    names: &[Alias],
    module: Option<&str>,
    level: Option<u32>,
    module_path: Option<&[String]>,
) {
    // Relative imports are not a public API, so we don't need to check them.
    if level.map_or(false, |level| level > 0) {
        return;
    }
    if let Some(module) = module {
        if module.starts_with("__") {
            return;
        }
        // Ignore private imports from the same module.
        // TODO(tjkuson): Detect namespace packages automatically.
        // https://github.com/astral-sh/ruff/issues/6114
        if let Some(module_path) = module_path {
            let root_module = module_path.first().unwrap();
            if module.starts_with(root_module) {
                return;
            }
        }
        if module.starts_with('_') || module.contains("._") {
            let private_name = module
                .split('.')
                .find(|name| name.starts_with('_'))
                .unwrap_or(module);
            let external_module = Some(
                module
                    .split('.')
                    .take_while(|name| !name.starts_with('_'))
                    .collect::<Vec<_>>()
                    .join("."),
            )
            .filter(|module| !module.is_empty());
            checker.diagnostics.push(Diagnostic::new(
                ImportPrivateName {
                    name: private_name.to_string(),
                    module: external_module,
                },
                stmt.range(),
            ));
        }
        for name in names {
            if matches!(name.name.as_str(), "_") || name.name.starts_with("__") {
                continue;
            }
            if name.name.starts_with('_') {
                checker.diagnostics.push(Diagnostic::new(
                    ImportPrivateName {
                        name: name.name.to_string(),
                        module: Some(module.to_string()),
                    },
                    name.range(),
                ));
            }
        }
    }
}
