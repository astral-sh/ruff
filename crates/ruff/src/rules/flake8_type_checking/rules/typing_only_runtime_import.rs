use std::path::Path;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_semantic::binding::{
    Binding, BindingKind, ExecutionContext, FromImportation, Importation, SubmoduleImportation,
};

use crate::rules::isort::{categorize, ImportSection, ImportType};
use crate::settings::Settings;

/// ## What it does
/// Checks for first-party imports that are only used for type annotations, but
/// aren't defined in a type-checking block.
///
/// ## Why is this bad?
/// Unused imports add a performance overhead at runtime, and risk creating
/// import cycles.
///
/// ## Example
/// ```python
/// from __future__ import annotations
///
/// import A
///
///
/// def foo(a: A) -> int:
///     return len(a)
/// ```
///
/// Use instead:
/// ```python
/// from __future__ import annotations
///
/// from typing import TYPE_CHECKING
///
/// if TYPE_CHECKING:
///     import A
///
///
/// def foo(a: A) -> int:
///     return len(a)
/// ```
///
/// ## References
/// - [PEP 536](https://peps.python.org/pep-0563/#runtime-annotation-resolution-and-type-checking)
#[violation]
pub struct TypingOnlyFirstPartyImport {
    pub full_name: String,
}

impl Violation for TypingOnlyFirstPartyImport {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!(
            "Move application import `{}` into a type-checking block",
            self.full_name
        )
    }
}

/// ## What it does
/// Checks for third-party imports that are only used for type annotations, but
/// aren't defined in a type-checking block.
///
/// ## Why is this bad?
/// Unused imports add a performance overhead at runtime, and risk creating
/// import cycles.
///
/// ## Example
/// ```python
/// from __future__ import annotations
///
/// import pandas as pd
///
///
/// def foo(df: pd.DataFrame) -> int:
///     return len(df)
/// ```
///
/// Use instead:
/// ```python
/// from __future__ import annotations
///
/// from typing import TYPE_CHECKING
///
/// if TYPE_CHECKING:
///     import pandas as pd
///
///
/// def foo(df: pd.DataFrame) -> int:
///     return len(df)
/// ```
///
/// ## References
/// - [PEP 536](https://peps.python.org/pep-0563/#runtime-annotation-resolution-and-type-checking)
#[violation]
pub struct TypingOnlyThirdPartyImport {
    pub full_name: String,
}

impl Violation for TypingOnlyThirdPartyImport {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!(
            "Move third-party import `{}` into a type-checking block",
            self.full_name
        )
    }
}

/// ## What it does
/// Checks for standard library imports that are only used for type
/// annotations, but aren't defined in a type-checking block.
///
/// ## Why is this bad?
/// Unused imports add a performance overhead at runtime, and risk creating
/// import cycles.
///
/// ## Example
/// ```python
/// from __future__ import annotations
///
/// from pathlib import Path
///
///
/// def foo(path: Path) -> str:
///     return str(path)
/// ```
///
/// Use instead:
/// ```python
/// /// from __future__ import annotations
///
/// from typing import TYPE_CHECKING
///
/// if TYPE_CHECKING:
///     from pathlib import Path
///
///
/// def foo(path: Path) -> str:
///     return str(path)
/// ```
///
/// ## References
/// - [PEP 536](https://peps.python.org/pep-0563/#runtime-annotation-resolution-and-type-checking)
#[violation]
pub struct TypingOnlyStandardLibraryImport {
    pub full_name: String,
}

impl Violation for TypingOnlyStandardLibraryImport {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!(
            "Move standard library import `{}` into a type-checking block",
            self.full_name
        )
    }
}

/// Return `true` if `this` is implicitly loaded via importing `that`.
fn is_implicit_import(this: &Binding, that: &Binding) -> bool {
    match &this.kind {
        BindingKind::Importation(Importation {
            full_name: this_name,
            ..
        })
        | BindingKind::SubmoduleImportation(SubmoduleImportation {
            name: this_name, ..
        }) => match &that.kind {
            BindingKind::FromImportation(FromImportation {
                full_name: that_name,
                ..
            }) => {
                // Ex) `pkg.A` vs. `pkg`
                this_name
                    .rfind('.')
                    .map_or(false, |i| this_name[..i] == *that_name)
            }
            BindingKind::Importation(Importation {
                full_name: that_name,
                ..
            })
            | BindingKind::SubmoduleImportation(SubmoduleImportation {
                name: that_name, ..
            }) => {
                // Ex) `pkg.A` vs. `pkg.B`
                this_name == that_name
            }
            _ => false,
        },
        BindingKind::FromImportation(FromImportation {
            full_name: this_name,
            ..
        }) => match &that.kind {
            BindingKind::Importation(Importation {
                full_name: that_name,
                ..
            })
            | BindingKind::SubmoduleImportation(SubmoduleImportation {
                name: that_name, ..
            }) => {
                // Ex) `pkg.A` vs. `pkg`
                this_name
                    .rfind('.')
                    .map_or(false, |i| &this_name[..i] == *that_name)
            }
            BindingKind::FromImportation(FromImportation {
                full_name: that_name,
                ..
            }) => {
                // Ex) `pkg.A` vs. `pkg.B`
                this_name.rfind('.').map_or(false, |i| {
                    that_name
                        .rfind('.')
                        .map_or(false, |j| this_name[..i] == that_name[..j])
                })
            }
            _ => false,
        },
        _ => false,
    }
}

/// Return `true` if `name` is exempt from typing-only enforcement.
fn is_exempt(name: &str, exempt_modules: &[&str]) -> bool {
    let mut name = name;
    loop {
        if exempt_modules.contains(&name) {
            return true;
        }
        match name.rfind('.') {
            Some(idx) => {
                name = &name[..idx];
            }
            None => return false,
        }
    }
}

/// TCH001
pub fn typing_only_runtime_import(
    binding: &Binding,
    runtime_imports: &[&Binding],
    package: Option<&Path>,
    settings: &Settings,
) -> Option<Diagnostic> {
    // If we're in un-strict mode, don't flag typing-only imports that are
    // implicitly loaded by way of a valid runtime import.
    if !settings.flake8_type_checking.strict
        && runtime_imports
            .iter()
            .any(|import| is_implicit_import(binding, import))
    {
        return None;
    }

    let full_name = match &binding.kind {
        BindingKind::Importation(Importation { full_name, .. }) => full_name,
        BindingKind::FromImportation(FromImportation { full_name, .. }) => full_name.as_str(),
        BindingKind::SubmoduleImportation(SubmoduleImportation { full_name, .. }) => full_name,
        _ => return None,
    };

    if is_exempt(
        full_name,
        &settings
            .flake8_type_checking
            .exempt_modules
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>(),
    ) {
        return None;
    }

    if matches!(binding.context, ExecutionContext::Runtime)
        && binding.typing_usage.is_some()
        && binding.runtime_usage.is_none()
        && binding.synthetic_usage.is_none()
    {
        // Extract the module base and level from the full name.
        // Ex) `foo.bar.baz` -> `foo`, `0`
        // Ex) `.foo.bar.baz` -> `foo`, `1`
        let level = full_name.chars().take_while(|c| *c == '.').count();

        // Categorize the import.
        match categorize(
            full_name,
            Some(level),
            &settings.src,
            package,
            &settings.isort.known_modules,
            settings.target_version,
        ) {
            ImportSection::Known(ImportType::LocalFolder | ImportType::FirstParty) => {
                Some(Diagnostic::new(
                    TypingOnlyFirstPartyImport {
                        full_name: full_name.to_string(),
                    },
                    binding.range,
                ))
            }
            ImportSection::Known(ImportType::ThirdParty) | ImportSection::UserDefined(_) => {
                Some(Diagnostic::new(
                    TypingOnlyThirdPartyImport {
                        full_name: full_name.to_string(),
                    },
                    binding.range,
                ))
            }
            ImportSection::Known(ImportType::StandardLibrary) => Some(Diagnostic::new(
                TypingOnlyStandardLibraryImport {
                    full_name: full_name.to_string(),
                },
                binding.range,
            )),
            ImportSection::Known(ImportType::Future) => {
                unreachable!("`__future__` imports should be marked as used")
            }
        }
    } else {
        None
    }
}
